//! worker 控制面测试。
//!
//! 这里只关心**worker 的决策**：收到一条命令之后，它向播放后端发了什么、向
//! 前端发布了什么、结束通知怎么走。跨线程的时序协议（seek 换代、缓冲门控、
//! 终局判定）由 `audio::session::tests` 在会话接口上覆盖，这里的 MockPlayer
//! 只暴露 worker 需要的那份快照，不重复那套时序。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::audio::OutputDeviceInfo;
use crate::audio::session::SessionOutcome;

use super::backend::{PlayerBackend, PlayerFactory, SessionReport};
use super::command::PlayerCommand;
use super::state::{PlaybackSnapshot, SnapshotCell};
use super::types::{
    AudioDeviceInfo, BackendFuture, BackendResult, CachedUrlPlaybackRequest, FinishReason,
    PlaybackOptions, PlaybackSource, PlaybackStatus, duration_to_millis,
};
use super::worker::WorkerCore;

// ------------------------------------------------------------------ 测试替身

#[derive(Default)]
struct MockSessionState {
    position: Duration,
    buffering: bool,
    volume: f64,
    volume_locked: bool,
    outcome: Option<SessionOutcome>,
}

#[derive(Clone)]
struct MockFactory {
    events: Arc<Mutex<Vec<String>>>,
    /// 每次 play_* 实际收到的播放选项，按调用顺序记录。设备切换会在新端点上
    /// 重放当前会话，这里用来验证重放用的是调用方此刻的选项而非开播时冻结的。
    play_options: Arc<Mutex<Vec<PlaybackOptions>>>,
    fail_device: Option<String>,
    /// 在这台设备上开播会失败（模拟“设备建得起来但打不开流”）。
    fail_play_device: Option<String>,
    devices: Vec<OutputDeviceInfo>,
}

impl MockFactory {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            play_options: Arc::new(Mutex::new(Vec::new())),
            fail_device: None,
            fail_play_device: None,
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

    fn with_fail_play_device(device_id: &str) -> Self {
        let mut factory = Self::new();
        factory.fail_play_device = Some(device_id.to_string());
        factory
    }

    fn with_default_device(device_id: &str) -> Self {
        let mut factory = Self::new();
        for device in factory.devices.iter_mut() {
            device.is_default = device.id == device_id;
        }
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

        Ok(MockPlayer {
            device_name: device_name.map(str::to_string),
            devices: self.devices.clone(),
            events: Arc::clone(&self.events),
            play_options: Arc::clone(&self.play_options),
            state: Arc::new(Mutex::new(MockSessionState {
                volume: 1.0,
                ..MockSessionState::default()
            })),
            fail_next_play: self.fail_play_device.as_deref().is_some()
                && self.fail_play_device.as_deref() == device_name,
        })
    }
}

struct MockPlayer {
    device_name: Option<String>,
    devices: Vec<OutputDeviceInfo>,
    events: Arc<Mutex<Vec<String>>>,
    play_options: Arc<Mutex<Vec<PlaybackOptions>>>,
    /// 与真实播放器一样：每个播放器实例有自己的一份会话状态，切换设备重建
    /// 实例时状态从零开始（进度由 worker 用 start_at 播种回来）。
    state: Arc<Mutex<MockSessionState>>,
    fail_next_play: bool,
}

impl MockPlayer {
    fn label(&self) -> &str {
        self.device_name.as_deref().unwrap_or("auto")
    }

    fn log(&self, message: String) {
        self.events.lock().unwrap().push(message);
    }

    fn with_state(&self, apply: impl FnOnce(&mut MockSessionState)) {
        apply(&mut self.state.lock().unwrap());
    }

    fn record_play_options(&self, options: PlaybackOptions) {
        self.play_options.lock().unwrap().push(options);
    }

