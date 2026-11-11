//! Compatibility for editors that save by renaming a temporary file over the source.
use notify::{event::ModifyKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{fs::File, path::Path, time::SystemTime};

pub fn watch_atomic_saves() -> Result<RecommendedWatcher, slint::PlatformError> {
    // Slint 1.18.1's watcher only emits Created/Deleted when the path's
    // existence changes. An atomic replacement leaves it present, so it
    // misses the save. Touch the replacement's mtime to emit a Modify event
    // for Slint's own watcher. Never rewrite content or react to our touch.
    let mut watcher = notify::recommended_watcher(|event: notify::Result<notify::Event>| {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                eprintln!("热加载文件监听失败: {error}");
                return;
            }
        };
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
        ) {
            return;
        }
        for path in event.paths {
            if !path.is_file() {
                continue;
            }
            if let Err(error) =
                File::open(&path).and_then(|file| file.set_modified(SystemTime::now()))
            {
                eprintln!("无法通知热加载更新 {}: {error}", path.display());
            }
        }
    })
    .map_err(|error| slint::PlatformError::from(error.to_string()))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for directory in ["ui", "assets"] {
        watcher
            .watch(&root.join(directory), RecursiveMode::Recursive)
            .map_err(|error| slint::PlatformError::from(error.to_string()))?;
    }
    eprintln!("UI 热加载已启用：保存 ui/ 下的 .slint 文件即可刷新界面。");
    Ok(watcher)
}
