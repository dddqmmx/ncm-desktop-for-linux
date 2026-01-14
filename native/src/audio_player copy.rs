use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::{Settings, StreamDownload};
use stream_download::storage::temp::TempStorageProvider;
use std::error::Error;
use std::num::NonZeroUsize;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions, Decoder};
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::io::{Read, Seek, SeekFrom};
use tokio::sync::Notify;

// --- 类型定义与辅助结构 ---

pub struct SeekableSource<R> {
    inner: R,
}

impl<R: Read + Seek + Send + Sync> SeekableSource<R> {
    pub fn new(inner: R) -> Self { Self { inner } }
}

impl<R: Read + Seek + Send + Sync> Read for SeekableSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> { self.inner.read(buf) }
}

impl<R: Read + Seek + Send + Sync> Seek for SeekableSource<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> { self.inner.seek(pos) }
}

impl<R: Read + Seek + Send + Sync> MediaSource for SeekableSource<R> {
    fn is_seekable(&self) -> bool { true }
    fn byte_len(&self) -> Option<u64> { None }
}

struct AudioMetadata {
    sample_rate: u32,
    channels: u16,
    track_id: u32,
    decoder: Box<dyn Decoder>,
    format_reader: Box<dyn FormatReader>,
}

pub(crate) struct SharedState { // 结构体本身也要设为 pub(crate) 或 pub
    pub(crate) is_paused: AtomicBool,
    pub(crate) current_frame: AtomicU64,
    pub(crate) sample_rate: AtomicU32,
    pub(crate) seek_request: Mutex<Option<Duration>>,
    pub(crate) is_terminating: AtomicBool,
    pub(crate) discard_buffer: AtomicBool,
    pub(crate) decoder_done: AtomicBool,
    pub(crate) is_finished: AtomicBool,
    pub(crate) finish_notify: Notify, // <--- 确保这里加上了 pub(crate)
}
pub struct AudioPlayer {
    device: cpal::Device,
    stream: Option<cpal::Stream>,
    state: Arc<SharedState>,
}

impl AudioPlayer {
    pub fn new(device_name_filter: Option<&str>) -> Result<Self, Box<dyn Error>> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name_filter {
            host.output_devices()?
                .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
                .ok_or("Device not found")?
        } else {
            host.default_output_device().ok_or("No default device")?
        };

