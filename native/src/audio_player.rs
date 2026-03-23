use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::StorageProvider;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::storage::temp::TempStorageProvider;
use stream_download::{Settings, StreamDownload, StreamPhase, StreamState};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::conv::ConvertibleSample;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{Sample, SampleFormat as SymphoniaSampleFormat};
use tokio::sync::Notify;

use crate::cache::song::SongStreamCacheMeta;

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

#[derive(Clone, Debug)]
struct PersistentFileStorageProvider {
    path: PathBuf,
}

impl PersistentFileStorageProvider {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl StorageProvider for PersistentFileStorageProvider {
    type Reader = BufReader<File>;
    type Writer = File;

    fn into_reader_writer(
        self,
        _content_length: Option<u64>,
    ) -> io::Result<(Self::Reader, Self::Writer)> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&self.path)?;
        let reader = BufReader::new(OpenOptions::new().read(true).open(&self.path)?);
        Ok((reader, writer))
    }
}

#[derive(Clone)]
struct SongCacheTracker {
    metadata_path: Arc<PathBuf>,
    inner: Arc<Mutex<SongCacheTrackerState>>,
}

struct SongCacheTrackerState {
    meta: SongStreamCacheMeta,
    last_persisted_bytes: u64,
}

impl SongCacheTracker {
    const PERSIST_STEP_BYTES: u64 = 128 * 1024;

    fn new(metadata_path: impl Into<PathBuf>) -> io::Result<Self> {
        let metadata_path = metadata_path.into();
        let raw = fs::read_to_string(&metadata_path)?;
        let meta = serde_json::from_str::<SongStreamCacheMeta>(&raw)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let last_persisted_bytes = meta.downloaded_bytes();

        Ok(Self {
            metadata_path: Arc::new(metadata_path),
            inner: Arc::new(Mutex::new(SongCacheTrackerState {
                meta,
                last_persisted_bytes,
            })),
        })
    }

    fn set_content_length(&self, content_length: Option<u64>) -> io::Result<()> {
        let mut state = self.inner.lock().unwrap();
        if state.meta.content_length == content_length {
            return Ok(());
        }

        state.meta.set_content_length(content_length);
        self.persist_locked(&mut state)
    }

    fn record_progress(&self, progress: StreamState, content_length: Option<u64>) {
        if let Err(err) = self.try_record_progress(progress, content_length) {
            eprintln!("[cache] failed to persist song stream metadata: {err}");
        }
    }

    fn try_record_progress(
        &self,
        progress: StreamState,
        content_length: Option<u64>,
    ) -> io::Result<()> {
        let mut state = self.inner.lock().unwrap();
        let previous_bytes = state.meta.downloaded_bytes();

        if state.meta.content_length != content_length {
            state.meta.set_content_length(content_length);
        }

        state.meta.add_range(progress.current_chunk.clone());
        if matches!(progress.phase, StreamPhase::Complete) {
            state.meta.mark_complete();
        }

        let downloaded_bytes = state.meta.downloaded_bytes();
        if matches!(progress.phase, StreamPhase::Complete)
            || downloaded_bytes.saturating_sub(state.last_persisted_bytes)
                >= Self::PERSIST_STEP_BYTES
            || downloaded_bytes < previous_bytes
        {
            self.persist_locked(&mut state)?;
        }

        Ok(())
    }

    fn persist_locked(&self, state: &mut SongCacheTrackerState) -> io::Result<()> {
        let path = self.metadata_path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_vec_pretty(&state.meta)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, serialized)?;
        fs::rename(&tmp_path, path)?;
        state.last_persisted_bytes = state.meta.downloaded_bytes();
        Ok(())
    }
}

struct AudioMetadata {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: Option<u32>,
    sample_format: Option<SymphoniaSampleFormat>,
    track_id: u32,
    decoder: Box<dyn Decoder>,
    format_reader: Box<dyn FormatReader>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_current: bool,
}

