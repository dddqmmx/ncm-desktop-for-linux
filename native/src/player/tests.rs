use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, oneshot};

use crate::audio::OutputDeviceInfo;

use super::backend::{PlayerBackend, PlayerFactory};
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{
    AudioDeviceInfo, BackendFuture, BackendResult, CachedUrlPlaybackRequest, PlaybackSource,
    PlaybackStatus, SignalFuture, duration_to_millis,
};
use super::worker::WorkerCore;

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
            devices: test_devices(),
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

    fn is_buffering(&self) -> bool {
        false
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
            devices: test_devices(),
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

    fn is_buffering(&self) -> bool {
        false
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

fn test_devices() -> Vec<OutputDeviceInfo> {
    vec![
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
    ]
}

fn create_shared_state() -> Arc<SharedState> {
    Arc::new(SharedState::new())
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

    assert_eq!(shared_state.progress_ms(), 12_500);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
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
    assert_eq!(shared_state.progress_ms(), 4_200);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
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
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Paused);
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

    assert_eq!(shared_state.progress_ms(), 8_000);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
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
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
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
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
}
