mod audio;
mod cache;

use napi::{Error, Result};
use napi_derive::napi;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::audio::{AudioPlayer, OutputDeviceInfo};
use crate::cache::{CacheBucket, CacheStats, CachedSongSource, NativeCacheService};

type BackendResult<T> = std::result::Result<T, String>;
type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = BackendResult<T>> + Send + 'a>>;
type SignalFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

// --- 全局静态运行时 ---
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("audio-worker")
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PlaybackSource {
    File(String),
    Url(String),
    CachedUrl(CachedUrlPlaybackRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedUrlPlaybackRequest {
    url: String,
    cache_path: String,
    metadata_path: String,
    duration_ms: Option<u64>,
    cache_ahead_secs: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

impl PlaybackStatus {
    fn as_u32(self) -> u32 {
        match self {
            Self::Stopped => 0,
            Self::Playing => 1,
            Self::Paused => 2,
        }
    }

    fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::Playing,
            2 => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_current: bool,
}

impl From<OutputDeviceInfo> for AudioDeviceInfo {
    fn from(value: OutputDeviceInfo) -> Self {
        Self {
            id: value.id,
            name: value.name,
            is_default: value.is_default,
            is_current: value.is_current,
        }
    }
}

enum PlayerCommand {
    PlayFile(String, Option<f64>),
    PlayUrl(String, Option<f64>),
    PlayUrlCached(CachedUrlPlaybackRequest, Option<f64>),
    Pause,
    Resume,
    Stop,
    Seek(f64),
    SwitchOutputDevice(Option<String>, oneshot::Sender<BackendResult<()>>),
    GetOutputDevices(oneshot::Sender<BackendResult<Vec<AudioDeviceInfo>>>),
    WaitFinished(oneshot::Sender<()>),
}

struct SharedState {
    progress_ms: AtomicU32,
    is_playing: AtomicU32, // 0: Stopped, 1: Playing, 2: Paused
}

trait PlayerBackend: Send {
    fn play_file<'a>(
        &'a mut self,
        path: &'a str,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()>;
    fn play_url<'a>(
        &'a mut self,
        url: &'a str,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()>;
    fn play_url_cached<'a>(
        &'a mut self,
        request: &'a CachedUrlPlaybackRequest,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()>;
    fn pause(&self);
    fn resume(&self);
    fn stop(&mut self);
    fn seek(&self, target: Duration);
    fn progress(&self) -> Duration;
    fn is_finished(&self) -> bool;
    fn wait_finished_signal(&self) -> SignalFuture;
    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>>;
}

trait PlayerFactory: Send + Sync + 'static {
    type Player: PlayerBackend;

    fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player>;
}

#[derive(Clone, Copy)]
struct AudioPlayerFactory;

struct NativePlayer(AudioPlayer);

impl PlayerBackend for NativePlayer {
    fn play_file<'a>(
        &'a mut self,
        path: &'a str,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_file(path, start_at)
                .await
                .map_err(|err| err.to_string())
        })
    }

    fn play_url<'a>(
        &'a mut self,
        url: &'a str,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_url(url, start_at)
                .await
                .map_err(|err| err.to_string())
        })
    }

    fn play_url_cached<'a>(
        &'a mut self,
        request: &'a CachedUrlPlaybackRequest,
        start_at: Option<Duration>,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_url_cached(
                    &request.url,
                    &request.cache_path,
                    &request.metadata_path,
                    request.duration_ms,
                    request.cache_ahead_secs,
                    start_at,
                )
                .await
                .map_err(|err| err.to_string())
        })
    }

    fn pause(&self) {
        self.0.pause();
    }

    fn resume(&self) {
        self.0.resume();
    }

    fn stop(&mut self) {
        self.0.stop();
    }

    fn seek(&self, target: Duration) {
        self.0.seek(target);
    }

    fn progress(&self) -> Duration {
        self.0.progress()
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn wait_finished_signal(&self) -> SignalFuture {
        let state = self.0.get_state();
        Box::pin(async move {
            if state.is_finished.load(Ordering::Relaxed) {
                return;
            }
            state.finish_notify.notified().await;
        })
    }

    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>> {
        self.0.output_devices().map_err(|err| err.to_string())
    }
}

impl PlayerFactory for AudioPlayerFactory {
    type Player = NativePlayer;

    fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player> {
        AudioPlayer::new(device_name)
            .map(NativePlayer)
            .map_err(|err| err.to_string())
    }
}

