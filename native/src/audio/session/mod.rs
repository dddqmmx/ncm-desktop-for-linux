//! 一次播放会话的共享状态。
//!
//! 输出回调、解码线程、控制面（worker / napi）三个线程只通过这里通信。模块的存在
//! 理由就是**让跨线程协议无法被部分更新**：
//!
//! 1. 任何"哪一代 + 到哪了"的读取都是一次原子读。位置与代际打包在
//!    [`EpochWord`] 里，读端不会拼出"新代际 + 旧位置"。
//! 2. seek 是一次**换时间线**：`schedule_seek` 立刻换代并把位置锚到请求点，
//!    旧代际的样本既进不了进度、也不会在换线后再被计入。旧实现里
//!    `has_seek_request` / `waiting_for_seek` / `discard_buffer` /
//!    `is_discarding_buffer` 四个标志由三个线程各自读改写，任何一处漏改都表现为
//!    "进度与听感脱节"，且没有编译期保护。
//! 3. 终局（自然播完 / 失败）只由产出侧和消费侧在**同一份**状态上判定：
//!    解码器 `drained` + 输出回调确认缓冲已空 → `Delivered`；流错误 → `Failed`。
//!    输出回调拿不到 Session 引用时无法把自己臆断成"播完了"。
//!
//! 线程约定：
//! - 输出回调路径（`OutputCursor`, `allow_output`, `publish_*`）**绝不阻塞**；
//!   唯一要等的资源是时钟锁，且用 `try_lock`。
//! - 解码线程可短暂阻塞（等输出侧为某一代际清空缓冲，有超时兜底）。
//! - 控制面可阻塞。

mod clock;
mod word;

pub(crate) use clock::SharedClock;
pub(crate) use clock::duration_to_frames;
pub(crate) use word::EpochWord;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::audio::output::OutputLatency;

/// `trim_until_frame` 的"无裁剪"哨兵。帧号不可能到这个量级（48kHz 下约 1200 万年）。
pub(crate) const NO_TRIM_FRAME: u64 = u64::MAX;

/// 解码器等待输出侧清空缓冲的上限。输出回调通常 20ms 一次；超时说明流已经不在
/// 跑（被 stop / 设备没了），继续等只会把解码线程永久钉在这里。
const DISCARD_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// 所有音量写入的统一净化：`clamp` 会原样放行 NaN，非有限值（NaN/±inf）一律按
/// 1.0 处理——驻留在音量位上会让软件音量静默失灵，直到下一次有效设置。
pub(crate) fn sanitize_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SeekRequest {
    pub(crate) target: Duration,
    /// 请求发起那一刻的时间线代际。解码完成时要确认没有被更新的 seek 顶掉。
    pub(crate) epoch: u64,
    /// 请求位置对应的采样帧，作为新一代时间线的锚点。
    pub(crate) requested_frame: u64,
}

/// 终局编码：一次原子读拿到"结束了吗 + 为什么 + 是不是设备丢失"。
/// 分开存的两个 bool 会让读者拼出"失败但未标记设备丢失"的组合。
struct OutcomeWord;

impl OutcomeWord {
    const RUNNING: u64 = 0;
    const DELIVERED: u64 = 1;
    const FAILED: u64 = 2;
    const FAILED_DEVICE_LOST: u64 = 3;

    fn decode(raw: u64) -> Option<SessionOutcome> {
        match raw {
            Self::RUNNING => None,
            Self::DELIVERED => Some(SessionOutcome::Delivered),
            Self::FAILED_DEVICE_LOST => Some(SessionOutcome::Failed { device_lost: true }),
            // 未识别的值（理论上不存在）按普通失败处理：宁可终止也不假装还在播。
            _ => Some(SessionOutcome::Failed { device_lost: false }),
        }
    }
}

/// 会话的终局。`None`（尚未结束）与"结束原因"分开表达，调用方不会把两者混淆。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionOutcome {
    /// 解码产物已经全部交付给输出设备。
    Delivered,
    /// 底层流致命错误终止了会话。
    Failed {
        /// 错误属于设备类（拔出 / 被抢占）：调用方可以尝试迁移到别的端点续播。
        device_lost: bool,
    },
}

/// 输出回调被允许做什么。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputBlock {
    /// 静音：seek 整备中 / 缓冲不足 / 会话已终止。
    Silence,
    /// 从 ring buffer 正常取样本。
    Deliver,
}

