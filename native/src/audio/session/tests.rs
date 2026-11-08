//! 会话共享状态的协议测试。
//!
//! 这些测试刻意把三个真实调用方（输出回调 / 解码线程 / 控制面）的时序交错起来，
//! 断言的是**可观察结果**：位置、输出是否放行、会话终局。实现内部用几个原子量
//! 无关紧要——换实现不该逼着改这些断言。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::*;

const SR: u32 = 48_000;
const CH: u16 = 2;
const RESYNC_SAMPLES: usize = SR as usize * CH as usize;

fn session() -> Arc<Session> {
    Arc::new(Session::new(SR, CH, 1.0, false, Duration::ZERO))
}

fn session_starting_at(start_at: Duration) -> Arc<Session> {
    Arc::new(Session::new(SR, CH, 1.0, false, start_at))
}

/// 一个"播放中"的会话：缓冲已过恢复阈值、输出已放行。
fn playing_session() -> Arc<Session> {
    let session = session();
    assert!(session.allow_output(RESYNC_SAMPLES));
    session
}

fn latency(ms: u64) -> OutputLatency {
    OutputLatency::reported(Duration::from_millis(ms))
}

/// 按输出回调的真实节奏交付 `frames` 帧（每块 10ms），返回"最后一块刚好播完"
/// 那一刻的位置。
///
/// 位置表示的是**可听**点：刚交给设备的一块还在缓冲里，墙钟走过那一块之后才
/// 算播完。真实回调每 20ms 来一次，一次交一整块；这里用同一个模型，测试用
/// `position_frames_at` 直接求那个时刻，不必 sleep。
/// 模拟解码线程对一次 seek 的完整处理：请求清空 → 等输出侧回执 → 完成。
/// 测试里直接确认回执（等价于"输出回调已经跑过并且清空了"）。
fn finish_seek(session: &Session, epoch: u64, trim_until: Option<u64>) {
    let discard = session.request_discard();
    session.acknowledge_discard(discard);
    session.finish_seek(epoch, trim_until, discard);
}

fn deliver(session: &Session, cursor: &mut OutputCursor, frames: u64, latency_ms: u64) -> u64 {
    const CHUNK_FRAMES: u64 = SR as u64 / 100;

    let mut remaining = frames;
    while remaining > 0 {
        let chunk = remaining.min(CHUNK_FRAMES);
        cursor.delivered(session, chunk, latency(latency_ms));
        remaining -= chunk;
    }

    let tail = frames.min(CHUNK_FRAMES);
    let played = Duration::from_secs_f64(tail as f64 / f64::from(SR))
        .saturating_add(Duration::from_millis(1));
    session.position_frames_at(Instant::now() + played)
}

// ---------------------------------------------------------------- 位置

/// 位置是可听点：刚交给设备的一块还在缓冲里，播完才计入进度。
#[test]
fn position_tracks_the_audible_point_not_the_just_submitted_block() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);

    let audible_at_block_end = deliver(&session, &mut cursor, 4_800, 0);

    assert_eq!(audible_at_block_end, 4_800);
    assert!(
        session.position_frames() < 4_800,
        "刚提交的块尚未发声，进度不应立刻跳到位"
    );
}

#[test]
fn clock_interpolates_between_callbacks_but_never_past_the_submitted_frame() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);

    // 一块恰好够播 10ms 的样本。
    let frames = u64::from(SR) / 100;
    cursor.delivered(&session, frames, latency(0));
    let published_at = Instant::now();

    // 播到一半：位置落在"块起点（0）"与"块终点（480）"之间。
    let midway = session.position_frames_at(published_at + Duration::from_millis(5));
    assert!(
        midway > 0 && midway < frames,
        "墙钟走了一半，估计值应在块内推进，实际 {midway}"
    );

    // 下一次回调迟迟不来：估计值被提交帧数钉住，不会凭空超前。
    assert_eq!(
        session.position_frames_at(published_at + Duration::from_secs(10)),
        frames
    );
}

#[test]
fn output_latency_pulls_the_reported_position_back_without_going_negative() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);

    cursor.delivered(&session, 4_800, latency(1_000));

    assert_eq!(
        session.position_frames(),
        0,
        "输出延迟大于已交付量时位置停在起点，不得下溢成天文数字"
    );
}