struct WorkerCore<P, F> {
    player: P,
    factory: F,
    shared_state: Arc<SharedState>,
    current_source: Option<PlaybackSource>,
}

impl<P, F> WorkerCore<P, F>
where
    P: PlayerBackend,
    F: PlayerFactory<Player = P>,
{
    fn new(player: P, factory: F, shared_state: Arc<SharedState>) -> Self {
        Self {
            player,
            factory,
            shared_state,
            current_source: None,
        }
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                cmd_opt = rx.recv() => {
                    match cmd_opt {
                        Some(cmd) => self.handle_command(cmd).await,
                        None => break,
                    }
                }
                _ = ticker.tick() => self.tick(),
            }
        }
    }

    async fn handle_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::PlayFile(path, start_secs) => {
                if let Err(err) = self
                    .play_source(
                        PlaybackSource::File(path),
                        start_secs_to_duration(start_secs),
                    )
                    .await
                {
                    eprintln!("Play file failed: {}", err);
                }
            }
            PlayerCommand::PlayUrl(url, start_secs) => {
                if let Err(err) = self
                    .play_source(PlaybackSource::Url(url), start_secs_to_duration(start_secs))
                    .await
                {
                    eprintln!("Play URL failed: {}", err);
                }
            }
            PlayerCommand::PlayUrlCached(request, start_secs) => {
                if let Err(err) = self
                    .play_source(
                        PlaybackSource::CachedUrl(request),
                        start_secs_to_duration(start_secs),
                    )
                    .await
                {
                    eprintln!("Play cached URL failed: {}", err);
                }
            }
            PlayerCommand::Pause => {
                self.player.pause();
                self.shared_state
                    .is_playing
                    .store(PlaybackStatus::Paused.as_u32(), Ordering::SeqCst);
            }
            PlayerCommand::Resume => {
                self.player.resume();
                self.shared_state
                    .is_playing
                    .store(PlaybackStatus::Playing.as_u32(), Ordering::SeqCst);
            }
            PlayerCommand::Stop => {
                self.player.stop();
                self.current_source = None;
                self.shared_state
                    .is_playing
                    .store(PlaybackStatus::Stopped.as_u32(), Ordering::SeqCst);
                self.shared_state.progress_ms.store(0, Ordering::SeqCst);
            }
            PlayerCommand::Seek(time_secs) => {
                self.player.seek(seconds_to_duration(time_secs));
            }
            PlayerCommand::SwitchOutputDevice(device_name, reply_tx) => {
                let result = self
                    .switch_output_device(normalize_device_name(device_name))
                    .await;
                let _ = reply_tx.send(result);
            }
            PlayerCommand::GetOutputDevices(reply_tx) => {
                let result = self
                    .player
                    .output_devices()
                    .map(|devices| devices.into_iter().map(AudioDeviceInfo::from).collect());
                let _ = reply_tx.send(result);
            }
            PlayerCommand::WaitFinished(done_tx) => {
                let wait_signal = self.player.wait_finished_signal();
                tokio::spawn(async move {
                    wait_signal.await;
                    let _ = done_tx.send(());
                });
            }
        }
    }

    async fn play_source(
        &mut self,
        source: PlaybackSource,
        start_at: Option<Duration>,
    ) -> BackendResult<()> {
        self.shared_state.progress_ms.store(
            duration_to_millis(start_at.unwrap_or(Duration::ZERO)),
            Ordering::SeqCst,
        );

        match Self::play_source_on(&mut self.player, &source, start_at).await {
            Ok(()) => {
                self.current_source = Some(source);
                self.shared_state
                    .is_playing
                    .store(PlaybackStatus::Playing.as_u32(), Ordering::SeqCst);
                Ok(())
            }
            Err(err) => {
                self.player.stop();
                self.current_source = None;
                self.shared_state
                    .is_playing
                    .store(PlaybackStatus::Stopped.as_u32(), Ordering::SeqCst);
                self.shared_state.progress_ms.store(0, Ordering::SeqCst);
                Err(err)
            }
        }
    }

    async fn play_source_on(
        player: &mut P,
        source: &PlaybackSource,
        start_at: Option<Duration>,
    ) -> BackendResult<()> {
        match source {
            PlaybackSource::File(path) => player.play_file(path, start_at).await,
            PlaybackSource::Url(url) => player.play_url(url, start_at).await,
            PlaybackSource::CachedUrl(request) => player.play_url_cached(request, start_at).await,
        }
    }

    fn is_requested_output_device_already_active(
        player: &P,
        device_name: Option<&str>,
    ) -> BackendResult<bool> {
        let devices = player.output_devices()?;
        let current_device = devices.iter().find(|device| device.is_current);

        match (device_name, current_device) {
            (Some(target_device_id), Some(current_device)) => Ok(current_device.id == target_device_id
                || current_device.name == target_device_id),
            (None, Some(current_device)) => Ok(current_device.is_default),
            _ => Ok(false),
        }
    }

    async fn switch_output_device(&mut self, device_name: Option<String>) -> BackendResult<()> {
        if Self::is_requested_output_device_already_active(&self.player, device_name.as_deref())? {
            return Ok(());
        }

        let playback_status = self.playback_status();
        let resume_source = self.current_source.clone();
        let resume_position = match playback_status {
            PlaybackStatus::Stopped => None,
            PlaybackStatus::Playing | PlaybackStatus::Paused => Some(self.player.progress()),
        };

        let mut next_player = self.factory.create(device_name.as_deref())?;

        if let Some(source) = resume_source.as_ref() {
            Self::play_source_on(&mut next_player, source, resume_position).await?;
            if playback_status == PlaybackStatus::Paused {
                next_player.pause();
            }
        }

        self.player.stop();
        self.player = next_player;

        if let Some(position) = resume_position {
            self.shared_state
                .progress_ms
                .store(duration_to_millis(position), Ordering::SeqCst);
        }
        self.shared_state
            .is_playing
            .store(playback_status.as_u32(), Ordering::SeqCst);

        Ok(())
    }

    fn tick(&mut self) {
        let playback_status = self.playback_status();
        if playback_status == PlaybackStatus::Stopped {
            return;
        }

        let progress = self.player.progress();
        self.shared_state
            .progress_ms
            .store(duration_to_millis(progress), Ordering::Relaxed);

        if playback_status == PlaybackStatus::Playing && self.player.is_finished() {
            self.current_source = None;
            self.shared_state
                .is_playing
                .store(PlaybackStatus::Stopped.as_u32(), Ordering::SeqCst);
        }
    }

    fn playback_status(&self) -> PlaybackStatus {
        PlaybackStatus::from_u32(self.shared_state.is_playing.load(Ordering::Relaxed))
    }
}

