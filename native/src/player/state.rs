use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::types::PlaybackStatus;

/// worker 与 napi getter 之间共享的 UI 快照。字段是相互独立的原子量，
/// 不存在跨字段一致性约束（读端只做展示），因此统一使用 Relaxed。
pub(crate) struct SharedState {
    progress_ms: AtomicU32,
    playback_status: AtomicU32,
    buffering: AtomicU32,
    /// 当前会话实际生效的音量（万分比，10_000 = 1.0 直通）。
    volume_milli: AtomicU32,
    /// 当前会话是否被严格 BitPerfect 锁定为 1.0 直通（SetVolume 被丢弃）。
    volume_locked: AtomicBool,
}

impl SharedState {
    pub(crate) fn new() -> Self {
        Self {
            progress_ms: AtomicU32::new(0),
            playback_status: AtomicU32::new(PlaybackStatus::Stopped.as_u32()),
            buffering: AtomicU32::new(0),
            volume_milli: AtomicU32::new(10_000),
            volume_locked: AtomicBool::new(false),
        }
    }

    pub(crate) fn progress_ms(&self) -> u32 {
        self.progress_ms.load(Ordering::Relaxed)
    }

    pub(crate) fn set_progress_ms(&self, progress_ms: u32) {
        self.progress_ms.store(progress_ms, Ordering::Relaxed);
    }

    pub(crate) fn playback_status(&self) -> PlaybackStatus {
        PlaybackStatus::from_u32(self.playback_status.load(Ordering::Relaxed))
    }

    pub(crate) fn set_playback_status(&self, status: PlaybackStatus) {
        self.playback_status
            .store(status.as_u32(), Ordering::Relaxed);
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playback_status.load(Ordering::Relaxed) == PlaybackStatus::Playing.as_u32()
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.playback_status.load(Ordering::Relaxed) == PlaybackStatus::Paused.as_u32()
    }

    pub(crate) fn is_buffering(&self) -> bool {
        self.buffering.load(Ordering::Relaxed) != 0
    }

    pub(crate) fn set_buffering(&self, buffering: bool) {
        self.buffering
            .store(u32::from(buffering), Ordering::Relaxed);
    }

    /// 当前会话实际生效的音量（0.0..=1.0）。渲染层轮询回读，用作失步校对。
    pub(crate) fn volume(&self) -> f64 {
        self.volume_milli.load(Ordering::Relaxed) as f64 / 10_000.0
    }

    /// 当前会话是否被严格 BitPerfect 锁定为 1.0 直通。
    pub(crate) fn volume_locked(&self) -> bool {
        self.volume_locked.load(Ordering::Relaxed)
    }

    /// worker 在每次会话状态变化处同步：开播（含设备切换重放）、SetVolume。
    /// 非有限值按 1.0 收敛，与 SharedState(audio) 的净化语义一致。
    pub(crate) fn set_volume(&self, volume: f64) {
        let sanitized = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.volume_milli
            .store((sanitized * 10_000.0).round() as u32, Ordering::Relaxed);
    }

    pub(crate) fn set_volume_locked(&self, locked: bool) {
        self.volume_locked.store(locked, Ordering::Relaxed);
    }

    pub(crate) fn reset_playback(&self) {
        self.set_playback_status(PlaybackStatus::Stopped);
        self.set_progress_ms(0);
        self.set_buffering(false);
        self.set_volume(1.0);
        self.set_volume_locked(false);
    }
}
