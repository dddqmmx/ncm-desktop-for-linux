use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::audio::OutputDeviceInfo;

use super::backend::{PlayerBackend, PlayerFactory};
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{
    AudioDeviceInfo, BackendFuture, BackendResult, CachedUrlPlaybackRequest, FinishReason,
    PlaybackOptions, PlaybackSource, PlaybackStatus, duration_to_millis,
};
use super::worker::WorkerCore;

#[derive(Clone)]
struct MockFactory {
    events: Arc<Mutex<Vec<String>>>,
    /// 每次 play_* 实际收到的播放选项，按调用顺序记录。设备切换会在新端点上
    /// 重放当前会话，这里用来验证重放用的是调用方此刻的选项而非开播时冻结的。
    play_options: Arc<Mutex<Vec<PlaybackOptions>>>,
    fail_device: Option<String>,
    devices: Vec<OutputDeviceInfo>,
}

impl MockFactory {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            play_options: Arc::new(Mutex::new(Vec::new())),
            fail_device: None,
            devices: test_devices(),
        }
    }

    /// 模拟 Linux 的设备列表：只枚举真实硬件端点，系统默认输出不在列表里，
    /// 因此没有任何设备带 is_current / is_default 标记。
    fn with_hardware_only_devices() -> Self {
        let mut factory = Self::new();
        factory.devices = vec![OutputDeviceInfo {
            id: "hw:CARD=0,DEV=0".to_string(),
            aliases: vec!["hw:CARD=0,DEV=0".to_string()],
            name: "HDA Intel PCH - Analog".to_string(),
            is_default: false,
            is_current: false,
        }];
        factory
    }

    fn with_fail_device(device_id: &str) -> Self {
        let mut factory = Self::new();
        factory.fail_device = Some(device_id.to_string());
        factory
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn play_options(&self) -> Vec<PlaybackOptions> {
        self.play_options.lock().unwrap().clone()
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
            Arc::clone(&self.play_options),
        ))
    }
}

struct MockPlayer {
    device_name: Option<String>,
    devices: Vec<OutputDeviceInfo>,
    events: Arc<Mutex<Vec<String>>>,
    play_options: Arc<Mutex<Vec<PlaybackOptions>>>,
    progress: Arc<Mutex<Duration>>,
    finished: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    fail_next_play: Arc<AtomicBool>,
    /// 模拟 AudioPlayer 的会话音量：set_volume 记种子、play_* 建新会话
    /// 用种子开播，回读方法从这里取值。
    volume_seed: Arc<Mutex<f64>>,
    volume_locked: Arc<Mutex<bool>>,
    /// 模拟设备丢失标记（handle_stream_error 的 device_lost）。
    device_lost: Arc<AtomicBool>,
}

impl MockPlayer {
    fn new(
        device_name: Option<String>,
        devices: Vec<OutputDeviceInfo>,
        events: Arc<Mutex<Vec<String>>>,
        play_options: Arc<Mutex<Vec<PlaybackOptions>>>,
    ) -> Self {
        Self {
            device_name,
            devices,
            events,
            play_options,
            progress: Arc::new(Mutex::new(Duration::ZERO)),
            finished: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            fail_next_play: Arc::new(AtomicBool::new(false)),
            volume_seed: Arc::new(Mutex::new(1.0)),
            volume_locked: Arc::new(Mutex::new(false)),
            device_lost: Arc::new(AtomicBool::new(false)),
        }
    }

    fn record_play_options(&self, options: PlaybackOptions) {
        self.play_options.lock().unwrap().push(options);
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
    }

    fn set_playback_failed(&self, failed: bool) {
        self.failed.store(failed, Ordering::SeqCst);
    }

    fn set_device_lost(&self, lost: bool) {
        self.device_lost.store(lost, Ordering::SeqCst);
    }

    fn arm_play_failure(&self) {
        self.fail_next_play.store(true, Ordering::SeqCst);
    }

    /// play_* 共用：若被标记为“下一次播放失败”，返回失败的 future。
    fn take_play_failure(&self) -> Option<BackendFuture<'static, ()>> {
        if self.fail_next_play.swap(false, Ordering::SeqCst) {
            self.log(format!("player[{}] play_failed", self.label()));
            Some(Box::pin(async { Err("mock play failure".to_string()) }))
        } else {
            None
        }
    }

    /// 模拟 AudioPlayer::setup_and_play 的会话建立：新会话从开播种子取音量，
    /// BitPerfect 会话锁定为 1.0 直通。
    fn begin_session(&self, options: &PlaybackOptions) {
        *self.volume_locked.lock().unwrap() = options.strict_bit_perfect;
        let session_volume = if options.strict_bit_perfect {
            1.0
        } else {
            *self.volume_seed.lock().unwrap()
        };
        *self.volume_seed.lock().unwrap() = session_volume;
    }
}