fn normalize_device_name(device_name: Option<String>) -> Option<String> {
    device_name.and_then(|name| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn seconds_to_duration(seconds: f64) -> Duration {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Duration::ZERO;
    }

    Duration::from_secs_f64(seconds)
}

fn start_secs_to_duration(start_secs: Option<f64>) -> Option<Duration> {
    start_secs.map(seconds_to_duration)
}

fn duration_to_millis(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}

#[napi]
pub struct PlayerService {
    sender: mpsc::UnboundedSender<PlayerCommand>,
    shared_state: Arc<SharedState>,
}

#[napi]
impl PlayerService {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<PlayerCommand>();
        let shared_state = Arc::new(SharedState {
            progress_ms: AtomicU32::new(0),
            is_playing: AtomicU32::new(PlaybackStatus::Stopped.as_u32()),
        });

        let factory = AudioPlayerFactory;
        let player = factory.create(None).map_err(Error::from_reason)?;
        let worker = WorkerCore::new(player, factory, Arc::clone(&shared_state));

        get_runtime().spawn(worker.run(rx));

        Ok(Self {
            sender: tx,
            shared_state,
        })
    }

    #[napi]
    pub fn play_file(&self, path: String, start_secs: Option<f64>) -> Result<()> {
        self.sender
            .send(PlayerCommand::PlayFile(path, start_secs))
            .map_err(|_| Error::from_reason("Background worker died"))
    }

    #[napi]
    pub fn play_url(&self, url: String, start_secs: Option<f64>) -> Result<()> {
        self.sender
            .send(PlayerCommand::PlayUrl(url, start_secs))
            .map_err(|_| Error::from_reason("Background worker died"))
    }

    #[napi]
    pub fn play_url_cached(
        &self,
        url: String,
        cache_path: String,
        metadata_path: String,
        duration_ms: Option<i64>,
        cache_ahead_secs: Option<u32>,
        start_secs: Option<f64>,
    ) -> Result<()> {
        self.sender
            .send(PlayerCommand::PlayUrlCached(
                CachedUrlPlaybackRequest {
                    url,
                    cache_path,
                    metadata_path,
                    duration_ms: duration_ms.map(|value| value.max(0) as u64),
                    cache_ahead_secs,
                },
                start_secs,
            ))
            .map_err(|_| Error::from_reason("Background worker died"))
    }

    #[napi]
    pub fn pause(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Pause);
        Ok(())
    }

    #[napi]
    pub fn resume(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Resume);
        Ok(())
    }

    #[napi]
    pub fn stop(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Stop);
        Ok(())
    }

    #[napi]
    pub fn seek(&self, time_secs: f64) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Seek(time_secs));
        Ok(())
    }

    #[napi]
    pub async fn switch_output_device(&self, device_id: Option<String>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::SwitchOutputDevice(device_id, tx))
            .map_err(|_| Error::from_reason("Background worker died"))?;

        rx.await
            .map_err(|_| Error::from_reason("Device switch interrupted"))?
            .map_err(Error::from_reason)
    }

    #[napi]
    pub async fn get_output_devices(&self) -> Result<Vec<AudioDeviceInfo>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::GetOutputDevices(tx))
            .map_err(|_| Error::from_reason("Background worker died"))?;

        rx.await
            .map_err(|_| Error::from_reason("Device query interrupted"))?
            .map_err(Error::from_reason)
    }

    // --- Getter 属性：JS 通过 player.progressMs 访问 ---

    #[napi(getter)]
    pub fn progress_ms(&self) -> u32 {
        self.shared_state.progress_ms.load(Ordering::Relaxed)
    }

    #[napi(getter)]
    pub fn is_playing(&self) -> bool {
        self.shared_state.is_playing.load(Ordering::Relaxed) == PlaybackStatus::Playing.as_u32()
    }

    #[napi(getter)]
    pub fn is_paused(&self) -> bool {
        self.shared_state.is_playing.load(Ordering::Relaxed) == PlaybackStatus::Paused.as_u32()
    }

    // --- 异步 API ---

    #[napi]
    pub async fn wait_finished(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::WaitFinished(tx))
            .map_err(|_| Error::from_reason("Worker shutdown"))?;
        rx.await
            .map_err(|_| Error::from_reason("Playback task interrupted"))
    }
}