/// 从第 N 秒开播：会话一建立就报 N 秒，同时给解码器留一个 seek 请求。
///
/// 位置锚定与"解码器跳到目标"必须是同一件事。若只锚进度不排 seek，解码器仍从
/// 文件开头解码——用户看到 0:42，听到的却是 0:00。
#[test]
fn a_session_starting_mid_track_anchors_the_position_and_asks_the_decoder_to_seek() {
    let session = session_starting_at(Duration::from_secs(42));

    assert_eq!(session.position_frames(), 42 * u64::from(SR));
    assert_eq!(session.position(), Duration::from_secs(42));
    assert!(session.seek_pending(), "在解码器跳到目标之前不放行输出");
    assert!(session.buffering());

    let request = session
        .try_take_seek_request()
        .unwrap()
        .expect("解码器必须收到开播落点");
    assert_eq!(request.target, Duration::from_secs(42));
    assert_eq!(request.requested_frame, 42 * u64::from(SR));

    finish_seek(&session, request.epoch, None);
    assert!(!session.seek_pending());
    assert_eq!(session.position_frames(), 42 * u64::from(SR));
}

#[test]
fn a_session_starting_at_zero_has_nothing_to_seek() {
    let session = session();

    assert!(!session.seek_pending());
    assert_eq!(session.try_take_seek_request().unwrap(), None);
}

// ---------------------------------------------------------------- seek

#[test]
fn seek_moves_the_position_to_the_request_immediately_and_mutes_output() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);
    deliver(&session, &mut cursor, 4_800, 0);

    session.schedule_seek(Duration::from_secs(30));

    assert_eq!(session.position_frames(), 30 * SR as u64);
    assert!(session.seek_pending());
    assert!(session.buffering(), "seek 整备期间 UI 应显示缓冲中");

    assert_eq!(
        cursor.begin(&session, RESYNC_SAMPLES, || {}),
        OutputBlock::Silence
    );
    assert!(session.seek_pending(), "输出侧不得自行放行");
}

/// 核心回归：换代之后，旧代际的回调仍在写进度。
///
/// `(代际, 帧数)` 打包在一次原子写里就是为了挡住这种写入——它带的是旧代际，
/// 必须整体作废，而不是把旧帧号合进新时间线。
#[test]
fn writes_from_the_previous_timeline_cannot_advance_the_new_one() {
    let session = playing_session();
    let mut stale_cursor = OutputCursor::attach(&session);
    deliver(&session, &mut stale_cursor, 4_800, 0);
    let previous_epoch = session.progress_word().epoch;

    session.schedule_seek(Duration::from_secs(30));

    // 旧回调换代后才把本块进度写回，并调用公开的发布接口。
    session.publish_progress(previous_epoch, 999_999);
    stale_cursor.delivered(&session, 96_000, latency(0));

    assert_eq!(
        session.position_frames(),
        30 * SR as u64,
        "旧代际的交付量不能混进 seek 之后的时间线"
    );
}

#[test]
fn finishing_a_superseded_seek_cannot_apply_its_trim_or_release_output() {
    let session = playing_session();
    let first = session.schedule_seek(Duration::from_secs(10));
    let second = session.schedule_seek(Duration::from_secs(20));
    assert_ne!(first, second);

    // 第一次 seek 的响应迟到：它属于旧代际，必须作废。
    finish_seek(&session, first, Some(10 * SR as u64));

    assert!(session.seek_pending(), "新一代仍在整备，输出不许放行");
    assert_eq!(session.trim_until(), None);
    assert_eq!(session.position_frames(), 20 * SR as u64);

    finish_seek(&session, second, Some(20 * SR as u64));
    assert!(!session.seek_pending());
    assert_eq!(session.trim_until(), Some(20 * SR as u64));
}

#[test]
fn seek_waits_for_the_output_side_to_flush_before_releasing_output() {
    let session = playing_session();
    let epoch = session.schedule_seek(Duration::from_secs(5));

    let discard = session.request_discard();
    let finisher = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || session.finish_seek(epoch, None, discard))
    };

    std::thread::sleep(Duration::from_millis(30));
    assert!(
        session.seek_pending(),
        "输出侧还没确认清空，解码完成也不许放行"
    );

    session.acknowledge_discard(discard);
    finisher.join().unwrap();
    assert!(!session.seek_pending());
}

