use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

pub(crate) const NO_TRIM_FRAME: u64 = u64::MAX;

#[derive(Debug)]
pub(crate) struct PlaybackClock {
    audible_frame_at_update: u64,
    submitted_frame: u64,
    sample_rate: u32,
    updated_at: Option<Instant>,
}

impl PlaybackClock {
    pub(crate) fn new() -> Self {
        Self {
            audible_frame_at_update: 0,
            submitted_frame: 0,
            sample_rate: 0,
            updated_at: None,
        }
    }

    pub(crate) fn reset_to(&mut self, frame: u64, sample_rate: u32) {
        self.audible_frame_at_update = frame;
        self.submitted_frame = frame;
        self.sample_rate = sample_rate;
        self.updated_at = None;
    }

    fn update(
        &mut self,
        audible_frame_at_update: u64,
        submitted_frame: u64,
        sample_rate: u32,
        updated_at: Instant,
    ) {
        self.audible_frame_at_update = audible_frame_at_update;
        self.submitted_frame = submitted_frame;
        self.sample_rate = sample_rate;
        self.updated_at = Some(updated_at);
    }

    fn estimate(&self, now: Instant) -> Option<u64> {
        let updated_at = self.updated_at?;
        if self.sample_rate == 0 {
            return Some(self.submitted_frame);
        }

        let elapsed_frames =
            duration_to_frames(now.saturating_duration_since(updated_at), self.sample_rate);
        Some(
            self.audible_frame_at_update
                .saturating_add(elapsed_frames)
                .min(self.submitted_frame),
        )
    }
}

pub(crate) struct SharedState {
    pub(crate) is_paused: AtomicBool,
    pub(crate) current_frame: AtomicU64,
    pub(crate) playback_clock: Mutex<PlaybackClock>,
    pub(crate) trim_until_frame: AtomicU64,
    pub(crate) sample_rate: AtomicU32,
    pub(crate) has_seek_request: AtomicBool,
    pub(crate) seek_request: Mutex<Option<Duration>>,
    pub(crate) is_terminating: AtomicBool,
    pub(crate) discard_buffer: AtomicBool,
    pub(crate) is_discarding_buffer: AtomicBool,
    pub(crate) decoder_done: AtomicBool,
    pub(crate) is_finished: AtomicBool,
    pub(crate) finish_notify: Notify,
    pub(crate) buffered_frames: AtomicU64,
    pub(crate) waiting_for_seek: AtomicBool,
}

impl SharedState {
    pub(crate) fn schedule_seek(&self, target: Duration) {
        let mut seek_req = self.seek_request.lock().unwrap();
        let sample_rate = self.sample_rate.load(std::sync::atomic::Ordering::Relaxed);
        let target_frame = (target.as_secs_f64() * sample_rate as f64) as u64;

        self.discard_buffer
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.current_frame
            .store(target_frame, std::sync::atomic::Ordering::SeqCst);
        self.waiting_for_seek
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.has_seek_request
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.trim_until_frame
            .store(NO_TRIM_FRAME, std::sync::atomic::Ordering::SeqCst);
        self.reset_playback_clock(target_frame);
        *seek_req = Some(target);
    }

    pub(crate) fn try_take_seek_request(&self) -> Result<Option<Duration>, ()> {
        match self.seek_request.try_lock() {
            Ok(mut seek_req) => Ok(seek_req.take()),
            Err(_) => Err(()),
        }
    }

    pub(crate) fn commit_seek_completion_if_current(
        &self,
        anchor_frame: u64,
        trim_until_frame: Option<u64>,
    ) -> bool {
        let seek_req = self.seek_request.lock().unwrap();
        if seek_req.is_none() {
            self.current_frame
                .store(anchor_frame, std::sync::atomic::Ordering::SeqCst);
            self.trim_until_frame.store(
                trim_until_frame.unwrap_or(NO_TRIM_FRAME),
                std::sync::atomic::Ordering::SeqCst,
            );
            self.has_seek_request
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.reset_playback_clock(anchor_frame);
            true
        } else {
            false
        }
    }

    pub(crate) fn update_playback_clock_from_output(
        &self,
        buffer_start_frame: u64,
        submitted_frame: u64,
        output_latency: Duration,
    ) {
        let sample_rate = self.sample_rate.load(std::sync::atomic::Ordering::Relaxed);
        let latency_frames = duration_to_frames(output_latency, sample_rate);
        let audible_frame_at_update = buffer_start_frame.saturating_sub(latency_frames);

        self.playback_clock.lock().unwrap().update(
            audible_frame_at_update,
            submitted_frame,
            sample_rate,
            Instant::now(),
        );
    }

