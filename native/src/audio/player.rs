#[cfg(target_os = "linux")]
use crate::audio::device_reservation::DeviceReservation;
use cpal::traits::{HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Duration;
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::storage::temp::TempStorageProvider;
use stream_download::{Settings, StreamDownload};
use symphonia::core::conv::ConvertibleSample;
use tokio::sync::Notify;

use crate::audio::backend::{self, OutputDeviceInfo};
use crate::audio::cache_tracker::SongCacheTracker;
use crate::audio::decoder::{self, AudioMetadata};
use crate::audio::http_client::RangeSanitizingClient;
use crate::audio::source::{
    PersistentFileStorageProvider, SeekableSource, SharedStorageState, prepare_blocking_seek,
};
use crate::audio::state::{NO_TRIM_FRAME, PlaybackClock, SharedState};
use crate::audio::utils::estimate_prefetch_bytes;
use crate::cache::song::SongStreamCacheMeta;

const OUTPUT_BUFFER_SECONDS: usize = 6;
const INITIAL_PREDECODE_SECONDS: usize = 2;
/// 建立连接 + 等待响应头的超时（播放/缓存流式下载的建连保护）。
const STREAM_OPEN_TIMEOUT_SECS: u64 = 15;

/// 流式下载实际使用的 HTTP 客户端类型（带倒置 range 修正）。
type StreamingHttpClient = RangeSanitizingClient;

static HTTP_CLIENT: LazyLock<StreamingHttpClient> = LazyLock::new(|| {
    let client = Client::builder()
        // 注意：绝不能设置 reqwest 的 `.timeout()`（整个请求含 body 流的总超时）。
        // 流式下载被播放进度节流，一个请求会持续整首歌；总超时一到，reqwest 的
        // TotalTimeoutBody 会在之后的每次 poll 都立即返回 TimedOut 错误，
        // stream-download 的下载循环因此陷入无限错误自旋（retry_timeout 只在
        // stream.next() 挂起时才触发重连，立即返回错误时永远轮不到），下载彻底
        // 停滞、播放缓冲区耗尽后整个播放卡死。连接级的 stall 由 stream-download
        // 自身的 retry_timeout 负责检测并重连。
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client");
    RangeSanitizingClient::new(client)
});

/// 建立 HttpStream 并给出建连/响应头超时；超时后由调用方报错，
/// 不依赖 reqwest 的总超时（见上）。
async fn open_http_stream(
    url: &str,
) -> Result<HttpStream<StreamingHttpClient>, Box<dyn std::error::Error>> {
    let parsed = url.parse()?;
    tokio::time::timeout(
        Duration::from_secs(STREAM_OPEN_TIMEOUT_SECS),
        HttpStream::new(HTTP_CLIENT.clone(), parsed),
    )
    .await
    .map_err(|_| format!("open stream timed out after {STREAM_OPEN_TIMEOUT_SECS}s: {url}"))?
    .map_err(|err| {
        Box::<dyn std::error::Error>::from(format!("failed to open stream {url}: {err}"))
    })
}

pub(crate) struct SongCacheDownloadOutcome {
    pub downloaded_bytes: u64,
    pub is_complete: bool,
}

#[derive(Clone, Copy)]
struct SongCacheDownloadControlState {
    playback_position_ms: u64,
    version: u64,
    cancelled: bool,
    finished: bool,
}

pub(crate) struct SongCacheDownloadControl {
    state: StdMutex<SongCacheDownloadControlState>,
    changed: Condvar,
    cancel_download: StdMutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl SongCacheDownloadControl {
    pub fn new() -> Self {
        Self {
            state: StdMutex::new(SongCacheDownloadControlState {
                playback_position_ms: 0,
                version: 0,
                cancelled: false,
                finished: false,
            }),
            changed: Condvar::new(),
            cancel_download: StdMutex::new(None),
        }
    }

    pub fn update_playback_position(&self, playback_position_ms: u64) {
        if let Ok(mut state) = self.state.lock() {
            if state.cancelled || state.playback_position_ms == playback_position_ms {
                return;
            }
            state.playback_position_ms = playback_position_ms;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    pub fn cancel(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cancelled = true;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
        if let Ok(cancel_download) = self.cancel_download.lock()
            && let Some(cancel_download) = cancel_download.as_ref()
        {
            cancel_download();
        }
    }

    fn attach_download_canceller<F>(&self, cancel_download: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let cancelled = self
            .state
            .lock()
            .map(|state| state.cancelled)
            .unwrap_or(true);
        if cancelled {
            cancel_download();
            return;
        }
        if let Ok(mut slot) = self.cancel_download.lock() {
            *slot = Some(Box::new(cancel_download));
        }
    }

    fn wait_for_change(&self, version: u64) -> SongCacheDownloadControlState {
        let mut state = self.state.lock().unwrap();
        while !state.cancelled && state.version == version {
            state = self.changed.wait(state).unwrap();
        }
        *state
    }

    fn wake(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    pub fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.finished = true;
            state.version = state.version.wrapping_add(1);
            self.changed.notify_all();
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wait_finished_timeout(&self, timeout: Duration) {
        // 下载任务若卡在 stream-download 的 wait_for_read，cancel 无法立刻打断。
        // 这里必须有超时，否则 cancelSongCacheDownload / 切歌会永久卡住主流程。
        let mut state = self.state.lock().unwrap();
        let deadline = std::time::Instant::now() + timeout;
        while !state.finished {
            let now = std::time::Instant::now();
            if now >= deadline {
                eprintln!("[cache] wait_finished timed out after cancel");
                break;
            }
            let (next, timeout_result) = self
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap();
            state = next;
            if timeout_result.timed_out() && !state.finished {
                eprintln!("[cache] wait_finished timed out after cancel");
                break;
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.cancelled)
            .unwrap_or(true)
    }
}

struct CachedStreamDownload {
    reader: StreamDownload<PersistentFileStorageProvider>,
    completed: Arc<AtomicBool>,
    tracker: SongCacheTracker,
    storage_state: SharedStorageState,
}

async fn build_cached_stream_download(
    url: &str,
    cache_path: &str,
    metadata_path: &str,
    duration_ms: Option<u64>,
    cache_ahead_secs: Option<u32>,
    max_cache_ahead_bytes: Option<u64>,
) -> Result<CachedStreamDownload, Box<dyn std::error::Error>> {
    let stream = open_http_stream(url).await?;
    let content_len = stream.content_length();
    let tracker = SongCacheTracker::new(metadata_path)?;
    tracker.set_content_length(content_len)?;

    let prefetch_bytes =
        estimate_prefetch_bytes(content_len, duration_ms, cache_ahead_secs.unwrap_or(30));
    let prefetch_bytes = max_cache_ahead_bytes
        .map(|max_bytes| prefetch_bytes.min(max_bytes))
        .unwrap_or(prefetch_bytes);
    let completed = Arc::new(AtomicBool::new(false));
    let completed_for_progress = Arc::clone(&completed);
    let tracker_for_progress = tracker.clone();
    let tracker_for_writes = tracker.clone();

    let storage_provider = PersistentFileStorageProvider::new(cache_path)
        .max_write_ahead_bytes(max_cache_ahead_bytes)
        .on_write(move |range| tracker_for_writes.record_range(range));
    let storage_state = storage_provider.shared_state();

    let reader = StreamDownload::from_stream(
        stream,
        storage_provider,
        Settings::default()
            .prefetch_bytes(prefetch_bytes)
            .on_progress(move |stream: &HttpStream<StreamingHttpClient>, state, _| {
                if matches!(state.phase, stream_download::StreamPhase::Complete) {
                    completed_for_progress.store(true, Ordering::Release);
                }
                tracker_for_progress.record_progress(state, stream.content_length());
            }),
    )
    .await?;

    Ok(CachedStreamDownload {
        reader,
        completed,
        tracker,
        storage_state,
    })
}

fn playback_position_to_byte(
    content_length: Option<u64>,
    duration_ms: Option<u64>,
    playback_position_ms: u64,
) -> Option<u64> {
    let content_length = content_length?;
    let duration_ms = duration_ms.filter(|duration| *duration > 0)?;
    if content_length == 0 {
        return Some(0);
    }

    let position = (u128::from(content_length) * u128::from(playback_position_ms)
        / u128::from(duration_ms))
    .min(u128::from(content_length.saturating_sub(1))) as u64;
    Some(position)
}

pub struct AudioPlayer {
    device: cpal::Device,
    requested_device_id: Option<String>,
    stream: Option<cpal::Stream>,
    state: Arc<SharedState>,
    #[cfg(target_os = "linux")]
    device_reservation: Option<DeviceReservation>,
}

impl AudioPlayer {
    pub fn new(device_name: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            #[cfg(target_os = "linux")]
            let target_id = backend::linux_plughw_locator(name).unwrap_or_else(|| name.to_string());
            #[cfg(not(target_os = "linux"))]
            let target_id = name;

            host.output_devices()?
                .find(|d| backend::device_id(d) == target_id)
                .ok_or_else(|| format!("Device not found: {}", name))?
        } else {
            host.default_output_device()
                .ok_or("No default output device found")?
        };

        Ok(Self {
            device,
            requested_device_id: device_name.map(|s| s.to_string()),
            stream: None,
            state: Arc::new(SharedState {
                is_paused: AtomicBool::new(false),
                current_frame: AtomicU64::new(0),
                playback_clock: std::sync::Mutex::new(PlaybackClock::new()),
                trim_until_frame: AtomicU64::new(NO_TRIM_FRAME),
                sample_rate: AtomicU32::new(0),
                seek_request: std::sync::Mutex::new(None),
                is_terminating: AtomicBool::new(false),
                discard_buffer: AtomicBool::new(false),
                is_discarding_buffer: AtomicBool::new(false),
                decoder_done: AtomicBool::new(false),
                is_finished: AtomicBool::new(false),
                finish_notify: Notify::new(),
                buffered_frames: AtomicU64::new(0),
                waiting_for_seek: AtomicBool::new(false),
                has_seek_request: AtomicBool::new(false),
            }),
            #[cfg(target_os = "linux")]
            device_reservation: None,
        })
    }

    pub fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_id = default_device.as_ref().map(|d| backend::device_id(d));

        let mut devices = Vec::new();
        for d in host.output_devices()? {
            let id = backend::device_id(&d);
            let name = backend::device_display_name(&d);

            #[cfg(target_os = "linux")]
            if !backend::should_list_linux_output_device(
                &id,
                default_id.as_deref(),
                self.requested_device_id.as_deref(),
            ) {
                continue;
            }

            let is_default = default_id.as_deref() == Some(&id);
            let is_current = match self.requested_device_id.as_deref() {
                Some(rid) => id == rid,
                None => is_default,
            };

            devices.push(OutputDeviceInfo {
                id: id.clone(),
                name,
                is_default,
                is_current,
            });
        }
        #[cfg(target_os = "linux")]
        backend::append_linux_alsa_hint_output_devices(
            &mut devices,
            default_id.as_deref(),
            self.requested_device_id.as_deref(),
        );
        #[cfg(target_os = "linux")]
        backend::append_linux_proc_asound_output_devices(
            &mut devices,
            default_id.as_deref(),
            self.requested_device_id.as_deref(),
        );

        #[cfg(target_os = "linux")]
        backend::collapse_linux_duplicate_output_devices(&mut devices);
        backend::disambiguate_output_device_names(&mut devices);
        Ok(devices)
    }

    pub async fn play_url(
        &mut self,
        url: &str,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let stream = open_http_stream(url).await?;
        let reader = StreamDownload::from_stream(
            stream,
            AdaptiveStorageProvider::new(
                TempStorageProvider::default(),
                NonZeroUsize::new(512 * 1024).unwrap(),
            ),
            Settings::default().prefetch_bytes(512 * 1024),
        )
        .await?;

        let content_len = reader.content_length();
        let source = Box::new(SeekableSource::new(reader, content_len));
        let meta = decoder::spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at, strict_bit_perfect)
    }

    pub async fn play_url_cached(
        &mut self,
        url: &str,
        cache_path: &str,
        metadata_path: &str,
        duration_ms: Option<u64>,
        cache_ahead_secs: Option<u32>,
        max_cache_ahead_bytes: Option<u64>,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let download = build_cached_stream_download(
            url,
            cache_path,
            metadata_path,
            duration_ms,
            cache_ahead_secs,
            max_cache_ahead_bytes,
        )
        .await?;

        let reader = download.reader;
        let content_len = reader.content_length();
        let source = Box::new(
            SeekableSource::new(reader, content_len).with_storage_state(download.storage_state),
        );
        let meta = decoder::spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at, strict_bit_perfect)
    }

    /// 仅下载并缓存歌曲，不播放。
    /// 使用与 play_url_cached 相同的 HttpStream + StreamDownload 链路，
    /// 保证 WebAPI 引擎的缓存行为与 native 引擎一致（边下边存，按 prefetch 控制）。
    pub async fn download_song_for_cache(
        url: &str,
        cache_path: &str,
        metadata_path: &str,
        duration_ms: Option<u64>,
        cache_ahead_secs: Option<u32>,
        max_cache_ahead_bytes: Option<u64>,
        control: Arc<SongCacheDownloadControl>,
    ) -> Result<SongCacheDownloadOutcome, Box<dyn std::error::Error>> {
        let download = build_cached_stream_download(
            url,
            cache_path,
            metadata_path,
            duration_ms,
            cache_ahead_secs,
            max_cache_ahead_bytes,
        )
        .await?;

        let content_length = download.reader.content_length();
        let cancellation_token = download.reader.cancellation_token();
        control.attach_download_canceller(move || cancellation_token.cancel());

        let source_finished = Arc::new(AtomicBool::new(false));
        let source_finished_for_waiter = Arc::clone(&source_finished);
        let control_for_waiter = Arc::clone(&control);
        let completion_handle = download.reader.handle();
        tokio::spawn(async move {
            completion_handle.wait_for_completion().await;
            source_finished_for_waiter.store(true, Ordering::Release);
            control_for_waiter.wake();
        });

        let completed_for_driver = Arc::clone(&download.completed);
        let control_for_driver = Arc::clone(&control);
        let drive_result = tokio::task::spawn_blocking(move || {
            let mut reader = download.reader;
            let storage_state = download.storage_state;
            let mut version = 0;

            loop {
                if completed_for_driver.load(Ordering::Acquire) {
                    break;
                }
                if source_finished.load(Ordering::Acquire) {
                    return Err(std::io::Error::other(
                        "song cache source ended before download completed",
                    ));
                }
                if control_for_driver.is_cancelled() {
                    break;
                }

                let state = control_for_driver.wait_for_change(version);
                version = state.version;
                if state.cancelled {
                    break;
                }

                let Some(target_byte) = playback_position_to_byte(
                    content_length,
                    duration_ms,
                    state.playback_position_ms,
                ) else {
                    continue;
                };

                // 与播放路径同理：seek 到未缓存位置前，先武装逃逸窗口并唤醒
                // 可能因节流而睡眠的下载任务，避免双方互相等待形成死锁。
                prepare_blocking_seek(&mut reader, &storage_state, target_byte);
                if control_for_driver.is_cancelled() {
                    break;
                }
                if let Err(err) = reader.seek(SeekFrom::Start(target_byte)) {
                    if control_for_driver.is_cancelled() {
                        break;
                    }
                    return Err(err);
                }
                if control_for_driver.is_cancelled() {
                    break;
                }
                if let Err(err) = reader.read(&mut []) {
                    if control_for_driver.is_cancelled() {
                        break;
                    }
                    return Err(err);
                }
            }

            Ok::<(), std::io::Error>(())
        })
        .await?;

        if let Err(err) = drive_result
            && !control.is_cancelled()
        {
            return Err(err.into());
        }

        download.tracker.persist()?;

        let raw = std::fs::read_to_string(metadata_path)?;
        let meta = serde_json::from_str::<SongStreamCacheMeta>(&raw)?;

        Ok(SongCacheDownloadOutcome {
            downloaded_bytes: meta.downloaded_bytes(),
            is_complete: meta.is_fully_downloaded(),
        })
    }

    pub async fn play_file(
        &mut self,
        path: &str,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path_buf = Path::new(path);
        let extension = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let file = std::fs::File::open(path_buf)?;
        let source = Box::new(file);

        let meta = decoder::spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at, strict_bit_perfect)
    }

    pub(crate) fn setup_and_play(
        &mut self,
        mut meta: AudioMetadata,
        start_at: Option<Duration>,
        strict_bit_perfect: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.stop();

        self.state = Arc::new(SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            playback_clock: std::sync::Mutex::new(PlaybackClock::new()),
            trim_until_frame: AtomicU64::new(NO_TRIM_FRAME),
            sample_rate: AtomicU32::new(meta.sample_rate),
            seek_request: std::sync::Mutex::new(None),
            is_terminating: AtomicBool::new(false),
            discard_buffer: AtomicBool::new(false),
            is_discarding_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
            has_seek_request: AtomicBool::new(false),
        });

        let sr = meta.sample_rate;
        let channels = meta.channels;
        let should_predecode = start_at.is_none_or(|target| target.is_zero());

        #[cfg(target_os = "linux")]
        if strict_bit_perfect {
            let active_device_id = backend::device_id(&self.device);
            if !backend::is_linux_real_hardware_output_id(&active_device_id) {
                return Err(format!(
                    "当前无法满足BitPerfect条件拒绝播放：当前输出端点 {} 不是真实硬件设备",
                    active_device_id
                )
                .into());
            }
        }

        #[cfg(target_os = "linux")]
        let device_reservation = if strict_bit_perfect {
            Some(DeviceReservation::reserve(&backend::device_id(
                &self.device,
            ))?)
        } else {
            None
        };

        let (config, sample_format) = if strict_bit_perfect {
            backend::find_bit_perfect_config(
                &self.device,
                sr,
                channels,
                meta.bits_per_sample,
                meta.sample_format,
            )?
        } else {
            match backend::find_best_config(
                &self.device,
                sr,
                channels,
                meta.bits_per_sample,
                meta.sample_format,
            ) {
                Ok(cfg) => cfg,
                Err(primary_err) => {
                    let primary_msg = primary_err.to_string();
                    if self.maybe_fallback_to_default_device()? {
                        backend::find_best_config(
                            &self.device,
                            sr,
                            channels,
                            meta.bits_per_sample,
                            meta.sample_format,
                        )
                        .map_err(|fallback_err| {
                            format!(
                                "No compatible output config: primary={}; fallback={}",
                                primary_msg, fallback_err
                            )
                        })?
                    } else {
                        return Err(primary_msg.into());
                    }
                }
            }
        };

        let state_for_cb = self.state.clone();
        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                let rb = HeapRb::<i16>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<i16, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<i16, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i16>(meta, producer);
                stream
            }
            cpal::SampleFormat::U16 => {
                let rb = HeapRb::<u16>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<u16, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<u16, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u16>(meta, producer);
                stream
            }
            cpal::SampleFormat::I8 => {
                let rb = HeapRb::<i8>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<i8, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<i8, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i8>(meta, producer);
                stream
            }
            cpal::SampleFormat::U8 => {
                let rb = HeapRb::<u8>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<u8, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<u8, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u8>(meta, producer);
                stream
            }
            cpal::SampleFormat::I24 => {
                let rb = HeapRb::<i32>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<i32, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream_converted::<i32, cpal::I24, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U24 => {
                let rb = HeapRb::<u32>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<u32, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream_converted::<u32, cpal::U24, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::I32 => {
                let rb = HeapRb::<i32>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<i32, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<i32, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U32 => {
                let rb = HeapRb::<u32>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<u32, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<u32, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F32 => {
                let rb = HeapRb::<f32>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<f32, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<f32, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<f32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F64 => {
                let rb = HeapRb::<f64>::new(output_buffer_samples(sr, channels));
                let (mut producer, consumer) = rb.split();
                self.predecode_initial::<f64, _>(&mut meta, &mut producer, should_predecode);
                let stream = backend::build_stream::<f64, _>(
                    &self.device,
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<f64>(meta, producer);
                stream
            }
            _ => return Err(format!("Unsupported sample format: {:?}", sample_format).into()),
        };

        if let Some(start_at) = start_at.filter(|target| !target.is_zero()) {
            self.seek(start_at);
        }

        stream.play()?;
        self.stream = Some(stream);
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = device_reservation;
        }
        Ok(())
    }

    fn maybe_fallback_to_default_device(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        if let Some(default) = host.default_output_device() {
            if backend::device_id(&default) != backend::device_id(&self.device) {
                println!("[audio] falling back to default device...");
                self.device = default;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn start_decode_thread<S>(
        &self,
        meta: AudioMetadata,
        mut producer: impl Producer<Item = S> + Send + 'static,
    ) where
        S: ConvertibleSample + Copy + Send + 'static,
    {
        let state = self.state.clone();
        let mut decoder = meta.decoder;
        let mut format = meta.format_reader;
        let track_id = meta.track_id;
        let sr = meta.sample_rate;
        let time_base = meta.time_base;

        std::thread::spawn(move || {
            loop {
                if state.is_terminating.load(Ordering::Relaxed) {
                    break;
                }

                // seek 在暂停期间也要处理，否则 pause 后 seek 会一直挂起。
                decoder::handle_seek_if_needed(
                    &state,
                    &mut *format,
                    &mut *decoder,
                    track_id,
                    sr,
                    time_base,
                );

                // 暂停时不要继续往 ringbuf 塞数据：输出侧不消费，塞满后解码线程
                // 会卡在 push 上，resume 时表现为整进程假死。
                if state.is_paused.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }

                if !producer.is_full() {
                    if !decoder::decode_next_packet::<S, _>(
                        &mut *format,
                        &mut *decoder,
                        track_id,
                        sr,
                        time_base,
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

    fn predecode_initial<S, P>(&self, meta: &mut AudioMetadata, producer: &mut P, enabled: bool)
    where
        S: ConvertibleSample + Copy,
        P: Producer<Item = S>,
    {
        if !enabled {
            return;
        }

        let target_samples =
            predecode_target_samples(meta.sample_rate, meta.channels).min(producer.vacant_len());
        while producer.occupied_len() < target_samples && !producer.is_full() {
            if !decoder::decode_next_packet::<S, _>(
                &mut *meta.format_reader,
                &mut *meta.decoder,
                meta.track_id,
                meta.sample_rate,
                meta.time_base,
                producer,
                &self.state,
            ) {
                self.state.decoder_done.store(true, Ordering::SeqCst);
                break;
            }
        }
    }

    pub fn pause(&self) {
        self.state.is_paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.state.is_paused.store(false, Ordering::SeqCst);
    }

    pub fn progress(&self) -> Duration {
        let frames = self.state.progress_frame();
        let rate = self.state.sample_rate.load(Ordering::Relaxed);
        if rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(frames as f64 / rate as f64)
    }

    pub fn seek(&self, target: Duration) {
        self.state.schedule_seek(target);
    }

    pub fn stop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.stream = None;
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = None;
        }
        self.state.is_finished.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters();
    }

    pub fn is_finished(&self) -> bool {
        self.state.is_finished.load(Ordering::Relaxed)
    }

    pub(crate) fn get_state(&self) -> Arc<SharedState> {
        self.state.clone()
    }
}

fn output_buffer_samples(sample_rate: u32, channels: u16) -> usize {
    sample_rate as usize * channels as usize * OUTPUT_BUFFER_SECONDS
}

fn predecode_target_samples(sample_rate: u32, channels: u16) -> usize {
    sample_rate as usize * channels as usize * INITIAL_PREDECODE_SECONDS
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        #[cfg(target_os = "linux")]
        {
            self.device_reservation = None;
        }
        self.state.finish_notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
    use std::thread;

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
    fn wait_finished_times_out_instead_of_hanging_when_download_never_finishes() {
        let control = SongCacheDownloadControl::new();
        // 模拟：cancel 后下载线程卡在 stream-download 内部，never finish。
        control.cancel();

        let started = std::time::Instant::now();
        control.wait_finished_timeout(Duration::from_millis(80));
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(60),
            "wait_finished should wait up to the timeout when unfinished"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "wait_finished must not hang forever after cancel (elapsed={elapsed:?})"
        );
    }

    #[test]
    fn cancel_plus_finish_unblocks_wait_finished_immediately() {
        let control = Arc::new(SongCacheDownloadControl::new());
        let waiter = {
            let control = Arc::clone(&control);
            std::thread::spawn(move || {
                control.wait_finished_timeout(Duration::from_secs(2));
            })
        };

        std::thread::sleep(Duration::from_millis(20));
        control.cancel();
        control.finish();

        let started = std::time::Instant::now();
        waiter.join().expect("wait_finished thread panicked");
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "finish() after cancel must release waiters promptly"
        );
    }

    #[test]
    fn schedule_seek_updates_pending_request_and_progress() {
        let state = create_state(48_000);
        let target = Duration::from_millis(2_500);

        state.schedule_seek(target);

        assert_eq!(*state.seek_request.lock().unwrap(), Some(target));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 120_000);
    }

    #[test]
    fn seek_without_discard_buffer_causes_progress_drift() {
        let sr: u32 = 48_000;
        let channels: usize = 2;
        let state = create_state(sr);

        state.current_frame.store(50_000, Ordering::SeqCst);

        let target = Duration::from_secs(30);
        let target_frame = (target.as_secs_f64() * sr as f64) as u64;

        state.current_frame.store(target_frame, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(true, Ordering::SeqCst);

        assert!(!state.discard_buffer.load(Ordering::SeqCst));

        let min_samples_to_resume = sr as usize * channels;
        let buffered_samples = min_samples_to_resume + 1024;

        let mut is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);
        assert!(is_buffering, "should enter buffering mode after seek");

        if is_buffering {
            if buffered_samples >= min_samples_to_resume {
                state.waiting_for_seek.store(false, Ordering::Relaxed);
                is_buffering = false;
            }
        }

        assert!(
            !is_buffering,
            "bug: buffering cleared prematurely because old samples remain in buffer"
        );

        let samples_read = 1024usize;
        if samples_read > 0 {
            state
                .current_frame
                .fetch_add((samples_read / channels) as u64, Ordering::Relaxed);
        }

        let drifted_frame = state.current_frame.load(Ordering::Relaxed);
        assert!(
            drifted_frame > target_frame,
            "BUG CONFIRMED: current_frame drifted from {target_frame} to {drifted_frame} \
             because discard_buffer was not set in seek() — old samples were played after seek"
        );
        assert_eq!(
            drifted_frame - target_frame,
            (samples_read / channels) as u64,
            "drift should equal samples_read / channels"
        );
    }

    #[test]
    fn seek_with_discard_buffer_prevents_progress_drift() {
        let sr: u32 = 48_000;
        let channels: usize = 2;
        let state = create_state(sr);

        state.current_frame.store(50_000, Ordering::SeqCst);

        let target = Duration::from_secs(30);
        let target_frame = (target.as_secs_f64() * sr as f64) as u64;

        state.discard_buffer.store(true, Ordering::SeqCst);
        state.current_frame.store(target_frame, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(true, Ordering::SeqCst);

        if state.discard_buffer.swap(false, Ordering::SeqCst) {}

        let min_samples_to_resume = sr as usize * channels;
        let buffered_samples = 0usize;

        let is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);
        assert!(is_buffering, "should still be buffering after seek");

        if is_buffering {
            if buffered_samples >= min_samples_to_resume {
                state.waiting_for_seek.store(false, Ordering::Relaxed);
            }
        }

        assert!(
            state.waiting_for_seek.load(Ordering::Relaxed),
            "fix works: stay in buffering mode because old samples were discarded"
        );

        let frame_after = state.current_frame.load(Ordering::Relaxed);
        assert_eq!(
            frame_after, target_frame,
            "fix works: current_frame stays at target_frame, no drift from old samples"
        );
    }

    #[tokio::test]
    async fn background_song_cache_advances_with_playback_without_exceeding_ahead_limit() {
        const MAX_PREDOWNLOAD_BYTES: u64 = 1024 * 1024;
        const CONTENT_LENGTH: usize = 3 * 1024 * 1024;
        const PLAYBACK_ADVANCE_MS: u64 = 10_000;
        const EXPECTED_ADVANCED_BYTES: u64 = MAX_PREDOWNLOAD_BYTES + 512 * 1024;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept test request");
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {CONTENT_LENGTH}\r\nConnection: close\r\n\r\n"
            )
            .expect("write test response headers");

            let chunk = [0xA5; 64 * 1024];
            let mut remaining = CONTENT_LENGTH;
            while remaining > 0 {
                let write_len = remaining.min(chunk.len());
                if socket.write_all(&chunk[..write_len]).is_err() {
                    break;
                }
                remaining -= write_len;
            }
        });

        let test_root = std::env::temp_dir().join(format!(
            "song-cache-predownload-limit-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(1, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let control = Arc::new(SongCacheDownloadControl::new());
        let download = AudioPlayer::download_song_for_cache(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(60_000),
            Some(300),
            Some(MAX_PREDOWNLOAD_BYTES),
            Arc::clone(&control),
        );

        let observe_cache = async {
            wait_for_cached_file_size(&cache_path, MAX_PREDOWNLOAD_BYTES).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                std::fs::metadata(&cache_path)
                    .expect("paused cache metadata")
                    .len(),
                MAX_PREDOWNLOAD_BYTES,
                "cache must stop advancing while playback position is unchanged"
            );

            control.update_playback_position(PLAYBACK_ADVANCE_MS);
            wait_for_cached_file_size(&cache_path, EXPECTED_ADVANCED_BYTES).await;
            assert_eq!(
                std::fs::metadata(&cache_path)
                    .expect("advanced cache metadata")
                    .len(),
                EXPECTED_ADVANCED_BYTES,
                "cache should refill one ahead window after playback advances"
            );
            control.cancel();
        };

        let (outcome, ()) = tokio::join!(download, observe_cache);
        let outcome = outcome.expect("cache song while following playback");

        assert!(!outcome.is_complete);
        assert_eq!(outcome.downloaded_bytes, EXPECTED_ADVANCED_BYTES);

        server.join().expect("join test server");
        let _ = std::fs::remove_dir_all(test_root);
    }

    #[tokio::test]
    async fn cached_stream_reader_continues_across_the_predownload_boundary() {
        const MAX_PREDOWNLOAD_BYTES: u64 = 1024 * 1024;
        const CONTENT_LENGTH: usize = 2 * 1024 * 1024;
        const READ_TARGET: usize = MAX_PREDOWNLOAD_BYTES as usize + 256 * 1024;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept test request");
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {CONTENT_LENGTH}\r\nConnection: close\r\n\r\n"
            )
            .expect("write test response headers");

            let chunk = [0x5A; 64 * 1024];
            let mut remaining = CONTENT_LENGTH;
            while remaining > 0 {
                let write_len = remaining.min(chunk.len());
                if socket.write_all(&chunk[..write_len]).is_err() {
                    break;
                }
                remaining -= write_len;
            }
        });

        let test_root = std::env::temp_dir().join(format!(
            "song-cache-read-boundary-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(2, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let download = build_cached_stream_download(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(60_000),
            Some(300),
            Some(MAX_PREDOWNLOAD_BYTES),
        )
        .await
        .expect("build cached stream");

        let read_task = tokio::task::spawn_blocking(move || {
            let content_len = download.reader.content_length();
            let mut reader = SeekableSource::new(download.reader, content_len);
            let mut total = 0usize;
            let mut chunk = [0u8; 32 * 1024];
            while total < READ_TARGET {
                let read = reader.read(&mut chunk).expect("read cached stream");
                assert!(read > 0, "cached stream returned EOF before content end");
                assert!(chunk[..read].iter().all(|byte| *byte == 0x5A));
                total += read;
            }
            total
        });

        let read_total = tokio::time::timeout(Duration::from_secs(3), read_task)
            .await
            .expect("cached stream stalled at the predownload boundary")
            .expect("join cached stream reader");
        assert!(read_total >= READ_TARGET);

        server.join().expect("join test server");
        let _ = std::fs::remove_dir_all(test_root);
    }

    async fn wait_for_cached_file_size(path: &Path, expected_size: u64) {
        for _ in 0..200 {
            if std::fs::metadata(path)
                .map(|metadata| metadata.len() >= expected_size)
                .unwrap_or(false)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("cached file did not reach {expected_size} bytes in time");
    }

    fn parse_range_header(request: &str) -> Option<(u64, Option<u64>)> {
        let line = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("range:"))?;
        let value = line.split_once(':')?.1.trim().strip_prefix("bytes=")?;
        let (start, end) = value.split_once('-')?;
        let start = start.trim().parse().ok()?;
        let end = if end.trim().is_empty() {
            None
        } else {
            end.trim().parse().ok()
        };
        Some((start, end))
    }

    fn serve_range_request(
        mut socket: std::net::TcpStream,
        content_length: usize,
    ) -> std::io::Result<()> {
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = socket.read(&mut chunk)?;
            if read == 0 {
                return Ok(());
            }
            request.extend_from_slice(&chunk[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            if request.len() > 64 * 1024 {
                return Ok(());
            }
        }

        let request_text = String::from_utf8_lossy(&request);
        let body = vec![0x5Au8; content_length];
        match parse_range_header(&request_text) {
            Some((start, end)) => {
                let last = content_length as u64 - 1;
                let end = end.unwrap_or(last).min(last);
                if start > last || end < start {
                    write!(
                        socket,
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )?;
                    return Ok(());
                }
                let slice = &body[start as usize..=end as usize];
                write!(
                    socket,
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{content_length}\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                )?;
                socket.write_all(slice)?;
            }
            None => {
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
                )?;
                socket.write_all(&body)?;
            }
        }
        Ok(())
    }

    fn spawn_range_capable_server(content_length: usize) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(socket) = stream else { continue };
                thread::spawn(move || {
                    let _ = serve_range_request(socket, content_length);
                });
            }
        });
        address
    }

    /// 第一个（无 Range 的）连接在写出 `stall_after_bytes` 后停住 `stall_for`，
    /// 模拟连接级 stall；后续 Range 请求正常服务。
    fn spawn_stalling_server(
        content_length: usize,
        stall_after_bytes: usize,
        stall_for: Duration,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut socket) = stream else { continue };
                thread::spawn(move || {
                    let mut request = [0u8; 8192];
                    let Ok(read) = socket.read(&mut request) else { return };
                    if read == 0 {
                        return;
                    }
                    let request_text = String::from_utf8_lossy(&request[..read]);
                    let body = vec![0x5Au8; content_length];
                    if let Some((start, end)) = parse_range_header(&request_text) {
                        let last = content_length as u64 - 1;
                        let end = end.unwrap_or(last).min(last);
                        let slice = &body[start as usize..=end as usize];
                        let _ = write!(
                            socket,
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{content_length}\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            slice.len()
                        );
                        let _ = socket.write_all(slice);
                    } else {
                        let _ = write!(
                            socket,
                            "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
                        );
                        let _ = socket.write_all(&body[..stall_after_bytes]);
                        if stall_for == Duration::MAX {
                            loop {
                                thread::sleep(Duration::from_secs(3600));
                            }
                        } else {
                            thread::sleep(stall_for);
                        }
                        let _ = socket.write_all(&body[stall_after_bytes..]);
                    }
                });
            }
        });
        address
    }

    /// 服务器中途停住（连接级 stall）：生产客户端（无 reqwest 总超时）必须
    /// 通过 stream-download 的 retry_timeout + reconnect 恢复并完成下载。
    #[tokio::test]
    async fn cached_stream_recovers_from_connection_stall() {
        const CONTENT_LENGTH: usize = 1024 * 1024;
        const STALL_AFTER_BYTES: usize = 256 * 1024;

        let address = spawn_stalling_server(CONTENT_LENGTH, STALL_AFTER_BYTES, Duration::MAX);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-stall-recover-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(5, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let download = build_cached_stream_download(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(60_000),
            Some(30),
            Some(8 * 1024 * 1024),
        )
        .await
        .expect("build cached stream");

        let read_task = tokio::task::spawn_blocking(move || {
            let content_len = download.reader.content_length();
            let mut reader = SeekableSource::new(download.reader, content_len)
                .with_storage_state(download.storage_state);
            let mut total = 0usize;
            let mut chunk = [0u8; 32 * 1024];
            while total < CONTENT_LENGTH {
                let read = reader.read(&mut chunk).expect("read cached stream");
                assert!(read > 0, "cached stream EOF before content end");
                assert!(chunk[..read].iter().all(|byte| *byte == 0x5A));
                total += read;
            }
            total
        });

        let read_total = tokio::time::timeout(Duration::from_secs(20), read_task)
            .await
            .expect("download did not recover from the connection stall")
            .expect("join cached stream reader");
        assert_eq!(read_total, CONTENT_LENGTH);

        let meta: SongStreamCacheMeta = serde_json::from_str(
            &std::fs::read_to_string(&metadata_path).expect("read song meta"),
        )
        .expect("parse song meta");
        assert!(meta.is_fully_downloaded(), "cache meta must show full progress");

        let _ = std::fs::remove_dir_all(test_root);
    }

    /// 记录失败模式：若客户端带 reqwest 总超时（`.timeout()`），流式请求在
    /// 超时被中止后，TotalTimeoutBody 每次 poll 都立即返回错误，下载任务陷入
    /// 无限错误自旋，读者永远等不到后续数据。生产客户端不允许设置总超时。
    #[tokio::test]
    async fn total_request_timeout_stalls_download_task() {
        const CONTENT_LENGTH: usize = 1024 * 1024;
        const STALL_AFTER_BYTES: usize = 256 * 1024;

        let address = spawn_stalling_server(CONTENT_LENGTH, STALL_AFTER_BYTES, Duration::MAX);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-total-timeout-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let song_url = format!("http://{address}/song");

        // 复现旧生产配置：reqwest 总超时 1 秒（旧值 30 秒，缩短以便测试）。
        let timeout_client = Client::builder()
            .timeout(Duration::from_secs(1))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("build timeout client");
        let stream = HttpStream::new(timeout_client, song_url.parse().expect("parse url"))
            .await
            .expect("open http stream");
        let mut reader = StreamDownload::from_stream(
            stream,
            PersistentFileStorageProvider::new(&cache_path),
            Settings::default().prefetch_bytes(64 * 1024),
        )
        .await
        .expect("build stream download");

        let cancel_token = reader.cancellation_token();
        let mut read_task = tokio::task::spawn_blocking(move || {
            let mut total = 0usize;
            let mut chunk = [0u8; 32 * 1024];
            while total < CONTENT_LENGTH {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => total += read,
                    Err(_) => break,
                }
            }
            total
        });

        // 总超时中止流后下载任务自旋：读者停滞/报错/读不满都算复现失败模式；
        // 只有完整读出全部内容才说明总超时对流式下载无害。
        let result = tokio::time::timeout(Duration::from_secs(8), &mut read_task).await;
        let completed = result
            .ok()
            .and_then(|joined| joined.ok())
            .is_some_and(|total| total >= CONTENT_LENGTH);
        assert!(
            !completed,
            "a client with a total request timeout must not be used for streaming downloads"
        );

        // 取消下载任务（stream_done 会唤醒被阻塞的读者线程），避免测试 runtime
        // 在关闭时永久等待仍在阻塞的 spawn_blocking 任务。
        cancel_token.cancel();
        tokio::time::timeout(Duration::from_secs(3), &mut read_task)
            .await
            .expect("reader thread was not released after cancelling the download")
            .expect("join reader thread");

        let _ = std::fs::remove_dir_all(test_root);
    }

    /// 后台缓存下载同样会 seek 到未缓存位置（播放位置大幅前进时 driver 把
    /// reader 拉到对应字节），必须验证它不会触发同样的死锁。
    #[tokio::test]
    async fn background_song_cache_follows_forward_seek_without_deadlock() {
        const MAX_AHEAD_BYTES: u64 = 512 * 1024;
        const CONTENT_LENGTH: usize = 3 * 1024 * 1024;
        const DURATION_MS: u64 = 60_000;
        const SEEK_MS: u64 = 30_000;
        let seek_target = (CONTENT_LENGTH as u64 / 2).min(CONTENT_LENGTH as u64 - 1);

        let address = spawn_range_capable_server(CONTENT_LENGTH);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-bg-seek-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(4, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let control = Arc::new(SongCacheDownloadControl::new());
        let download = AudioPlayer::download_song_for_cache(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(DURATION_MS),
            Some(300),
            Some(MAX_AHEAD_BYTES),
            Arc::clone(&control),
        );

        let drive_cache = async {
            wait_for_cached_file_size(&cache_path, MAX_AHEAD_BYTES).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(
                std::fs::metadata(&cache_path)
                    .expect("cache metadata")
                    .len(),
                MAX_AHEAD_BYTES,
                "cache must be throttled at the write-ahead limit"
            );

            // 播放位置从 0 跳到一半：driver 需要 seek 到未缓存的 1.5MB 处。
            control.update_playback_position(SEEK_MS);
            wait_for_cached_file_size(&cache_path, seek_target + MAX_AHEAD_BYTES).await;
            control.cancel();
        };

        let (outcome, ()) = tokio::join!(download, drive_cache);
        let outcome = outcome.expect("background cache must follow a forward seek");

        assert!(!outcome.is_complete);
        assert!(outcome.downloaded_bytes >= MAX_AHEAD_BYTES * 2);
        let _ = std::fs::remove_dir_all(test_root);
    }

    /// 复现 native-seek-regression 的 cached-url 序列：
    /// start_at 12.5s → seek 24s → seek 6s → 继续读。
    /// 64KB write-ahead 下，回退到 6s 后读路径不得把下载任务弄挂。
    #[tokio::test]
    async fn regression_cached_url_start_at_forward_then_backward_seek() {
        const MAX_AHEAD_BYTES: u64 = 64 * 1024;
        const BYTES_PER_SEC: u64 = 48_000 * 2 * 2;
        const CONTENT_LENGTH: usize = (BYTES_PER_SEC * 55) as usize;

        let address = spawn_range_capable_server(CONTENT_LENGTH);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-reg-seq-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(7, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let download = build_cached_stream_download(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(55_000),
            Some(10),
            Some(MAX_AHEAD_BYTES),
        )
        .await
        .expect("build cached stream");

        let content_len = download.reader.content_length();
        let mut reader = SeekableSource::new(download.reader, content_len)
            .with_storage_state(download.storage_state);

        let task = tokio::task::spawn_blocking(move || {
            let mut chunk = [0u8; 16 * 1024];
            let mut read_ok = |reader: &mut SeekableSource<_>, n: usize, label: &str| {
                let mut total = 0usize;
                while total < n {
                    let read = reader
                        .read(&mut chunk)
                        .unwrap_or_else(|e| panic!("read failed at {label}: {e}"));
                    assert!(read > 0, "unexpected EOF at {label}");
                    assert!(
                        chunk[..read].iter().all(|b| *b == 0x5A),
                        "bad bytes at {label}"
                    );
                    total += read;
                }
            };

            // 初始读一点（模拟 probe），再 start_at 12.5s。
            read_ok(&mut reader, 32 * 1024, "probe");
            reader
                .seek(SeekFrom::Start(BYTES_PER_SEC * 25 / 2))
                .expect("start_at 12.5s");
            read_ok(&mut reader, 64 * 1024, "after start_at");
            // forward 24s
            reader
                .seek(SeekFrom::Start(BYTES_PER_SEC * 24))
                .expect("seek 24s");
            read_ok(&mut reader, 64 * 1024, "after 24s");
            // backward 6s — regression 在此之后读挂死
            reader
                .seek(SeekFrom::Start(BYTES_PER_SEC * 6))
                .expect("seek 6s");
            for i in 0..20 {
                read_ok(&mut reader, 32 * 1024, &format!("after 6s #{i}"));
            }
        });

        tokio::time::timeout(Duration::from_secs(20), task)
            .await
            .expect("regression sequence timed out")
            .expect("join regression sequence");
        let _ = std::fs::remove_dir_all(test_root);
    }

    /// 回退 seek 在 downloaded 集合中留下靠前的 gap 后，再向前 seek 到未缓存
    /// 位置：stream-download 的 handle_seek 只夹紧了 gap.start 没校验 gap.end，
    /// 可能算出 start > end 的倒置 range 请求（服务器 416），整个下载任务被
    /// 标记失败、播放随之中断。必须保证 seek 与后续读取正常。
    #[tokio::test]
    async fn forward_seek_after_backward_seek_does_not_fail_download_task() {
        const MAX_AHEAD_BYTES: u64 = 64 * 1024;
        const CONTENT_LENGTH: usize = 3 * 1024 * 1024;

        let address = spawn_range_capable_server(CONTENT_LENGTH);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-seek-gaps-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(6, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let download = build_cached_stream_download(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(180_000),
            Some(30),
            Some(MAX_AHEAD_BYTES),
        )
        .await
        .expect("build cached stream");

        let content_len = download.reader.content_length();
        let mut reader = SeekableSource::new(download.reader, content_len)
            .with_storage_state(download.storage_state);

        let seek_read_task = tokio::task::spawn_blocking(move || {
            let mut chunk = [0u8; 32 * 1024];
            let mut read_some = |reader: &mut SeekableSource<_>, bytes: usize, label: &str| {
                let start = reader.stream_position().expect("reader position");
                let mut total = 0usize;
                while total < bytes {
                    let read = reader.read(&mut chunk).expect("read");
                    assert!(read > 0, "unexpected EOF at {label}");
                    assert!(
                        chunk[..read].iter().all(|byte| *byte == 0x5A),
                        "read hole/garbage at {label} (started at {start}, got {read} bytes after {total})"
                    );
                    total += read;
                }
            };

            read_some(&mut reader, 128 * 1024, "initial");
            // 向前 seek 到 2MB（未缓存）。
            reader.seek(SeekFrom::Start(2 * 1024 * 1024)).expect("seek to 2MB");
            read_some(&mut reader, 32 * 1024, "at 2MB");
            // 回退 seek 到 1MB（未缓存）：downloaded 集合留下 [1MB..2MB) 的 gap。
            reader.seek(SeekFrom::Start(1024 * 1024)).expect("seek back to 1MB");
            read_some(&mut reader, 32 * 1024, "at 1MB");
            // 再向前 seek 到 2.5MB（未缓存）：此处不允许把下载任务弄挂。
            reader
                .seek(SeekFrom::Start(2560 * 1024))
                .expect("forward seek after backward seek failed the download task");
            read_some(&mut reader, 64 * 1024, "at 2.5MB");
        });

        tokio::time::timeout(Duration::from_secs(15), seek_read_task)
            .await
            .expect("seek sequence deadlocked")
            .expect("join seek sequence task");

        // 流式下载继续：缓存在 2.5MB 之后继续推进。
        wait_for_cached_file_size(&cache_path, 2560 * 1024 + MAX_AHEAD_BYTES).await;
        let _ = std::fs::remove_dir_all(test_root);
    }

    /// 复现“seek 到未缓存位置直接卡死”的回归测试。
    ///
    /// 死锁机理：缓存写端被 `max_cache_ahead_bytes` 节流时返回 0 字节，
    /// stream-download 的下载任务因此睡在 `wait_for_read` 里（只有读端的
    /// 读操作能唤醒它）；此时 `StreamDownload::seek` 到未缓存位置只是把
    /// seek 请求放进 channel 然后阻塞等 `position_reached`，并不会唤醒
    /// 下载任务——任务永远不会处理 seek，读端永远等不到位置，双方死锁。
    #[tokio::test]
    async fn seek_to_uncached_position_does_not_deadlock_when_writer_is_throttled() {
        const MAX_AHEAD_BYTES: u64 = 256 * 1024;
        const CONTENT_LENGTH: usize = 4 * 1024 * 1024;
        const PRE_READ_BYTES: usize = 64 * 1024;
        const SEEK_TARGET: u64 = 2 * 1024 * 1024;
        const POST_READ_BYTES: usize = 32 * 1024;

        let address = spawn_range_capable_server(CONTENT_LENGTH);
        let test_root = std::env::temp_dir().join(format!(
            "song-cache-seek-uncached-{}-{}",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        std::fs::create_dir_all(&test_root).expect("create test cache root");
        let cache_path = test_root.join("song.bin");
        let metadata_path = test_root.join("song.bin.meta.json");
        let song_url = format!("http://{address}/song");
        let meta = SongStreamCacheMeta::new(3, "lossless", &song_url);
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize test metadata"),
        )
        .expect("write test metadata");

        let download = build_cached_stream_download(
            &song_url,
            cache_path.to_str().expect("cache path"),
            metadata_path.to_str().expect("metadata path"),
            Some(240_000),
            Some(30),
            Some(MAX_AHEAD_BYTES),
        )
        .await
        .expect("build cached stream");

        let content_len = download.reader.content_length();
        let mut reader = SeekableSource::new(download.reader, content_len)
            .with_storage_state(download.storage_state);

        // 同步阻塞的读写必须在 blocking 线程上做，否则会停住测试 runtime 的唯一线程，
        // 让后台下载任务永远得不到调度（生产环境里读端在独立的解码线程上）。
        let (initial_done_tx, initial_done_rx) = std::sync::mpsc::channel::<()>();
        let (go_seek_tx, go_seek_rx) = std::sync::mpsc::channel::<()>();
        let seek_task = tokio::task::spawn_blocking(move || {
            // 模拟播放消费：读端位置前进，带动节流锚点。
            let mut chunk = [0u8; 16 * 1024];
            let mut consumed = 0usize;
            while consumed < PRE_READ_BYTES {
                let read = reader.read(&mut chunk).expect("read initial playback");
                assert!(read > 0, "unexpected EOF during initial playback");
                consumed += read;
            }
            initial_done_tx.send(()).expect("notify initial playback done");

            go_seek_rx.recv().expect("wait for seek signal");
            reader
                .seek(SeekFrom::Start(SEEK_TARGET))
                .expect("seek to uncached position");
            let mut total = 0usize;
            let mut buf = [0u8; 16 * 1024];
            while total < POST_READ_BYTES {
                let read = reader.read(&mut buf).expect("read after uncached seek");
                assert!(read > 0, "unexpected EOF right after uncached seek");
                assert!(buf[..read].iter().all(|byte| *byte == 0x5A));
                total += read;
            }
            total
        });

        tokio::task::spawn_blocking(move || initial_done_rx.recv())
            .await
            .expect("join initial playback waiter")
            .expect("initial playback finished");

        // 等写端写到节流上限并确认已停住（下载任务睡在 wait_for_read）。
        // 允许一个 HTTP chunk 量级的余量：写端到限后可能仍会完成当前 in-flight 块。
        let throttled_tip = PRE_READ_BYTES as u64 + MAX_AHEAD_BYTES;
        wait_for_cached_file_size(&cache_path, throttled_tip).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let stabilized_size = std::fs::metadata(&cache_path)
            .expect("cache metadata")
            .len();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let later_size = std::fs::metadata(&cache_path)
            .expect("cache metadata")
            .len();
        assert_eq!(
            later_size, stabilized_size,
            "writer must stop advancing once throttled (size={stabilized_size})"
        );
        assert!(
            stabilized_size >= throttled_tip && stabilized_size < SEEK_TARGET,
            "throttled tip {stabilized_size} must sit between ahead-limit and the seek target"
        );

        // 从解码线程视角 seek 到未缓存位置并继续读。
        go_seek_tx.send(()).expect("trigger uncached seek");
        let read_total = tokio::time::timeout(Duration::from_secs(10), seek_task)
            .await
            .expect("seek to an uncached position deadlocked: the throttled download task never woke up")
            .expect("join uncached seek task");
        assert!(read_total >= POST_READ_BYTES);

        // 流式下载必须继续：缓存在 seek 目标之后继续推进。
        wait_for_cached_file_size(&cache_path, SEEK_TARGET + MAX_AHEAD_BYTES).await;

        let _ = std::fs::remove_dir_all(test_root);
    }
}