#[test]
fn seek_completes_even_if_the_session_terminates_mid_flush() {
    let session = playing_session();
    let epoch = session.schedule_seek(Duration::from_secs(5));

    let started = Instant::now();
    session.terminate();
    let discard = session.request_discard();
    session.finish_seek(epoch, None, discard);

    assert!(
        started.elapsed() < DISCARD_ACK_TIMEOUT,
        "会话终止后必须立刻放弃等待输出侧确认"
    );
    assert!(session.is_terminated());
    assert_eq!(
        session.outcome(),
        None,
        "主动终止的会话不会因为 seek 未确认而被改写成失败"
    );
}

/// 输出侧在超时内没有确认清空（设备消失 / 驱动停摆）：继续放行就等于让旧
/// 样本接着播、进度却锚在新位置。会话必须按设备丢失终止。
#[test]
fn a_seek_whose_flush_never_completes_ends_the_session_as_device_loss() {
    let session = Arc::new(
        Session::new(SR, CH, 1.0, false, Duration::ZERO)
            .shorten_discard_timeout(Duration::from_millis(40)),
    );
    assert!(session.allow_output(RESYNC_SAMPLES));
    let epoch = session.schedule_seek(Duration::from_secs(30));

    // 输出侧始终不回执。
    session.finish_seek(epoch, None, session.request_discard());

    assert_eq!(
        session.outcome(),
        Some(SessionOutcome::Failed { device_lost: true }),
        "无输出的 seek 不能静默放行"
    );
}

/// 迟到的 seek 完成不能把自己的裁剪点盖到**新**时间线上：那会让新时间线的
/// 头几秒被静默丢掉（声音从目标之后才开始，进度却是从目标开始）。
#[test]
fn a_late_seek_completion_cannot_plant_its_trim_on_the_new_timeline() {
    let session = playing_session();
    let stale_epoch = session.schedule_seek(Duration::from_secs(10));
    let current_epoch = session.schedule_seek(Duration::from_secs(20));

    // 第一次 seek 的完成此刻才到（它属于前一个代际）。
    session.finish_seek(stale_epoch, Some(10 * SR as u64), session.request_discard());

    assert_eq!(
        session.trim_until(),
        None,
        "旧代际的裁剪点不得作用于新时间线"
    );
    assert!(session.seek_pending(), "新 seek 仍在整备");

    finish_seek(&session, current_epoch, Some(20 * SR as u64));
    assert_eq!(session.trim_until(), Some(20 * SR as u64));
}

#[test]
fn trim_is_cleared_once_the_first_packet_reaches_the_target() {
    let session = playing_session();
    let epoch = session.schedule_seek(Duration::from_secs(5));
    finish_seek(&session, epoch, Some(240_000));

    assert_eq!(session.trim_until(), Some(240_000));
    session.clear_trim();
    assert_eq!(session.trim_until(), None);
}

#[test]
fn a_seek_without_pre_target_audio_needs_no_trim() {
    let session = playing_session();
    let epoch = session.schedule_seek(Duration::from_secs(5));
    finish_seek(&session, epoch, None);

    assert_eq!(session.trim_until(), None);
    assert!(!session.seek_pending());
}

// ---------------------------------------------------------------- 输出门控

#[test]
fn output_stays_muted_until_the_resync_threshold_is_reached() {
    let session = session();

    assert!(!session.allow_output(0));
    assert!(session.buffering());
    assert!(!session.allow_output(RESYNC_SAMPLES / 2));
    assert!(session.allow_output(RESYNC_SAMPLES));
    assert!(!session.buffering());

    // 已经在放：缓冲降到阈值以下也继续放行，不做每个回调都重新判定。
    assert!(session.allow_output(RESYNC_SAMPLES / 2));
}

#[test]
fn buffer_starvation_re_enters_buffering_unless_the_decoder_is_done() {
    let session = session();
    assert!(session.allow_output(RESYNC_SAMPLES));

    session.on_output_starved();
    assert!(session.buffering());
    assert_eq!(session.outcome(), None);

    session.mark_drained();
    assert!(session.allow_output(0), "解码完成时空缓冲也应放行");
    session.on_output_starved();
    assert_eq!(session.outcome(), Some(SessionOutcome::Delivered));
    assert!(!session.buffering());
}