        Ok(Self {
            device,
            stream: None,
            state: Arc::new(SharedState {
                is_paused: AtomicBool::new(false),
                current_frame: AtomicU64::new(0),
                sample_rate: AtomicU32::new(44100),
                seek_request: Mutex::new(None),
                is_terminating: AtomicBool::new(false),
                discard_buffer: AtomicBool::new(false),
                decoder_done: AtomicBool::new(false),
                is_finished: AtomicBool::new(false),
                finish_notify: Notify::new(),
            }),
        })
    }

    pub fn get_state(&self) -> Arc<SharedState> {
        self.state.clone()
    }

    // --- 核心业务方法 ---

    pub async fn play_url(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        let extension = std::path::Path::new(url).extension().and_then(|s| s.to_str()).map(|s| s.to_string());
        let stream = HttpStream::<Client>::create(url.parse()?).await?;
        let reader = StreamDownload::from_stream(
            stream,
            AdaptiveStorageProvider::new(TempStorageProvider::default(), NonZeroUsize::new(512 * 1024).unwrap()),
            Settings::default().prefetch_bytes(512 * 1024),
        ).await?;

        let source = Box::new(SeekableSource::new(reader));
        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta)
    }

    pub async fn play_file(&mut self, path: &str) -> Result<(), Box<dyn Error>> {
        let path_buf = std::path::Path::new(path);
        let extension = path_buf.extension().and_then(|s| s.to_str()).map(|s| s.to_string());
        let file = std::fs::File::open(path_buf)?;
        let source = Box::new(file);

        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta)
    }

    async fn spawn_probe_task(
        &self,
        source: Box<dyn MediaSource>,
        extension: Option<String>
    ) -> Result<AudioMetadata, Box<dyn Error>> {
        tokio::task::spawn_blocking(move || {
            Self::probe_source(source, extension)
        }).await?.map_err(|e| e as Box<dyn std::error::Error>)
    }

    fn probe_source(source: Box<dyn MediaSource>, extension: Option<String>) -> Result<AudioMetadata, Box<dyn Error + Send + Sync>> {
        let mss = MediaSourceStream::new(source, Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = extension { hint.with_extension(&ext); }

        let probed = symphonia::default::get_probe().format(
            &hint, mss, &FormatOptions::default(), &MetadataOptions::default()
        )?;

        let format = probed.format;
        let track = format.tracks().iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or("No audio track")?;

        let sr = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.unwrap().count() as u16;
        let decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

        Ok(AudioMetadata {
            sample_rate: sr,
            channels,
            track_id: track.id,
            decoder,
            format_reader: format,
        })
    }

fn setup_and_play(&mut self, meta: AudioMetadata) -> Result<(), Box<dyn Error>> {
    self.stop();

    self.state.is_terminating.store(false, Ordering::SeqCst);
    self.state.decoder_done.store(false, Ordering::SeqCst);
    self.state.is_finished.store(false, Ordering::SeqCst);

    let sr = meta.sample_rate;
    let channels = meta.channels;
    self.state.sample_rate.store(sr, Ordering::SeqCst);
    self.state.current_frame.store(0, Ordering::SeqCst);

    let (config, sample_format) = self.find_best_config(sr, channels)?;
    if config.sample_rate != sr {
        return Err(format!("Sample-rate mismatch: file={}Hz, output={}Hz", sr, config.sample_rate).into());
    }

    // 对于多通道音频，增加缓冲区大小，并确保是通道数的整数倍
    let buffer_frames = sr as usize * 2; // 2秒缓冲
    let buffer_samples = buffer_frames * channels as usize;
    let rb = HeapRb::<i32>::new(buffer_samples);
    let (producer, consumer) = rb.split();

    self.start_decode_thread(meta, producer);

    let state_for_cb = self.state.clone();
    let stream = match sample_format {
        cpal::SampleFormat::I16 => self.build_stream::<i16, _>(&config, consumer, state_for_cb, channels as usize)?,
        cpal::SampleFormat::I32 => self.build_stream::<i32, _>(&config, consumer, state_for_cb, channels as usize)?,
        cpal::SampleFormat::F32 => self.build_stream::<f32, _>(&config, consumer, state_for_cb, channels as usize)?,
        _ => return Err(format!("Unsupported sample format: {:?}", sample_format).into()),
    };

    stream.play()?;
    self.stream = Some(stream);
    Ok(())
}

fn start_decode_thread(&self, meta: AudioMetadata, mut producer: impl Producer<Item = i32> + Send + 'static ) {
    let state = self.state.clone();
    let mut decoder = meta.decoder;
    let mut format = meta.format_reader;
    let track_id = meta.track_id;
    let sr = meta.sample_rate;
    let channels = meta.channels as usize; // 添加这行

    std::thread::spawn(move || {
        loop {
            if state.is_terminating.load(Ordering::Relaxed) { break; }

            Self::handle_seek_if_needed(&state, &mut *format, &mut *decoder, track_id, sr);

            if !producer.is_full() {
                if !Self::decode_next_packet(&mut *format, &mut *decoder, track_id, &mut producer, &state, channels) {
                    state.decoder_done.store(true, Ordering::SeqCst);
                    break;
                }
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });
}


    fn handle_seek_if_needed(state: &SharedState, format: &mut dyn FormatReader, decoder: &mut dyn Decoder, track_id: u32, sr: u32) {
        let seek_req = { state.seek_request.lock().unwrap().take() };
        if let Some(target) = seek_req {
            let _ = decoder.reset();
            state.discard_buffer.store(true, Ordering::SeqCst);
            let _ = format.seek(SeekMode::Accurate, SeekTo::Time {
                time: symphonia::core::units::Time::from(target.as_secs_f64()),
                track_id: Some(track_id),
            });
            let target_frame = (target.as_secs_f64() * sr as f64) as u64;
            state.current_frame.store(target_frame, Ordering::SeqCst);
            state.decoder_done.store(false, Ordering::SeqCst); // 如果 seek 发生在结束后，需要重置
        }
    }

fn decode_next_packet<P: Producer<Item = i32>>(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    producer: &mut P,
    state: &SharedState,
    channels: usize,
) -> bool {
    match format.next_packet() {
        Ok(packet) => {
            if packet.track_id() != track_id { return true; }
            if let Ok(decoded) = decoder.decode(&packet) {
                let spec = *decoded.spec();
                let num_frames = decoded.frames();
                let mut sample_buf = SampleBuffer::<i32>::new(num_frames as u64, spec);
                sample_buf.copy_interleaved_ref(decoded);

                Self::push_samples_blocking(producer, sample_buf.samples(), state, channels);
            }
            true
        }
        Err(symphonia::core::errors::Error::IoError(_)) => {
            std::thread::sleep(Duration::from_millis(100));
            true
        }
        Err(_) => false,
    }
}

fn push_samples_blocking<P: Producer<Item = i32>>(
    producer: &mut P,
    samples: &[i32],
    state: &SharedState,
    channels: usize,
) {
    // 确保样本数是通道数的整数倍
    let num_frames = samples.len() / channels;
    let valid_samples = num_frames * channels;

    let mut written = 0;
    while written < valid_samples {
        if state.is_terminating.load(Ordering::Relaxed) { break; }

        // 计算当前帧的起始位置
        let frame_start = (written / channels) * channels;
        let remaining = valid_samples - frame_start;

        // 尝试写入，但确保不会跨帧分割
        let to_write = remaining.min(producer.vacant_len());
        let frames_to_write = to_write / channels;
        let samples_to_write = frames_to_write * channels;

        if samples_to_write > 0 {
            let n = producer.push_slice(&samples[frame_start..frame_start + samples_to_write]);
            written = frame_start + n;
        }

        if written < valid_samples {
            std::thread::sleep(Duration::from_micros(500)); // 减少sleep时间
        }
    }
}

fn build_stream<T, C>(
    &self,
    config: &cpal::StreamConfig,
    mut consumer: C,
    state: Arc<SharedState>,
    channels: usize,
) -> Result<cpal::Stream, Box<dyn Error>>
where
    T: cpal::SizedSample + cpal::FromSample<i32>,
    C: Consumer<Item = i32> + Send + 'static,
{
    let stream = self.device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // 处理 discard 请求 - 确保丢弃到帧边界
            if state.discard_buffer.swap(false, Ordering::SeqCst) {
                let mut discarded = 0;
                while let Some(_) = consumer.try_pop() {
                    discarded += 1;
                }
                // 如果丢弃的样本数不是通道数的整数倍，继续丢弃到边界
                let remainder = discarded % channels;
                if remainder != 0 {
                    for _ in 0..(channels - remainder) {
                        consumer.try_pop();
                    }
                }
            }

            if state.is_paused.load(Ordering::Relaxed) {
                data.fill(T::EQUILIBRIUM);
                return;
            }

            let num_frames = data.len() / channels;
            let mut frames_read = 0;

            // 按帧读取
            for frame_idx in 0..num_frames {
                let base_idx = frame_idx * channels;

                // 先尝试读取第一个样本，确认缓冲区有数据
                if let Some(first_sample) = consumer.try_pop() {
                    data[base_idx] = T::from_sample(first_sample);

                    // 读取该帧的剩余通道
                    let mut all_channels_read = true;
                    for ch in 1..channels {
                        if let Some(sample) = consumer.try_pop() {
                            data[base_idx + ch] = T::from_sample(sample);
                        } else {
                            // 缓冲区在帧中间耗尽 - 这是不应该发生的情况
                            // 填充静音并记录警告
                            for remaining_ch in ch..channels {
                                data[base_idx + remaining_ch] = T::EQUILIBRIUM;
                            }
                            all_channels_read = false;
                            eprintln!("Warning: Buffer underrun mid-frame at channel {}/{}", ch, channels);
                            break;
                        }
                    }

                    if all_channels_read {
                        frames_read += 1;
                    } else {
                        // 帧不完整，填充剩余帧
                        for remaining_frame in (frame_idx + 1)..num_frames {
                            let idx = remaining_frame * channels;
                            for ch in 0..channels {
                                data[idx + ch] = T::EQUILIBRIUM;
                            }
                        }
                        break;
                    }
                } else {
                    // 缓冲区为空，填充静音
                    for remaining_frame in frame_idx..num_frames {
                        let idx = remaining_frame * channels;
                        for ch in 0..channels {
                            data[idx + ch] = T::EQUILIBRIUM;
                        }
                    }
                    break;
                }
            }

            // 检查是否播放结束
            if frames_read == 0 && state.decoder_done.load(Ordering::SeqCst) {
                if !state.is_finished.swap(true, Ordering::SeqCst) {
                    state.finish_notify.notify_waiters();
                }
            }

            state.current_frame.fetch_add(frames_read as u64, Ordering::Relaxed);
        },
        |err| eprintln!("Playback error: {}", err),
        None
    )?;
    Ok(stream)
}

  fn find_best_config(
      &self,
      target_sr: u32,
      channels: u16,
  ) -> Result<(cpal::StreamConfig, cpal::SampleFormat), Box<dyn Error>> {
      println!(
          "[audio] target request: sample_rate={}Hz, channels={}",
          target_sr, channels
      );

      let mut candidates = Vec::new();
      for c in self.device.supported_output_configs()? {
          if c.channels() == channels
              && target_sr >= c.min_sample_rate()
              && target_sr <= c.max_sample_rate()
          {
              println!(
                  "[audio] candidate: fmt={:?}, channels={}, sr_range={}~{}",
                  c.sample_format(),
                  c.channels(),
                  c.min_sample_rate(),
                  c.max_sample_rate()
              );
              candidates.push(c);
          }
      }

      let prefer = [
          cpal::SampleFormat::I32,
          cpal::SampleFormat::I16,
          cpal::SampleFormat::F32,
      ];

      for fmt in prefer {
          if let Some(c) = candidates.iter().find(|c| c.sample_format() == fmt) {
              println!(
                  "[audio] SELECTED: fmt={:?}, sample_rate={}Hz, channels={}",
                  fmt, target_sr, channels
              );
              let config: cpal::StreamConfig = c.with_sample_rate(target_sr).into();
              return Ok((config, fmt));
          }
      }

      println!("[audio] NO MATCH for requested config");
      Err("Hardware doesn't support file's SR/Channels".into())
  }

    // --- 控制 API ---

    pub fn pause(&self) { self.state.is_paused.store(true, Ordering::SeqCst); }
    pub fn resume(&self) { self.state.is_paused.store(false, Ordering::SeqCst); }
    pub fn progress(&self) -> Duration {
        let frames = self.state.current_frame.load(Ordering::Relaxed);
        let rate = self.state.sample_rate.load(Ordering::Relaxed);
        if rate == 0 { return Duration::ZERO; }
        Duration::from_secs_f64(frames as f64 / rate as f64)
    }
    pub fn seek(&self, time_secs: u32) {
        let mut seek_req = self.state.seek_request.lock().unwrap();
        *seek_req = Some(Duration::from_secs(time_secs as u64));
    }

    pub fn stop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.stream = None;
        self.state.current_frame.store(0, Ordering::SeqCst);
        self.state.is_paused.store(false, Ordering::SeqCst);
        let mut seek_req = self.state.seek_request.lock().unwrap();
        *seek_req = None;

        // 如果有任务在等待结束，手动停止也应触发通知避免死锁
        self.state.is_finished.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters();
    }

    /// 高性能异步等待：直到播放自然结束或被 stop
    pub async fn wait_finished(&self) {
        if self.state.is_finished.load(Ordering::Relaxed) {
            return;
        }
        self.state.finish_notify.notified().await;
    }

    pub fn is_finished(&self) -> bool {
        self.state.is_finished.load(Ordering::Relaxed)
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters(); // 唤醒可能的等待者
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::interval;

    use super::*;

    fn setup_player() -> Option<AudioPlayer> {
        match AudioPlayer::new(None) {
            Ok(p) => Some(p),
            Err(e) => {
                println!("Skipping test: No audio device available ({})", e);
                None
            }
        }
    }

    #[tokio::test]
    async fn test_play_until_end() {
        let mut player = match setup_player() {
            Some(p) => p,
            None => return,
        };

        let test_file = "/home/dddqmmx/Downloads/74cd_0b57_db2f_70dca450aaf3f1ae70e88507811644c9.flac"; // 请确保路径正确

        if std::path::Path::new(test_file).exists() {
            println!("Playing file...");
            player.play_file(test_file).await.unwrap();

            // 使用高性能等待
            println!("Waiting for audio to finish...");
            player.wait_finished().await;
            println!("Playback finished naturally.");
        }
    }

    #[tokio::test]
    async fn test_play_url_with_realtime_progress() -> Result<(), Box<dyn std::error::Error>> {
        // 1. 初始化播放器
        let mut player = match AudioPlayer::new(None) {
            Ok(p) => p,
            Err(e) => {
                println!("跳过测试：未找到音频输出设备 ({})", e);
                return Ok(());
            }
        };

        // 2. 使用一个公共的测试音频 URL (SoundHelix 是常用的测试源)
        let test_url = "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3";
        println!("正在请求 URL: {}", test_url);

        // 3. 开始播放
        // play_url 会在预加载（Prefetch）完成后返回
        player.play_url(test_url).await?;
        println!("播放器已启动...");

        // 4. 实时输出进度
        let mut timer = interval(Duration::from_millis(1000));
        let timeout = Duration::from_secs(30); // 我们只测试前 30 秒，防止测试运行时间过长
        let start_inst = std::time::Instant::now();

        loop {
            timer.tick().await;

            let progress = player.progress();
            let is_finished = player.is_finished();

            // 格式化输出进度
            println!(
                "进度: [{:02}:{:02}] | 是否结束: {}",
                progress.as_secs() / 60,
                progress.as_secs() % 60,
                is_finished
            );

            // 检查是否播放自然结束
            if is_finished {
                println!("检测到播放自然结束。");
                break;
            }

            // 达到测试设定的超时时间则强制退出
            if start_inst.elapsed() >= timeout {
                println!("已达到 30 秒测试时长限制，准备停止...");
                break;
            }
        }

        // 5. 停止播放
        player.stop();
        println!("测试完成。");

        Ok(())
    }
}