impl PlayerBackend for MockPlayer {
    fn play_file<'a>(
        &'a mut self,
        path: &'a str,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        self.record_play_options(options);
        if let Some(failure) = self.take_play_failure() {
            return failure;
        }
        self.begin_session(&options);
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
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        self.record_play_options(options);
        if let Some(failure) = self.take_play_failure() {
            return failure;
        }
        self.begin_session(&options);
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
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        self.record_play_options(options);
        if let Some(failure) = self.take_play_failure() {
            return failure;
        }
        self.begin_session(&options);
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

    fn set_volume(&mut self, volume: f64) {
        let sanitized = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        *self.volume_seed.lock().unwrap() = sanitized;
        if !*self.volume_locked.lock().unwrap() {
            self.log(format!("player[{}] set_volume:{}", self.label(), sanitized));
        }
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

    fn playback_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
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

    fn session_volume(&self) -> f64 {
        if *self.volume_locked.lock().unwrap() {
            1.0
        } else {
            *self.volume_seed.lock().unwrap()
        }
    }

    fn session_volume_locked(&self) -> bool {
        *self.volume_locked.lock().unwrap()
    }

    fn device_lost(&self) -> bool {
        self.device_lost.load(Ordering::SeqCst)
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
        _options: PlaybackOptions,
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
        _options: PlaybackOptions,
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
        _options: PlaybackOptions,
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
        self.release_device();
    }

    fn seek(&self, target: Duration) {
        *self.progress.lock().unwrap() = target;
    }

    fn set_volume(&mut self, _volume: f64) {}

    fn progress(&self) -> Duration {
        *self.progress.lock().unwrap()
    }

    fn is_buffering(&self) -> bool {
        false
    }

    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn playback_failed(&self) -> bool {
        false
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

    fn session_volume(&self) -> f64 {
        1.0
    }

    fn session_volume_locked(&self) -> bool {
        false
    }

    fn device_lost(&self) -> bool {
        false
    }
}

fn test_devices() -> Vec<OutputDeviceInfo> {
    vec![
        OutputDeviceInfo {
            id: "speaker".to_string(),
            aliases: vec!["speaker".to_string()],
            name: "Speaker".to_string(),
            is_default: true,
            is_current: false,
        },
        OutputDeviceInfo {
            id: "headphones".to_string(),
            aliases: vec!["headphones".to_string()],
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
    let worker = WorkerCore::new(factory.clone(), Arc::clone(&shared_state)).unwrap();
    (worker, shared_state, factory)
}

#[tokio::test]
async fn play_url_uses_requested_start_time() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::Url(
                "https://example.com/test.mp3".to_string(),
                PlaybackOptions::default(),
            ),
            Some(12.5),
            None,
        ))
        .await;

    assert_eq!(shared_state.progress_ms(), 12_500);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        worker.current_source,
        Some(PlaybackSource::Url(
            "https://example.com/test.mp3".to_string(),
            PlaybackOptions::default()
        ))
    );
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_url:https://example.com/test.mp3@12500".to_string()
        ]
    );
}

#[tokio::test]
async fn switch_output_device_restarts_active_source_from_current_progress() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;
    worker.player.set_progress(Duration::from_millis(4_200));

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.progress_ms(), 4_200);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    // 旧播放器必须在新设备开流之前停掉：独占会话握着 ALSA hw: 与 PipeWire
    // 预留，新旧目标同卡时不先释放就会 EBUSY。
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string(),
            "create:headphones".to_string(),
            "player[headphones] set_volume:1".to_string(),
            "player[auto] stop".to_string(),
            "player[headphones] play_file:/tmp/test.flac@4200".to_string()
        ]
    );
}