    /// 模拟开播：进度跳到 start_at，缓冲/终局清零，音量按开播种子生效。
    fn begin_session(&self, start_at: Duration, options: PlaybackOptions) {
        self.with_state(|state| {
            state.position = start_at;
            state.buffering = false;
            state.outcome = None;
            state.volume_locked = options.strict_bit_perfect;
            if options.strict_bit_perfect {
                state.volume = 1.0;
            }
        });
    }

    fn take_play_failure(&mut self) -> Option<BackendFuture<'static, ()>> {
        if std::mem::take(&mut self.fail_next_play) {
            self.log(format!("player[{}] play_failed", self.label()));
            Some(Box::pin(async { Err("mock play failure".to_string()) }))
        } else {
            None
        }
    }

    fn play_future<'a>(
        &mut self,
        label_event: String,
        start_at: Duration,
    ) -> BackendFuture<'a, ()> {
        self.log(label_event);
        self.with_state(|state| {
            state.position = start_at;
            state.buffering = false;
            state.outcome = None;
        });
        Box::pin(async { Ok(()) })
    }

    // --- 测试驱动入口（模拟真实播放器内部发生的事） ---

    fn set_progress(&self, progress: Duration) {
        self.with_state(|state| state.position = progress);
    }

    fn set_buffering(&self, buffering: bool) {
        self.with_state(|state| state.buffering = buffering);
    }

    fn finish_naturally(&self) {
        self.with_state(|state| {
            state.outcome = Some(SessionOutcome::Delivered);
            state.buffering = false;
        });
    }

    fn fail_with_device_loss(&self) {
        self.with_state(|state| {
            state.outcome = Some(SessionOutcome::Failed { device_lost: true });
            state.buffering = false;
        });
    }

    fn fail_without_device_loss(&self) {
        self.with_state(|state| {
            state.outcome = Some(SessionOutcome::Failed { device_lost: false });
            state.buffering = false;
        });
    }

    fn arm_play_failure(&mut self) {
        self.fail_next_play = true;
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
        self.begin_session(start_at.unwrap_or_default(), options);
        let event = format!(
            "player[{}] play_file:{path}@{}",
            self.label(),
            duration_to_millis(start_at.unwrap_or_default())
        );
        self.play_future(event, start_at.unwrap_or_default())
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
        self.begin_session(start_at.unwrap_or_default(), options);
        let event = format!(
            "player[{}] play_url:{url}@{}",
            self.label(),
            duration_to_millis(start_at.unwrap_or_default())
        );
        self.play_future(event, start_at.unwrap_or_default())
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
        self.begin_session(start_at.unwrap_or_default(), options);
        let event = format!(
            "player[{}] play_url_cached:{}->{}@{}",
            self.label(),
            request.url,
            request.cache_path,
            duration_to_millis(start_at.unwrap_or_default())
        );
        self.play_future(event, start_at.unwrap_or_default())
    }

    fn pause(&self) {
        self.log(format!("player[{}] pause", self.label()));
    }

    fn resume(&self) {
        self.log(format!("player[{}] resume", self.label()));
    }

    fn stop(&mut self) {
        self.log(format!("player[{}] stop", self.label()));
    }

    fn seek(&self, target: Duration) {
        self.log(format!(
            "player[{}] seek:{}",
            self.label(),
            duration_to_millis(target)
        ));
        self.with_state(|state| {
            state.position = target;
            state.buffering = true;
        });
    }

    fn set_volume(&mut self, volume: f64) {
        let sanitized = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let locked = self.state.lock().unwrap().volume_locked;
        if !locked {
            self.log(format!("player[{}] set_volume:{}", self.label(), sanitized));
        }
        self.with_state(|state| {
            if !state.volume_locked {
                state.volume = sanitized;
            }
        });
    }

    fn session_report(&self) -> SessionReport {
        let state = self.state.lock().unwrap();
        SessionReport {
            position: state.position,
            buffering: state.buffering,
            volume: state.volume,
            volume_locked: state.volume_locked,
            outcome: state.outcome,
        }
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

/// 设备"被占用"的工厂：同一端点同时只能有一个实例，用来验证切换顺序
/// （必须先停旧的再开新的）。
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

        Ok(BusyMockPlayer {
            device_id,
            devices: self.devices.clone(),
            events: Arc::clone(&self.events),
            active_devices: Arc::clone(&self.active_devices),
            state: Mutex::new(MockSessionState {
                volume: 1.0,
                ..MockSessionState::default()
            }),
            released: false,
        })
    }
}

