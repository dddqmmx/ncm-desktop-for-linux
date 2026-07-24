use crate::audio::state::{NO_TRIM_FRAME, SharedState};
use ringbuf::traits::Producer;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::conv::ConvertibleSample;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{Sample, SampleFormat as SymphoniaSampleFormat};
use symphonia::core::units::TimeBase;

pub(crate) struct AudioMetadata {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) bits_per_sample: Option<u32>,
    pub(crate) sample_format: Option<SymphoniaSampleFormat>,
    pub(crate) time_base: Option<TimeBase>,
    pub(crate) track_id: u32,
    pub(crate) decoder: Box<dyn Decoder>,
    pub(crate) format_reader: Box<dyn FormatReader>,
}

pub(crate) async fn spawn_probe_task(
    source: Box<dyn MediaSource>,
    extension: Option<String>,
) -> Result<AudioMetadata, Box<dyn std::error::Error>> {
    tokio::task::spawn_blocking(move || probe_source(source, extension))
        .await?
        .map_err(|e| e as Box<dyn std::error::Error>)
}

pub(crate) async fn probe_file_duration_ms(
    path: String,
) -> Result<u64, Box<dyn std::error::Error>> {
    tokio::task::spawn_blocking(move || probe_file_duration_ms_blocking(&path))
        .await?
        .map_err(|err| err as Box<dyn std::error::Error>)
}

fn probe_file_duration_ms_blocking(
    file_path: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let path = Path::new(file_path);
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No audio track")?;
    let track_id = track.id;
    let time_base = track.codec_params.time_base;
    let sample_rate = track.codec_params.sample_rate;
    let start_ts = track.codec_params.start_ts;

    if let Some(frames) = track.codec_params.n_frames
        && let Some(duration_ms) = duration_ticks_to_ms(frames, time_base, sample_rate)
    {
        return Ok(duration_ms);
    }

    // Some containers do not publish n_frames in their headers. Reading packet headers to EOF
    // obtains the full duration without decoding PCM samples.
    let mut end_ts = start_ts;
    loop {
        match format.next_packet() {
            Ok(packet) if packet.track_id() == track_id => {
                end_ts = end_ts.max(packet.ts().saturating_add(packet.dur()));
            }
            Ok(_) => {}
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => continue,
            Err(error) => return Err(Box::new(error)),
        }
    }

    duration_ticks_to_ms(end_ts.saturating_sub(start_ts), time_base, sample_rate)
        .ok_or_else(|| "Audio duration is unavailable".into())
}

fn duration_ticks_to_ms(
    ticks: u64,
    time_base: Option<TimeBase>,
    sample_rate: Option<u32>,
) -> Option<u64> {
    if ticks == 0 {
        return None;
    }

    if let Some(time_base) = time_base {
        let time = time_base.calc_time(ticks);
        let millis = u128::from(time.seconds)
            .saturating_mul(1_000)
            .saturating_add((time.frac * 1_000.0).round() as u128);
        return Some(millis.min(u128::from(u64::MAX)) as u64);
    }

    sample_rate.filter(|rate| *rate > 0).map(|rate| {
        ((u128::from(ticks) * 1_000) / u128::from(rate)).min(u128::from(u64::MAX)) as u64
    })
}