#[tokio::test]
async fn set_volume_is_reapplied_to_the_rebuilt_player_after_device_switch() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;
    worker.handle_command(PlayerCommand::SetVolume(0.4)).await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;

    assert!(rx.await.unwrap().is_ok());
    // Play 前先把最新音量种进实例，切设备重建的新实例同样要在开播前拿到它；
    // session 回读值（shared_state.volume）是渲染层轮询校对的权威。
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string(),
            "player[auto] set_volume:0.4".to_string(),
            "create:headphones".to_string(),
            "player[headphones] set_volume:0.4".to_string(),
            "player[auto] stop".to_string(),
            "player[headphones] play_file:/tmp/test.flac@0".to_string(),
        ]
    );
    assert!((shared_state.volume() - 0.4).abs() < 1e-9);
    assert!(!shared_state.volume_locked());
}

/// 音量随会话实时可调（不同于开播即冻结的独占/BitPerfect 选项）：
/// 每次开播用渲染层最近下发的值做种子，一次 SetVolume 的丢失只影响
/// 当前会话，绝不会让新会话带着陈旧音量开播。
#[tokio::test]
async fn play_command_seeds_the_session_with_the_latest_synced_volume() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker.handle_command(PlayerCommand::SetVolume(0.3)).await;
    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:0.3".to_string(),
            "player[auto] set_volume:0.3".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string(),
        ],
        "SetVolume 先落会话，Play 再用最新值做种子——两次事件都必须在开播前"
    );
    assert!((shared_state.volume() - 0.3).abs() < 1e-9);
}

/// BitPerfect 会话锁定为 1.0 直通：SetVolume 更新种子但不落进会话，
/// 回读上报锁定状态，渲染层据此禁用滑块。
#[tokio::test]
async fn bit_perfect_session_locks_volume_to_unity_and_reports_locked() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker.handle_command(PlayerCommand::SetVolume(0.3)).await;
    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File(
                "/tmp/test.flac".to_string(),
                PlaybackOptions {
                    strict_bit_perfect: true,
                    exclusive: true,
                },
            ),
            Some(0.0),
            None,
        ))
        .await;
    worker.handle_command(PlayerCommand::SetVolume(0.5)).await;

    // 锁定会话丢弃中途设置，回读保持 1.0 + locked。
    assert_eq!(shared_state.volume(), 1.0);
    assert!(shared_state.volume_locked());
}

/// 回归：设备切换必须用调用方此刻的播放选项重放，而不是开播时冻结在
/// `PlaybackSource` 里的那份。
///
/// 会话选项是 `Play` 那一刻定死的。用户在独占/BitPerfect 下播着歌，再去设置里
/// 选“系统默认输出”时，重放若照搬旧选项，就会带着 exclusive/strict_bit_perfect
/// 打到虚拟端点上——原生侧以“不是真实硬件设备”拒绝，切换失败并回滚，用户被
/// 锁死在原来的硬件设备上，关掉开关也没用（开关只影响下一首）。
#[tokio::test]
async fn switch_output_device_replays_with_the_callers_current_options() {
    let factory = MockFactory::new();
    let (mut worker, _shared_state, factory) = create_worker(factory);

    let exclusive = PlaybackOptions {
        strict_bit_perfect: true,
        exclusive: true,
    };
    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), exclusive),
            Some(0.0),
            None,
        ))
        .await;

    // 先落到一个硬件端点上（用户在设置里选了自己的 DAC）。
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;
    assert!(rx.await.unwrap().is_ok());

    // 再切到“系统默认输出”（None）并带上此刻的设置：独占/BitPerfect 均已关闭。
    let shared = PlaybackOptions {
        strict_bit_perfect: false,
        exclusive: false,
    };
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(None, Some(shared), tx))
        .await;
    assert!(rx.await.unwrap().is_ok());

    assert_eq!(
        factory.play_options(),
        vec![exclusive, exclusive, shared],
        "重放必须用调用方传来的新选项，照搬会话里冻结的旧选项会让切到虚拟端点必然失败"
    );

    // 新选项要落到会话上：后续再切设备不能又退回开播时那份。
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;
    assert!(rx.await.unwrap().is_ok());
    assert_eq!(
        factory.play_options(),
        vec![exclusive, exclusive, shared, shared]
    );
}

/// 不传选项时沿用会话原有设置：老调用方（以及内部只想换端点的路径）不该被
/// 悄悄改成非独占。
#[tokio::test]
async fn switch_output_device_without_options_keeps_the_session_options() {
    let factory = MockFactory::new();
    let (mut worker, _shared_state, factory) = create_worker(factory);

    let exclusive = PlaybackOptions {
        strict_bit_perfect: true,
        exclusive: true,
    };
    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), exclusive),
            Some(0.0),
            None,
        ))
        .await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;
    assert!(rx.await.unwrap().is_ok());

    assert_eq!(factory.play_options(), vec![exclusive, exclusive]);
}

