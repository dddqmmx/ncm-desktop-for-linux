//! 音频引擎。
//!
//! 分四层，各自只依赖下一层：
//!
//! ```text
//!   player (AudioPlayer)   一次播放的生命周期：开流、预解码、切歌、停流
//!     ├─ session           跨线程协议（输出回调 / 解码线程 / 控制面）
//!     ├─ decoder           解复用与解码；seek 落点、裁剪
//!     ├─ output            cpal 设备身份、格式协商、输出回调
//!     └─ downloads         HTTP 流式下载与歌曲缓存写入
//!   source / cache_tracker 支撑：可 seek 的流式源、缓存进度跟踪
//!   codecs               解码器注册表（含 Opus）与音源参数归一化
//! ```
//!
//! 阅读顺序建议：`session`（并发约定）→ `output::stream::render_block`（消费侧）
//! → `decoder`（产出侧）→ `player`（把三者接起来）。

pub(crate) mod cache_tracker;
pub mod codecs;
pub(crate) mod decoder;
pub(crate) mod device_reservation;
pub(crate) mod downloads;
pub(crate) mod http_client;
pub(crate) mod output;
pub(crate) mod player;
pub(crate) mod session;
pub(crate) mod source;
pub mod utils;

pub use output::OutputDeviceInfo;
pub use player::AudioPlayer;

/// Probe metadata without constructing an audio output device.
pub async fn probe_file_duration_ms(path: impl AsRef<std::path::Path>) -> Result<u64, String> {
    decoder::probe_file_duration_ms(path.as_ref().to_string_lossy().into_owned()).await.map_err(|e| e.to_string())
}
