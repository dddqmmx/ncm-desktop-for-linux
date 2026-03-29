use cpal::traits::{HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use std::path::Path;
use std::num::NonZeroUsize;
use symphonia::core::conv::ConvertibleSample;
use tokio::sync::Notify;
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::{Settings, StreamDownload};
use stream_download::source::SourceStream;
use stream_download::storage::adaptive::AdaptiveStorageProvider;
use stream_download::storage::temp::TempStorageProvider;

use crate::audio::state::SharedState;
use crate::audio::decoder::{self, AudioMetadata};
use crate::audio::backend::{self, OutputDeviceInfo};
use crate::audio::source::{SeekableSource, PersistentFileStorageProvider};
use crate::audio::cache_tracker::SongCacheTracker;
use crate::audio::utils::estimate_prefetch_bytes;

pub struct AudioPlayer {
    device: cpal::Device,
    stream: Option<cpal::Stream>,
    state: Arc<SharedState>,
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
            stream: None,
            state: Arc::new(SharedState {
                is_paused: AtomicBool::new(false),
                current_frame: AtomicU64::new(0),
                sample_rate: AtomicU32::new(0),
                seek_request: std::sync::Mutex::new(None),
                is_terminating: AtomicBool::new(false),
                discard_buffer: AtomicBool::new(false),
                decoder_done: AtomicBool::new(false),
                is_finished: AtomicBool::new(false),
                finish_notify: Notify::new(),
                buffered_frames: AtomicU64::new(0),
                waiting_for_seek: AtomicBool::new(false),
                has_seek_request: AtomicBool::new(false),
            }),
        })
    }

    pub fn list_output_devices() -> Vec<OutputDeviceInfo> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_id = default_device.as_ref().map(backend::device_id);

        let mut devices = Vec::new();
        if let Ok(it) = host.output_devices() {
            for d in it {
                let id = backend::device_id(&d);
                let name = backend::device_display_name(&d);

                #[cfg(target_os = "linux")]
                if !backend::should_list_linux_output_device(&id, default_id.as_deref(), None) {
                    continue;
                }

                devices.push(OutputDeviceInfo {
                    id,
                    name,
                    is_default: default_id.as_ref().map_or(false, |did| did == &backend::device_id(&d)),
                    is_current: false,
                });
            }
        }

        backend::disambiguate_output_device_names(&mut devices);
        devices
    }

    pub fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();
        let default_id = default_device.as_ref().map(|d| backend::device_id(d));
        let current_id = backend::device_id(&self.device);

        let mut devices = Vec::new();
        for d in host.output_devices()? {
            let id = backend::device_id(&d);
            let name = backend::device_display_name(&d);

            #[cfg(target_os = "linux")]
            if !backend::should_list_linux_output_device(&id, default_id.as_deref(), Some(&current_id)) {
                continue;
            }

            devices.push(OutputDeviceInfo {
                id: id.clone(),
                name,
                is_default: default_id.as_deref() == Some(&id),
                is_current: id == current_id,
            });
        }

        backend::disambiguate_output_device_names(&mut devices);
        Ok(devices)
    }

    pub fn set_output_device(&mut self, device_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        
        #[cfg(target_os = "linux")]
        let target_id = backend::linux_plughw_locator(device_id).unwrap_or_else(|| device_id.to_string());
        #[cfg(not(target_os = "linux"))]
        let target_id = device_id;

        let device = host.output_devices()?.find(|d| backend::device_id(d) == target_id);
        
        if let Some(d) = device {
            self.device = d;
        } else {
            return Err(format!("Device not found: {}", device_id).into());
        }
        Ok(())
    }

    pub async fn play_url(
        &mut self,
        url: &str,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
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

        let content_len = reader.content_length();
        let source = Box::new(SeekableSource::new(reader, content_len));
        let meta = decoder::spawn_probe_task(source, extension).await?;
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let extension = Path::new(url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let stream = HttpStream::<Client>::create(url.parse()?).await?;
        let content_len = stream.content_length();
        let tracker = SongCacheTracker::new(metadata_path)?;
        tracker.set_content_length(content_len)?;

        let prefetch_bytes = estimate_prefetch_bytes(content_len, duration_ms, cache_ahead_secs.unwrap_or(30));
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
        let meta = decoder::spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at)
    }

    pub async fn play_file(
        &mut self,
        path: &str,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path_buf = Path::new(path);
        let extension = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let file = std::fs::File::open(path_buf)?;
        let source = Box::new(file);

        let meta = decoder::spawn_probe_task(source, extension).await?;
        self.setup_and_play(meta, start_at)
    }

    pub(crate) fn setup_and_play(
        &mut self,
        meta: AudioMetadata,
        start_at: Option<Duration>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.stop();

        self.state = Arc::new(SharedState {
            is_paused: AtomicBool::new(false),
            current_frame: AtomicU64::new(0),
            sample_rate: AtomicU32::new(meta.sample_rate),
            seek_request: std::sync::Mutex::new(None),
            is_terminating: AtomicBool::new(false),
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

        let (config, sample_format) = match backend::find_best_config(
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
                    ).map_err(|fallback_err| {
                        format!("No compatible output config: primary={}; fallback={}", primary_msg, fallback_err)
                    })?
                } else {
                    return Err(primary_msg.into());
                }
            }
        };

        let state_for_cb = self.state.clone();
        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                let rb = HeapRb::<i16>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<i16, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<i16>(meta, producer);
                stream
            }
            cpal::SampleFormat::U16 => {
                let rb = HeapRb::<u16>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<u16, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<u16>(meta, producer);
                stream
            }
            cpal::SampleFormat::I8 => {
                let rb = HeapRb::<i8>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<i8, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<i8>(meta, producer);
                stream
            }
            cpal::SampleFormat::U8 => {
                let rb = HeapRb::<u8>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<u8, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<u8>(meta, producer);
                stream
            }
            cpal::SampleFormat::I24 => {
                let rb = HeapRb::<i32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream_converted::<i32, cpal::I24, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U24 => {
                let rb = HeapRb::<u32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream_converted::<u32, cpal::U24, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::I32 => {
                let rb = HeapRb::<i32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<i32, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<i32>(meta, producer);
                stream
            }
            cpal::SampleFormat::U32 => {
                let rb = HeapRb::<u32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<u32, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<u32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F32 => {
                let rb = HeapRb::<f32>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<f32, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
                self.start_decode_thread::<f32>(meta, producer);
                stream
            }
            cpal::SampleFormat::F64 => {
                let rb = HeapRb::<f64>::new(sr as usize * channels as usize * 2);
                let (producer, consumer) = rb.split();
                let stream = backend::build_stream::<f64, _>(&self.device, &config, consumer, state_for_cb, channels as usize)?;
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

        std::thread::spawn(move || {
            loop {
                if state.is_terminating.load(Ordering::Relaxed) {
                    break;
                }

                decoder::handle_seek_if_needed(&state, &mut *format, &mut *decoder, track_id, sr);

                if !producer.is_full() {
                    if !decoder::decode_next_packet::<S, _>(
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

    pub fn seek(&self, target: Duration) {
        {
            let mut seek_req = self.state.seek_request.lock().unwrap();
            *seek_req = Some(target);
        }

        let target_frame =
            (target.as_secs_f64() * self.state.sample_rate.load(Ordering::Relaxed) as f64) as u64;
        self.state.current_frame.store(target_frame, Ordering::SeqCst);
        self.state.waiting_for_seek.store(true, Ordering::SeqCst);
        self.state.has_seek_request.store(true, Ordering::SeqCst);
    }

    pub fn stop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.stream = None;
        self.state.is_finished.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters();
    }

    pub fn is_finished(&self) -> bool {
        self.state.is_finished.load(Ordering::Relaxed)
    }

    pub(crate) fn get_state(&self) -> Arc<SharedState> {
        self.state.clone()
    }

    pub async fn wait_finished(&self) {
        if self.is_finished() {
            return;
        }
        self.state.finish_notify.notified().await;
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.state.is_terminating.store(true, Ordering::SeqCst);
        self.state.finish_notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};

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
    fn schedule_seek_updates_pending_request_and_progress() {
        let state = create_state(48_000);
        let target = Duration::from_millis(2_500);

        {
            let mut seek_req = state.seek_request.lock().unwrap();
            *seek_req = Some(target);
        }
        let target_frame = (target.as_secs_f64() * state.sample_rate.load(Ordering::Relaxed) as f64) as u64;
        state.current_frame.store(target_frame, Ordering::SeqCst);
        state.waiting_for_seek.store(true, Ordering::SeqCst);
        state.has_seek_request.store(true, Ordering::SeqCst);

        assert_eq!(*state.seek_request.lock().unwrap(), Some(target));
        assert!(state.has_seek_request.load(Ordering::SeqCst));
        assert!(state.waiting_for_seek.load(Ordering::SeqCst));
        assert_eq!(state.current_frame.load(Ordering::SeqCst), 120_000);
    }
}