fn probe_source(
    source: Box<dyn MediaSource>,
    extension: Option<String>,
) -> Result<AudioMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let mss = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(&ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No audio track")?;

    let track_id = track.id;
    let mut sr = track.codec_params.sample_rate;
    let mut channels = track
        .codec_params
        .channels
        .map(|channels| channels.count() as u16);
    let bits_per_sample = track.codec_params.bits_per_sample;
    let sample_format = track.codec_params.sample_format;
    let time_base = track.codec_params.time_base;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let decoder_params = decoder.codec_params();
    sr = sr.or(decoder_params.sample_rate);
    channels = channels.or_else(|| {
        decoder_params
            .channels
            .map(|channels| channels.count() as u16)
    });

    let (sr, channels) = match (sr, channels) {
        (Some(sr), Some(channels)) if channels > 0 => (sr, channels),
        _ => {
            let decoded_spec = decode_stream_spec(&mut *format, &mut *decoder, track_id)?;
            decoder.reset();
            format.seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time: symphonia::core::units::Time::from(0.0),
                    track_id: Some(track_id),
                },
            )?;
            decoder.reset();

            (
                sr.filter(|sample_rate| *sample_rate > 0)
                    .unwrap_or(decoded_spec.sample_rate),
                channels
                    .filter(|channels| *channels > 0)
                    .unwrap_or(decoded_spec.channels),
            )
        }
    };

    Ok(AudioMetadata {
        sample_rate: sr,
        channels,
        bits_per_sample,
        sample_format,
        time_base,
        track_id,
        decoder,
        format_reader: format,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedStreamSpec {
    sample_rate: u32,
    channels: u16,
}

fn decode_stream_spec(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
) -> Result<DecodedStreamSpec, Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let packet = format.next_packet()?;
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = decoded.spec();
                let channels = spec.channels.count() as u16;
                if spec.rate == 0 || channels == 0 {
                    return Err("Decoded audio stream has invalid signal spec".into());
                }

                return Ok(DecodedStreamSpec {
                    sample_rate: spec.rate,
                    channels,
                });
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(Box::new(err)),
        }
    }
}

pub(crate) fn handle_seek_if_needed(
    state: &SharedState,
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    sr: u32,
    time_base: Option<TimeBase>,
) {
    let seek_req = match state.try_take_seek_request() {
        Ok(request) => request,
        Err(()) => {
            println!("[Seek-Check] 警告：Seek 锁竞争中...");
            return;
        }
    };

    if let Some(target) = seek_req {
        println!(
            "[Seek-Check] >>> 收到 Seek 请求，目标时间: {:?} <<<",
            target
        );

        state.discard_buffer.store(true, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);

        decoder.reset();

        println!("[Seek-Check] 正在执行底层的 format.seek (网络 IO 可能在此阻塞)...");
        let start = std::time::Instant::now();
        let res = format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: symphonia::core::units::Time::from(target.as_secs_f64()),
                track_id: Some(track_id),
            },
        );
        println!(
            "[Seek-Check] 底层 seek 完成，耗时: {:?}, 结果: {:?}",
            start.elapsed(),
            res
        );

        if let Err(err) = res {
            fail_playback_after_stream_error(state, "seek", &err);
            return;
        }

        state.buffered_frames.store(0, Ordering::SeqCst);
        let requested_frame = (target.as_secs_f64() * sr as f64) as u64;
        // Symphonia Accurate seek 可能从请求位置之前的 packet/keyframe 开始解码。
        // 进度应该锚到用户请求的位置，同时把 actual..requested 之间的样本裁掉；
        // 只锚到 actual 会让 UI 看起来一致，但用户实际听到的仍不是 requested 位置。
        let completion = seek_completion_plan(&res, requested_frame, sr, time_base, track_id);
        state.decoder_done.store(false, Ordering::SeqCst);
        // 注意：此处不再二次拉高 discard_buffer。前置那次（见上）已负责排空旧的
        // seek 前样本；seek 完成后 ringbuf 里即将被写入的是目标位置的正常样本，再次置
        // discard 会把正确样本一并丢弃，导致 current_frame 停在锚点而 ringbuf 被反复清空，
        // 表现为进度与播放脱节。
        wait_for_pending_discard_to_drain(state);
        if state.is_terminating.load(Ordering::Relaxed) {
            return;
        }

        let committed = state.commit_seek_completion_if_current(
            completion.anchor_frame,
            completion.trim_until_frame,
        );
        println!(
            "[Seek-Check] 状态重置完成，锚定帧: {} (请求帧: {}, actual_frame: {:?}, trim_until: {:?}, actual_ts: {:?}, committed: {})",
            completion.anchor_frame,
            requested_frame,
            completion.actual_frame,
            completion.trim_until_frame,
            res.as_ref().ok().map(|s| s.actual_ts),
            committed
        );
    }
}

