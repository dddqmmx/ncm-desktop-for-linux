//! 缓存文件的原子写入。
//!
//! 同一个文件可能被两条线程同时写：后台下载线程在写 `<song>.meta.json` 的进度，
//! 控制面在准备播放时也会改写同一份 meta；`index.json` 与 `settings.json` 则由
//! 多条 napi 调用共用。写入必须满足两条：
//!
//! 1. **读者永远看不到半截内容**：先写临时文件再 `rename`（同目录内 rename 是
//!    原子的）。
//! 2. **两个写者不能互相踩**：临时文件名必须唯一。共用 `<path>.tmp` 时，A 写完
//!    临时文件、B 又写同一个临时文件、A rename 走的是 B 的内容，B 的 rename 则
//!    以 ENOENT 失败——既是丢更新，也是一次莫名其妙的 I/O 错误。
//!
//! 并发写的语义是"最后完成的那次生效"，不做跨线程的读-改-写合并；需要合并的
//! 状态（下载进度、目录索引）由各自的持有者在锁内完成。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cache::error::CacheResult;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 与目标同目录的临时文件名。同目录才能保证 rename 不跨文件系统。
fn temp_path_for(path: &Path) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cache".to_string());
    let temp_name = format!(".{file_name}.{}.{sequence}.tmp", std::process::id());
    path.with_file_name(temp_name)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> CacheResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = temp_path_for(path);
    // 写入失败时顺手清掉临时文件，避免在半途退出的路径上留下垃圾。
    if let Err(err) = fs::write(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(err.into());
    }
    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err.into());
    }
    Ok(())
}

pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> CacheResult<()> {
    let serialized = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cache-io-util-{tag}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_the_content_and_leaves_no_temporary_file_behind() {
        let dir = temp_dir("basic");
        let path = dir.join("index.json");

        atomic_write(&path, b"hello").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"hello");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "index.json")
            .collect();
        assert!(leftovers.is_empty(), "残留临时文件：{leftovers:?}");

        let _ = fs::remove_dir_all(dir);
    }

    /// 两个线程并发写同一个文件：两次写入都必须成功，且最终内容是其中**完整**
    /// 的一份。共用 `.tmp` 名字的旧实现会在这里以 ENOENT 失败。
    #[test]
    fn concurrent_writers_never_fail_and_never_tear() {
        let dir = temp_dir("concurrent");
        let path = dir.join("index.json");
        let a = vec![b'a'; 64 * 1024];
        let b = vec![b'b'; 64 * 1024];

        let mut handles = Vec::new();
        for payload in [a.clone(), b.clone()] {
            let path = path.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..50 {
                    atomic_write(&path, &payload).expect("并发写不得失败");
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let content = fs::read(&path).unwrap();
        assert_eq!(content.len(), 64 * 1024);
        let is_all_a = content.iter().all(|byte| *byte == b'a');
        let is_all_b = content.iter().all(|byte| *byte == b'b');
        assert!(
            is_all_a || is_all_b,
            "读者只能看到某一次完整写入，不能是两次写入的混合"
        );

        let _ = fs::remove_dir_all(dir);
    }

    /// 写失败（目标是个目录）时临时文件必须被清掉，不能在缓存目录里越积越多。
    #[test]
    fn a_failed_rename_cleans_up_the_temporary_file() {
        let dir = temp_dir("failure");
        let path = dir.join("target");
        fs::create_dir(&path).unwrap();

        assert!(atomic_write(&path, b"data").is_err());

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "target")
            .collect();
        assert!(leftovers.is_empty(), "失败路径残留临时文件：{leftovers:?}");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_writers_to_different_files_do_not_share_a_temp_name() {
        let dir = temp_dir("distinct");
        let shared = Arc::new(dir.clone());
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let dir = Arc::clone(&shared);
                thread::spawn(move || {
                    let path = dir.join("same-name.json");
                    atomic_write(&path, format!("writer-{index}").as_bytes()).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let content = fs::read_to_string(dir.join("same-name.json")).unwrap();
        assert!(content.starts_with("writer-"));

        let _ = fs::remove_dir_all(dir);
    }
}
