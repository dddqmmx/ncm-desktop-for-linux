//! UI 轮询读到的播放状态。
//!
//! 一次原子读取出**完整**快照：进度、播放/暂停/停止、缓冲中、音量、BitPerfect
//! 锁定。分成五个独立原子量读时，轮询正好落在会话切换中间就会拼出
//! "已停止 + 上一首的进度 + 新的音量"这类组合，UI 会闪一下错误状态。
//!
//! 写者是单一的（worker 循环），读者是任意多个 napi getter。因此打包成 u64
//! 后不需要锁：单写者顺序覆盖，读者要么看到旧快照、要么看到新快照。

use std::sync::atomic::{AtomicU64, Ordering};

use super::types::PlaybackStatus;

/// 进度位宽：34 位 ≈ 199 天，足够任何曲目；超出按上限截断（不会回绕成 0）。
const PROGRESS_BITS: u32 = 34;
const STATUS_SHIFT: u32 = PROGRESS_BITS;
const BUFFERING_SHIFT: u32 = STATUS_SHIFT + 2;
const VOLUME_SHIFT: u32 = BUFFERING_SHIFT + 1;
/// 音量按千分比存 0..=1000（10 位）。
const VOLUME_BITS: u32 = 10;
const VOLUME_UNITY: u32 = 1_000;
const LOCKED_SHIFT: u32 = VOLUME_SHIFT + VOLUME_BITS;

const PROGRESS_MASK: u64 = (1u64 << PROGRESS_BITS) - 1;

/// 播放状态的完整快照。`Copy`，可随意传阅，不存在"读到一半被改写"。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaybackSnapshot {
    pub progress_ms: u32,
    pub status: PlaybackStatus,
    /// 输出侧正在等缓冲（UI 显示 loading）。
    pub buffering: bool,
    /// 当前会话实际生效的音量（0.0..=1.0）。
    pub volume: f64,
    /// 当前会话被严格 BitPerfect 锁定为 1.0 直通。
    pub volume_locked: bool,
}

impl PlaybackSnapshot {
    /// 静止状态：没有会话时的权威快照。
    pub(crate) const IDLE: Self = Self {
        progress_ms: 0,
        status: PlaybackStatus::Stopped,
        buffering: false,
        volume: 1.0,
        volume_locked: false,
    };

    fn volume_milli(self) -> u32 {
        if self.volume.is_finite() {
            (self.volume.clamp(0.0, 1.0) * f64::from(VOLUME_UNITY)).round() as u32
        } else {
            VOLUME_UNITY
        }
    }

    fn pack(self) -> u64 {
        let progress = u64::from(self.progress_ms).min(PROGRESS_MASK);
        progress
            | (u64::from(self.status.as_u32()) << STATUS_SHIFT)
            | (u64::from(self.buffering) << BUFFERING_SHIFT)
            | (u64::from(self.volume_milli()) << VOLUME_SHIFT)
            | (u64::from(self.volume_locked) << LOCKED_SHIFT)
    }

    fn unpack(raw: u64) -> Self {
        let volume_milli = (raw >> VOLUME_SHIFT) & ((1 << VOLUME_BITS) - 1);
        Self {
            progress_ms: (raw & PROGRESS_MASK) as u32,
            status: PlaybackStatus::from_u32(((raw >> STATUS_SHIFT) & 0b11) as u32),
            buffering: (raw >> BUFFERING_SHIFT) & 1 == 1,
            volume: volume_milli as f64 / f64::from(VOLUME_UNITY),
            volume_locked: (raw >> LOCKED_SHIFT) & 1 == 1,
        }
    }

    pub(crate) fn with_progress(mut self, progress_ms: u32) -> Self {
        self.progress_ms = progress_ms;
        self
    }

    pub(crate) fn with_status(mut self, status: PlaybackStatus) -> Self {
        self.status = status;
        self
    }

    pub(crate) fn with_buffering(mut self, buffering: bool) -> Self {
        self.buffering = buffering;
        self
    }

    pub(crate) fn with_volume(mut self, volume: f64, locked: bool) -> Self {
        self.volume = volume;
        self.volume_locked = locked;
        self
    }
}

/// 快照的共享单元。唯一写者是 worker，读者是 napi getter。
pub(crate) struct SnapshotCell {
    packed: AtomicU64,
}

impl SnapshotCell {
    pub(crate) fn new() -> Self {
        Self {
            packed: AtomicU64::new(PlaybackSnapshot::IDLE.pack()),
        }
    }

    pub(crate) fn read(&self) -> PlaybackSnapshot {
        PlaybackSnapshot::unpack(self.packed.load(Ordering::Acquire))
    }

    /// 整体替换。单写者，不存在丢失更新；Release 保证读者看到快照的同时
    /// 不会读到比它更旧的其它原子量。
    pub(crate) fn publish(&self, snapshot: PlaybackSnapshot) {
        self.packed.store(snapshot.pack(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn every_field_survives_a_round_trip() {
        let snapshot = PlaybackSnapshot {
            progress_ms: 3_600_000,
            status: PlaybackStatus::Paused,
            buffering: true,
            volume: 0.437,
            volume_locked: true,
        };
        assert_eq!(PlaybackSnapshot::unpack(snapshot.pack()), snapshot);
    }

    #[test]
    fn extreme_values_are_clamped_instead_of_corrupting_neighbours() {
        let snapshot = PlaybackSnapshot {
            progress_ms: u32::MAX,
            status: PlaybackStatus::Playing,
            buffering: true,
            volume: 42.0,
            volume_locked: true,
        };
        let decoded = PlaybackSnapshot::unpack(snapshot.pack());
        assert_eq!(decoded.status, PlaybackStatus::Playing);
        assert!(decoded.buffering);
        assert_eq!(decoded.volume, 1.0);
        assert!(decoded.volume_locked);
        assert_eq!(decoded.progress_ms, PROGRESS_MASK as u32);
    }

    #[test]
    fn non_finite_volume_is_normalized_to_unity() {
        let snapshot = PlaybackSnapshot::IDLE.with_volume(f64::NAN, false);
        assert_eq!(PlaybackSnapshot::unpack(snapshot.pack()).volume, 1.0);
    }

    #[test]
    fn idle_snapshot_is_the_documented_resting_state() {
        let cell = SnapshotCell::new();
        assert_eq!(cell.read(), PlaybackSnapshot::IDLE);
    }

    /// 读者永远拿不到"半个快照"：一次 load 就是一次完整状态的映射。
    #[test]
    fn concurrent_readers_never_observe_a_torn_snapshot() {
        let cell = Arc::new(SnapshotCell::new());
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let reader = {
            let cell = Arc::clone(&cell);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let snapshot = cell.read();
                    // 所有非静止快照都是 Playing + 进度 == 音量千分比 × 1000
                    // 的构造，任何撕裂组合都会破坏这条不变量。
                    if snapshot.status == PlaybackStatus::Stopped {
                        continue;
                    }
                    assert_eq!(snapshot.status, PlaybackStatus::Playing);
                    assert_eq!(
                        snapshot.progress_ms,
                        (snapshot.volume * 1000.0).round() as u32 * 1000
                    );
                }
            })
        };

        for step in 1..=2_000u32 {
            let volume = f64::from(step % 1_000 + 1) / 1000.0;
            cell.publish(PlaybackSnapshot {
                progress_ms: (step % 1_000 + 1) * 1000,
                status: PlaybackStatus::Playing,
                buffering: false,
                volume,
                volume_locked: false,
            });
        }
        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
    }
}