pub(crate) struct SharedState {
    pub(crate) is_paused: AtomicBool,
    pub(crate) current_frame: AtomicU64,
    pub(crate) sample_rate: AtomicU32,
    pub(crate) has_seek_request: AtomicBool, // 新增：轻量级标记
    pub(crate) seek_request: Mutex<Option<Duration>>,
    pub(crate) is_terminating: AtomicBool,
    pub(crate) discard_buffer: AtomicBool,
    pub(crate) decoder_done: AtomicBool,
    pub(crate) is_finished: AtomicBool,
    pub(crate) finish_notify: Notify,
    pub(crate) buffered_frames: AtomicU64, // 新：缓冲的帧数（每帧含 channels 个采样）
    pub(crate) waiting_for_seek: AtomicBool, // 新：seek 后等待缓冲完成标记
}
pub struct AudioPlayer {
    device: cpal::Device,
    stream: Option<cpal::Stream>,
    state: Arc<SharedState>,
}

impl AudioPlayer {
    fn is_supported_output_format(fmt: cpal::SampleFormat) -> bool {
        matches!(
            fmt,
            cpal::SampleFormat::I8
                | cpal::SampleFormat::U8
                | cpal::SampleFormat::I16
                | cpal::SampleFormat::U16
                | cpal::SampleFormat::I24
                | cpal::SampleFormat::U24
                | cpal::SampleFormat::I32
                | cpal::SampleFormat::U32
                | cpal::SampleFormat::F32
                | cpal::SampleFormat::F64
        )
    }

    fn device_desc(device: &cpal::Device) -> String {
        let name = Self::device_display_name(device);
        let id = Self::device_id(device);
        if name == id {
            name
        } else {
            format!("{name} [{id}]")
        }
    }

    fn device_id(device: &cpal::Device) -> String {
        device
            .id()
            .map(|id| id.to_string())
            .or_else(|_| {
                device
                    .description()
                    .map(|desc| desc.driver().unwrap_or(desc.name()).to_string())
            })
            .unwrap_or_else(|_| "<unknown>".to_string())
    }

    fn device_display_name(device: &cpal::Device) -> String {
        device
            .description()
            .map(|desc| desc.name().trim().to_string())
            .or_else(|_| device.id().map(|id| id.to_string()))
            .unwrap_or_else(|_| "<unknown>".to_string())
    }

    #[cfg(target_os = "linux")]
    fn linux_device_locator(device_id: &str) -> &str {
        device_id.strip_prefix("alsa:").unwrap_or(device_id)
    }

    #[cfg(target_os = "linux")]
    fn linux_plughw_locator(device_id: &str) -> Option<String> {
        Self::linux_device_locator(device_id)
            .strip_prefix("hw:")
            .map(|suffix| format!("plughw:{suffix}"))
    }

    fn device_matches_filter(device: &cpal::Device, filter: &str) -> bool {
        let id = Self::device_id(device);
        if id == filter {
            return true;
        }

        #[cfg(target_os = "linux")]
        if Self::linux_device_locator(&id) == filter {
            return true;
        }

        let name = Self::device_display_name(device);
        name == filter || name.contains(filter)
    }

    #[cfg(target_os = "linux")]
    fn is_linux_hw_device(device: &cpal::Device) -> bool {
        Self::is_linux_hw_device_id(&Self::device_id(device))
    }

    #[cfg(target_os = "linux")]
    fn is_linux_hw_device_id(device_id: &str) -> bool {
        let locator = Self::linux_device_locator(device_id);
        locator.starts_with("hw:") && !locator.starts_with("plughw:")
    }

    #[cfg(target_os = "linux")]
    fn should_list_linux_output_device(
        device_id: &str,
        current_id: Option<&str>,
        default_id: Option<&str>,
    ) -> bool {
        if Some(device_id) == current_id
            && Some(device_id) != default_id
            && !Self::is_linux_hw_device_id(device_id)
        {
            return true;
        }

        if Some(device_id) == default_id && !Self::is_linux_hw_device_id(device_id) {
            return false;
        }

        Self::is_linux_hw_device_id(device_id)
    }

    fn disambiguate_output_device_names(devices: &mut [OutputDeviceInfo]) {
        let mut name_counts = HashMap::new();
        for device in devices.iter() {
            *name_counts.entry(device.name.clone()).or_insert(0usize) += 1;
        }

        for device in devices.iter_mut() {
            if name_counts.get(&device.name).copied().unwrap_or_default() > 1 {
                device.name = format!("{} [{}]", device.name, device.id);
            }
        }
    }

