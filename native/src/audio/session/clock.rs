//! 可听的播放位置：输出回调每写一次，读端据此在两次回调之间插值。
//!
//! 时钟带**代际**（epoch）。seek 会换代，读者只接受与当前位置同一代的估计值，
//! 否则回退到已提交帧数——这样"旧时钟 + 新位置"的撕裂组合永远不会被算进进度。
//!
//! 写入方（输出回调）用 `try_lock`：音频线程绝不阻塞在一个被普通线程持有的锁上
//! （优先级反转会导致 xrun）。抢不到就跳过本次发布，下一次回调（约 20ms 后）再发，
//! 因为每次发布都带绝对帧数，跳过不会累积误差。

use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlaybackClock {
    epoch: u64,
    audible_frame_at_update: u64,
    submitted_frame: u64,
    sample_rate: u32,
    updated_at: Option<Instant>,
}

impl PlaybackClock {
    pub(crate) fn new() -> Self {
        Self {
            epoch: 0,
            audible_frame_at_update: 0,
            submitted_frame: 0,
            sample_rate: 0,
            updated_at: None,
        }
    }

    pub(crate) fn reset_to(&mut self, epoch: u64, frame: u64, sample_rate: u32) {
        self.epoch = epoch;
        self.audible_frame_at_update = frame;
        self.submitted_frame = frame;
        self.sample_rate = sample_rate;
        self.updated_at = None;
    }

    pub(crate) fn publish(
        &mut self,
        epoch: u64,
        audible_frame_at_update: u64,
        submitted_frame: u64,
        sample_rate: u32,
        updated_at: Instant,
    ) {
        self.epoch = epoch;
        self.audible_frame_at_update = audible_frame_at_update;
        self.submitted_frame = submitted_frame;
        self.sample_rate = sample_rate;
        self.updated_at = Some(updated_at);
    }

    /// 恢复播放：从 `frame`（已知可听点）重新起算，仍以已提交帧数为上限。
    /// 若不带上真实的 `submitted`，恢复后头一个回调周期里位置会被钉住不动。
    pub(crate) fn resume_from(
        &mut self,
        epoch: u64,
        frame: u64,
        submitted_frame: u64,
        sample_rate: u32,
        now: Instant,
    ) {
        self.publish(epoch, frame, submitted_frame.max(frame), sample_rate, now);
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// 从 `updated_at` 起按墙钟插值，上限是调用方给的已提交帧数。
    pub(crate) fn estimate(&self, now: Instant) -> Option<u64> {
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

/// 时钟的唯一共享入口。写 = 输出回调（尽力，不阻塞）；读 = 任意线程（可阻塞）。
pub(crate) struct SharedClock {
    inner: Mutex<PlaybackClock>,
}

impl SharedClock {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(PlaybackClock::new()),
        }
    }

    /// 音频线程专用：锁被占就放弃本次发布，绝不等待。
    pub(crate) fn publish_without_blocking(&self, publish: impl FnOnce(&mut PlaybackClock)) {
        match self.inner.try_lock() {
            Ok(mut clock) => publish(&mut clock),
            // 抢不到（读端正在取快照）：本次跳过，下个回调会带绝对帧数重发。
            Err(TryLockError::WouldBlock) => {}
            // 锁中毒只意味着某个写者 panic 过；时钟是纯数据，继续用内层值发布，
            // 别让一次 panic 把音频线程的时钟永久冻住。
            Err(TryLockError::Poisoned(poison)) => publish(&mut poison.into_inner()),
        }
    }

    /// 控制面专用（seek / 换会话）。允许短暂阻塞：此刻音频回调只会跳过一次发布。
    pub(crate) fn reset(&self, epoch: u64, frame: u64, sample_rate: u32) {
        self.lock().reset_to(epoch, frame, sample_rate);
    }

    /// 控制面用的阻塞版发布（恢复播放时重置插值起点）。音频线程绝不能走这里。
    pub(crate) fn publish(&self, publish: impl FnOnce(&mut PlaybackClock)) {
        publish(&mut self.lock());
    }

    pub(crate) fn snapshot(&self) -> PlaybackClock {
        *self.lock()
    }

    fn lock(&self) -> MutexGuard<'_, PlaybackClock> {
        // 锁中毒只可能来自一次 panic；时钟是可重建的纯数据，用内层值继续，
        // 让播放链路的其它线程不必因为一次 panic 永久失效。
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// Duration → 采样帧（按给定采样率）。
pub(crate) fn duration_to_frames(duration: Duration, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }

    (duration.as_secs_f64() * sample_rate as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn estimate_advances_with_wall_clock_but_never_past_submitted() {
        let mut clock = PlaybackClock::new();
        let now = Instant::now();
        clock.publish(1, 10_000, 12_000, 48_000, now);

        // 墙钟走过 10ms = 480 帧。
        assert_eq!(
            clock.estimate(now + Duration::from_millis(10)),
            Some(10_480)
        );

        // 墙钟走过很久：估计值被已提交帧数钉住，不会凭空超前。
        assert_eq!(clock.estimate(now + Duration::from_secs(10)), Some(12_000));
        assert_eq!(clock.epoch(), 1);
    }

    #[test]
    fn estimate_is_unavailable_before_the_first_publish() {
        let clock = PlaybackClock::new();
        assert_eq!(clock.estimate(Instant::now()), None);
    }

    #[test]
    fn audio_side_publish_never_waits_for_a_reader() {
        let clock = Arc::new(SharedClock::new());
        clock.reset(1, 0, 48_000);

        // 模拟读端（进度轮询）正持有锁。
        let reader = {
            let clock = Arc::clone(&clock);
            std::thread::spawn(move || {
                let _guard = clock.lock();
                std::thread::sleep(Duration::from_millis(120));
            })
        };
        std::thread::sleep(Duration::from_millis(20));

        let started = Instant::now();
        clock.publish_without_blocking(|clock| clock.publish(1, 1, 1, 48_000, Instant::now()));
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "音频线程的时钟发布必须是无等待的（实际耗时 {:?}）",
            started.elapsed()
        );

        reader.join().unwrap();
    }

    #[test]
    fn poisoned_clock_still_serves_readers_and_accepts_publishes() {
        let clock = Arc::new(SharedClock::new());
        clock.reset(2, 480, 48_000);

        let poisoner = Arc::clone(&clock);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock();
            panic!("poison the clock on purpose");
        })
        .join();

        // 读端照常拿到上一次重置写入的代际。
        assert_eq!(clock.snapshot().epoch(), 2);

        // 音频线程的发布也不因为锁中毒而失效——否则时钟会永久冻住。
        let now = Instant::now();
        clock.publish_without_blocking(|clock| clock.publish(2, 1_000, 2_000, 48_000, now));
        assert_eq!(clock.snapshot().epoch(), 2);
        assert_eq!(clock.snapshot().estimate(now), Some(1_000));
    }
}
