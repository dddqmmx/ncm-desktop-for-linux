use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::{Settings, StreamDownload};
use stream_download::storage::temp::TempStorageProvider;
use std::error::Error;
use std::fs::File;
use std::num::NonZeroUsize;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::io::{Read, Seek, SeekFrom};

pub struct SeekableSource<R> {
    inner: R,
}

impl<R> SeekableSource<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
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
        None
    }
}


// 共享状态
struct SharedState {
    is_paused: AtomicBool,
    current_frame: AtomicU64,
    sample_rate: AtomicU32,
    seek_request: Mutex<Option<Duration>>,
    is_terminating: AtomicBool,
    // 新增：用于通知回调线程立即清空缓冲区
    discard_buffer: AtomicBool,
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
                .find(|d| {
                    d.description()
                        .map(|desc| desc.name().contains(name))
                        .unwrap_or(false)
                })
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
            }),
        })
    }

    fn setup_and_play<R: MediaSource + 'static>(
        &mut self,
        source: R,
        extension: Option<String>,
    ) -> Result<(), Box<dyn Error>> {
        // 1. 停止旧流并重置状态
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.stream = None; // 这会丢弃旧的 cpal::Stream
        self.state.is_terminating.store(false, Ordering::SeqCst);
        self.state.discard_buffer.store(false, Ordering::SeqCst);

        let state = self.state.clone();

        // 2. 创建环形缓冲区 (缓冲约 1-2 秒)
        let rb = HeapRb::<f32>::new(44100 * 2 * 2);
        let (mut producer, mut consumer) = rb.split();

        // 3. 启动后台解码线程
        // === 替换后的解码线程部分（放在 setup_and_play 中） ===
        std::thread::spawn(move || {
            let mss = MediaSourceStream::new(Box::new(source), Default::default());
            let mut hint = Hint::new();
            if let Some(ext) = extension { hint.with_extension(&ext); }

            let probed = match symphonia::default::get_probe().format(
                &hint, mss, &FormatOptions::default(), &MetadataOptions::default()
            ) {
                Ok(p) => p,
                Err(e) => { eprintln!("Probe error: {}", e); return; }
            };

            let mut format = probed.format;
            let track = format.tracks().iter()
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL).unwrap();
            let track_id = track.id;
            let mut decoder = symphonia::default::get_codecs()
                .make(&track.codec_params, &DecoderOptions::default()).unwrap();

            let sr = track.codec_params.sample_rate.unwrap_or(44100);
            state.sample_rate.store(sr, Ordering::SeqCst);
            let channels = track.codec_params.channels.unwrap().count();

            loop {
                if state.is_terminating.load(Ordering::Relaxed) { break; }

                // --- 处理 Seek 请求 ---
                let seek_req = { state.seek_request.lock().unwrap().take() };
                if let Some(target) = seek_req {
                    // 先重置解码器状态
                    let _ = decoder.reset();
                    // 标记回调线程清空缓冲区（回调看到后会清掉 consumer 的数据）
                    state.discard_buffer.store(true, Ordering::SeqCst);

                    // 执行 seek（以时间为单位）
                    let seek_to = SeekTo::Time {
                        time: symphonia::core::units::Time::from(target.as_secs_f64()),
                        track_id: Some(track_id),
                    };
                    match format.seek(SeekMode::Accurate, seek_to) {
                        Ok(_seeked) => {
                            // 关键：把进度写成“帧数”，不要直接用 seeked.actual_ts（单位可能不同）
                            let target_frames = (target.as_secs_f64() * sr as f64).round() as u64;
                            state.current_frame.store(target_frames, Ordering::SeqCst);
                        }
                        Err(e) => {
                            eprintln!("Seek failed: {:?}", e);
                        }
                    }
                }

                // --- 解码数据 ---
                if !producer.is_full() {
                    match format.next_packet() {
                        Ok(packet) => {
                            if packet.track_id() != track_id { continue; }
                            match decoder.decode(&packet) {
                                Ok(decoded) => {
                                    let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                                    sample_buf.copy_interleaved_ref(decoded);
                                    let samples = sample_buf.samples();

                                    let mut written = 0;
                                    while written < samples.len() {
                                        if state.is_terminating.load(Ordering::Relaxed) { return; }
                                        // 如果在写入中途收到新 Seek 请求，立即中断当前包的推送
                                        if state.seek_request.lock().unwrap().is_some() { break; }

                                        let n = producer.push_slice(&samples[written..]);
                                        written += n;
                                        if n == 0 { std::thread::sleep(Duration::from_millis(10)); }
                                    }
                                }
                                Err(e) => { eprintln!("Decode error: {}", e); }
                            }
                        }
                        Err(_) => { // 文件/流末尾，稍微等待
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        });


        // 4. CPAL 音频流回调
        let config = self.device.default_output_config()?.config();
        let channels = config.channels as usize;
        let state_for_cb = self.state.clone();

        let stream = self.device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // 关键修复点 4: 检测并执行冲刷操作
                if state_for_cb.discard_buffer.swap(false, Ordering::SeqCst) {
                    // 消费掉所有现有数据，清空缓冲区
                    let _ = consumer.pop_iter().count();
                }

                if state_for_cb.is_paused.load(Ordering::Relaxed) {
                    data.fill(0.0);
                    return;
                }

                let read = consumer.pop_slice(data);
                if read < data.len() {
                    data[read..].fill(0.0);
                }

                // 更新播放进度
                let frames = (read / channels) as u64;
                state_for_cb.current_frame.fetch_add(frames, Ordering::Relaxed);
            },
            |err| eprintln!("Playback error: {}", err),
            None
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub async fn play_url(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        let extension = std::path::Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let settings = Settings::default().prefetch_bytes(512 * 1024);
        let stream = HttpStream::<Client>::create(url.parse()?).await?;

        let reader = StreamDownload::from_stream(
            stream,
            AdaptiveStorageProvider::new(
                TempStorageProvider::default(),
                NonZeroUsize::new(512 * 1024).unwrap(),
            ),
            settings,
        ).await?;

        let source = SeekableSource::new(reader);
        // ✅ 直接传 reader
        self.setup_and_play(source, extension)
    }


    pub fn play_file(&mut self, path: &str) -> Result<(), Box<dyn Error>> {
        let extension = std::path::Path::new(path)
            .extension().and_then(|s| s.to_str()).map(|s| s.to_string());
        let file = File::open(path)?;
        self.setup_and_play(file, extension)
    }

    pub fn pause(&self) { self.state.is_paused.store(true, Ordering::SeqCst); }
    pub fn resume(&self) { self.state.is_paused.store(false, Ordering::SeqCst); }
    pub fn stop(&mut self) { self.stream = None; }

    pub fn progress(&self) -> Duration {
        let frames = self.state.current_frame.load(Ordering::Relaxed);
        let rate = self.state.sample_rate.load(Ordering::Relaxed);
        if rate == 0 { return Duration::ZERO; }
        Duration::from_secs_f64(frames as f64 / rate as f64)
    }

    pub fn seek(&self, time: u32) {
        let mut seek_req = self.state.seek_request.lock().unwrap();
        *seek_req = Some(Duration::from_secs(time as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    // 辅助函数：尝试初始化播放器，如果环境不支持音频设备则跳过测试
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
    async fn test_play_and_controls() {
        let mut player = match setup_player() {
            Some(p) => p,
            None => return,
        };

        let test_url = "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3";
        // let test_file = "/home/dddqmmx/Music/test.flac";

        println!("Attempting to play URL...");
        let result = player.play_url(test_url).await;
        // let result = player.play_file(test_file);

        // 必须让线程休眠，否则进程直接结束
        println!("Playing...");

        assert!(result.is_ok(), "Play URL failed: {:?}", result.err());

        sleep(Duration::from_secs(3)).await;
        println!("try seek...");
        player.seek(60);
        sleep(Duration::from_secs(2)).await; // 等待 2 秒
        println!("time...{:?}", player.progress()); // 预期结果 62s

        println!("try seek...");
        player.seek(0);
        sleep(Duration::from_secs(2)).await; // 等待 2 秒
        println!("time...{:?}", player.progress()); //预期结果  2s

        sleep(Duration::from_secs(0)).await; // 等待 2 秒
    }

    // #[tokio::test]
    // async fn test_invalid_url() {
    //     let mut player = match setup_player() {
    //         Some(p) => p,
    //         None => return,
    //     };

    //     // 测试无效的 URL
    //     let result = player.play_url("http://invalid.url/audio.mp3").await;
    //     assert!(result.is_err(), "Should fail with invalid URL");
    // }

    #[tokio::test]
    async fn test_file_not_found() {
        let mut player = match setup_player() {
            Some(p) => p,
            None => return,
        };

        let result = player.play_file("non_existent_file.mp3");
        assert!(result.is_err(), "Should fail when file does not exist");
    }
}


fn main () {

}