    fn preferred_output_formats(
        bits_per_sample: Option<u32>,
        source_sample_format: Option<SymphoniaSampleFormat>,
    ) -> Vec<cpal::SampleFormat> {
        let mut prefer = Vec::with_capacity(12);
        let mut push_unique = |fmt: cpal::SampleFormat| {
            if !prefer.contains(&fmt) {
                prefer.push(fmt);
            }
        };

        if let Some(fmt) = source_sample_format {
            match fmt {
                SymphoniaSampleFormat::S8 => {
                    push_unique(cpal::SampleFormat::I8);
                    push_unique(cpal::SampleFormat::I16);
                    push_unique(cpal::SampleFormat::I24);
                    push_unique(cpal::SampleFormat::I32);
                }
                SymphoniaSampleFormat::U8 => {
                    push_unique(cpal::SampleFormat::U8);
                    push_unique(cpal::SampleFormat::U16);
                    push_unique(cpal::SampleFormat::U24);
                    push_unique(cpal::SampleFormat::U32);
                }
                SymphoniaSampleFormat::S16 => {
                    push_unique(cpal::SampleFormat::I16);
                    push_unique(cpal::SampleFormat::I24);
                    push_unique(cpal::SampleFormat::I32);
                }
                SymphoniaSampleFormat::U16 => {
                    push_unique(cpal::SampleFormat::U16);
                    push_unique(cpal::SampleFormat::U24);
                    push_unique(cpal::SampleFormat::U32);
                }
                SymphoniaSampleFormat::S24 => {
                    push_unique(cpal::SampleFormat::I24);
                    push_unique(cpal::SampleFormat::I32);
                }
                SymphoniaSampleFormat::U24 => {
                    push_unique(cpal::SampleFormat::U24);
                    push_unique(cpal::SampleFormat::U32);
                }
                SymphoniaSampleFormat::S32 => {
                    push_unique(cpal::SampleFormat::I32);
                    push_unique(cpal::SampleFormat::I24);
                }
                SymphoniaSampleFormat::U32 => {
                    push_unique(cpal::SampleFormat::U32);
                    push_unique(cpal::SampleFormat::U24);
                }
                SymphoniaSampleFormat::F32 => {
                    push_unique(cpal::SampleFormat::F32);
                    push_unique(cpal::SampleFormat::F64);
                }
                SymphoniaSampleFormat::F64 => {
                    push_unique(cpal::SampleFormat::F64);
                    push_unique(cpal::SampleFormat::F32);
                }
            }
        }

        match bits_per_sample {
            Some(0..=8) => {
                push_unique(cpal::SampleFormat::I8);
                push_unique(cpal::SampleFormat::U8);
            }
            Some(9..=16) => {
                push_unique(cpal::SampleFormat::I16);
                push_unique(cpal::SampleFormat::U16);
            }
            Some(17..=24) => {
                push_unique(cpal::SampleFormat::I24);
                push_unique(cpal::SampleFormat::U24);
            }
            Some(_) => {
                push_unique(cpal::SampleFormat::I32);
                push_unique(cpal::SampleFormat::U32);
            }
            None => {}
        }

        // Practical compatibility fallback order.
        for fmt in [
            cpal::SampleFormat::I32,
            cpal::SampleFormat::U32,
            cpal::SampleFormat::I24,
            cpal::SampleFormat::U24,
            cpal::SampleFormat::F32,
            cpal::SampleFormat::F64,
            cpal::SampleFormat::I16,
            cpal::SampleFormat::U16,
            cpal::SampleFormat::I8,
            cpal::SampleFormat::U8,
        ] {
            push_unique(fmt);
        }

        prefer
    }