    pub(crate) fn progress_frame(&self) -> u64 {
        let submitted_frame = self
            .current_frame
            .load(std::sync::atomic::Ordering::Relaxed);
        self.playback_clock
            .lock()
            .unwrap()
            .estimate(Instant::now())
            .unwrap_or(submitted_frame)
            .min(submitted_frame)
    }

    pub(crate) fn reset_playback_clock(&self, frame: u64) {
        let sample_rate = self.sample_rate.load(std::sync::atomic::Ordering::Relaxed);
        self.playback_clock
            .lock()
            .unwrap()
            .reset_to(frame, sample_rate);
    }

    pub(crate) fn clear_trim(&self) {
        self.trim_until_frame
            .store(NO_TRIM_FRAME, std::sync::atomic::Ordering::SeqCst);
    }
}

pub(crate) fn duration_to_frames(duration: Duration, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }

    (duration.as_secs_f64() * sample_rate as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn create_state(sample_rate: u32) -> SharedState {
        SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            playback_clock: Mutex::new(PlaybackClock::new()),
            trim_until_frame: AtomicU64::new(NO_TRIM_FRAME),
            sample_rate: AtomicU32::new(sample_rate),
            has_seek_request: AtomicBool::new(false),
            seek_request: Mutex::new(None),
            is_terminating: AtomicBool::new(false),
            discard_buffer: AtomicBool::new(false),
            is_discarding_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
        }
    }

    #[test]
    fn scheduled_seek_sets_progress_anchor_and_flush_flags() {
        let state = create_state(48_000);
        let target = Duration::from_millis(2_500);

        state.schedule_seek(target);

        assert_eq!(*state.seek_request.lock().unwrap(), Some(target));
        assert!(state.discard_buffer.load(Ordering::SeqCst));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 120_000);
    }

    #[test]
    fn taking_scheduled_seek_keeps_pending_write_gate() {
        let state = create_state(48_000);
        let target = Duration::from_secs(30);

        state.schedule_seek(target);

        assert_eq!(state.try_take_seek_request().unwrap(), Some(target));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert_eq!(state.try_take_seek_request().unwrap(), None);
    }

    #[test]
    fn seek_completion_commits_only_when_no_new_seek_is_pending() {
        let state = create_state(48_000);

        state.schedule_seek(Duration::from_secs(10));
        assert_eq!(
            state.try_take_seek_request().unwrap(),
            Some(Duration::from_secs(10))
        );
        state.schedule_seek(Duration::from_secs(20));

        assert!(!state.commit_seek_completion_if_current(480_000, Some(500_000)));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert_eq!(
            state.current_frame.load(Ordering::SeqCst),
            960_000,
            "newer scheduled seek target must not be overwritten by an older seek completion"
        );
        assert_eq!(state.trim_until_frame.load(Ordering::SeqCst), NO_TRIM_FRAME);

        assert_eq!(
            state.try_take_seek_request().unwrap(),
            Some(Duration::from_secs(20))
        );
        assert!(state.commit_seek_completion_if_current(960_000, Some(960_000)));
        assert!(!state.has_seek_request.load(Ordering::SeqCst));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 960_000);
        assert_eq!(state.trim_until_frame.load(Ordering::SeqCst), 960_000);
    }

    #[test]
    fn playback_clock_reports_audible_frame_not_submitted_frame() {
        let state = create_state(48_000);
        state.current_frame.store(1_000_000, Ordering::SeqCst);

        state.update_playback_clock_from_output(1_000_000, 1_001_024, Duration::from_millis(50));

        let progress = state.progress_frame();
        assert!(
            progress < 1_000_000,
            "progress must account for output latency; old logic would report submitted frame"
        );
        assert!(
            progress >= 997_500 && progress <= 999_000,
            "expected around 997600 frames, got {progress}"
        );
    }

    #[test]
    fn playback_clock_never_reports_beyond_submitted_frame() {
        let mut clock = PlaybackClock::new();
        let now = Instant::now();
        clock.update(10_000, 10_500, 48_000, now - Duration::from_secs(10));

        assert_eq!(clock.estimate(now), Some(10_500));
    }
}