#[tokio::test]
async fn switch_output_device_preserves_paused_state() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::Url(
                "https://example.com/test.mp3".to_string(),
                PlaybackOptions::default(),
            ),
            Some(1.0),
            None,
        ))
        .await;
    worker.player.set_progress(Duration::from_millis(3_000));
    worker.handle_command(PlayerCommand::Pause).await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Paused);
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_url:https://example.com/test.mp3@1000".to_string(),
            "player[auto] pause".to_string(),
            "create:headphones".to_string(),
            "player[headphones] set_volume:1".to_string(),
            "player[auto] stop".to_string(),
            "player[headphones] play_url:https://example.com/test.mp3@3000".to_string(),
            "player[headphones] pause".to_string()
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
                aliases: vec!["speaker".to_string()],
            },
            AudioDeviceInfo {
                id: "headphones".to_string(),
                name: "Headphones".to_string(),
                is_default: false,
                is_current: false,
                aliases: vec!["headphones".to_string()],
            }
        ]
    );
}

#[tokio::test]
async fn tick_marks_finished_playback_as_stopped() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
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
        .handle_command(PlayerCommand::Play(
            PlaybackSource::Url(
                "https://example.com/test.mp3".to_string(),
                PlaybackOptions::default(),
            ),
            Some(2.0),
            None,
        ))
        .await;
    worker.player.set_progress(Duration::from_millis(2_500));

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("broken".to_string()),
            None,
            tx,
        ))
        .await;

    let err = rx.await.unwrap().unwrap_err();
    assert_eq!(err, "failed to create device: broken");
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        worker.current_source,
        Some(PlaybackSource::Url(
            "https://example.com/test.mp3".to_string(),
            PlaybackOptions::default()
        ))
    );
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_url:https://example.com/test.mp3@2000".to_string(),
            "create:broken".to_string()
        ]
    );
}

#[tokio::test]
async fn switching_default_to_default_without_listed_default_device_is_a_noop() {
    // Linux 上系统默认输出不在设备列表里：过去仅凭列表判断“是否已激活”，
    // “默认→默认”会被误判成需要切换，每次恢复播放前都重建整条播放链路。
    let factory = MockFactory::with_hardware_only_devices();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(None, None, tx))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string()
        ],
        "switching default->default must not rebuild the player"
    );
}

#[tokio::test]
async fn device_switch_does_not_resolve_pending_wait_finished() {
    // 结束信号必须跟随播放会话：切换输出设备会 stop 旧播放器实例，但歌曲仍在
    // 新实例上继续播放。若此时结束信号被误触发，前端 waitForEnd 会把它当成
    // 播放中断并强行停止——表现为“暂停后无法恢复播放”。
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (finished_tx, mut finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    worker.handle_command(PlayerCommand::Pause).await;

    let (switch_tx, switch_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            switch_tx,
        ))
        .await;
    assert!(switch_rx.await.unwrap().is_ok());

    worker.handle_command(PlayerCommand::Resume).await;
    worker.tick();

    assert_eq!(
        finished_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty),
        "device switch mid-session must not resolve wait_finished"
    );
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);

    // 歌曲真正播完时才通知，且原因为自然播完。
    worker.player.set_finished(true);
    worker.tick();
    assert_eq!(finished_rx.try_recv(), Ok(FinishReason::Ended));
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
}

#[tokio::test]
async fn stop_command_resolves_pending_wait_finished() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    worker.handle_command(PlayerCommand::Stop).await;

    assert_eq!(finished_rx.await, Ok(FinishReason::Stopped));
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
}

#[tokio::test]
async fn wait_finished_resolves_immediately_when_nothing_is_playing() {
    let factory = MockFactory::new();
    let (mut worker, _shared_state, _factory) = create_worker(factory);

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    assert_eq!(finished_rx.await, Ok(FinishReason::Ended));
}

