//! 后台歌曲缓存的下载登记簿。
//!
//! 一条后台下载的生命周期由**登记簿**仲裁，而不是由它自己判断"我还在不在":
//! 取消、被同曲的新下载顶替、整个缓存被清空，都可能发生在下载任务收尾之前。
//! 如果收尾方自己写目录，就会出现"用户点了清空，几秒后缓存里又冒出一条"——
//! 条目指向的文件可能已被删除，或被重新创建成半截文件。
//!
//! 因此所有"下载完成后写目录 / 清理文件"的动作都必须向登记簿**声明**一次：
//! [`DownloadRegistry::complete_if_current`]。登记簿在持锁状态下判定身份、执行
//! 写入；取消与清空也在同一把锁下先失效登记再动文件。两者因此不会交错：
//! 要么收尾先完成（清空随后把条目一起删掉），要么清空先发生（收尾直接作废）。
//!
//! 锁序：登记簿 → 缓存状态。调用方不得在持有缓存状态锁时回调这里。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::audio::downloads::SongCacheDownloadControl;

#[derive(Default)]
pub(crate) struct DownloadRegistry {
    inner: Mutex<HashMap<String, Arc<SongCacheDownloadControl>>>,
}

impl DownloadRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 登记一条新下载。同一路径上若已有旧下载，旧的一律取消——同曲重播时
    /// 旧任务仍在往同一个文件写，不取消就是两个写者对同一段字节。
    pub(crate) fn register(&self, metadata_path: &str, control: Arc<SongCacheDownloadControl>) {
        let previous = self.lock().insert(metadata_path.to_string(), control);
        if let Some(previous) = previous {
            previous.cancel();
            previous.finish();
        }
    }

    /// 取出一条下载的控制句柄（上报播放位置用）。不改变登记状态。
    pub(crate) fn control(&self, metadata_path: &str) -> Option<Arc<SongCacheDownloadControl>> {
        self.lock().get(metadata_path).cloned()
    }

    /// 取消并移除一条下载。只发取消信号，不等待后台任务收尾——下载线程若卡在
    /// 底层阻塞点，等待会拖死切歌/清空整条链路。
    pub(crate) fn cancel(&self, metadata_path: &str) -> bool {
        let control = self.lock().remove(metadata_path);
        let Some(control) = control else {
            return false;
        };
        control.cancel();
        control.finish();
        true
    }

    /// 取消全部下载（清空缓存时用），返回被取消的数量。
    pub(crate) fn cancel_all(&self) -> usize {
        let drained = self
            .lock()
            .drain()
            .map(|(_, control)| control)
            .collect::<Vec<_>>();
        let count = drained.len();
        for control in drained {
            control.cancel();
            control.finish();
        }
        count
    }

    /// 下载收尾：只有当这条下载仍是该路径的当前登记时，才执行 `commit`。
    ///
    /// `commit` 在持锁状态下运行，因此与 [`cancel`](Self::cancel) /
    /// [`cancel_all`](Self::cancel_all) 串行——后者恰好是"删文件"的那一侧。
    /// 返回 `false` 表示这条下载已被取消或顶替，调用方不得再写任何东西。
    pub(crate) fn complete_if_current<T>(
        &self,
        metadata_path: &str,
        control: &Arc<SongCacheDownloadControl>,
        commit: impl FnOnce() -> T,
    ) -> Option<T> {
        let mut guard = self.lock();
        let is_current = guard
            .get(metadata_path)
            .is_some_and(|active| Arc::ptr_eq(active, control));
        if !is_current || control.is_cancelled() {
            if is_current {
                guard.remove(metadata_path);
            }
            control.finish();
            return None;
        }

        let result = commit();
        guard.remove(metadata_path);
        control.finish();
        Some(result)
    }

    /// 当前是否还有这条下载的登记（测试与诊断用）。
    #[cfg(test)]
    pub(crate) fn is_registered(&self, metadata_path: &str) -> bool {
        self.lock().contains_key(metadata_path)
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<SongCacheDownloadControl>>> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    fn control() -> Arc<SongCacheDownloadControl> {
        Arc::new(SongCacheDownloadControl::new())
    }

    #[test]
    fn a_current_download_gets_to_commit_and_is_then_unregistered() {
        let registry = DownloadRegistry::new();
        let control = control();
        registry.register("/tmp/song.meta.json", Arc::clone(&control));

        let committed = registry.complete_if_current("/tmp/song.meta.json", &control, || 42);

        assert_eq!(committed, Some(42));
        assert!(!registry.is_registered("/tmp/song.meta.json"));
    }

    #[test]
    fn a_cancelled_download_cannot_write_anything() {
        let registry = DownloadRegistry::new();
        let control = control();
        registry.register("/tmp/song.meta.json", Arc::clone(&control));
        assert!(registry.cancel("/tmp/song.meta.json"));

        let committed = registry.complete_if_current("/tmp/song.meta.json", &control, || {
            panic!("cancelled download must not commit")
        });

        assert_eq!(committed, None);
    }

    /// 清空缓存之后迟到的收尾不许复活条目——这是"点了清空，几秒后缓存又冒出来"
    /// 的根因。
    #[test]
    fn a_late_finish_after_clear_does_not_resurrect_the_entry() {
        let registry = DownloadRegistry::new();
        let control = control();
        registry.register("/tmp/song.meta.json", Arc::clone(&control));

        assert_eq!(registry.cancel_all(), 1);

        let committed = registry.complete_if_current("/tmp/song.meta.json", &control, || ());
        assert_eq!(committed, None);
        assert!(!registry.is_registered("/tmp/song.meta.json"));
    }

    /// 同曲被新下载顶替：旧任务收尾时必须作废，不能把旧结果写进新条目。
    #[test]
    fn a_superseded_download_loses_the_right_to_commit() {
        let registry = DownloadRegistry::new();
        let old = control();
        let new = control();
        registry.register("/tmp/song.meta.json", Arc::clone(&old));
        registry.register("/tmp/song.meta.json", Arc::clone(&new));

        assert!(old.is_cancelled(), "旧下载必须被取消");
        assert_eq!(
            registry.complete_if_current("/tmp/song.meta.json", &old, || ()),
            None
        );
        assert_eq!(
            registry.complete_if_current("/tmp/song.meta.json", &new, || "new"),
            Some("new")
        );
    }

    /// 收尾与清空互斥：清空在收尾写入之前/之后发生都是确定的结果，
    /// 不存在"清空删完文件、收尾又写下条目"的交错。
    #[test]
    fn completion_and_clear_are_serialized() {
        let registry = Arc::new(DownloadRegistry::new());
        let control = control();
        registry.register("/tmp/song.meta.json", Arc::clone(&control));

        let (started_tx, started_rx) = mpsc::channel();
        let committed = Arc::new(AtomicBool::new(false));

        let finisher = {
            let registry = Arc::clone(&registry);
            let control = Arc::clone(&control);
            let committed = Arc::clone(&committed);
            std::thread::spawn(move || {
                registry.complete_if_current("/tmp/song.meta.json", &control, || {
                    started_tx.send(()).unwrap();
                    // 模拟写目录（持锁）。
                    std::thread::sleep(Duration::from_millis(30));
                    committed.store(true, Ordering::SeqCst);
                })
            })
        };

        // 等收尾进入 commit，再去清空：必须等它写完才能返回（串行化）。
        started_rx.recv().unwrap();
        registry.cancel_all();
        assert!(
            committed.load(Ordering::SeqCst),
            "清空必须排在已完成判定之后，而不是与写入交错"
        );

        finisher.join().unwrap();
        assert!(!registry.is_registered("/tmp/song.meta.json"));
    }
}