pub(crate) struct Session {
    sample_rate: u32,
    channels: u16,

    /// 当前时间线代际。seek 换代；同一代际内的位置是单调的。
    epoch: AtomicU64,
    /// 打包的 `(代际, 已提交帧数)`。三个写者（输出回调 publish、schedule_seek 锚定、
    /// 失败终止）全部经 CAS 校验代际，过期写入自动作废。
    progress: AtomicU64,
    /// 等输出侧确认清空的上限。默认 2 秒；测试注入更短的值。
    discard_timeout: Duration,
    /// 清空请求的序号（单调递增）。解码线程每请求一次 +1。
    discard_generation: AtomicU64,
    /// 输出回调已确认清空到哪个序号。解码器据此决定能否写入新一代样本。
    discard_completed: AtomicU64,
    /// 某一代 seek 尚未整备完毕：输出保持静音，解码不再推送旧代样本。
    seek_pending: AtomicBool,
    /// 待处理的 seek。解码线程只在拿到锁时取走，抢不到就下一轮再来。
    seek_slot: Mutex<Option<SeekRequest>>,
    /// 解码起点裁剪：新一代时间线里，早于该帧的样本丢弃（symphonia Accurate seek
    /// 落在请求点之前）。
    ///
    /// 与代际**打包在一起**。若拆成两个字段，一次迟到的 seek 完成（旧代际）
    /// 可能恰好落在新 seek 换代之后、写入裁剪点之前，把旧裁剪点盖到新时间线上——
    /// 这会让新时间线的头几秒被静默丢掉。打包后旧代际的写入直接作废。
    trim: AtomicU64,

    /// 输出侧是否在等缓冲。UI 可见；也是 `allow_output` 状态机的唯一状态位。
    buffering: AtomicBool,
    paused: AtomicBool,
    /// 暂停那一刻的可听位置，与代际一起打包。
    ///
    /// 暂停期间墙钟照样在走，若继续用时钟插值，位置会缓慢爬向"已提交帧数"
    /// （暂停前刚交付、还没发声的那一块）。暂停要的是**冻住**，而暂停点在
    /// 产生新的 seek（换代）之后立即失效——代际不匹配时自动回落到时钟。
    paused_position: AtomicU64,

    /// 解码器已到 EOF / 不会再产出样本。
    drained: AtomicBool,
    /// 会话终局，打包成一个原子量（见 [`OutcomeWord`]）。分开存会有"读到
    /// played_failed 但还没读到 device_lost"的组合，迁移决策正好依赖这两者的
    /// 一致性。
    outcome: AtomicU64,
    /// 主动终止（stop / 换会话）。此后到达的流错误一律忽略。
    terminated: AtomicBool,

    /// 软件音量（f32 位模式存储），1.0 = 原样直通。
    volume: AtomicU32,
    /// BitPerfect 会话锁定音量为 1.0，`set_volume` 直接忽略。
    volume_locked: AtomicBool,

    clock: SharedClock,
}