#[tokio::test]
async fn starting_a_new_source_resolves_the_previous_sessions_wait_finished() {
    let factory = MockFactory::new();
    let (mut worker, _shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/first.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/second.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    assert_eq!(finished_rx.await, Ok(FinishReason::Replaced));
}

#[tokio::test]
async fn switching_to_the_concrete_default_device_while_following_default_is_a_noop() {
    // 播放器跟随系统默认（requested=None），用户点选列表中恰为当前默认的具体设备。
    // 快速路径只认“双方都是默认”，此场景必须由列表匹配 is_current + id 识别为无需切换。
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("speaker".to_string()),
            None,
            tx,
        ))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string()
        ],
        "selecting the device that is already current must not rebuild the player"
    );
}

#[tokio::test]
async fn switching_by_display_name_to_the_current_device_is_a_noop() {
    // 前端传来的选择串可能是显示名而非设备 id，列表匹配需宽松认名。
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("Speaker".to_string()),
            None,
            tx,
        ))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string()
        ],
        "matching the current device by display name must not rebuild the player"
    );
}

#[tokio::test]
async fn switching_to_follow_default_when_pinned_to_the_default_device_is_a_noop() {
    // 播放器被显式固定在 headphones，而 headphones 恰是系统默认设备。切回
    // “跟随默认”时快速路径不命中（Some != None），由列表匹配 current.is_default
    // 判定无需重建。注意：requested_device 保持 Some，仍固定在该具体设备上。
    let mut factory = MockFactory::new();
    factory.devices = vec![
        OutputDeviceInfo {
            id: "speaker".to_string(),
            aliases: vec!["speaker".to_string()],
            name: "Speaker".to_string(),
            is_default: false,
            is_current: false,
        },
        OutputDeviceInfo {
            id: "headphones".to_string(),
            aliases: vec!["headphones".to_string()],
            name: "Headphones".to_string(),
            is_default: true,
            is_current: false,
        },
    ];
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    // 先切到具名设备 headphones（重建播放器，requested = Some("headphones")）。
    let (pin_tx, pin_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            pin_tx,
        ))
        .await;
    assert!(pin_rx.await.unwrap().is_ok());
    let events_after_pin = factory.events();

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(None, None, tx))
        .await;

    assert!(rx.await.unwrap().is_ok());
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(
        factory.events(),
        events_after_pin,
        "switching to follow-default while pinned to the default device must not rebuild"
    );
}

#[tokio::test]
async fn fatal_error_while_paused_ends_session_and_reports_error() {
    // 暂停期间底层流仍可能因致命错误终止（is_finished + playback_failed 同时置位）。
    // tick 必须在 Paused 状态下同样收尾会话，否则前端的 wait_finished 永远悬挂。
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    worker.handle_command(PlayerCommand::Pause).await;

    worker.player.set_playback_failed(true);
    worker.player.set_finished(true);
    worker.tick();

    assert_eq!(finished_rx.await, Ok(FinishReason::Error));
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
    assert!(
        worker
            .player
            .events
            .lock()
            .unwrap()
            .contains(&"player[auto] stop".to_string()),
        "player.stop() must be invoked to teardown failed stream"
    );
}

#[tokio::test]
async fn failed_play_start_clears_session_and_reports_the_error() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/first.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    worker.player.arm_play_failure();
    let (play_tx, play_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/broken.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            Some(play_tx),
        ))
        .await;

    // 启动失败通过 reply 通道上报给调用方……
    assert!(play_rx.await.unwrap().is_err());
    // ……上一个会话的监听者则在新请求进入时已按“被替换”收尾。
    assert_eq!(finished_rx.await, Ok(FinishReason::Replaced));
    assert_eq!(worker.current_source, None);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);

    // 失败后无会话，新的 wait_finished 立即返回。
    let (tx, rx) = oneshot::channel();
    worker.handle_command(PlayerCommand::WaitFinished(tx)).await;
    assert_eq!(rx.await, Ok(FinishReason::Ended));
}

