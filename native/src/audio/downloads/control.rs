//! 后台歌曲缓存的下载控制：取消、播放位置上报、完成等待。
//!
//! 下载任务与调用方（缓存服务、切歌逻辑）之间的唯一握手对象。三条约定：
//!
//! - **取消只发信号、不等收尾**：下载线程可能卡在底层阻塞点，等待会拖死切歌；
//! - **等完成必须有超时**：否则一次挂起的下载会让调用方永久卡住；
//! - **播放位置只在变化时推进版本号**：驱动下载任务从节流中醒来，不重发 seek。

use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Duration;

pub(crate) struct SongCacheDownloadOutcome {
    pub downloaded_bytes: u64,
    pub is_complete: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct SongCacheDownloadControlState {
    pub(crate) playback_position_ms: u64,
    pub(crate) version: u64,
    pub(crate) cancelled: bool,
    finished: bool,
}

pub(crate) struct SongCacheDownloadControl {
    state: StdMutex<SongCacheDownloadControlState>,
    changed: Condvar,
    cancel_download: StdMutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl SongCacheDownloadControl {
    pub fn new() -> Self {
        Self {
            state: StdMutex::new(SongCacheDownloadControlState {
                playback_position_ms: 0,
                version: 0,
                cancelled: false,
                finished: false,
            }),
            changed: Condvar::new(),
            cancel_download: StdMutex::new(None),
        }
    }

    pub fn update_playback_position(&self, playback_position_ms: u64) {
        if let Ok(mut state) = self.state.lock() {
            if state.cancelled || state.playback_position_ms == playback_position_ms {
                return;
            }
            state.playback_position_ms = playback_position_ms;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    pub fn cancel(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cancelled = true;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
        if let Ok(cancel_download) = self.cancel_download.lock()
            && let Some(cancel_download) = cancel_download.as_ref()
        {
            cancel_download();
        }
    }

    pub(crate) fn attach_download_canceller<F>(&self, cancel_download: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let cancelled = self
            .state
            .lock()
            .map(|state| state.cancelled)
            .unwrap_or(true);
        if cancelled {
            cancel_download();
            return;
        }
        if let Ok(mut slot) = self.cancel_download.lock() {
            *slot = Some(Box::new(cancel_download));
        }
    }

    pub(crate) fn wait_for_change(&self, version: u64) -> SongCacheDownloadControlState {
        let mut state = self.state.lock().unwrap();
        while !state.cancelled && state.version == version {
            state = self.changed.wait(state).unwrap();
        }
        *state
    }

    pub(crate) fn wake(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    pub fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.finished = true;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    /// 只有测试在等它：生产路径的取消从不等待收尾（见模块注释）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wait_finished_timeout(&self, timeout: Duration) {
        // 下载任务若卡在 stream-download 的 wait_for_read，cancel 无法立刻打断。
        // 这里必须有超时，否则 cancelSongCacheDownload / 切歌会永久卡住主流程。
        let mut state = self.state.lock().unwrap();
        let deadline = std::time::Instant::now() + timeout;
        while !state.finished {
            let now = std::time::Instant::now();
            if now >= deadline {
                eprintln!("[cache] wait_finished timed out after cancel");
                break;
            }
            let (next, timeout_result) = self
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap();
            state = next;
            if timeout_result.timed_out() && !state.finished {
                eprintln!("[cache] wait_finished timed out after cancel");
                break;
            }
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.cancelled)
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn wait_finished_times_out_instead_of_hanging_when_download_never_finishes() {
        let control = SongCacheDownloadControl::new();
        // 模拟：cancel 后下载线程卡在 stream-download 内部，never finish。
        control.cancel();

        let started = std::time::Instant::now();
        control.wait_finished_timeout(Duration::from_millis(80));
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(60),
            "wait_finished should wait up to the timeout when unfinished"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "wait_finished must not hang forever after cancel (elapsed={elapsed:?})"
        );
    }

    #[test]
    fn finish_unblocks_waiters_immediately() {
        let control = Arc::new(SongCacheDownloadControl::new());
        let waiter = {
            let control = Arc::clone(&control);
            std::thread::spawn(move || control.wait_finished_timeout(Duration::from_secs(2)))
        };

        std::thread::sleep(Duration::from_millis(20));
        control.cancel();
        control.finish();

        let started = std::time::Instant::now();
        waiter.join().expect("wait_finished thread panicked");
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "finish() after cancel must release waiters promptly"
        );
    }

    /// 播放位置只在**变化**时推进版本号：轮询周期比播放进度粒度更密，
    /// 同一位置的重复上报不该反复唤醒下载任务。
    #[test]
    fn repeated_positions_do_not_wake_the_download_task() {
        let control = SongCacheDownloadControl::new();
        control.update_playback_position(5_000);
        let version = control.state.lock().unwrap().version;

        control.update_playback_position(5_000);
        assert_eq!(control.state.lock().unwrap().version, version);

        control.update_playback_position(5_100);
        assert_ne!(control.state.lock().unwrap().version, version);
    }

    #[test]
    fn cancellation_is_sticky() {
        let control = SongCacheDownloadControl::new();
        control.cancel();
        assert!(control.is_cancelled());
        assert!(control.state.lock().unwrap().cancelled);
    }
}