#[napi]
pub struct CacheService {
    service: Arc<NativeCacheService>,
}

#[napi]
impl CacheService {
    #[napi(constructor)]
    pub fn new(root_dir: String, max_size_bytes: Option<i64>) -> Result<Self> {
        let root_dir = PathBuf::from(root_dir);
        let fallback_max_size_bytes =
            max_size_bytes.unwrap_or((512 * 1024 * 1024) as i64).max(0) as u64;
        let service = NativeCacheService::new(root_dir, fallback_max_size_bytes)
            .map_err(|err| Error::from_reason(err.to_string()))?;

        Ok(Self {
            service: Arc::new(service),
        })
    }

    #[napi]
    pub fn get_stats(&self) -> Result<CacheStats> {
        self.service
            .get_stats()
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn get_json(&self, bucket: String, key: String) -> Result<Option<String>> {
        let bucket = CacheBucket::try_from(bucket.as_str())
            .map_err(|err| Error::from_reason(err.to_string()))?;

        self.service
            .get_json(bucket, &key)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn put_json(&self, bucket: String, key: String, value: String) -> Result<CacheStats> {
        let bucket = CacheBucket::try_from(bucket.as_str())
            .map_err(|err| Error::from_reason(err.to_string()))?;

        self.service
            .put_json(bucket, &key, &value)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn set_max_size_bytes(&self, max_size_bytes: i64) -> Result<CacheStats> {
        self.service
            .set_max_size_bytes(max_size_bytes.max(0) as u64)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn get_song_cache_ahead_secs(&self) -> Result<u32> {
        self.service
            .get_song_cache_ahead_secs()
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn set_song_cache_ahead_secs(&self, song_cache_ahead_secs: u32) -> Result<u32> {
        self.service
            .set_song_cache_ahead_secs(song_cache_ahead_secs)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn clear(&self) -> Result<CacheStats> {
        self.service
            .clear()
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn cache_remote_file(
        &self,
        bucket: String,
        key: String,
        url: String,
    ) -> Result<Option<String>> {
        let bucket = CacheBucket::try_from(bucket.as_str())
            .map_err(|err| Error::from_reason(err.to_string()))?;

        self.service
            .cache_remote_file(bucket, &key, &url)
            .await
            .map(|path| path.map(|path| path.to_string_lossy().into_owned()))
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub fn prepare_song_source(
        &self,
        song_id: i64,
        quality: String,
        url: String,
    ) -> Result<CachedSongSource> {
        self.service
            .prepare_song_source(song_id, &quality, &url)
            .map_err(|err| Error::from_reason(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::Notify;

    #[derive(Clone)]
    struct MockFactory {
        events: Arc<Mutex<Vec<String>>>,
        fail_device: Option<String>,
        devices: Vec<OutputDeviceInfo>,
    }

    impl MockFactory {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                fail_device: None,
                devices: vec![
                    OutputDeviceInfo {
                        id: "speaker".to_string(),
                        name: "Speaker".to_string(),
                        is_default: true,
                        is_current: false,
                    },
                    OutputDeviceInfo {
                        id: "headphones".to_string(),
                        name: "Headphones".to_string(),
                        is_default: false,
                        is_current: false,
                    },
                ],
            }
        }

        fn with_fail_device(device_id: &str) -> Self {
            let mut factory = Self::new();
            factory.fail_device = Some(device_id.to_string());
            factory
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl PlayerFactory for MockFactory {
        type Player = MockPlayer;

        fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player> {
            let label = device_name.unwrap_or("auto").to_string();
            self.events.lock().unwrap().push(format!("create:{label}"));

            if self
                .fail_device
                .as_deref()
                .is_some_and(|fail_device| Some(fail_device) == device_name)
            {
                return Err(format!("failed to create device: {label}"));
            }

            Ok(MockPlayer::new(
                device_name.map(str::to_string),
                self.devices.clone(),
                Arc::clone(&self.events),
            ))
        }
    }

    struct MockPlayer {
        device_name: Option<String>,
        devices: Vec<OutputDeviceInfo>,
        events: Arc<Mutex<Vec<String>>>,
        progress: Arc<Mutex<Duration>>,
        finished: Arc<AtomicBool>,
        finish_notify: Arc<Notify>,
    }

    impl MockPlayer {
        fn new(
            device_name: Option<String>,
            devices: Vec<OutputDeviceInfo>,
            events: Arc<Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                device_name,
                devices,
                events,
                progress: Arc::new(Mutex::new(Duration::ZERO)),
                finished: Arc::new(AtomicBool::new(false)),
                finish_notify: Arc::new(Notify::new()),
            }
        }

        fn label(&self) -> &str {
            self.device_name.as_deref().unwrap_or("auto")
        }

        fn log(&self, message: String) {
            self.events.lock().unwrap().push(message);
        }

        fn set_progress(&self, progress: Duration) {
            *self.progress.lock().unwrap() = progress;
        }

        fn set_finished(&self, finished: bool) {
            self.finished.store(finished, Ordering::SeqCst);
            if finished {
                self.finish_notify.notify_waiters();
            }
        }
    }

    impl PlayerBackend for MockPlayer {
        fn play_file<'a>(
            &'a mut self,
            path: &'a str,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let label = self.label().to_string();
            let path = path.to_string();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{label}] play_file:{path}@{}",
                duration_to_millis(start_at)
            ));
            self.set_progress(start_at);
            self.set_finished(false);

            Box::pin(async { Ok(()) })
        }

        fn play_url<'a>(
            &'a mut self,
            url: &'a str,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let label = self.label().to_string();
            let url = url.to_string();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{label}] play_url:{url}@{}",
                duration_to_millis(start_at)
            ));
            self.set_progress(start_at);
            self.set_finished(false);

            Box::pin(async { Ok(()) })
        }

        fn play_url_cached<'a>(
            &'a mut self,
            request: &'a CachedUrlPlaybackRequest,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let label = self.label().to_string();
            let url = request.url.clone();
            let cache_path = request.cache_path.clone();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{label}] play_url_cached:{url}->{cache_path}@{}",
                duration_to_millis(start_at)
            ));
            self.set_progress(start_at);
            self.set_finished(false);