struct BusyMockPlayer {
    device_id: String,
    devices: Vec<OutputDeviceInfo>,
    events: Arc<Mutex<Vec<String>>>,
    active_devices: Arc<Mutex<HashMap<String, usize>>>,
    state: Mutex<MockSessionState>,
    released: bool,
}

impl BusyMockPlayer {
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
        self.log(format!(
            "player[{}] play_file:{path}@{}",
            self.device_id,
            duration_to_millis(start_at.unwrap_or_default())
        ));
        self.state.lock().unwrap().position = start_at.unwrap_or_default();
        Box::pin(async { Ok(()) })
    }

    fn play_url<'a>(
        &'a mut self,
        url: &'a str,
        start_at: Option<Duration>,
        _options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        self.log(format!(
            "player[{}] play_url:{url}@{}",
            self.device_id,
            duration_to_millis(start_at.unwrap_or_default())
        ));
        self.state.lock().unwrap().position = start_at.unwrap_or_default();
        Box::pin(async { Ok(()) })
    }

    fn play_url_cached<'a>(
        &'a mut self,
        request: &'a CachedUrlPlaybackRequest,
        start_at: Option<Duration>,
        _options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        self.log(format!(
            "player[{}] play_url_cached:{}->{}@{}",
            self.device_id,
            request.url,
            request.cache_path,
            duration_to_millis(start_at.unwrap_or_default())
        ));
        self.state.lock().unwrap().position = start_at.unwrap_or_default();
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
        self.release_device();
    }

    fn seek(&self, target: Duration) {
        self.state.lock().unwrap().position = target;
    }

    fn set_volume(&mut self, _volume: f64) {}

    fn session_report(&self) -> SessionReport {
        let state = self.state.lock().unwrap();
        SessionReport {
            position: state.position,
            buffering: state.buffering,
            volume: state.volume,
            volume_locked: state.volume_locked,
            outcome: state.outcome,
        }
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

fn create_worker(
    factory: MockFactory,
) -> (
    WorkerCore<MockPlayer, MockFactory>,
    Arc<SnapshotCell>,
    MockFactory,
) {
    let snapshot = Arc::new(SnapshotCell::new());
    let worker = WorkerCore::new(factory.clone(), Arc::clone(&snapshot)).unwrap();
    (worker, snapshot, factory)
}

fn play_file_command(path: &str) -> PlayerCommand {
    PlayerCommand::Play(
        PlaybackSource::File(path.to_string(), PlaybackOptions::default()),
        Some(0.0),
        None,
    )
}

async fn switch(
    worker: &mut WorkerCore<MockPlayer, MockFactory>,
    device: Option<&str>,
    options: Option<PlaybackOptions>,
) -> BackendResult<()> {
    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::SwitchOutputDevice(
            device.map(str::to_string),
            options,
            tx,
        ))
        .await;
    rx.await.unwrap()
}

async fn wait_finished(
    worker: &mut WorkerCore<MockPlayer, MockFactory>,
) -> oneshot::Receiver<FinishReason> {
    let (tx, rx) = oneshot::channel();
    worker.handle_command(PlayerCommand::WaitFinished(tx)).await;
    rx
}

fn snapshot_status(snapshot: &SnapshotCell) -> PlaybackStatus {
    snapshot.read().status
}

// ------------------------------------------------------------------ 开播

