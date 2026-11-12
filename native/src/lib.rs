// 对外公开只为让 examples/ 下的诊断工具驱动真实播放链路；napi 导出的仍只有
// `#[napi]` 标注的 CacheService / PlayerService。
pub mod audio;
mod bindings;
mod cache;
pub mod diagnostics;
mod player;
mod runtime;

pub use bindings::{CacheService, PlayerService};

mod service_error;
pub use cache::{CacheStats, CachedSongSource, SongCacheProgress};
pub use player::{AudioDeviceInfo, PlaybackSnapshot, PlaybackStatus};