#[test]
fn delivered_outcome_requires_drained_decoder_and_an_emptied_buffer() {
    let session = session();
    assert!(session.allow_output(RESYNC_SAMPLES));

    session.mark_drained();
    assert_eq!(session.outcome(), None, "还有缓冲样本未播出，不算结束");

    session.on_output_starved();
    assert_eq!(session.outcome(), Some(SessionOutcome::Delivered));
}

#[test]
fn a_finished_session_mutes_any_further_output() {
    let session = session();
    let mut cursor = OutputCursor::attach(&session);
    assert_eq!(
        cursor.begin(&session, RESYNC_SAMPLES, || {}),
        OutputBlock::Deliver
    );

    session.mark_drained();
    session.on_output_starved();

    assert_eq!(
        cursor.begin(&session, RESYNC_SAMPLES, || {}),
        OutputBlock::Silence
    );
}

#[test]
fn pausing_mutes_output_without_losing_the_position() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);
    deliver(&session, &mut cursor, 2_400, 0);

    session.set_paused(true);
    assert_eq!(
        cursor.begin(&session, RESYNC_SAMPLES, || {}),
        OutputBlock::Silence
    );
    // 暂停期间墙钟照走，位置必须冻住——既不清零，也不爬向"已提交但未发声"的那一块。
    let paused_at = session.position_frames();
    assert!(paused_at <= 2_400);
    assert_eq!(
        session.position_frames_at(Instant::now() + Duration::from_secs(30)),
        paused_at,
        "暂停期间位置不得随墙钟前进"
    );

    session.set_paused(false);
    assert_eq!(
        cursor.begin(&session, RESYNC_SAMPLES, || {}),
        OutputBlock::Deliver
    );
}

// ---------------------------------------------------------------- 终局

#[test]
fn device_loss_keeps_the_position_so_the_caller_can_migrate() {
    let session = playing_session();
    let mut cursor = OutputCursor::attach(&session);
    deliver(&session, &mut cursor, 96_000, 0);

    session.fail(true, "device gone");

    assert_eq!(
        session.outcome(),
        Some(SessionOutcome::Failed { device_lost: true })
    );
    assert_eq!(
        session.position_frames_at(Instant::now() + Duration::from_secs(1)),
        96_000,
        "迁移续播要按当前位置重放，失败时不得把位置清零"
    );
}

#[test]
fn a_stream_error_after_termination_is_ignored() {
    let session = playing_session();
    session.terminate();

    session.fail(true, "teardown noise");

    assert_eq!(session.outcome(), None);
    assert!(session.is_terminated());
}

#[test]
fn ordinary_stream_failure_is_not_reported_as_device_loss() {
    let session = playing_session();
    session.fail(false, "decode blew up");

    assert_eq!(
        session.outcome(),
        Some(SessionOutcome::Failed { device_lost: false })
    );
}

#[test]
fn repeated_failures_keep_the_first_classification() {
    let session = playing_session();
    session.fail(false, "decode blew up");
    session.fail(true, "device hint arrives later");

    assert_eq!(
        session.outcome(),
        Some(SessionOutcome::Failed { device_lost: false }),
        "首个错误的原因决定迁移策略，后来的噪声不得改写它"
    );
}

// ---------------------------------------------------------------- 音量

#[test]
fn volume_is_clamped_and_ignored_while_locked() {
    let session = session();
    assert_eq!(session.volume(), 1.0);

    session.set_volume(0.5);
    assert_eq!(session.volume(), 0.5);

    session.set_volume(2.0);
    assert_eq!(session.volume(), 1.0);
    session.set_volume(-1.0);
    assert_eq!(session.volume(), 0.0);

    // NaN/±inf 由汇聚点统一按 1.0 处理，不得驻留进位存储。
    session.set_volume(f32::NAN);
    assert_eq!(session.volume(), 1.0);
    session.set_volume(f32::INFINITY);
    assert_eq!(session.volume(), 1.0);

    let locked = Session::new(SR, CH, 1.0, true, Duration::ZERO);
    locked.set_volume(0.3);
    assert_eq!(locked.volume(), 1.0);
    assert!(locked.volume_locked());
}

// ---------------------------------------------------------------- 并发