#[tokio::test]
async fn play_url_uses_requested_start_time() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());

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

    let published: PlaybackSnapshot = snapshot.read();
    assert_eq!(published.progress_ms, 12_500);
    assert_eq!(published.status, PlaybackStatus::Playing);
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
async fn seek_publishes_the_target_position_and_buffering_immediately() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    worker.handle_command(PlayerCommand::Seek(42.5)).await;

    let published = snapshot.read();
    assert_eq!(published.progress_ms, 42_500);
    assert!(published.buffering, "seek 之后 UI 必须立刻显示缓冲中");
}

#[tokio::test]
async fn pause_and_resume_are_reflected_in_the_published_snapshot() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    worker.handle_command(PlayerCommand::Pause).await;
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Paused);

    worker.handle_command(PlayerCommand::Resume).await;
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
}

/// 快照里的进度、状态、音量必须属于同一次读：worker 在一次发布里同时写入它们。
#[tokio::test]
async fn tick_publishes_progress_status_and_volume_together() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    worker.handle_command(PlayerCommand::SetVolume(0.25)).await;
    worker.player.set_progress(Duration::from_millis(7_500));
    worker.player.set_buffering(true);

    worker.tick();

    let published = snapshot.read();
    assert_eq!(published.progress_ms, 7_500);
    assert!(published.buffering);
    assert!((published.volume - 0.25).abs() < 1e-9);
    assert_eq!(published.status, PlaybackStatus::Playing);
}

// ------------------------------------------------------------------ 音量

#[tokio::test]
async fn set_volume_is_reapplied_to_the_rebuilt_player_after_device_switch() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    worker.handle_command(PlayerCommand::SetVolume(0.4)).await;

    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());

    // Play 前先把最新音量种进实例，切设备重建的新实例同样要在开播前拿到它；
    // 快照里的音量是渲染层轮询校对的权威。
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
    let published = snapshot.read();
    assert!((published.volume - 0.4).abs() < 1e-9);
    assert!(!published.volume_locked);
}

/// 音量随会话实时可调（不同于开播即冻结的独占/BitPerfect 选项）：
/// 每次开播用渲染层最近下发的值做种子，一次 SetVolume 的丢失只影响
/// 当前会话，绝不会让新会话带着陈旧音量开播。
#[tokio::test]
async fn play_command_seeds_the_session_with_the_latest_synced_volume() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());

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
    assert!((snapshot.read().volume - 0.3).abs() < 1e-9);
}

/// BitPerfect 会话锁定为 1.0 直通：SetVolume 更新种子但不落进会话，
/// 回读上报锁定状态，渲染层据此禁用滑块。
#[tokio::test]
async fn bit_perfect_session_locks_volume_to_unity_and_reports_locked() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());

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

    let published = snapshot.read();
    assert_eq!(published.volume, 1.0);
    assert!(published.volume_locked);
}

#[tokio::test]
async fn non_finite_volume_is_normalized_before_reaching_the_backend() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    worker
        .handle_command(PlayerCommand::SetVolume(f64::NAN))
        .await;

    assert_eq!(snapshot.read().volume, 1.0);
}

// ------------------------------------------------------------------ 输出设备

#[tokio::test]
async fn switch_output_device_restarts_active_source_from_current_progress() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    worker.player.set_progress(Duration::from_millis(4_200));

    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());

    assert_eq!(snapshot.read().progress_ms, 4_200);
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
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