impl Session {
    /// `start_at` 是会话的**起点**而不是一次 seek：此刻还没有任何样本存在，
    /// 不需要换代、不需要清缓冲，也就不会出现"开播 seek 与首个输出回调抢跑"
    /// 的窗口。运行中的跳转走 [`schedule_seek`](Self::schedule_seek)。
    pub(crate) fn new(
        sample_rate: u32,
        channels: u16,
        volume: f32,
        volume_locked: bool,
        start_at: Duration,
    ) -> Self {
        let start_frame = duration_to_frames(start_at, sample_rate);
        let started_mid_track = start_frame > 0;
        let session = Self {
            sample_rate,
            channels,
            epoch: AtomicU64::new(0),
            progress: AtomicU64::new(EpochWord::new(0, start_frame).pack()),
            discard_timeout: DISCARD_ACK_TIMEOUT,
            discard_generation: AtomicU64::new(0),
            discard_completed: AtomicU64::new(0),
            // 从中间开播：解码器还停在文件开头，必须跳一次。把它作为会话的
            // 第一个 seek 请求留下——位置锚定与"解码器跳到目标"是同一件事，
            // 分给两处各做一次就会出现"进度 3:00、声音从头开始"。
            seek_pending: AtomicBool::new(started_mid_track),
            seek_slot: Mutex::new(started_mid_track.then_some(SeekRequest {
                target: start_at,
                epoch: 0,
                requested_frame: start_frame,
            })),
            trim: AtomicU64::new(0),
            buffering: AtomicBool::new(started_mid_track),
            paused: AtomicBool::new(false),
            paused_position: AtomicU64::new(0),
            drained: AtomicBool::new(false),
            outcome: AtomicU64::new(OutcomeWord::RUNNING),
            terminated: AtomicBool::new(false),
            volume: AtomicU32::new(sanitize_volume(volume).to_bits()),
            volume_locked: AtomicBool::new(volume_locked),
            clock: SharedClock::new(),
        };
        session.clock.reset(0, start_frame, sample_rate);
        session
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 测试用：缩短等待输出侧确认清空的上限。
    #[cfg(test)]
    pub(crate) fn shorten_discard_timeout(mut self, timeout: Duration) -> Self {
        self.discard_timeout = timeout;
        self
    }

    // ---------------------------------------------------------------- 位置

    fn progress_word(&self) -> EpochWord {
        EpochWord::unpack(self.progress.load(Ordering::Acquire))
    }

    /// 可听位置（采样帧）。时钟估计与已提交帧数必须属于同一代际——否则回退到
    /// 已提交帧数。这条检查就是"进度不会跑到另一条时间线上"的保证。
    pub(crate) fn position_frames(&self) -> u64 {
        self.position_frames_at(Instant::now())
    }

    /// 同上，但在调用方给定的时刻求值。测试用它把时间推进到"这块样本已经播完"，
    /// 不必真的 sleep。
    pub(crate) fn position_frames_at(&self, now: Instant) -> u64 {
        let word = self.progress_word();
        if self.paused.load(Ordering::Acquire) {
            let paused = EpochWord::unpack(self.paused_position.load(Ordering::Acquire));
            if paused.epoch == word.epoch {
                return paused.value.min(word.value);
            }
        }

        let clock = self.clock.snapshot();
        if clock.epoch() != word.epoch {
            return word.value;
        }

        clock.estimate(now).unwrap_or(word.value).min(word.value)
    }

    pub(crate) fn position(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }

        Duration::from_secs_f64(self.position_frames() as f64 / f64::from(self.sample_rate))
    }