#[tokio::test]
async fn stop_resolves_every_pending_wait_finished_listener() {
    // 同一会话可能挂着多个结束监听（渲染进程 waitForEnd、主进程组合调用等），
    // 会话结束时必须全部通知，不允许只保留最后一个。
    let factory = MockFactory::new();
    let (mut worker, _shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (first_tx, first_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(first_tx))
        .await;
    let (second_tx, second_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(second_tx))
        .await;

    worker.handle_command(PlayerCommand::Stop).await;

    assert_eq!(first_rx.await, Ok(FinishReason::Stopped));
    assert_eq!(second_rx.await, Ok(FinishReason::Stopped));
}

#[tokio::test]
async fn switching_to_the_already_active_device_is_a_noop() {
    let factory = BusyFactory::new();
    let shared_state = create_shared_state();
    let mut worker = WorkerCore::new(factory.clone(), Arc::clone(&shared_state)).unwrap();

    let (switch_tx, switch_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            switch_tx,
        ))
        .await;
    assert!(switch_rx.await.unwrap().is_ok());

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
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

/// 设备在会话进行中消失（拔出）：worker 必须把会话自动迁移到系统默认端点
/// 续播，而不是终止——用户拔掉 USB 耳机时歌应当落到扬声器继续放。迁移成功
/// 时 wait_finished 的监听者不受扰动（会话没有终结），前端无感知。
#[tokio::test]
async fn device_loss_migrates_the_session_to_the_default_output_instead_of_ending_it() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;
    // 落到一个具名设备上（迁移只在绑定了具名设备时有意义——本来就在系统
    // 默认上时设备消失意味着系统输出全没了，无处可迁）。
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;
    assert!(rx.await.unwrap().is_ok());
    worker.player.set_progress(Duration::from_millis(12_000));

    // 挂上结束监听者：迁移成功绝不许触发它（前端会把中途结束信号当成
    // 播放中断处理）。
    let (finished_tx, mut finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    // 设备拔出：流以设备错误终止。
    worker.player.set_device_lost(true);
    worker.player.set_playback_failed(true);
    worker.player.set_finished(true);
    worker.tick();

    // tick 只标记迁移（迁移是 async），run 循环紧接着执行。
    assert!(worker.pending_device_migration);
    worker.pending_device_migration = false;
    worker.run_pending_device_migration().await;

    // 会话在新实例上按当前进度重放，选项剥离独占/BitPerfect（虚拟端点会拒绝），
    // 结束监听者没有被触发。
    assert_eq!(
        factory.events(),
        vec![
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[auto] play_file:/tmp/test.flac@0".to_string(),
            "create:headphones".to_string(),
            "player[headphones] set_volume:1".to_string(),
            "player[auto] stop".to_string(),
            "player[headphones] play_file:/tmp/test.flac@0".to_string(),
            "create:auto".to_string(),
            "player[auto] set_volume:1".to_string(),
            "player[headphones] stop".to_string(),
            "player[auto] play_file:/tmp/test.flac@12000".to_string(),
        ],
        "migration must rebuild on the default endpoint and replay from current progress"
    );
    assert_eq!(factory.play_options().last(), Some(&PlaybackOptions::default()));
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Playing);
    assert_eq!(shared_state.progress_ms(), 12_000);
    assert_eq!(
        finished_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty),
        "device-loss migration must not resolve wait_finished"
    );
    assert_eq!(
        worker.current_source,
        Some(PlaybackSource::File(
            "/tmp/test.flac".to_string(),
            PlaybackOptions::default()
        )),
        "the session survives the migration (it continues on the default output)"
    );
}

/// 普通流错误（解码/网络）不是设备丢失：会话按错误终止，绝不能误触发迁移。
#[tokio::test]
async fn non_device_stream_error_terminates_the_session_without_migration() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            Some("headphones".to_string()),
            None,
            tx,
        ))
        .await;
    assert!(rx.await.unwrap().is_ok());

    let (finished_tx, finished_rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::WaitFinished(finished_tx))
        .await;

    // 解码器致命错误：playback_failed 但 device_lost 未置位。
    worker.player.set_playback_failed(true);
    worker.player.set_finished(true);
    worker.tick();

    assert!(!worker.pending_device_migration);
    assert_eq!(finished_rx.await, Ok(FinishReason::Error));
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
}

/// 在系统默认端点上跟随播放时设备消失（例如拔掉了唯一的声卡）：无处可迁，
/// 会话按错误终止。
#[tokio::test]
async fn device_loss_while_following_the_default_output_ends_the_session() {
    let factory = MockFactory::new();
    let (mut worker, shared_state, _factory) = create_worker(factory);

    worker
        .handle_command(PlayerCommand::Play(
            PlaybackSource::File("/tmp/test.flac".to_string(), PlaybackOptions::default()),
            Some(0.0),
            None,
        ))
        .await;

    worker.player.set_device_lost(true);
    worker.player.set_playback_failed(true);
    worker.player.set_finished(true);
    worker.tick();

    // requested_device 是 None（跟随默认），不迁移。
    assert!(!worker.pending_device_migration);
    assert_eq!(shared_state.playback_status(), PlaybackStatus::Stopped);
}