            Box::pin(async { Ok(()) })
        }

        fn pause(&self) {
            self.log(format!("player[{}] pause", self.label()));
        }

        fn resume(&self) {
            self.log(format!("player[{}] resume", self.label()));
        }

        fn stop(&mut self) {
            self.log(format!("player[{}] stop", self.label()));
            self.set_finished(true);
        }

        fn seek(&self, target: Duration) {
            self.log(format!(
                "player[{}] seek:{}",
                self.label(),
                duration_to_millis(target)
            ));
            self.set_progress(target);
        }

        fn progress(&self) -> Duration {
            *self.progress.lock().unwrap()
        }

        fn is_finished(&self) -> bool {
            self.finished.load(Ordering::SeqCst)
        }

        fn wait_finished_signal(&self) -> SignalFuture {
            let finished = Arc::clone(&self.finished);
            let notify = Arc::clone(&self.finish_notify);
            Box::pin(async move {
                if finished.load(Ordering::SeqCst) {
                    return;
                }
                notify.notified().await;
            })
        }

        fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>> {
            let current_id = self.device_name.as_deref().unwrap_or("speaker");
            Ok(self
                .devices
                .iter()
                .cloned()
                .map(|mut device| {
                    device.is_current = device.id == current_id;
                    device
                })
                .collect())
        }
    }

    #[derive(Clone)]
    struct BusyFactory {
        active_devices: Arc<Mutex<HashMap<String, usize>>>,
        events: Arc<Mutex<Vec<String>>>,
        devices: Vec<OutputDeviceInfo>,
    }

    impl BusyFactory {
        fn new() -> Self {
            Self {
                active_devices: Arc::new(Mutex::new(HashMap::new())),
                events: Arc::new(Mutex::new(Vec::new())),
                devices: vec![
                    OutputDeviceInfo {
                        id: "speaker".to_string(),
                        name: "Speaker".to_string(),
                        is_default: true,
                        is_current: false,
                    },
                    OutputDeviceInfo {
                        id: "headphones".to_string(),
                        name: "Headphones".to_string(),
                        is_default: false,
                        is_current: false,
                    },
                ],
            }
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl PlayerFactory for BusyFactory {
        type Player = BusyMockPlayer;

        fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player> {
            let device_id = device_name.unwrap_or("speaker").to_string();
            self.events
                .lock()
                .unwrap()
                .push(format!("create:{device_id}"));

            let mut active_devices = self.active_devices.lock().unwrap();
            let active_count = active_devices.entry(device_id.clone()).or_insert(0);
            if *active_count > 0 {
                return Err(format!("device busy: {device_id}"));
            }
            *active_count += 1;
            drop(active_devices);

            Ok(BusyMockPlayer::new(
                device_id,
                self.devices.clone(),
                Arc::clone(&self.events),
                Arc::clone(&self.active_devices),
            ))
        }
    }

    struct BusyMockPlayer {
        device_id: String,
        devices: Vec<OutputDeviceInfo>,
        events: Arc<Mutex<Vec<String>>>,
        active_devices: Arc<Mutex<HashMap<String, usize>>>,
        progress: Arc<Mutex<Duration>>,
        finished: Arc<AtomicBool>,
        finish_notify: Arc<Notify>,
        released: bool,
    }

    impl BusyMockPlayer {
        fn new(
            device_id: String,
            devices: Vec<OutputDeviceInfo>,
            events: Arc<Mutex<Vec<String>>>,
            active_devices: Arc<Mutex<HashMap<String, usize>>>,
        ) -> Self {
            Self {
                device_id,
                devices,
                events,
                active_devices,
                progress: Arc::new(Mutex::new(Duration::ZERO)),
                finished: Arc::new(AtomicBool::new(false)),
                finish_notify: Arc::new(Notify::new()),
                released: false,
            }
        }

        fn log(&self, message: String) {
            self.events.lock().unwrap().push(message);
        }

        fn release_device(&mut self) {
            if self.released {
                return;
            }

            let mut active_devices = self.active_devices.lock().unwrap();
            if let Some(active_count) = active_devices.get_mut(&self.device_id) {
                *active_count = active_count.saturating_sub(1);
                if *active_count == 0 {
                    active_devices.remove(&self.device_id);
                }
            }
            self.released = true;
        }
    }

    impl Drop for BusyMockPlayer {
        fn drop(&mut self) {
            self.release_device();
        }
    }

    impl PlayerBackend for BusyMockPlayer {
        fn play_file<'a>(
            &'a mut self,
            path: &'a str,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let path = path.to_string();
            let device_id = self.device_id.clone();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{device_id}] play_file:{path}@{}",
                duration_to_millis(start_at)
            ));
            *self.progress.lock().unwrap() = start_at;
            self.finished.store(false, Ordering::SeqCst);

            Box::pin(async { Ok(()) })
        }

        fn play_url<'a>(
            &'a mut self,
            url: &'a str,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let url = url.to_string();
            let device_id = self.device_id.clone();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{device_id}] play_url:{url}@{}",
                duration_to_millis(start_at)
            ));
            *self.progress.lock().unwrap() = start_at;
            self.finished.store(false, Ordering::SeqCst);

            Box::pin(async { Ok(()) })
        }

        fn play_url_cached<'a>(
            &'a mut self,
            request: &'a CachedUrlPlaybackRequest,
            start_at: Option<Duration>,
        ) -> BackendFuture<'a, ()> {
            let device_id = self.device_id.clone();
            let url = request.url.clone();
            let cache_path = request.cache_path.clone();
            let start_at = start_at.unwrap_or(Duration::ZERO);
            self.log(format!(
                "player[{device_id}] play_url_cached:{url}->{cache_path}@{}",
                duration_to_millis(start_at)
            ));
            *self.progress.lock().unwrap() = start_at;
            self.finished.store(false, Ordering::SeqCst);

            Box::pin(async { Ok(()) })
        }

        fn pause(&self) {
            self.log(format!("player[{}] pause", self.device_id));
        }

        fn resume(&self) {
            self.log(format!("player[{}] resume", self.device_id));
        }

        fn stop(&mut self) {
            self.log(format!("player[{}] stop", self.device_id));
            self.finished.store(true, Ordering::SeqCst);
            self.finish_notify.notify_waiters();
            self.release_device();
        }

        fn seek(&self, target: Duration) {
            *self.progress.lock().unwrap() = target;
        }

        fn progress(&self) -> Duration {
            *self.progress.lock().unwrap()
        }

        fn is_finished(&self) -> bool {
            self.finished.load(Ordering::SeqCst)
        }

        fn wait_finished_signal(&self) -> SignalFuture {
            let finished = Arc::clone(&self.finished);
            let notify = Arc::clone(&self.finish_notify);
            Box::pin(async move {
                if finished.load(Ordering::SeqCst) {
                    return;
                }
                notify.notified().await;
            })
        }

        fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>> {
            Ok(self
                .devices
                .iter()
                .cloned()
                .map(|mut device| {
                    device.is_current = device.id == self.device_id;
                    device
                })
                .collect())
        }
    }

    fn create_shared_state() -> Arc<SharedState> {
        Arc::new(SharedState {
            progress_ms: AtomicU32::new(0),
            is_playing: AtomicU32::new(PlaybackStatus::Stopped.as_u32()),
        })
    }

    fn create_worker(
        factory: MockFactory,
    ) -> (
        WorkerCore<MockPlayer, MockFactory>,
        Arc<SharedState>,
        MockFactory,
    ) {
        let shared_state = create_shared_state();
        let player = factory.create(None).unwrap();
        let worker = WorkerCore::new(player, factory.clone(), Arc::clone(&shared_state));
        (worker, shared_state, factory)
    }

    #[tokio::test]
    async fn play_url_uses_requested_start_time() {
        let factory = MockFactory::new();
        let (mut worker, shared_state, factory) = create_worker(factory);

        worker
            .handle_command(PlayerCommand::PlayUrl(
                "https://example.com/test.mp3".to_string(),
                Some(12.5),
            ))
            .await;

        assert_eq!(shared_state.progress_ms.load(Ordering::SeqCst), 12_500);
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Playing.as_u32()
        );
        assert_eq!(
            worker.current_source,
            Some(PlaybackSource::Url(
                "https://example.com/test.mp3".to_string()
            ))
        );
        assert_eq!(
            factory.events(),
            vec![
                "create:auto".to_string(),
                "player[auto] play_url:https://example.com/test.mp3@12500".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn switch_output_device_restarts_active_source_from_current_progress() {
        let factory = MockFactory::new();
        let (mut worker, shared_state, factory) = create_worker(factory);

        worker
            .handle_command(PlayerCommand::PlayFile(
                "/tmp/test.flac".to_string(),
                Some(0.0),
            ))
            .await;
        worker.player.set_progress(Duration::from_millis(4_200));

        let (tx, rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::SwitchOutputDevice(
                Some("headphones".to_string()),
                tx,
            ))
            .await;

        assert!(rx.await.unwrap().is_ok());
        assert_eq!(shared_state.progress_ms.load(Ordering::SeqCst), 4_200);
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Playing.as_u32()
        );
        assert_eq!(
            factory.events(),
            vec![
                "create:auto".to_string(),
                "player[auto] play_file:/tmp/test.flac@0".to_string(),
                "create:headphones".to_string(),
                "player[headphones] play_file:/tmp/test.flac@4200".to_string(),
                "player[auto] stop".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn switch_output_device_preserves_paused_state() {
        let factory = MockFactory::new();
        let (mut worker, shared_state, factory) = create_worker(factory);

        worker
            .handle_command(PlayerCommand::PlayUrl(
                "https://example.com/test.mp3".to_string(),
                Some(1.0),
            ))
            .await;
        worker.player.set_progress(Duration::from_millis(3_000));
        worker.handle_command(PlayerCommand::Pause).await;

        let (tx, rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::SwitchOutputDevice(
                Some("headphones".to_string()),
                tx,
            ))
            .await;

        assert!(rx.await.unwrap().is_ok());
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Paused.as_u32()
        );
        assert_eq!(
            factory.events(),
            vec![
                "create:auto".to_string(),
                "player[auto] play_url:https://example.com/test.mp3@1000".to_string(),
                "player[auto] pause".to_string(),
                "create:headphones".to_string(),
                "player[headphones] play_url:https://example.com/test.mp3@3000".to_string(),
                "player[headphones] pause".to_string(),
                "player[auto] stop".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn get_output_devices_returns_current_and_default_flags() {
        let factory = MockFactory::new();
        let (mut worker, _shared_state, _factory) = create_worker(factory);

        let (tx, rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::GetOutputDevices(tx))
            .await;

        let devices = rx.await.unwrap().unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices,
            vec![
                AudioDeviceInfo {
                    id: "speaker".to_string(),
                    name: "Speaker".to_string(),
                    is_default: true,
                    is_current: true,
                },
                AudioDeviceInfo {
                    id: "headphones".to_string(),
                    name: "Headphones".to_string(),
                    is_default: false,
                    is_current: false,
                }
            ]
        );
    }

    #[tokio::test]
    async fn tick_marks_finished_playback_as_stopped() {
        let factory = MockFactory::new();
        let (mut worker, shared_state, _factory) = create_worker(factory);

        worker
            .handle_command(PlayerCommand::PlayFile(
                "/tmp/test.flac".to_string(),
                Some(0.0),
            ))
            .await;
        worker.player.set_progress(Duration::from_millis(8_000));
        worker.player.set_finished(true);

        worker.tick();

        assert_eq!(shared_state.progress_ms.load(Ordering::SeqCst), 8_000);
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Stopped.as_u32()
        );
        assert_eq!(worker.current_source, None);
    }

    #[tokio::test]
    async fn failed_device_switch_keeps_existing_playback_state() {
        let factory = MockFactory::with_fail_device("broken");
        let (mut worker, shared_state, factory) = create_worker(factory);

        worker
            .handle_command(PlayerCommand::PlayUrl(
                "https://example.com/test.mp3".to_string(),
                Some(2.0),
            ))
            .await;
        worker.player.set_progress(Duration::from_millis(2_500));

        let (tx, rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::SwitchOutputDevice(
                Some("broken".to_string()),
                tx,
            ))
            .await;

        let err = rx.await.unwrap().unwrap_err();
        assert_eq!(err, "failed to create device: broken");
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Playing.as_u32()
        );
        assert_eq!(
            worker.current_source,
            Some(PlaybackSource::Url(
                "https://example.com/test.mp3".to_string()
            ))
        );
        assert_eq!(
            factory.events(),
            vec![
                "create:auto".to_string(),
                "player[auto] play_url:https://example.com/test.mp3@2000".to_string(),
                "create:broken".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn switching_to_the_already_active_device_is_a_noop() {
        let factory = BusyFactory::new();
        let shared_state = create_shared_state();
        let player = factory.create(None).unwrap();
        let mut worker = WorkerCore::new(player, factory.clone(), Arc::clone(&shared_state));

        let (switch_tx, switch_rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::SwitchOutputDevice(
                Some("headphones".to_string()),
                switch_tx,
            ))
            .await;
        assert!(switch_rx.await.unwrap().is_ok());

        worker
            .handle_command(PlayerCommand::PlayFile(
                "/tmp/test.flac".to_string(),
                Some(0.0),
            ))
            .await;

        let (tx, rx) = oneshot::channel();
        worker
            .handle_command(PlayerCommand::SwitchOutputDevice(
                Some("headphones".to_string()),
                tx,
            ))
            .await;

        assert!(rx.await.unwrap().is_ok());
        assert_eq!(
            factory.events(),
            vec![
                "create:speaker".to_string(),
                "create:headphones".to_string(),
                "player[speaker] stop".to_string(),
                "player[headphones] play_file:/tmp/test.flac@0".to_string()
            ]
        );
        assert_eq!(
            shared_state.is_playing.load(Ordering::SeqCst),
            PlaybackStatus::Playing.as_u32()
        );
    }
}