/// 两个真实线程交错：输出回调持续交付、控制面不断 seek（前后跳动）。
///
/// 断言的是打包原子量要保住的性质——**读到的 `(代际, 帧数)` 永远自洽**：
/// 帧数落在该代际自己的锚点与"本代际允许交付的预算"之间。若位置与代际分开
/// 存放，读者就会拼出"新代际 + 旧代际帧数"的组合，进度瞬间跳到另一条时间线
/// （seek 后 UI 显示错位置、甚至倒退到几十秒前）。
///
/// 这里直接读内部 `(代际, 帧数)` 字：位置 API 会把两段信息压成一个标量，
/// 标量本身看不出撕裂，压测必须看那对原始值。
#[test]
fn concurrent_delivery_and_seeking_never_produce_a_torn_epoch_word() {
    const STEPS: u64 = 400;
    const PER_EPOCH_BUDGET: u64 = 960;

    let session = session();
    assert!(session.allow_output(RESYNC_SAMPLES));

    // 每次 seek 的锚点，按代际记录。前后交替的落点让"新代际 + 旧帧数"这种
    // 撕裂组合立刻越界。
    let anchors = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let delivered = Arc::new(AtomicU64::new(0));

    let deliverer = {
        let session = Arc::clone(&session);
        let stop = Arc::clone(&stop);
        let delivered = Arc::clone(&delivered);
        let anchors = Arc::clone(&anchors);
        std::thread::spawn(move || {
            let mut cursor = OutputCursor::attach(&session);
            while !stop.load(Ordering::Relaxed) {
                if session.is_terminated() {
                    break;
                }
                match cursor.begin(&session, RESYNC_SAMPLES, || {}) {
                    OutputBlock::Deliver => {
                        // 每代际只交付一小段：不这样，交付量会淹没锚点之间的间隔，
                        // 撕裂组合就落在"合法范围"里而测不出来。
                        let word = session.progress_word();
                        let anchor = *anchors.lock().unwrap().get(&word.epoch).unwrap_or(&0);
                        if word.value >= anchor + PER_EPOCH_BUDGET {
                            std::thread::yield_now();
                            continue;
                        }
                        cursor.delivered(&session, 96, latency(0));
                        delivered.fetch_add(96, Ordering::Relaxed);
                    }
                    OutputBlock::Silence => std::thread::yield_now(),
                }
            }
        })
    };

    let observer = {
        let session = Arc::clone(&session);
        let anchors = Arc::clone(&anchors);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut checked = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let word = session.progress_word();
                let anchors = anchors.lock().unwrap();
                let Some(&anchor) = anchors.get(&word.epoch) else {
                    // 代际刚换、锚点还没登记：读到的必然是极旧或极新的合法值，
                    // 下一轮再查。
                    drop(anchors);
                    std::thread::yield_now();
                    continue;
                };
                assert!(
                    word.value >= anchor,
                    "代际 {} 的帧数 {} 小于本代锚点 {}：位置与代际不匹配",
                    word.epoch,
                    word.value,
                    anchor
                );
                assert!(
                    word.value <= anchor + PER_EPOCH_BUDGET + 96,
                    "代际 {} 的帧数 {} 超出本代锚点 {} 的交付预算：读到了另一条时间线的帧号",
                    word.epoch,
                    word.value,
                    anchor
                );
                drop(anchors);
                checked += 1;
            }
            checked
        })
    };

    for step in 0..STEPS {
        // 前后交替：1s ↔ 20s。回退 seek 是撕裂最容易现形的地方。
        let target_secs = if step % 2 == 0 { 20 } else { 1 };
        let target_frames = target_secs * u64::from(SR);

        session.schedule_seek(Duration::from_secs(target_secs));
        let epoch = session.progress_word().epoch;
        anchors.lock().unwrap().insert(epoch, target_frames);
        assert!(
            session.position_frames() >= target_frames,
            "schedule_seek 返回后位置必须已锚到请求点"
        );

        finish_seek(&session, epoch, None);
        std::thread::sleep(Duration::from_micros(40));
    }

    stop.store(true, Ordering::Relaxed);
    deliverer.join().unwrap();
    let checked = observer.join().unwrap();

    assert!(checked > 0, "观察线程一次都没读到有效代际，压测没有意义");
    assert!(
        delivered.load(Ordering::Relaxed) > 0,
        "交付线程一次都没交付，压测没有意义"
    );
}