    fn maybe_fallback_to_default_device(&mut self) -> Result<bool, Box<dyn Error>> {
        #[cfg(target_os = "linux")]
        {
            // Only fallback from direct `hw:*` selection. This keeps bit-perfect preferred
            // while preserving compatibility if the direct device cannot satisfy the track.
            if !Self::is_linux_hw_device(&self.device) {
                return Ok(false);
            }

            let host = cpal::default_host();
            let current_id = Self::device_id(&self.device);

            if let Some(plughw_locator) = Self::linux_plughw_locator(&current_id)
                && let Some(compat_device) = host.output_devices()?.find(|device| {
                    Self::linux_device_locator(&Self::device_id(device)) == plughw_locator
                })
            {
                println!(
                    "[audio] fallback to compatibility output device on the same hardware: {}",
                    Self::device_desc(&compat_device)
                );
                self.device = compat_device;
                return Ok(true);
            }

            let default_device = match host.default_output_device() {
                Some(device) => device,
                None => return Ok(false),
            };

            let default_id = Self::device_id(&default_device);
            if current_id == default_id {
                return Ok(false);
            }

            println!(
                "[audio] fallback to default output device for compatibility: {}",
                Self::device_desc(&default_device)
            );
            self.device = default_device;
            Ok(true)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(false)
        }
    }

    fn select_output_device(
        host: &cpal::Host,
        device_name_filter: Option<&str>,
    ) -> Result<cpal::Device, Box<dyn Error>> {
        let device = if let Some(name) = device_name_filter {
            host.output_devices()?
                .find(|d| Self::device_matches_filter(d, name))
                .ok_or("Device not found")?
        } else {
            host.default_output_device().ok_or("No default device")?
        };

        Ok(device)
    }