/// 回归：设备切换必须用调用方此刻的播放选项重放，而不是开播时冻结在
/// `PlaybackSource` 里的那份。
///
/// 会话选项是 `Play` 那一刻定死的。用户在独占/BitPerfect 下播着歌，再去设置里
/// 选“系统默认输出”时，重放若照搬旧选项，就会带着 exclusive/strict_bit_perfect
/// 打到虚拟端点上——原生侧以“不是真实硬件设备”拒绝，切换失败并回滚，用户被
/// 锁死在原来的硬件设备上，关掉开关也没用（开关只影响下一首）。
#[tokio::test]
async fn switch_output_device_replays_with_the_callers_current_options() {
    let (mut worker, _snapshot, factory) = create_worker(MockFactory::new());

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
    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());

    // 再切到“系统默认输出”（None）并带上此刻的设置：独占/BitPerfect 均已关闭。
    let shared = PlaybackOptions {
        strict_bit_perfect: false,
        exclusive: false,
    };
    assert!(switch(&mut worker, None, Some(shared)).await.is_ok());

    assert_eq!(
        factory.play_options(),
        vec![exclusive, exclusive, shared],
        "重放必须用调用方传来的新选项，照搬会话里冻结的旧选项会让切到虚拟端点必然失败"
    );

    // 新选项要落到会话上：后续再切设备不能又退回开播时那份。
    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    assert_eq!(
        factory.play_options(),
        vec![exclusive, exclusive, shared, shared]
    );
}

/// 不传选项时沿用会话原有设置：老调用方（以及内部只想换端点的路径）不该被
/// 悄悄改成非独占。
#[tokio::test]
async fn switch_output_device_without_options_keeps_the_session_options() {
    let (mut worker, _snapshot, factory) = create_worker(MockFactory::new());

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

    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    assert_eq!(factory.play_options(), vec![exclusive, exclusive]);
}

#[tokio::test]
async fn switch_output_device_preserves_paused_state() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
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

    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());

    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Paused);
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
    let (mut worker, _snapshot, _factory) = create_worker(MockFactory::new());

    let (tx, rx) = oneshot::channel();
    worker
        .handle_command(PlayerCommand::GetOutputDevices(tx))
        .await;

    let devices = rx.await.unwrap().unwrap();
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
async fn failed_device_switch_keeps_existing_playback_state() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::with_fail_device("broken"));
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

    let err = switch(&mut worker, Some("broken"), None).await.unwrap_err();

    assert_eq!(err, "failed to create device: broken");
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
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

/// 新端点建得起来但打不开流（格式不支持、被独占）：会话必须搬回原设备继续，
/// 不能顺带把正在听的歌一起弄停。
/// 新端点建得起来但打不开流（格式不支持、被独占）：会话必须搬回原设备继续，
/// 不能顺带把正在听的歌一起弄停，也不能被这次没生效的新选项改写。
#[tokio::test]
async fn switch_that_fails_to_start_on_the_new_device_rolls_back_to_the_original_options() {
    let (mut worker, snapshot, factory) =
        create_worker(MockFactory::with_fail_play_device("headphones"));
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
    worker.player.set_progress(Duration::from_millis(5_000));

    let shared = PlaybackOptions {
        strict_bit_perfect: false,
        exclusive: false,
    };
    let err = switch(&mut worker, Some("headphones"), Some(shared))
        .await
        .unwrap_err();
    assert_eq!(err, "mock play failure");

    assert_eq!(
        factory.play_options(),
        vec![exclusive, shared, exclusive],
        "开播用旧选项；新端点按调用方选项重放；回滚必须用回原封不动的旧会话选项"
    );
    assert_eq!(
        snapshot_status(&snapshot),
        PlaybackStatus::Playing,
        "切换失败不能把正在听的歌一起停掉"
    );
    assert_eq!(
        worker.current_source,
        Some(PlaybackSource::File(
            "/tmp/test.flac".to_string(),
            exclusive
        )),
        "会话保持原样（含原选项），下次切设备才有正确的基线"
    );
}