fn wait_for_pending_discard_to_drain(state: &SharedState) {
    while (state.discard_buffer.load(Ordering::SeqCst)
        || state.is_discarding_buffer.load(Ordering::SeqCst))
        && !state.is_terminating.load(Ordering::Relaxed)
    {
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeekCompletionPlan {
    pub(crate) anchor_frame: u64,
    pub(crate) trim_until_frame: Option<u64>,
    pub(crate) actual_frame: Option<u64>,
}

/// 由 `format.seek` 返回的 `SeekedTo` 计算 seek 完成后的进度锚点和裁剪目标。
///
/// `actual_ts` 是 Symphonia 真正落点的 track timebase 时间戳。若 actual 落在请求位置
/// 之前，解码线程需要继续从 actual 解码，但进入 ring buffer 前必须裁到 requested。
/// 因此 `current_frame`/进度锚到 requested，`trim_until_frame` 也设为 requested。
pub(crate) fn seek_completion_plan(
    seeked: &Result<SeekedTo, SymphoniaError>,
    requested_frame: u64,
    sample_rate: u32,
    time_base: Option<TimeBase>,
    expected_track_id: u32,
) -> SeekCompletionPlan {
    let actual_frame = seek_actual_frame(
        seeked,
        requested_frame,
        sample_rate,
        time_base,
        expected_track_id,
    );
    let trim_until_frame = actual_frame
        .filter(|actual| *actual < requested_frame)
        .map(|_| requested_frame);

    SeekCompletionPlan {
        anchor_frame: requested_frame,
        trim_until_frame,
        actual_frame,
    }
}

fn seek_actual_frame(
    seeked: &Result<SeekedTo, SymphoniaError>,
    requested_frame: u64,
    sample_rate: u32,
    time_base: Option<TimeBase>,
    expected_track_id: u32,
) -> Option<u64> {
    match seeked {
        Ok(result) => {
            if result.track_id != expected_track_id {
                return None;
            }

            let Some(actual) = timestamp_to_frame(result.actual_ts, sample_rate, time_base) else {
                return None;
            };

            // Accurate seek 正常应满足 actual <= requested。若 demuxer 返回请求之后的
            // 异常 actual，不据此裁剪，避免误丢掉用户请求位置之后的有效样本。
            if actual <= requested_frame {
                Some(actual)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

fn timestamp_to_frame(ts: u64, sample_rate: u32, time_base: Option<TimeBase>) -> Option<u64> {
    let time_base = time_base?;
    if time_base.numer == 0 || time_base.denom == 0 {
        return None;
    }

    let frames = u128::from(ts)
        .saturating_mul(u128::from(time_base.numer))
        .saturating_mul(u128::from(sample_rate))
        / u128::from(time_base.denom);
    u64::try_from(frames).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeekPacketTrim {
    skip_frames: usize,
    keep_frames: usize,
    clear_trim: bool,
}

fn seek_packet_trim(
    packet_start_frame: Option<u64>,
    packet_frames: usize,
    trim_until_frame: u64,
) -> SeekPacketTrim {
    if trim_until_frame == NO_TRIM_FRAME {
        return SeekPacketTrim {
            skip_frames: 0,
            keep_frames: packet_frames,
            clear_trim: false,
        };
    }

    let Some(packet_start_frame) = packet_start_frame else {
        return SeekPacketTrim {
            skip_frames: packet_frames,
            keep_frames: 0,
            clear_trim: false,
        };
    };

    let packet_end_frame = packet_start_frame.saturating_add(packet_frames as u64);
    if packet_end_frame <= trim_until_frame {
        return SeekPacketTrim {
            skip_frames: packet_frames,
            keep_frames: 0,
            clear_trim: false,
        };
    }

    let skip_frames = trim_until_frame
        .saturating_sub(packet_start_frame)
        .min(packet_frames as u64) as usize;
    SeekPacketTrim {
        skip_frames,
        keep_frames: packet_frames.saturating_sub(skip_frames),
        clear_trim: true,
    }
}

pub(crate) fn decode_next_packet<S, P>(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    sample_rate: u32,
    time_base: Option<TimeBase>,
    producer: &mut P,
    state: &SharedState,
) -> bool
where
    S: ConvertibleSample + Copy,
    P: Producer<Item = S>,
{
    match format.next_packet() {
        Ok(packet) => {
            if packet.track_id() != track_id {
                return true;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let num_frames = decoded.frames();
                    if num_frames == 0 {
                        return true;
                    }

                    let mut sample_buf = SampleBuffer::<S>::new(num_frames as u64, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    let trim = seek_packet_trim(
                        timestamp_to_frame(packet.ts(), sample_rate, time_base),
                        num_frames,
                        state.trim_until_frame.load(Ordering::Relaxed),
                    );

                    if trim.clear_trim {
                        state.clear_trim();
                    }

                    if trim.keep_frames > 0 {
                        let channels = spec.channels.count();
                        let skip_samples = trim
                            .skip_frames
                            .saturating_mul(channels)
                            .min(sample_buf.samples().len());
                        push_samples_blocking::<S, _>(
                            producer,
                            &sample_buf.samples()[skip_samples..],
                            state,
                        );
                    }
                }
                Err(symphonia::core::errors::Error::DecodeError(e)) => {
                    eprintln!("[Decoder] 解码包失败（跳过）: {}", e);
                }
                Err(_) => {}
            }
            true
        }
        Err(symphonia::core::errors::Error::IoError(e)) => {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                println!("[Decoder] 到达文件末尾 (EOF)");
                return false;
            }
            if is_stream_download_failure(&e) {
                fail_playback_after_stream_error(state, "read", &e);
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
            true
        }
        Err(e) => {
            println!("[Decoder] 停止解码: {:?}", e);
            false
        }
    }
}

fn is_stream_download_failure(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::Other && err.to_string().contains("stream failed to download")
}

fn fail_playback_after_stream_error(
    state: &SharedState,
    operation: &str,
    err: &dyn std::fmt::Display,
) {
    eprintln!("[Decoder] stream {operation} failed, stopping playback: {err}");
    state.discard_buffer.store(true, Ordering::SeqCst);
    state.waiting_for_seek.store(false, Ordering::SeqCst);
    state.has_seek_request.store(false, Ordering::SeqCst);
    state.buffered_frames.store(0, Ordering::SeqCst);
    state.current_frame.store(0, Ordering::SeqCst);
    state.clear_trim();
    state
        .playback_clock
        .lock()
        .unwrap()
        .reset_to(0, state.sample_rate.load(Ordering::SeqCst));
    state.decoder_done.store(true, Ordering::SeqCst);
    state.is_terminating.store(true, Ordering::SeqCst);
    if !state.is_finished.swap(true, Ordering::SeqCst) {
        state.finish_notify.notify_waiters();
    }
}

fn push_samples_blocking<S, P>(producer: &mut P, samples: &[S], state: &SharedState)
where
    S: Sample + Copy,
    P: Producer<Item = S>,
{
    let mut written = 0;
    let mut retry_count = 0;
    while written < samples.len() {
        if state.is_terminating.load(Ordering::Relaxed) {
            break;
        }

        if state.has_seek_request.load(Ordering::Relaxed) {
            println!("[Seek-Check] 检测到新 Seek 请求，中断当前数据推送");
            return;
        }

        // 暂停时输出回调不再消费 ringbuf。若这里仍死等“有空位”，
        // 解码线程会在 pause 期间永久阻塞，resume/seek/切歌都可能卡住。
        if state.is_paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        let n = producer.push_slice(&samples[written..]);
        if n > 0 {
            written += n;
            retry_count = 0;
        } else {
            retry_count += 1;
            if retry_count % 100 == 0 {
                println!(
                    "[Seek-Check] 写入阻塞中... Buffer 状态: Full, 播放暂停: {}, 缓冲模式: {}",
                    state.is_paused.load(Ordering::Relaxed),
                    state.waiting_for_seek.load(Ordering::Relaxed)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::state::PlaybackClock;
    use std::borrow::Cow;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec};
    use tokio::sync::Notify;

    #[test]
    fn converts_track_ticks_to_milliseconds() {
        assert_eq!(
            duration_ticks_to_ms(8_000, Some(TimeBase::new(1, 8_000)), Some(8_000)),
            Some(1_000)
        );
        assert_eq!(duration_ticks_to_ms(22_050, None, Some(44_100)), Some(500));
        assert_eq!(
            duration_ticks_to_ms(0, Some(TimeBase::new(1, 1_000)), None),
            None
        );
    }

    fn create_state() -> SharedState {
        SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(48_000),
            playback_clock: Mutex::new(PlaybackClock::new()),
            trim_until_frame: AtomicU64::new(NO_TRIM_FRAME),
            sample_rate: AtomicU32::new(48_000),
            has_seek_request: AtomicBool::new(true),
            seek_request: Mutex::new(Some(Duration::from_secs(1))),
            is_terminating: AtomicBool::new(false),
            discard_buffer: AtomicBool::new(false),
            is_discarding_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(12_000),
            waiting_for_seek: AtomicBool::new(true),
        }
    }

    #[test]
    fn push_samples_blocking_does_not_hang_while_paused_on_full_buffer() {
        use ringbuf::HeapRb;
        use ringbuf::traits::{Producer, Split};
        use std::sync::atomic::Ordering;
        use std::thread;
        use std::time::{Duration, Instant};

        let state = Arc::new(create_state());
        state.has_seek_request.store(false, Ordering::SeqCst);
        state.is_paused.store(true, Ordering::SeqCst);

        // 填满 ringbuf，模拟 pause 后输出侧停止消费、解码侧缓冲区已满。
        let rb = HeapRb::<i16>::new(8);
        let (mut producer, _consumer) = rb.split();
        assert_eq!(producer.push_slice(&[1, 2, 3, 4, 5, 6, 7, 8]), 8);

        let state_for_push = Arc::clone(&state);
        let push_thread = thread::spawn(move || {
            // 旧逻辑会在 buffer full 上永久 sleep 重试，且 pause 期间输出不消费，
            // 导致解码线程卡死。修复后 pause 时会主动让出并响应 terminating。
            push_samples_blocking(&mut producer, &[9i16; 16], &state_for_push);
        });

        thread::sleep(Duration::from_millis(40));
        assert!(
            !push_thread.is_finished(),
            "paused decode push should wait, but must remain interruptible"
        );

        state.is_terminating.store(true, Ordering::SeqCst);
        let started = Instant::now();
        push_thread
            .join()
            .expect("push thread panicked while paused on full buffer");
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "push_samples_blocking must exit promptly when terminating during pause"
        );
    }

    #[test]
    fn stream_download_failure_is_fatal() {
        let err = std::io::Error::new(std::io::ErrorKind::Other, "stream failed to download");

        assert!(is_stream_download_failure(&err));
    }

    #[test]
    fn stream_error_stops_playback_state() {
        let state = create_state();

        fail_playback_after_stream_error(&state, "seek", &"stream failed to download");

        assert!(state.discard_buffer.load(Ordering::SeqCst));
        assert!(!state.waiting_for_seek.load(Ordering::SeqCst));
        assert!(!state.has_seek_request.load(Ordering::SeqCst));
        assert_eq!(state.buffered_frames.load(Ordering::SeqCst), 0);
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 0);
        assert_eq!(state.trim_until_frame.load(Ordering::SeqCst), NO_TRIM_FRAME);
        assert!(state.decoder_done.load(Ordering::SeqCst));
        assert!(state.is_terminating.load(Ordering::SeqCst));
        assert!(state.is_finished.load(Ordering::SeqCst));
    }

    #[test]
    fn zero_frame_decoded_buffer_would_panic_without_skip_guard() {
        let spec = SignalSpec::new(48_000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let decoded = AudioBuffer::<f32>::new(0, spec);

        assert_eq!(decoded.frames(), 0);
        let copy_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut sample_buf = SampleBuffer::<f32>::new(decoded.frames() as u64, spec);
            sample_buf.copy_interleaved_ref(AudioBufferRef::F32(Cow::Borrowed(&decoded)));
        }));

        assert!(
            copy_result.is_err(),
            "Symphonia panics when copying a zero-frame multichannel buffer; production must skip it"
        );
    }

    #[test]
    fn seek_completion_waits_for_pending_discard_to_drain_before_decoding() {
        let state = Arc::new(create_state());
        state.discard_buffer.store(true, Ordering::SeqCst);

        let state_for_callback = Arc::clone(&state);
        let callback = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            state_for_callback
                .discard_buffer
                .store(false, Ordering::SeqCst);
        });

        let started_at = Instant::now();
        wait_for_pending_discard_to_drain(&state);
        callback.join().unwrap();

        assert!(!state.discard_buffer.load(Ordering::SeqCst));
        assert!(
            started_at.elapsed() >= Duration::from_millis(15),
            "seek completion must not race ahead and let post-seek samples be written before old samples are drained"
        );

        state.is_discarding_buffer.store(true, Ordering::SeqCst);
        let state_for_callback = Arc::clone(&state);
        let callback = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            state_for_callback
                .is_discarding_buffer
                .store(false, Ordering::SeqCst);
        });

        let started_at = Instant::now();
        wait_for_pending_discard_to_drain(&state);
        callback.join().unwrap();

        assert!(!state.is_discarding_buffer.load(Ordering::SeqCst));
        assert!(
            started_at.elapsed() >= Duration::from_millis(15),
            "seek completion must also wait while the output callback is actively draining"
        );
    }

    use symphonia::core::formats::SeekedTo;
    use symphonia::core::units::TimeBase;

    fn seeked_to(actual_ts: u64, requested_ts: u64, track_id: u32) -> SeekedTo {
        SeekedTo {
            track_id,
            required_ts: requested_ts,
            actual_ts,
        }
    }

    /// 有损格式（Vorbis/Opus/MP3）下 Accurate seek 常从请求位置之前的 packet/keyframe
    /// 开始解码。只把进度锚到 actual 或 requested 都不够；必须让进度锚到 requested，
    /// 并把 actual..requested 的 decoded samples 裁掉，第一帧写入 ring buffer 才是目标。
    #[test]
    fn seek_completion_anchors_requested_and_trims_pre_target_audio() {
        let sr: u32 = 48_000;
        let time_base = TimeBase::new(1, sr);
        let target = Duration::from_secs_f64(30.0);
        let requested_frame = (target.as_secs_f64() * sr as f64) as u64;
        let actual_frame = requested_frame - 1024;
        let seeked: Result<SeekedTo, SymphoniaError> =
            Ok(seeked_to(actual_frame, requested_frame, 0));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, Some(actual_frame));
        assert_eq!(completion.trim_until_frame, Some(requested_frame));

        let trim = seek_packet_trim(
            Some(actual_frame),
            2048,
            completion.trim_until_frame.unwrap(),
        );
        assert_eq!(
            trim,
            SeekPacketTrim {
                skip_frames: 1024,
                keep_frames: 1024,
                clear_trim: true,
            }
        );
    }

    /// MP4/M4A 等容器的 actual_ts 是 track timebase 时间戳，不能直接当 sample frame。
    /// 这个测试用 1/1000 的 timebase 模拟容器 timescale，确保裁剪起点按采样帧计算。
    #[test]
    fn seek_completion_converts_container_timebase_before_trimming() {
        let sr: u32 = 48_000;
        let time_base = TimeBase::new(1, 1_000);
        let requested_frame = 30 * sr as u64;
        let actual_ts_ms = 29_500;
        let expected_actual_frame = 29_500 * sr as u64 / 1_000;
        let seeked: Result<SeekedTo, SymphoniaError> = Ok(seeked_to(actual_ts_ms, 30_000, 0));

        assert_eq!(actual_ts_ms, 29_500, "old direct-ts-as-frame anchor");
        assert_ne!(
            actual_ts_ms, expected_actual_frame,
            "test must prove timestamp units differ from sample frames"
        );

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, Some(expected_actual_frame));
        assert_eq!(completion.trim_until_frame, Some(requested_frame));
    }

    /// Accurate seek 保证 actual <= requested；若 demuxer 在流式/部分下载场景里返回了一个
    /// 落在请求 *之后* 的异常 actual，不据此裁剪，避免丢掉目标之后的有效样本。
    #[test]
    fn seek_completion_does_not_trim_when_actual_is_past_request() {
        let sr: u32 = 48_000;
        let time_base = TimeBase::new(1, sr);
        let target = Duration::from_secs_f64(30.0);
        let requested_frame = (target.as_secs_f64() * sr as f64) as u64;
        let seeked: Result<SeekedTo, SymphoniaError> =
            Ok(seeked_to(requested_frame + 10_000, requested_frame, 0));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, None);
        assert_eq!(completion.trim_until_frame, None);
    }

    #[test]
    fn seek_completion_does_not_trim_on_seek_error() {
        let sr: u32 = 48_000;
        let time_base = TimeBase::new(1, sr);
        let target = Duration::from_secs_f64(30.0);
        let requested_frame = (target.as_secs_f64() * sr as f64) as u64;
        let seeked: Result<SeekedTo, SymphoniaError> = Err(SymphoniaError::IoError(
            std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        ));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, None);
        assert_eq!(completion.trim_until_frame, None);
    }

    #[test]
    fn seek_completion_does_not_trim_when_timebase_is_missing() {
        let sr: u32 = 48_000;
        let requested_frame = 30 * sr as u64;
        let seeked: Result<SeekedTo, SymphoniaError> =
            Ok(seeked_to(requested_frame - 1024, requested_frame, 0));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, None, 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, None);
        assert_eq!(completion.trim_until_frame, None);
    }

    #[test]
    fn seek_completion_does_not_trim_when_seeked_track_does_not_match() {
        let sr: u32 = 48_000;
        let time_base = TimeBase::new(1, sr);
        let requested_frame = 30 * sr as u64;
        let seeked: Result<SeekedTo, SymphoniaError> =
            Ok(seeked_to(requested_frame - 1024, requested_frame, 9));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        assert_eq!(completion.anchor_frame, requested_frame);
        assert_eq!(completion.actual_frame, None);
        assert_eq!(completion.trim_until_frame, None);
    }

    #[test]
    fn seek_packet_trim_discards_packets_fully_before_target() {
        let trim = seek_packet_trim(Some(1_000), 100, 1_200);
        assert_eq!(
            trim,
            SeekPacketTrim {
                skip_frames: 100,
                keep_frames: 0,
                clear_trim: false,
            }
        );
    }

    #[test]
    fn seek_packet_trim_keeps_only_frames_at_or_after_target() {
        let samples: Vec<i16> = (0..12).collect();
        let channels = 2;
        let trim = seek_packet_trim(Some(1_000), 6, 1_004);
        let skip_samples = trim.skip_frames * channels;

        assert_eq!(
            trim,
            SeekPacketTrim {
                skip_frames: 4,
                keep_frames: 2,
                clear_trim: true,
            }
        );
        assert_eq!(&samples[skip_samples..], &[8, 9, 10, 11]);
    }

    #[test]
    fn seek_packet_trim_clears_when_first_packet_starts_after_target() {
        let trim = seek_packet_trim(Some(1_300), 100, 1_200);
        assert_eq!(
            trim,
            SeekPacketTrim {
                skip_frames: 0,
                keep_frames: 100,
                clear_trim: true,
            }
        );
    }

    #[test]
    fn seek_packet_trim_drops_unknown_timestamp_until_a_mappable_packet_arrives() {
        let trim = seek_packet_trim(None, 100, 1_200);
        assert_eq!(
            trim,
            SeekPacketTrim {
                skip_frames: 100,
                keep_frames: 0,
                clear_trim: false,
            }
        );
    }

    /// 端到端状态决策：seek 完成后 current_frame 必须是请求位置，同时 SharedState
    /// 记录 trim_until。否则后续输出回调会从 actual 开始计进度，或把 pre-target 音频送出。
    #[test]
    fn current_frame_anchors_to_requested_and_records_trim_after_seek_decision() {
        let sr: u32 = 48_000;
        let state = create_state();
        state.sample_rate.store(sr, Ordering::SeqCst);

        let target = Duration::from_secs_f64(30.0);
        let requested_frame = (target.as_secs_f64() * sr as f64) as u64;
        let actual_frame = requested_frame - 2048;
        let time_base = TimeBase::new(1, sr);
        let seeked: Result<SeekedTo, SymphoniaError> =
            Ok(seeked_to(actual_frame, requested_frame, 0));

        let completion = seek_completion_plan(&seeked, requested_frame, sr, Some(time_base), 0);
        state
            .current_frame
            .store(completion.anchor_frame, Ordering::SeqCst);
        state.trim_until_frame.store(
            completion.trim_until_frame.unwrap_or(NO_TRIM_FRAME),
            Ordering::SeqCst,
        );

        assert_eq!(
            state.current_frame.load(Ordering::SeqCst),
            requested_frame,
            "seek 后进度必须锚到用户请求位置，而不是 Symphonia 的提前落点"
        );
        assert_eq!(
            state.trim_until_frame.load(Ordering::SeqCst),
            requested_frame
        );
        assert_eq!(completion.actual_frame, Some(actual_frame));
    }
}