    /// 输出回调每交付一块样本后调用：推进提交位置并发布时钟。被 seek 换代后会
    /// 自动作废（CAS 校验代际），所以这里不需要调用方再判断一次。
    pub(crate) fn publish_progress(&self, epoch: u64, frames: u64) {
        let mut current = self.progress.load(Ordering::Acquire);
        loop {
            let word = EpochWord::unpack(current);
            if word.epoch != epoch || word.value >= frames {
                return;
            }

            let next = EpochWord::new(epoch, frames).pack();
            match self.progress.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// 发布可听位置估计。`block_start_frame` 是本块样本起始帧，`submitted_frame`
    /// 是本块结束帧；输出延迟从起始帧里扣，估计值再用已提交帧数封顶。
    pub(crate) fn publish_clock(
        &self,
        epoch: u64,
        block_start_frame: u64,
        submitted_frame: u64,
        latency: OutputLatency,
    ) {
        let latency_frames = duration_to_frames(latency.total(), self.sample_rate);
        self.clock.publish_without_blocking(|clock| {
            clock.publish(
                epoch,
                block_start_frame.saturating_sub(latency_frames),
                submitted_frame,
                self.sample_rate,
                Instant::now(),
            );
        });
    }

    // ---------------------------------------------------------------- seek

    /// 换代并锚定到请求位置。立即生效：UI 看到位置跳到目标点、输出静音等新一代
    /// 样本，旧代样本的进度写入随之作废。
    pub(crate) fn schedule_seek(&self, target: Duration) -> u64 {
        let epoch = self.next_epoch();
        let requested_frame = duration_to_frames(target, self.sample_rate);

        // 顺序有讲究：先挂起（输出停止交付），再发布新代锚点。
        self.seek_pending.store(true, Ordering::Release);
        self.set_trim(epoch, None);
        self.progress.store(
            EpochWord::new(epoch, requested_frame).pack(),
            Ordering::Release,
        );
        self.clock.reset(epoch, requested_frame, self.sample_rate);
        self.buffering.store(true, Ordering::Release);

        *self
            .seek_slot
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(SeekRequest {
            target,
            epoch,
            requested_frame,
        });
        epoch
    }

    fn next_epoch(&self) -> u64 {
        let mut current = self.epoch.load(Ordering::Acquire);
        loop {
            let next = (current & word::EPOCH_MASK).wrapping_add(1) & word::EPOCH_MASK;
            match self.epoch.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return next,
                Err(actual) => current = actual,
            }
        }
    }

    /// 解码线程取走待处理 seek。返回 `Err(())` 表示锁竞争，下一轮再试。
    pub(crate) fn try_take_seek_request(&self) -> Result<Option<SeekRequest>, ()> {
        match self.seek_slot.try_lock() {
            Ok(mut slot) => Ok(slot.take()),
            Err(_) => Err(()),
        }
    }

    /// 请求输出侧清空 ring buffer，返回这次请求的序号。
    ///
    /// **只有解码线程会调用**，而且总是在它自己停止写入之后、准备写入新一代
    /// 样本之前。回调侧看到未完成的请求就清空并回执，于是"清空"必然落在
    /// "最后一次旧样本写入"与"第一次新样本写入"之间——两端都在同一条线程上
    /// 排好序，不存在把正确目标样本一起排空的窗口。
    pub(crate) fn request_discard(&self) -> u64 {
        self.discard_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// 输出回调完成一次清空后的回执。
    pub(crate) fn acknowledge_discard(&self, generation: u64) {
        self.discard_completed.store(generation, Ordering::Release);
    }

    fn pending_discard(&self) -> Option<u64> {
        let generation = self.discard_generation.load(Ordering::Acquire);
        (self.discard_completed.load(Ordering::Acquire) < generation).then_some(generation)
    }

    /// 解码完成一次 seek：等输出侧清空旧样本 → 落裁剪点 → 放开输出。
    ///
    /// 若期间又来了更新的 seek（代际已变），本次完成作废：新一代的锚点已经生效，
    /// 旧一代的裁剪点不能覆盖它。
    pub(crate) fn finish_seek(&self, epoch: u64, trim_until: Option<u64>, discard: u64) {
        // 已经被更新的 seek 顶掉：直接作废，连等都不必等——新一代有自己的清空
        // 请求与裁剪点，旧一代的完成只可能是噪声。
        if self.progress_word().epoch != epoch || self.terminated.load(Ordering::Acquire) {
            return;
        }

        if !self.wait_for_discard(discard) {
            // 输出侧在超时内没有清空缓冲：流已经不在正常消费了（设备消失、
            // 驱动停摆）。此时放开输出会让"旧样本继续播、进度却锚在新位置"，
            // 正是要消灭的那种漂移；按设备丢失终止，由上层决定迁移或收尾。
            self.fail(true, "[session] 输出侧不再消费缓冲，seek 无法安全完成");
            return;
        }
        if self.terminated.load(Ordering::Acquire) {
            return;
        }
        if self.progress_word().epoch != epoch {
            return;
        }

        self.set_trim(epoch, trim_until);
        // 最后一步才放开输出：此前任何回调都只能在 Silence 分支打转。
        self.seek_pending.store(false, Ordering::Release);
    }

    /// 等输出侧确认清空到 `generation`。返回 false 表示超时或会话已结束。
    fn wait_for_discard(&self, generation: u64) -> bool {
        let deadline = Instant::now() + self.discard_timeout;
        loop {
            if self.discard_completed.load(Ordering::Acquire) >= generation {
                return true;
            }
            // 会话已经结束：输出回调可能再也不会来，继续等只会白等两秒。
            if self.terminated.load(Ordering::Acquire) || self.outcome().is_some() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn seek_pending(&self) -> bool {
        self.seek_pending.load(Ordering::Acquire)
    }

    /// 当前时间线的裁剪点（采样帧）。其他代际留下的值一律不可见。
    pub(crate) fn trim_until(&self) -> Option<u64> {
        let word = EpochWord::unpack(self.trim.load(Ordering::Acquire));
        if word.epoch != self.progress_word().epoch || word.value == 0 {
            return None;
        }
        Some(word.value - 1)
    }

    fn set_trim(&self, epoch: u64, frame: Option<u64>) {
        // 0 表示"无裁剪"；帧号本身偏移 1 存储，于是 0 号帧也能表达。
        let value = frame.map(|frame| frame.saturating_add(1)).unwrap_or(0);
        self.trim
            .store(EpochWord::new(epoch, value).pack(), Ordering::Release);
    }

    /// 落到目标的第一个 packet 之后清掉裁剪点（同一代际内）。
    pub(crate) fn clear_trim(&self) {
        let epoch = self.progress_word().epoch;
        self.set_trim(epoch, None);
    }

    // ---------------------------------------------------------------- 输出侧

    pub(crate) fn set_buffering(&self, buffering: bool) {
        self.buffering.store(buffering, Ordering::Release);
    }

    pub(crate) fn buffering(&self) -> bool {
        self.buffering.load(Ordering::Acquire)
    }

    /// 输出回调能否交付样本。跨越恢复阈值才放开，缓冲耗尽（且解码未完成）时
    /// 重新收紧——这条状态机与 `buffering` 是同一个标志，不允许两处各判一次。
    pub(crate) fn allow_output(&self, occupied_samples: usize) -> bool {
        if self.seek_pending.load(Ordering::Acquire) {
            self.set_buffering(true);
            return false;
        }

        let resync_threshold = self.sample_rate as usize * self.channels as usize;
        let drained = self.drained.load(Ordering::Acquire);

        if self.buffering.load(Ordering::Acquire) {
            if occupied_samples >= resync_threshold || drained {
                self.set_buffering(false);
                return true;
            }
            return false;
        }

        if occupied_samples == 0 && !drained {
            self.set_buffering(true);
            return false;
        }

        true
    }

    /// 回调取不到样本（缓冲已空）：解码完成了就是"交付完毕"，否则重新进入缓冲。
    pub(crate) fn on_output_starved(&self) {
        if self.drained.load(Ordering::Acquire) {
            // 只有第一个终局写入生效：已经因错误终止的会话不会在这里被改写成
            // "自然播完"。
            let _ = self.outcome.compare_exchange(
                OutcomeWord::RUNNING,
                OutcomeWord::DELIVERED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            self.set_buffering(false);
        } else {
            self.set_buffering(true);
        }
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// 暂停 / 恢复。
    ///
    /// 暂停时把当前可听点连代际一起记下来，位置从此冻住；恢复时从该点重新
    /// 起算，插值立刻继续（而不是钉在暂停点等下一个回调）。
    pub(crate) fn set_paused(&self, paused: bool) {
        if paused {
            let word = self.progress_word();
            let frame = self.position_frames();
            self.paused_position
                .store(EpochWord::new(word.epoch, frame).pack(), Ordering::Release);
            self.paused.store(true, Ordering::Release);
            return;
        }

        if !self.paused.swap(false, Ordering::AcqRel) {
            return;
        }
        let word = self.progress_word();
        let frozen = EpochWord::unpack(self.paused_position.load(Ordering::Acquire));
        let restart_frame = if frozen.epoch == word.epoch {
            frozen.value
        } else {
            word.value
        };
        let sample_rate = self.sample_rate;
        // 控制面线程可以短暂阻塞：这里必须真的重建插值起点，否则恢复后的第一个
        // 回调周期里位置会从"暂停前最后一次发布的时刻"重新插值，反而跳得更远。
        self.clock.publish(|clock| {
            clock.resume_from(
                word.epoch,
                restart_frame,
                word.value,
                sample_rate,
                Instant::now(),
            );
        });
    }

    // ---------------------------------------------------------------- 终局

    /// 解码器不再产出样本（自然 EOF，或致命解码错误）。
    pub(crate) fn mark_drained(&self) {
        self.drained.store(true, Ordering::Release);
    }

    /// 底层流致命错误。`device_lost` 区分设备类错误（可迁移续播）与普通流错误。
    /// 主动终止之后的错误一律忽略：那是 stop 自己拆流引起的。
    ///
    /// 首个终局写入决定迁移策略：后来的错误只当噪声，不得改写它。
    pub(crate) fn fail(&self, device_lost: bool, message: &str) {
        if self.terminated.load(Ordering::Acquire) {
            return;
        }

        let failed = if device_lost {
            OutcomeWord::FAILED_DEVICE_LOST
        } else {
            OutcomeWord::FAILED
        };
        if self
            .outcome
            .compare_exchange(
                OutcomeWord::RUNNING,
                failed,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            // 已有终局（自然播完或首个错误）：保持原分类。
            return;
        }

        eprintln!("{message}");
        // 保留当前位置：设备丢失后调用方要按它迁移续播。
        self.seek_pending.store(false, Ordering::Release);
        self.drained.store(true, Ordering::Release);
        self.set_buffering(false);
    }

    /// 主动终止会话（stop / 换播放器实例 / 重放）。幂等。
    pub(crate) fn terminate(&self) {
        self.terminated.store(true, Ordering::Release);
        self.seek_pending.store(false, Ordering::Release);
        self.set_buffering(false);
    }

    pub(crate) fn is_terminated(&self) -> bool {
        self.terminated.load(Ordering::Acquire)
    }

    /// 会话是否已经结束（含自然播完与失败）。返回 `None` 表示仍在进行。
    /// 终局与"是否设备丢失"来自同一次原子读，不存在撕裂组合。
    pub(crate) fn outcome(&self) -> Option<SessionOutcome> {
        OutcomeWord::decode(self.outcome.load(Ordering::Acquire))
    }

    // ---------------------------------------------------------------- 音量

    pub(crate) fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub(crate) fn volume_locked(&self) -> bool {
        self.volume_locked.load(Ordering::Relaxed)
    }

    pub(crate) fn set_volume(&self, volume: f32) {
        if self.volume_locked() {
            return;
        }
        self.volume
            .store(sanitize_volume(volume).to_bits(), Ordering::Relaxed);
    }
}

/// 输出回调在一次交付中的视角：它自己所属的代际与游标。
///
/// 游标是**回调私有**的：旧代样本的推进只发生在旧游标上，换代时整体丢弃，
/// 绝不会被加进新代时间线。每次回调都只通过 `Session` 的 CAS 发布。
pub(crate) struct OutputCursor {
    epoch: u64,
    cursor: u64,
}

impl OutputCursor {
    pub(crate) fn attach(session: &Session) -> Self {
        let word = session.progress_word();
        Self {
            epoch: word.epoch,
            cursor: word.value,
        }
    }

    /// 回调开头的完整判定。`clear_buffer` 只在需要排空旧代样本时调用
    /// （消费端类型只有调用方知道）。
    pub(crate) fn begin(
        &mut self,
        session: &Session,
        occupied_samples: usize,
        clear_buffer: impl FnOnce(),
    ) -> OutputBlock {
        let word = session.progress_word();
        let flush = if word.epoch != self.epoch {
            // 换代：旧游标作废，重新从新时间线的锚点起算。
            self.epoch = word.epoch;
            self.cursor = word.value;
            true
        } else {
            session.pending_discard().is_some()
        };

        if flush {
            // 先清空再回执：解码线程收到回执后才写新一代样本，因此这里排掉的
            // 一定是旧代样本（含 schedule_seek 之后、解码线程接手之前漏写的
            // 那一小段）。
            clear_buffer();
            if let Some(generation) = session.pending_discard() {
                session.acknowledge_discard(generation);
            }
            session.set_buffering(true);
            return OutputBlock::Silence;
        }

        // 会话已终结（自然播完或失败）后一律静音：此时缓冲里的残留样本没有
        // 意义，再交付只会让位置越过终点。
        if session.outcome().is_some() || session.is_terminated() || session.seek_pending() {
            session.set_buffering(true);
            return OutputBlock::Silence;
        }

        if session.is_paused() {
            return OutputBlock::Silence;
        }

        if !session.allow_output(occupied_samples) {
            return OutputBlock::Silence;
        }

        OutputBlock::Deliver
    }

    /// 交付 `frames` 帧之后推进位置并发布时钟。
    pub(crate) fn delivered(&mut self, session: &Session, frames: u64, latency: OutputLatency) {
        let block_start = self.cursor;
        self.cursor = self.cursor.saturating_add(frames);
        session.publish_progress(self.epoch, self.cursor);
        session.publish_clock(self.epoch, block_start, self.cursor, latency);
    }

    pub(crate) fn starved(&self, session: &Session) {
        session.on_output_starved();
    }
}

#[cfg(test)]
mod tests;
