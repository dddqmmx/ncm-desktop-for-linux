//! 播放控制面。
//!
//! 分层（自上而下）：
//! - [`service`]：napi 接口，只做参数净化与命令投递；
//! - [`worker`]：唯一控制线程，串行处理命令与 tick，负责所有播放策略；
//! - [`backend`]：端点上的播放器抽象（`PlayerBackend` / `PlayerFactory`），
//!   真实实现转发给 `audio::player`，测试用假实现；
//! - [`state`]：发布给 UI 的播放快照。
//!
//! 跨线程时序（seek 换代、缓冲门控、终局判定）不在这里，在 `audio::session`。

mod backend;
mod command;
mod service;
mod state;
mod types;
mod worker;

pub use service::PlayerService;

#[cfg(test)]
mod tests;

pub use state::PlaybackSnapshot;
pub use types::{AudioDeviceInfo, PlaybackStatus};
