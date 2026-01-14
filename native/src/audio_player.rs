use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use std::error::Error;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::storage::temp::TempStorageProvider;
use stream_download::{Settings, StreamDownload};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::Notify;

// --- 类型定义与辅助结构 ---

pub struct SeekableSource<R> {
    inner: R,
    len: Option<u64>, // 新增长度字段
}

impl<R: Read + Seek + Send + Sync> SeekableSource<R> {
    pub fn new(inner: R, len: Option<u64>) -> Self {
        Self { inner, len }
    }
}

impl<R: Read + Seek + Send + Sync> Read for SeekableSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read + Seek + Send + Sync> Seek for SeekableSource<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<R: Read + Seek + Send + Sync> MediaSource for SeekableSource<R> {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        self.len // 返回实际长度
    }
}


struct AudioMetadata {
    sample_rate: u32,
    channels: u16,
    track_id: u32,
    decoder: Box<dyn Decoder>,
    format_reader: Box<dyn FormatReader>,
}

pub(crate) struct SharedState {
    pub(crate) is_paused: AtomicBool,
    pub(crate) current_frame: AtomicU64,
    pub(crate) sample_rate: AtomicU32,
    pub(crate) channels: AtomicU32,          // 新：每帧的声道数
    pub(crate) has_seek_request: AtomicBool, // 新增：轻量级标记
    pub(crate) seek_request: Mutex<Option<Duration>>,
    pub(crate) is_terminating: AtomicBool,
    pub(crate) discard_buffer: AtomicBool,
    pub(crate) decoder_done: AtomicBool,
    pub(crate) is_finished: AtomicBool,
    pub(crate) finish_notify: Notify,
    pub(crate) buffered_frames: AtomicU64, // 新：缓冲的帧数（每帧含 channels 个采样）
    pub(crate) min_buffer_frames: AtomicU64, // 新：seek 后需要的最小帧数（阈值）
    pub(crate) waiting_for_seek: AtomicBool, // 新：seek 后等待缓冲完成标记
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
                channels: AtomicU32::new(2), // 新增字段，默认 2 声道
                seek_request: Mutex::new(None),
                is_terminating: AtomicBool::new(false),
                discard_buffer: AtomicBool::new(false),
                decoder_done: AtomicBool::new(false),
                is_finished: AtomicBool::new(false),
                finish_notify: Notify::new(),
                buffered_frames: AtomicU64::new(0),   // 新增字段
                min_buffer_frames: AtomicU64::new(0), // 新增字段
                waiting_for_seek: AtomicBool::new(false),
                has_seek_request: AtomicBool::new(false), // 新增字段
            }),
        })
    }

    pub fn get_state(&self) -> Arc<SharedState> {
        self.state.clone()
    }

    // --- 核心业务方法 ---

    pub async fn play_url(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        let extension = std::path::Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let stream = HttpStream::<Client>::create(url.parse()?).await?;
        let reader = StreamDownload::from_stream(
            stream,
            AdaptiveStorageProvider::new(
                TempStorageProvider::default(),
                NonZeroUsize::new(512 * 1024).unwrap(),
            ),
            Settings::default().prefetch_bytes(512 * 1024),
        )
        .await?;

        // 获取长度：StreamDownload 提供了 content_length()
        let content_len = reader.content_length();

        // 传入长度
        let source = Box::new(SeekableSource::new(reader, content_len));
        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta)
    }

    pub async fn play_file(&mut self, path: &str) -> Result<(), Box<dyn Error>> {
        let path_buf = std::path::Path::new(path);
        let extension = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let file = std::fs::File::open(path_buf)?;
        let source = Box::new(file);

        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta)
    }

    async fn spawn_probe_task(
        &self,
        source: Box<dyn MediaSource>,
        extension: Option<String>,
    ) -> Result<AudioMetadata, Box<dyn Error>> {
        tokio::task::spawn_blocking(move || Self::probe_source(source, extension))
            .await?
            .map_err(|e| e as Box<dyn std::error::Error>)
    }

    fn probe_source(
        source: Box<dyn MediaSource>,
        extension: Option<String>,
    ) -> Result<AudioMetadata, Box<dyn Error + Send + Sync>> {
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
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        Ok(AudioMetadata {
            sample_rate: sr,
            channels,
            track_id: track.id,
            decoder,
            format_reader: format,
        })
    }

    fn setup_and_play(&mut self, meta: AudioMetadata) -> Result<(), Box<dyn Error>> {
        self.stop(); // 停止并重置之前的所有状态

        self.state.is_terminating.store(false, Ordering::SeqCst);
        self.state.decoder_done.store(false, Ordering::SeqCst);
        self.state.is_finished.store(false, Ordering::SeqCst);

        let sr = meta.sample_rate;
        let channels = meta.channels;
        self.state.sample_rate.store(sr, Ordering::SeqCst);
        self.state.channels.store(channels as u32, Ordering::SeqCst);
        self.state.current_frame.store(0, Ordering::SeqCst);
        self.state.buffered_frames.store(0, Ordering::SeqCst);
        self.state.min_buffer_frames.store(0, Ordering::SeqCst);
        self.state.waiting_for_seek.store(false, Ordering::SeqCst);

        let (config, sample_format) = self.find_best_config(sr, channels)?;
        if config.sample_rate != sr {
            return Err(format!(
                "Sample-rate mismatch: file={}Hz, output={}Hz",
                sr, config.sample_rate
            )
            .into());
        }

        let rb = HeapRb::<i32>::new(sr as usize * channels as usize * 2); // 减小了些缓冲区，20秒稍大
        let (producer, consumer) = rb.split();

        self.start_decode_thread(meta, producer);

        let state_for_cb = self.state.clone();
        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                self.build_stream::<i16, _>(&config, consumer, state_for_cb, channels as usize)?
            }
            cpal::SampleFormat::I32 => {
                self.build_stream::<i32, _>(&config, consumer, state_for_cb, channels as usize)?
            }
            cpal::SampleFormat::F32 => {
                self.build_stream::<f32, _>(&config, consumer, state_for_cb, channels as usize)?
            }
            _ => return Err(format!("Unsupported sample format: {:?}", sample_format).into()),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn start_decode_thread(
        &self,
        meta: AudioMetadata,
        mut producer: impl Producer<Item = i32> + Send + 'static,
    ) {
        let state = self.state.clone();
        let mut decoder = meta.decoder;
        let mut format = meta.format_reader;
        let track_id = meta.track_id;
        let sr = meta.sample_rate;

        std::thread::spawn(move || {
            loop {
                if state.is_terminating.load(Ordering::Relaxed) {
                    break;
                }

                Self::handle_seek_if_needed(&state, &mut *format, &mut *decoder, track_id, sr);

                if !producer.is_full() {
                    // 如果 decode_next_packet 返回 false，说明文件读取完毕或出错
                    if !Self::decode_next_packet(
                        &mut *format,
                        &mut *decoder,
                        track_id,
                        &mut producer,
                        &state,
                    ) {
                        state.decoder_done.store(true, Ordering::SeqCst);
                        break;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
    }

    fn handle_seek_if_needed(
        state: &SharedState,
        format: &mut dyn FormatReader,
        decoder: &mut dyn Decoder,
        track_id: u32,
        sr: u32,
    ) {
        // 检查点 1: 尝试获取锁
        // println!("[Seek-Check] 正在尝试获取 Seek 锁...");
        let seek_req = {
            match state.seek_request.try_lock() {
                Ok(mut guard) => guard.take(),
                Err(_) => {
                    // 如果锁被主线程长期占用，这里会打印
                    println!("[Seek-Check] 警告：Seek 锁竞争中...");
                    return;
                }
            }
        };

        if let Some(target) = seek_req {
            println!(
                "[Seek-Check] >>> 收到 Seek 请求，目标时间: {:?} <<<",
                target
            );

            state.discard_buffer.store(true, Ordering::SeqCst);
            state.waiting_for_seek.store(true, Ordering::SeqCst);
            state.has_seek_request.store(false, Ordering::SeqCst); // 重置标记

            let _ = decoder.reset();

            // 检查点 2: 耗时最长的底层 Seek
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

            // 检查点 3: 重置状态
            state.buffered_frames.store(0, Ordering::SeqCst);
            let target_frame = (target.as_secs_f64() * sr as f64) as u64;
            state.current_frame.store(target_frame, Ordering::SeqCst);
            state.decoder_done.store(false, Ordering::SeqCst);
            println!("[Seek-Check] 状态重置完成，准备重新开始解码");
        }
    }

    fn decode_next_packet<P: Producer<Item = i32>>(
        format: &mut dyn FormatReader,
        decoder: &mut dyn Decoder,
        track_id: u32,
        producer: &mut P,
        state: &SharedState,
    ) -> bool {
        match format.next_packet() {
            Ok(packet) => {
                if packet.track_id() != track_id {
                    return true;
                }
                match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let num_frames = decoded.frames();
                        let mut sample_buf = SampleBuffer::<i32>::new(num_frames as u64, spec);
                        sample_buf.copy_interleaved_ref(decoded);
                        Self::push_samples_blocking(producer, sample_buf.samples(), state);
                    }
                    Err(symphonia::core::errors::Error::DecodeError(e)) => {
                        // Seek 之后经常会出现解码错误，跳过即可，不要退出线程
                        eprintln!("[Decoder] 解码包失败（跳过）: {}", e);
                    }
                    Err(_) => {
                        // 其他解析错误也尝试继续
                    }
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
                // 如果是真正的致命错误或 EOF
                println!("[Decoder] 停止解码: {:?}", e);
                false
            }
        }
    }
    fn push_samples_blocking<P: Producer<Item = i32>>(
        producer: &mut P,
        samples: &[i32],
        state: &SharedState,
    ) {
        let mut written = 0;
        let mut retry_count = 0;
        while written < samples.len() {
            if state.is_terminating.load(Ordering::Relaxed) {
                break;
            }

            // 检查点 4: 检查是否在推送旧数据时被新请求中断
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
                // 检查点 5: 如果 Buffer 满了且重试次数过多，打印状态
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
    fn build_stream<T, C>(
        &self,
        config: &cpal::StreamConfig,
        mut consumer: C,
        state: Arc<SharedState>,
        channels: usize,
    ) -> Result<cpal::Stream, Box<dyn Error>>
    where
        T: cpal::SizedSample + cpal::FromSample<i32>,
        C: Consumer<Item = i32> + Observer<Item = i32> + Send + 'static,
    {
        let stream = self.device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                // 1. 处理 Seek 触发的强制清理
                if state.discard_buffer.swap(false, Ordering::SeqCst) {
                    while consumer.try_pop().is_some() {}
                }

                let buffered_samples = consumer.occupied_len();
                let sr = state.sample_rate.load(Ordering::Relaxed) as usize;
                let decoder_done = state.decoder_done.load(Ordering::Relaxed);

                // 设定恢复播放所需的最小采样数（例如 1 秒的数据量）
                let min_samples_to_resume = sr * channels;

                // 2. 自动进入/退出“缓冲中”状态的逻辑
                let mut is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);

                if is_buffering {
                    // 关键：如果 decoder_done 是 true，说明后面没数据了，必须退出缓冲模式
                    if buffered_samples >= min_samples_to_resume || decoder_done {
                        state.waiting_for_seek.store(false, Ordering::Relaxed);
                        println!("[Audio] 缓冲结束，当前 Buffer: {} 采样", buffered_samples);
                    } else {
                        data.fill(T::EQUILIBRIUM);
                        return;
                    }
                }

                // 如果当前没在缓冲，但 Buffer 见底了（且还没播完），强制进入缓冲模式
                if !is_buffering && buffered_samples == 0 && !decoder_done {
                    state.waiting_for_seek.store(true, Ordering::Relaxed);
                    is_buffering = true;
                    // println!("[Audio] Buffer exhausted, re-buffering...");
                }

                // 如果正在缓冲中
                if is_buffering {
                    // 检查是否攒够了数据，或者文件已经解码完了（没法再攒了）
                    if buffered_samples >= min_samples_to_resume || decoder_done {
                        state.waiting_for_seek.store(false, Ordering::Relaxed);
                        // println!("[Audio] Buffering complete, resuming...");
                    } else {
                        // 数据不够，直接填静音并返回，不消费 consumer
                        data.fill(T::EQUILIBRIUM);
                        return;
                    }
                }

                // 3. 暂停处理
                if state.is_paused.load(Ordering::Relaxed) {
                    data.fill(T::EQUILIBRIUM);
                    return;
                }

                // 4. 正常消费数据
                let mut samples_read = 0usize;
                for sample in data.iter_mut() {
                    if let Some(s) = consumer.try_pop() {
                        *sample = T::from_sample(s);
                        samples_read += 1;
                    } else {
                        // Buffer 在消费过程中抽干了
                        *sample = T::EQUILIBRIUM;
                    }
                }

                // 5. 更新进度与结束判定
                if samples_read > 0 {
                    state
                        .current_frame
                        .fetch_add((samples_read / channels) as u64, Ordering::Relaxed);
                } else if decoder_done {
                    // 只有 Buffer 空了且解码器关了，才叫真播完了
                    if !state.is_finished.swap(true, Ordering::SeqCst) {
                        state.finish_notify.notify_waiters();
                    }
                }
            },
            |err| {
                // 此时 err 如果还是 Underrun，通常是驱动级别的警告，
                // 但因为我们填充了静音，它不会导致音频咔哒声或死锁。
                // eprintln!("Driver message: {}", err);
            },
            None,
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

    pub fn pause(&self) {
        self.state.is_paused.store(true, Ordering::SeqCst);
    }
    pub fn resume(&self) {
        self.state.is_paused.store(false, Ordering::SeqCst);
    }
    pub fn progress(&self) -> Duration {
        let frames = self.state.current_frame.load(Ordering::Relaxed);
        let rate = self.state.sample_rate.load(Ordering::Relaxed);
        if rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(frames as f64 / rate as f64)
    }
    // 修改 seek API：在外部调用时也设置 waiting_for_seek 以及清 0 buffered_frames
    pub fn seek(&self, time_secs: u32) {
        {
            // 使用花括号缩小锁的作用域
            let mut seek_req = self.state.seek_request.lock().unwrap();
            *seek_req = Some(Duration::from_secs(time_secs as u64));
        } // 锁在这里立即释放

        // 这个标记一定要在锁释放后立即设为 true
        self.state.has_seek_request.store(true, Ordering::SeqCst);
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
    use std::thread::sleep;

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

        let test_file =
            "/home/dddqmmx/Downloads/74cd_0b57_db2f_70dca450aaf3f1ae70e88507811644c9.flac"; // 请确保路径正确

        if std::path::Path::new(test_file).exists() {
            println!("Playing file...");
            player.play_file(test_file).await.unwrap();

            sleep(Duration::from_secs(3));
            player.seek(200);
            sleep(Duration::from_secs(3));

            player.seek(0);

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