#[tokio::test]
async fn switching_default_to_default_without_listed_default_device_is_a_noop() {
    // Linux 上系统默认输出不在设备列表里：过去仅凭列表判断“是否已激活”，
    // “默认→默认”会被误判成需要切换，每次恢复播放前都重建整条播放链路。
    let (mut worker, snapshot, factory) = create_worker(MockFactory::with_hardware_only_devices());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    assert!(switch(&mut worker, None, None).await.is_ok());

    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
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
async fn switching_to_the_concrete_default_device_while_following_default_is_a_noop() {
    // 播放器跟随系统默认（requested=None），用户点选列表中恰为当前默认的具体设备。
    // 快速路径只认“双方都是默认”，此场景必须由列表匹配 is_current + id 识别为无需切换。
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    assert!(switch(&mut worker, Some("speaker"), None).await.is_ok());

    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
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
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    assert!(switch(&mut worker, Some("Speaker"), None).await.is_ok());

    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
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
    let (mut worker, snapshot, factory) =
        create_worker(MockFactory::with_default_device("headphones"));
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    let events_after_pin = factory.events();

    assert!(switch(&mut worker, None, None).await.is_ok());

    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
    assert_eq!(
        factory.events(),
        events_after_pin,
        "switching to follow-default while pinned to the default device must not rebuild"
    );
}

#[tokio::test]
async fn switching_to_the_already_active_device_is_a_noop() {
    let factory = BusyFactory::new();
    let snapshot = Arc::new(SnapshotCell::new());
    let mut worker = WorkerCore::new(factory.clone(), Arc::clone(&snapshot)).unwrap();

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
        .handle_command(play_file_command("/tmp/test.flac"))
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
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
}

// ------------------------------------------------------------------ wait_finished

#[tokio::test]
async fn device_switch_does_not_resolve_pending_wait_finished() {
    // 结束信号必须跟随播放会话：切换输出设备会 stop 旧播放器实例，但歌曲仍在
    // 新实例上继续播放。若此时结束信号被误触发，前端 waitForEnd 会把它当成
    // 播放中断并强行停止——表现为“暂停后无法恢复播放”。
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    let mut finished_rx = wait_finished(&mut worker).await;
    worker.handle_command(PlayerCommand::Pause).await;
    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    worker.handle_command(PlayerCommand::Resume).await;
    worker.tick();

    assert_eq!(
        finished_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty),
        "device switch mid-session must not resolve wait_finished"
    );
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);

    // 歌曲真正播完时才通知，且原因为自然播完。
    worker.player.finish_naturally();
    worker.tick();
    assert_eq!(finished_rx.try_recv(), Ok(FinishReason::Ended));
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
}

#[tokio::test]
async fn stop_command_resolves_pending_wait_finished() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    let finished_rx = wait_finished(&mut worker).await;

    worker.handle_command(PlayerCommand::Stop).await;

    assert_eq!(finished_rx.await, Ok(FinishReason::Stopped));
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(snapshot.read(), PlaybackSnapshot::IDLE);
}

#[tokio::test]
async fn wait_finished_resolves_immediately_when_nothing_is_playing() {
    let (mut worker, _snapshot, _factory) = create_worker(MockFactory::new());
    let finished_rx = wait_finished(&mut worker).await;
    assert_eq!(finished_rx.await, Ok(FinishReason::Ended));
}

#[tokio::test]
async fn starting_a_new_source_resolves_the_previous_sessions_wait_finished() {
    let (mut worker, _snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/first.flac"))
        .await;
    let finished_rx = wait_finished(&mut worker).await;

    worker
        .handle_command(play_file_command("/tmp/second.flac"))
        .await;

    assert_eq!(finished_rx.await, Ok(FinishReason::Replaced));
}

#[tokio::test]
async fn stop_resolves_every_pending_wait_finished_listener() {
    // 同一会话可能挂着多个结束监听（渲染进程 waitForEnd、主进程组合调用等），
    // 会话结束时必须全部通知，不允许只保留最后一个。
    let (mut worker, _snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    let first_rx = wait_finished(&mut worker).await;
    let second_rx = wait_finished(&mut worker).await;

    worker.handle_command(PlayerCommand::Stop).await;

    assert_eq!(first_rx.await, Ok(FinishReason::Stopped));
    assert_eq!(second_rx.await, Ok(FinishReason::Stopped));
}

// ------------------------------------------------------------------ 终结

#[tokio::test]
async fn tick_marks_finished_playback_as_stopped() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    worker.player.set_progress(Duration::from_millis(8_000));
    worker.player.finish_naturally();

    worker.tick();

    assert_eq!(snapshot.read().progress_ms, 8_000, "收尾要保留结束时的位置");
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
}

/// 暂停期间底层流仍可能因致命错误终止。tick 必须在 Paused 状态下同样收尾，
/// 否则前端的 wait_finished 永远悬挂。
#[tokio::test]
async fn fatal_error_while_paused_ends_session_and_reports_error() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    let finished_rx = wait_finished(&mut worker).await;
    worker.handle_command(PlayerCommand::Pause).await;

    worker.player.fail_without_device_loss();
    worker.tick();

    assert_eq!(finished_rx.await, Ok(FinishReason::Error));
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
    assert!(
        _factory_events_contains_stop(&worker),
        "player.stop() must be invoked to teardown the failed stream"
    );
}

