use std::sync::atomic::{AtomicU32, Ordering};

use super::types::PlaybackStatus;

pub(crate) struct SharedState {
    progress_ms: AtomicU32,
    playback_status: AtomicU32,
    buffering: AtomicU32,
}

impl SharedState {
    pub(crate) fn new() -> Self {
        Self {
            progress_ms: AtomicU32::new(0),
            playback_status: AtomicU32::new(PlaybackStatus::Stopped.as_u32()),
            buffering: AtomicU32::new(0),
        }
    }

    pub(crate) fn progress_ms(&self) -> u32 {
        self.progress_ms.load(Ordering::Relaxed)
    }

    pub(crate) fn set_progress_ms(&self, progress_ms: u32, ordering: Ordering) {
        self.progress_ms.store(progress_ms, ordering);
    }

    pub(crate) fn playback_status(&self) -> PlaybackStatus {
        PlaybackStatus::from_u32(self.playback_status.load(Ordering::Relaxed))
    }

    pub(crate) fn set_playback_status(&self, status: PlaybackStatus, ordering: Ordering) {
        self.playback_status.store(status.as_u32(), ordering);
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

    pub(crate) fn set_buffering(&self, buffering: bool, ordering: Ordering) {
        self.buffering.store(u32::from(buffering), ordering);
    }

    pub(crate) fn reset_playback(&self) {
        self.set_playback_status(PlaybackStatus::Stopped, Ordering::SeqCst);
        self.set_progress_ms(0, Ordering::SeqCst);
        self.set_buffering(false, Ordering::SeqCst);
    }
}
