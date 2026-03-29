use std::sync::atomic::Ordering;
use std::time::Duration;
use ringbuf::traits::Producer;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::conv::ConvertibleSample;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{Sample, SampleFormat as SymphoniaSampleFormat};
use crate::audio::state::SharedState;

pub(crate) struct AudioMetadata {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) bits_per_sample: Option<u32>,
    pub(crate) sample_format: Option<SymphoniaSampleFormat>,
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

    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No audio track")?;

    let sr = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.unwrap().count() as u16;
    let bits_per_sample = track.codec_params.bits_per_sample;
    let sample_format = track.codec_params.sample_format;
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    Ok(AudioMetadata {
        sample_rate: sr,
        channels,
        bits_per_sample,
        sample_format,
        track_id: track.id,
        decoder,
        format_reader: format,
    })
}

pub(crate) fn handle_seek_if_needed(
    state: &SharedState,
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    sr: u32,
) {
    let seek_req = {
        match state.seek_request.try_lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => {
                println!("[Seek-Check] 警告：Seek 锁竞争中...");
                return;
            }
        }
    };

    if let Some(target) = seek_req {
        println!("[Seek-Check] >>> 收到 Seek 请求，目标时间: {:?} <<<", target);

        state.discard_buffer.store(true, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(false, Ordering::SeqCst);

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

        state.buffered_frames.store(0, Ordering::SeqCst);
        let target_frame = (target.as_secs_f64() * sr as f64) as u64;
        state.current_frame.store(target_frame, Ordering::SeqCst);
        state.decoder_done.store(false, Ordering::SeqCst);
        println!("[Seek-Check] 状态重置完成，准备重新开始解码");
    }
}

pub(crate) fn decode_next_packet<S, P>(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
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
                    let mut sample_buf = SampleBuffer::<S>::new(num_frames as u64, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    push_samples_blocking::<S, _>(producer, sample_buf.samples(), state);
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
            std::thread::sleep(Duration::from_millis(100));
            true
        }
        Err(e) => {
            println!("[Decoder] 停止解码: {:?}", e);
            false
        }
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