fn _factory_events_contains_stop(worker: &WorkerCore<MockPlayer, MockFactory>) -> bool {
    worker
        .player
        .events
        .lock()
        .unwrap()
        .contains(&"player[auto] stop".to_string())
}

#[tokio::test]
async fn failed_play_start_clears_session_and_reports_the_error() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/first.flac"))
        .await;
    let finished_rx = wait_finished(&mut worker).await;

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
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);

    // 失败后无会话，新的 wait_finished 立即返回。
    assert_eq!(
        wait_finished(&mut worker).await.await,
        Ok(FinishReason::Ended)
    );
}

// ------------------------------------------------------------------ 设备丢失迁移

/// 设备在会话进行中消失（拔出）：worker 必须把会话自动迁移到系统默认端点
/// 续播，而不是终止——用户拔掉 USB 耳机时歌应当落到扬声器继续放。迁移成功
/// 时 wait_finished 的监听者不受扰动（会话没有终结），前端无感知。
#[tokio::test]
async fn device_loss_migrates_the_session_to_the_default_output_instead_of_ending_it() {
    let (mut worker, snapshot, factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    // 落到一个具名设备上（迁移只在绑定了具名设备时有意义——本来就在系统
    // 默认上时设备消失意味着系统输出全没了，无处可迁）。
    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    worker.player.set_progress(Duration::from_millis(12_000));

    // 挂上结束监听者：迁移成功绝不许触发它（前端会把中途结束信号当成
    // 播放中断处理）。
    let mut finished_rx = wait_finished(&mut worker).await;

    // 设备拔出：流以设备错误终止。
    worker.player.fail_with_device_loss();
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
    assert_eq!(
        factory.play_options().last(),
        Some(&PlaybackOptions::default())
    );
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Playing);
    assert_eq!(snapshot.read().progress_ms, 12_000);
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
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;
    assert!(switch(&mut worker, Some("headphones"), None).await.is_ok());
    let finished_rx = wait_finished(&mut worker).await;

    // 解码器致命错误：outcome 是 Failed，但不带 device_lost。
    worker.player.fail_without_device_loss();
    worker.tick();

    assert!(!worker.pending_device_migration);
    assert_eq!(finished_rx.await, Ok(FinishReason::Error));
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
}

/// 在系统默认端点上跟随播放时设备消失（例如拔掉了唯一的声卡）：无处可迁，
/// 会话按错误终止。
#[tokio::test]
async fn device_loss_while_following_the_default_output_ends_the_session() {
    let (mut worker, snapshot, _factory) = create_worker(MockFactory::new());
    worker
        .handle_command(play_file_command("/tmp/test.flac"))
        .await;

    worker.player.fail_with_device_loss();
    worker.tick();

    // requested_device 是 None（跟随默认），不迁移。
    assert!(!worker.pending_device_migration);
    assert_eq!(snapshot_status(&snapshot), PlaybackStatus::Stopped);
    assert_eq!(worker.current_source, None);
}