    pub fn new(device_name_filter: Option<&str>) -> Result<Self, Box<dyn Error>> {
        let host = cpal::default_host();
        let device = Self::select_output_device(&host, device_name_filter)?;

        println!(
            "[audio] using output device: {}",
            Self::device_desc(&device)
        );
        #[cfg(target_os = "linux")]
        {
            let is_hw = Self::is_linux_hw_device(&device);
            if !is_hw {
                println!(
                    "[audio] WARNING: non-`hw:*` device selected; bit-perfect may NOT be guaranteed (system mixer/resampler may be active)."
                );
            }
        }

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
                buffered_frames: AtomicU64::new(0), // 新增字段
                waiting_for_seek: AtomicBool::new(false),
                has_seek_request: AtomicBool::new(false), // 新增字段
            }),
        })
    }

    pub fn get_state(&self) -> Arc<SharedState> {
        self.state.clone()
    }

    pub fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, Box<dyn Error>> {
        let host = cpal::default_host();
        let default_id = host
            .default_output_device()
            .map(|device| Self::device_id(&device));
        let current_id = Self::device_id(&self.device);

        let mut devices = Vec::new();
        let mut seen_ids = HashSet::new();
        for device in host.output_devices()? {
            let id = Self::device_id(&device);
            if !seen_ids.insert(id.clone()) {
                continue;
            }

            #[cfg(target_os = "linux")]
            if !Self::should_list_linux_output_device(
                &id,
                Some(current_id.as_str()),
                default_id.as_deref(),
            ) {
                continue;
            }

            devices.push(OutputDeviceInfo {
                id: id.clone(),
                name: Self::device_display_name(&device),
                is_default: default_id.as_deref() == Some(id.as_str()),
                is_current: id == current_id,
            });
        }

        Self::disambiguate_output_device_names(&mut devices);
        devices.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(devices)
    }

    // --- 核心业务方法 ---

    pub async fn play_url(
        &mut self,
        url: &str,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn Error>> {
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
        self.setup_and_play(meta, start_at)
    }

    pub async fn play_url_cached(
        &mut self,
        url: &str,
        cache_path: &str,
        metadata_path: &str,
        duration_ms: Option<u64>,
        cache_ahead_secs: Option<u32>,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let stream = HttpStream::<Client>::create(url.parse()?).await?;
        let content_len = stream.content_length();
        let tracker = SongCacheTracker::new(metadata_path)?;
        tracker.set_content_length(content_len)?;

        let prefetch_bytes =
            estimate_prefetch_bytes(content_len, duration_ms, cache_ahead_secs.unwrap_or(30));
        let reader = StreamDownload::from_stream(
            stream,
            PersistentFileStorageProvider::new(cache_path),
            Settings::default()
                .prefetch_bytes(prefetch_bytes)
                .on_progress(move |stream: &HttpStream<Client>, state, _| {
                    tracker.record_progress(state, stream.content_length());
                }),
        )
        .await?;

        let source = Box::new(SeekableSource::new(reader, content_len));
        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at)
    }

    pub async fn play_file(
        &mut self,
        path: &str,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn Error>> {
        let path_buf = std::path::Path::new(path);
        let extension = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let file = std::fs::File::open(path_buf)?;
        let source = Box::new(file);

        let meta = self.spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at)
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

    fn setup_and_play(
        &mut self,
        meta: AudioMetadata,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn Error>> {
        // 1. 先调用 stop 确保旧流销毁，旧 state 被标记为终止
        self.stop();

        // 2. 彻底替换 state，确保新旧音频状态完全隔离
        // 这样旧线程永远看不到新音频的 false 标记，它只能看到旧 state 的 is_terminating = true
        self.state = Arc::new(SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            sample_rate: AtomicU32::new(meta.sample_rate),
            seek_request: Mutex::new(None),
            is_terminating: AtomicBool::new(false), // 新 state 初始为 false
            discard_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
            has_seek_request: AtomicBool::new(false),
        });

        let sr = meta.sample_rate;
        let channels = meta.channels;

        // 3. 硬件配置检查
        let (config, sample_format) = match self.find_best_config(
            sr,
            channels,
            meta.bits_per_sample,
            meta.sample_format,
        ) {
            Ok(cfg) => cfg,
            Err(primary_err) => {
                let primary_msg = primary_err.to_string();

                if self.maybe_fallback_to_default_device()? {
                    println!("[audio] retrying config selection on default output device...");
                    self.find_best_config(
                            sr,
                            channels,
                            meta.bits_per_sample,
                            meta.sample_format,
                        )
                        .map_err(|fallback_err| {
                            format!(
                                "No compatible output config for this track. primary_error={}; fallback_error={}",
                                primary_msg, fallback_err
                            )
                        })?
                } else {
                    return Err(primary_msg.into());
                }
            }
        };

        if meta.bits_per_sample.unwrap_or(16) > 16
            && matches!(
                sample_format,
                cpal::SampleFormat::I16 | cpal::SampleFormat::U16
            )
        {
            println!(
                "[audio] WARNING: source is {}-bit but output uses {:?}; precision may be reduced.",
                meta.bits_per_sample.unwrap_or(0),
                sample_format
            );
        }

        // 4. 创建全新的缓冲区和解码线程
        let state_for_cb = self.state.clone();
        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                let rb = HeapRb::<i16>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<i16, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i16>(meta, producer);
                stream
            }
            cpal::SampleFormat::U16 => {
                let rb = HeapRb::<u16>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<u16, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u16>(meta, producer);
                stream
            }
            cpal::SampleFormat::I8 => {
                let rb = HeapRb::<i8>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream =
                    self.build_stream::<i8, _>(&config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<i8>(meta, producer);
                stream
            }
            cpal::SampleFormat::U8 => {
                let rb = HeapRb::<u8>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream =
                    self.build_stream::<u8, _>(&config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<u8>(meta, producer);
                stream
            }
            cpal::SampleFormat::I24 => {
                let rb = HeapRb::<i32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream_converted::<i32, cpal::I24, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U24 => {
                let rb = HeapRb::<u32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream_converted::<u32, cpal::U24, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::I32 => {
                let rb = HeapRb::<i32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<i32, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U32 => {
                let rb = HeapRb::<u32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<u32, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F32 => {
                let rb = HeapRb::<f32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<f32, _>(
                    &config,
                    consumer,
                    state_for_cb,
                    channels as usize,
                )?;
                self.start_decode_thread::<f32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F64 => {
                let rb = HeapRb::<f64>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = self.build_stream::<f64, _>(
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
            Self::schedule_seek(&self.state, start_at);
        }

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
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

        std::thread::spawn(move || {
            loop {
                if state.is_terminating.load(Ordering::Relaxed) {
                    break;
                }

                Self::handle_seek_if_needed(&state, &mut *format, &mut *decoder, track_id, sr);

                if !producer.is_full() {
                    // 如果 decode_next_packet 返回 false，说明文件读取完毕或出错
                    if !Self::decode_next_packet::<S, _>(
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

            decoder.reset();

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

    fn decode_next_packet<S, P>(
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
                        Self::push_samples_blocking::<S, _>(producer, sample_buf.samples(), state);
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
    fn build_stream<S, C>(
        &self,
        config: &cpal::StreamConfig,
        mut consumer: C,
        state: Arc<SharedState>,
        channels: usize,
    ) -> Result<cpal::Stream, Box<dyn Error>>
    where
        S: cpal::SizedSample + Send + 'static,
        C: Consumer<Item = S> + Observer<Item = S> + Send + 'static,
    {
        let stream = self.device.build_output_stream(
            config,
            move |data: &mut [S], _: &cpal::OutputCallbackInfo| {
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
                        data.fill(S::EQUILIBRIUM);
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
                        data.fill(S::EQUILIBRIUM);
                        return;
                    }
                }

                // 3. 暂停处理
                if state.is_paused.load(Ordering::Relaxed) {
                    data.fill(S::EQUILIBRIUM);
                    return;
                }

                // 4. 正常消费数据
                let mut samples_read = 0usize;
                for sample in data.iter_mut() {
                    if let Some(s) = consumer.try_pop() {
                        *sample = s;
                        samples_read += 1;
                    } else {
                        // Buffer 在消费过程中抽干了
                        *sample = S::EQUILIBRIUM;
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
            |_err| {
                // 此时 err 如果还是 Underrun，通常是驱动级别的警告，
                // 但因为我们填充了静音，它不会导致音频咔哒声或死锁。
                // eprintln!("Driver message: {}", err);
            },
            None,
        )?;
        Ok(stream)
    }

    fn build_stream_converted<In, Out, C>(
        &self,
        config: &cpal::StreamConfig,
        mut consumer: C,
        state: Arc<SharedState>,
        channels: usize,
    ) -> Result<cpal::Stream, Box<dyn Error>>
    where
        In: Copy + Send + 'static,
        Out: cpal::SizedSample + cpal::FromSample<In> + Send + 'static,
        C: Consumer<Item = In> + Observer<Item = In> + Send + 'static,
    {
        let stream = self.device.build_output_stream(
            config,
            move |data: &mut [Out], _: &cpal::OutputCallbackInfo| {
                if state.discard_buffer.swap(false, Ordering::SeqCst) {
                    while consumer.try_pop().is_some() {}
                }

                let buffered_samples = consumer.occupied_len();
                let sr = state.sample_rate.load(Ordering::Relaxed) as usize;
                let decoder_done = state.decoder_done.load(Ordering::Relaxed);
                let min_samples_to_resume = sr * channels;

                let mut is_buffering = state.waiting_for_seek.load(Ordering::Relaxed);
                if is_buffering {
                    if buffered_samples >= min_samples_to_resume || decoder_done {
                        state.waiting_for_seek.store(false, Ordering::Relaxed);
                    } else {
                        data.fill(Out::EQUILIBRIUM);
                        return;
                    }
                }

                if !is_buffering && buffered_samples == 0 && !decoder_done {
                    state.waiting_for_seek.store(true, Ordering::Relaxed);
                    is_buffering = true;
                }

                if is_buffering {
                    if buffered_samples >= min_samples_to_resume || decoder_done {
                        state.waiting_for_seek.store(false, Ordering::Relaxed);
                    } else {
                        data.fill(Out::EQUILIBRIUM);
                        return;
                    }
                }

                if state.is_paused.load(Ordering::Relaxed) {
                    data.fill(Out::EQUILIBRIUM);
                    return;
                }

                let mut samples_read = 0usize;
                for sample in data.iter_mut() {
                    if let Some(s) = consumer.try_pop() {
                        *sample = Out::from_sample(s);
                        samples_read += 1;
                    } else {
                        *sample = Out::EQUILIBRIUM;
                    }
                }

                if samples_read > 0 {
                    state
                        .current_frame
                        .fetch_add((samples_read / channels) as u64, Ordering::Relaxed);
                } else if decoder_done && !state.is_finished.swap(true, Ordering::SeqCst) {
                    state.finish_notify.notify_waiters();
                }
            },
            |_err| {},
            None,
        )?;
        Ok(stream)
    }

    fn find_best_config(
        &self,
        target_sr: u32,
        channels: u16,
        bits_per_sample: Option<u32>,
        source_sample_format: Option<SymphoniaSampleFormat>,
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

        let prefer = Self::preferred_output_formats(bits_per_sample, source_sample_format);
        for fmt in prefer.iter() {
            if let Some(c) = candidates.iter().find(|c| c.sample_format() == *fmt) {
                println!(
                    "[audio] SELECTED: fmt={:?}, sample_rate={}Hz, channels={}",
                    fmt, target_sr, channels
                );
                let config: cpal::StreamConfig = c.with_sample_rate(target_sr).into();
                return Ok((config, *fmt));
            }
        }

        if let Some(c) = candidates
            .iter()
            .find(|c| Self::is_supported_output_format(c.sample_format()))
        {
            println!(
                "[audio] WARNING: preferred sample formats not found, fallback to {:?}.",
                c.sample_format()
            );
            let config: cpal::StreamConfig = c.with_sample_rate(target_sr).into();
            return Ok((config, c.sample_format()));
        }

        println!("[audio] NO MATCH for requested sample_rate/channels in supported sample formats");
        Err(
            "Hardware doesn't support file's sample-rate/channels in compatible sample format"
                .into(),
        )
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
    fn schedule_seek(state: &SharedState, target: Duration) {
        {
            let mut seek_req = state.seek_request.lock().unwrap();
            *seek_req = Some(target);
        }

        let target_frame =
            (target.as_secs_f64() * state.sample_rate.load(Ordering::Relaxed) as f64) as u64;
        state.current_frame.store(target_frame, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(true, Ordering::SeqCst);
    }

    pub fn seek(&self, target: Duration) {
        Self::schedule_seek(&self.state, target);
    }

    pub fn stop(&mut self) {
        // 1. 先设置旧状态的终止标记，让旧线程在任何检查点都能发现并退出
        self.state.is_terminating.store(true, Ordering::SeqCst);

        // 2. 销毁 cpal 流，这会触发旧 consumer 的 Drop
        self.stream = None;

        // 3. 唤醒所有在 wait_finished 上的等待者
        self.state.is_finished.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters();
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

fn estimate_prefetch_bytes(
    content_length: Option<u64>,
    duration_ms: Option<u64>,
    cache_ahead_secs: u32,
) -> u64 {
    let ahead_secs = u64::from(cache_ahead_secs.clamp(5, 300));
    let min_prefetch = 256 * 1024;
    let max_prefetch = 32 * 1024 * 1024;

    let estimated = match (content_length, duration_ms.filter(|duration| *duration > 0)) {
        (Some(content_length), Some(duration_ms)) => {
            let bytes_per_second =
                ((content_length as u128) * 1000 / u128::from(duration_ms)).max(1) as u64;
            bytes_per_second.saturating_mul(ahead_secs)
        }
        _ => ahead_secs.saturating_mul(256 * 1024),
    };

    estimated.clamp(min_prefetch, max_prefetch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn create_state(sample_rate: u32) -> SharedState {
        SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            sample_rate: AtomicU32::new(sample_rate),
            has_seek_request: AtomicBool::new(false),
            seek_request: Mutex::new(None),
            is_terminating: AtomicBool::new(false),
            discard_buffer: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            is_finished: AtomicBool::new(false),
            finish_notify: Notify::new(),
            buffered_frames: AtomicU64::new(0),
            waiting_for_seek: AtomicBool::new(false),
        }
    }

    #[test]
    fn preferred_output_formats_prioritizes_source_format() {
        let formats =
            AudioPlayer::preferred_output_formats(Some(24), Some(SymphoniaSampleFormat::F32));

        assert_eq!(formats[0], cpal::SampleFormat::F32);
        assert_eq!(formats[1], cpal::SampleFormat::F64);
        assert!(formats.contains(&cpal::SampleFormat::I24));
    }

    #[test]
    fn preferred_output_formats_keeps_fallbacks_unique() {
        let formats =
            AudioPlayer::preferred_output_formats(Some(16), Some(SymphoniaSampleFormat::S16));

        let mut unique_formats = Vec::new();
        for format in &formats {
            if !unique_formats.contains(format) {
                unique_formats.push(*format);
            }
        }

        let unique_count = unique_formats.len();
        assert_eq!(unique_count, formats.len());
        assert_eq!(formats[0], cpal::SampleFormat::I16);
        assert!(formats.contains(&cpal::SampleFormat::F32));
    }

    #[test]
    fn schedule_seek_updates_pending_request_and_progress() {
        let state = create_state(48_000);
        let target = Duration::from_millis(2_500);

        AudioPlayer::schedule_seek(&state, target);

        assert_eq!(*state.seek_request.lock().unwrap(), Some(target));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 120_000);
    }

    #[test]
    fn schedule_seek_handles_zero_position() {
        let state = Arc::new(create_state(44_100));
        AudioPlayer::schedule_seek(&state, Duration::ZERO);

        assert_eq!(*state.seek_request.lock().unwrap(), Some(Duration::ZERO));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 0);
        assert!(state.has_seek_request.load(Ordering::SeqCst));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_plughw_locator_rewrites_hw_device_ids() {
        assert_eq!(
            AudioPlayer::linux_plughw_locator("alsa:hw:CARD=Device,DEV=0").as_deref(),
            Some("plughw:CARD=Device,DEV=0")
        );
        assert_eq!(
            AudioPlayer::linux_plughw_locator("hw:CARD=Device,DEV=1").as_deref(),
            Some("plughw:CARD=Device,DEV=1")
        );
        assert_eq!(
            AudioPlayer::linux_plughw_locator("plughw:CARD=Device,DEV=0"),
            None
        );
        assert_eq!(AudioPlayer::linux_plughw_locator("default"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_output_device_filter_skips_virtual_defaults_but_keeps_real_devices() {
        assert!(AudioPlayer::should_list_linux_output_device(
            "hw:CARD=0,DEV=0",
            Some("default"),
            Some("default")
        ));
        assert!(!AudioPlayer::should_list_linux_output_device(
            "default",
            Some("default"),
            Some("default")
        ));
        assert!(AudioPlayer::should_list_linux_output_device(
            "pulse",
            Some("pulse"),
            Some("default")
        ));
        assert!(!AudioPlayer::should_list_linux_output_device(
            "pulse",
            Some("hw:CARD=0,DEV=0"),
            Some("default")
        ));
    }

    #[test]
    fn duplicate_output_device_names_are_disambiguated_with_ids() {
        let mut devices = vec![
            OutputDeviceInfo {
                id: "hw:CARD=0,DEV=0".to_string(),
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: false,
            },
            OutputDeviceInfo {
                id: "hw:CARD=1,DEV=0".to_string(),
                name: "USB DAC".to_string(),
                is_default: false,
                is_current: true,
            },
            OutputDeviceInfo {
                id: "default".to_string(),
                name: "System Default".to_string(),
                is_default: true,
                is_current: false,
            },
        ];

        AudioPlayer::disambiguate_output_device_names(&mut devices);

        assert_eq!(devices[0].name, "USB DAC [hw:CARD=0,DEV=0]");
        assert_eq!(devices[1].name, "USB DAC [hw:CARD=1,DEV=0]");
        assert_eq!(devices[2].name, "System Default");
    }

    #[test]
    fn persistent_file_storage_provider_keeps_reader_and_writer_offsets_independent() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (mut reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .into_reader_writer(None)
            .unwrap();

        writer.write_all(b"ID3abcdef").unwrap();
        writer.flush().unwrap();
        writer.seek(SeekFrom::End(0)).unwrap();

        let mut header = [0u8; 3];
        reader.read_exact(&mut header).unwrap();

        assert_eq!(&header, b"ID3");
        let _ = std::fs::remove_file(path);
    }
}
