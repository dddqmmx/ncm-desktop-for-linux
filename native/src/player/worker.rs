use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::audio::backend;

use super::backend::{PlayerBackend, PlayerFactory};
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{
    AudioDeviceInfo, BackendResult, FinishReason, PlaybackOptions, PlaybackSource, PlaybackStatus,
    duration_to_millis, seconds_to_duration, start_secs_to_duration,
};

pub(crate) struct WorkerCore<P, F> {
    pub(crate) player: P,
    factory: F,
    shared_state: Arc<SharedState>,
    pub(crate) current_source: Option<PlaybackSource>,
    /// 创建当前播放器时请求的输出设备（None = 跟随系统默认）。设备切换策略是
    /// worker 的事，由 worker 自己记录，不落在 PlayerBackend 契约里；仅在真正
    /// 重建播放器时更新，noop 切换不改写，保证与播放器实际绑定的设备一致。
    requested_device: Option<String>,
    /// 等待“当前播放会话结束”的监听者。结束信号跟随会话（current_source）而非
    /// 播放器实例：切换输出设备会重建播放器并 stop 旧实例，但会话仍在继续，
    /// 此时绝不能触发结束通知——前端会把中途的结束信号当成播放异常中断处理。
    finish_waiters: Vec<oneshot::Sender<FinishReason>>,
    /// 渲染层最近一次 SetVolume 下发的音量。独占/BitPerfect 是开播即冻结的
    /// 会话选项，音量却要实时可调，所以它不进 PlaybackOptions：每次开播/重放
    /// 都用这份最新值做种子，会话实际生效值由 player 的会话状态回读。
    synced_volume: f64,
    /// 设备丢失后的自动迁移请求（tick 检测、run 循环的 async 上下文执行）。
    /// tick 本身是同步函数，switch_output_device 的重放链路是 async，只能把
    /// 迁移推迟到下一次 select 轮询——期间进度轮询可能多看到一次旧进度，
    /// 无碍。
    pub(crate) pending_device_migration: bool,
}

impl<P, F> WorkerCore<P, F>
where
    P: PlayerBackend,
    F: PlayerFactory<Player = P>,
{
    pub(crate) fn new(factory: F, shared_state: Arc<SharedState>) -> BackendResult<Self> {
        let player = factory.create(None)?;
        Ok(Self {
            player,
            factory,
            shared_state,
            current_source: None,
            requested_device: None,
            finish_waiters: Vec::new(),
            synced_volume: 1.0,
            pending_device_migration: false,
        })
    }

    /// 结束当前会话：清空 current_source 并按原因通知所有结束监听者。
    /// 所有让会话终结的路径都必须经过这里，避免监听者被遗漏或误触发。
    fn end_session(&mut self, reason: FinishReason) {
        self.current_source = None;
        for waiter in self.finish_waiters.drain(..) {
            let _ = waiter.send(reason);
        }
    }

    /// 停掉播放器、终结会话并复位共享 UI 状态。“停止 + end_session +
    /// reset_playback”这个三连必须成套执行，抽在这里防止各路径漂移。
    /// 会话回读值随复位归 1.0/未锁定：没有活跃会话就没有“生效音量”可言。
    fn abort_session(&mut self, reason: FinishReason) {
        self.player.stop();
        self.end_session(reason);
        self.shared_state.reset_playback();
    }

    pub(crate) async fn run(mut self, mut rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(16));

        loop {
            tokio::select! {
                cmd_opt = rx.recv() => {
                    match cmd_opt {
                        Some(cmd) => self.handle_command(cmd).await,
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    self.tick();
                    // tick 可能标记了设备丢失迁移（迁移本身是 async，tick 里
                    // 做不了），趁同一轮把它处理掉，别让错误终止抢先。
                    while self.pending_device_migration {
                        self.pending_device_migration = false;
                        self.run_pending_device_migration().await;
                    }
                }
            }
        }
    }

    pub(crate) async fn handle_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::Play(source, start_secs, reply_tx) => {
                let kind = source.kind();
                // 音量随会话实时可调（不同于开播即冻结的独占/BitPerfect 选项），
                // 开播前先把渲染层最近下发的值种进新会话，防止前端漏发/丢包
                // 导致新会话带着旧值开播。
                self.player.set_volume(self.synced_volume);
                let result = self
                    .play_source(source, start_secs_to_duration(start_secs))
                    .await;
                if let Err(err) = &result {
                    eprintln!("Play {} failed: {}", kind, err);
                }
                if let Some(reply_tx) = reply_tx {
                    let _ = reply_tx.send(result);
                }
            }
            PlayerCommand::Pause => {
                self.player.pause();
                self.shared_state
                    .set_playback_status(PlaybackStatus::Paused);
            }
            PlayerCommand::Resume => {
                self.player.resume();
                self.shared_state
                    .set_playback_status(PlaybackStatus::Playing);
            }
            PlayerCommand::Stop => {
                self.abort_session(FinishReason::Stopped);
            }
            PlayerCommand::Seek(time_secs) => {
                self.player.seek(seconds_to_duration(time_secs));
                self.shared_state.set_buffering(true);
            }
            PlayerCommand::SetVolume(volume) => {
                // 非有限值（NaN/±inf）按 1.0 收敛：底层 player 的 set_volume
                // 同样净化（所有写入的汇聚点），这里保持同一语义。
                self.synced_volume = if volume.is_finite() {
                    volume.clamp(0.0, 1.0)
                } else {
                    1.0
                };
                self.player.set_volume(self.synced_volume);
                self.shared_state.set_volume(self.player.session_volume());
            }
            PlayerCommand::SwitchOutputDevice(device_name, options, reply_tx) => {
                let result = self
                    .switch_output_device(normalize_device_name(device_name), options)
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
                if self.current_source.is_none() {
                    let _ = done_tx.send(FinishReason::Ended);
                } else {
                    self.finish_waiters.push(done_tx);
                }
            }
        }
    }

    async fn play_source(
        &mut self,
        source: PlaybackSource,
        start_at: Option<Duration>,
    ) -> BackendResult<()> {
        // 新会话开始，上一个会话即告结束（正常流程里前端已先行 Stop，这里兜底）。
        self.end_session(FinishReason::Replaced);
        self.shared_state
            .set_progress_ms(duration_to_millis(start_at.unwrap_or(Duration::ZERO)));
        self.shared_state.set_buffering(true);

        match Self::play_source_on(&mut self.player, &source, start_at).await {
            Ok(()) => {
                self.current_source = Some(source);
                self.shared_state
                    .set_playback_status(PlaybackStatus::Playing);
                // 会话状态（实际音量/BitPerfect 锁定）随开播发布，供前端
                // 200ms 轮询回读——渲染层据此校对 SetVolume 是否真的落地。
                self.shared_state.set_volume(self.player.session_volume());
                self.shared_state
                    .set_volume_locked(self.player.session_volume_locked());
                Ok(())
            }
            Err(err) => {
                // 入口处已通知过监听者，这里的 end_session 只为维持不变式。
                self.abort_session(FinishReason::Stopped);
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
            PlaybackSource::File(path, options) => player.play_file(path, start_at, *options).await,
            PlaybackSource::Url(url, options) => player.play_url(url, start_at, *options).await,
            PlaybackSource::CachedUrl(request, options) => {
                player.play_url_cached(request, start_at, *options).await
            }
        }
    }

    fn is_requested_output_device_already_active(
        &self,
        device_name: Option<&str>,
    ) -> BackendResult<bool> {
        // 双方都是“跟随系统默认”时无需切换。Linux 上设备列表只枚举真实硬件端点，
        // “系统默认”不在列表里也不带 is_current 标记，仅靠下面的列表匹配会把
        // “默认→默认”误判为需要切换，导致每次恢复播放前都重建整条播放链路。
        // 注意：具名设备不能走这条捷径——目标设备可能已被拔出，必须落到列表匹配
        // 乃至 factory.create，让“设备不存在”的错误传给前端触发回退默认的逻辑。
        if device_name.is_none() && self.requested_device.is_none() {
            return Ok(true);
        }

        let devices = self.player.output_devices()?;
        let current_device = devices.iter().find(|device| device.is_current);

        // device_name 是前端传来的选择串，主路径是设备 id（设置页 option 绑定的
        // 就是 device.id），id 用端点等价比较：同一端点有数字卡号与字符卡名两种
        // 写法，纯字符串相等会把“已经在这个设备上”误判成需要切换，每次播放前
        // 都白白重建一次播放链路。显示名等值只是旧持久化偏好的兜底——注意两个
        // 同型号接口的显示名可能相同，这里命中时宁可多一次切换也不能静默不切。
        match (device_name, current_device) {
            (Some(requested), Some(current_device)) => Ok(backend::output_device_ids_match(
                &current_device.id,
                requested,
            ) || current_device.name == requested),
            (None, Some(current_device)) => Ok(current_device.is_default),
            _ => Ok(false),
        }
    }

    async fn switch_output_device(
        &mut self,
        device_name: Option<String>,
        options: Option<PlaybackOptions>,
    ) -> BackendResult<()> {
        if self.is_requested_output_device_already_active(device_name.as_deref())? {
            // 设备没变但选项可能变了（例如用户刚关掉独占），留给下一首生效即可，
            // 这里不重建播放链路——为改个开关打断正在听的歌不划算。
            return Ok(());
        }

        let playback_status = self.shared_state.playback_status();
        // 重放用调用方此刻的选项，而不是开播时冻结的那份：切到系统默认输出这类
        // 虚拟端点时，旧选项里的独占/BitPerfect 会让重放被原生侧直接拒绝，
        // 用户于是被锁死在原来的硬件设备上，怎么切都回不去。
        let original_source = self.current_source.clone();
        let resume_source = match (original_source.as_ref(), options) {
            (Some(source), Some(options)) => Some(source.with_options(options)),
            (source, _) => source.cloned(),
        };
        let resume_position = match playback_status {
            PlaybackStatus::Stopped => None,
            PlaybackStatus::Playing | PlaybackStatus::Paused => Some(self.player.progress()),
        };

        // 先只建实例——此时还没打开设备，“设备不存在”的错误在这里就被拦下，
        // 正在播放的链路完全不受影响。
        let mut next_player = self.factory.create(device_name.as_deref())?;
        // 重放的会话音量按渲染层最新的值开播。种子必须在 play_* 建新会话之前
        // 落好：放后面就成了开播后的中途调整，用户音量低于 100% 时切设备会先
        // 全音量出声再骤降——一声爆音。无条件设置，不依赖新实例的默认值巧合。
        next_player.set_volume(self.synced_volume);

        // 打开新设备前必须先停旧播放器：独占会话握着 ALSA hw: 与 PipeWire 预留，
        // 新旧目标落在同一张声卡上时（换 DEV、系统默认↔该声卡、重新应用设置），
        // 不先释放就是自己把自己挡在门外（EBUSY / 预留自撞）。
        self.player.stop();

        if let Some(source) = resume_source.as_ref() {
            if let Err(err) = Self::play_source_on(&mut next_player, source, resume_position).await
            {
                // 回滚用原封不动的旧会话：切换没成功，原设备上的独占/BitPerfect
                // 就该保持原样，不能被这次没生效的新选项顺手改掉。
                if let Some(original) = original_source.as_ref() {
                    self.restore_playback_after_failed_switch(
                        original,
                        resume_position,
                        playback_status,
                    )
                    .await;
                }
                return Err(err);
            }
            if playback_status == PlaybackStatus::Paused {
                next_player.pause();
            }
        }

        self.player = next_player;
        self.requested_device = device_name;
        // 会话后续的重放（再次切设备）都要沿用这份新选项。
        self.current_source = resume_source;

        if let Some(position) = resume_position {
            self.shared_state
                .set_progress_ms(duration_to_millis(position));
        }
        self.shared_state.set_buffering(self.player.is_buffering());
        self.shared_state.set_playback_status(playback_status);
        self.shared_state.set_volume(self.player.session_volume());
        self.shared_state
            .set_volume_locked(self.player.session_volume_locked());

        Ok(())
    }

    /// 切换失败后把会话搬回原设备。新设备打不开（格式不支持、被别的程序独占）
    /// 不该顺带把正在听的歌一起弄停；只有原设备也起不来才认定会话结束。
    async fn restore_playback_after_failed_switch(
        &mut self,
        source: &PlaybackSource,
        resume_position: Option<Duration>,
        playback_status: PlaybackStatus,
    ) {
        match Self::play_source_on(&mut self.player, source, resume_position).await {
            Ok(()) => {
                if playback_status == PlaybackStatus::Paused {
                    self.player.pause();
                }
                self.shared_state.set_playback_status(playback_status);
            }
            Err(err) => {
                eprintln!(
                    "Restore playback on the previous output device failed: {}",
                    err
                );
                // 新旧端点都没能承接会话：这是异常终止而非用户主动停止。
                // 前端只对 'ended'/'error' 做收尾，报成 Stopped 会把 UI 留在
                // “播放中却无声”的死会话上。
                self.abort_session(FinishReason::Error);
            }
        }
    }

    pub(crate) fn tick(&mut self) {
        let playback_status = self.shared_state.playback_status();
        if playback_status == PlaybackStatus::Stopped {
            return;
        }

        self.shared_state
            .set_progress_ms(duration_to_millis(self.player.progress()));
        self.shared_state.set_buffering(self.player.is_buffering());

        // 此处 playback_status 只可能是 Playing 或 Paused（Stopped 已在函数开头返回）。
        // Paused 时 is_finished 仍可能置位：致命流错误随时会设置它；自然播完也可能
        // 恰好在 Pause 命令处理前的输出回调里 latch（暂停生效后回调提前返回，不再
        // 检测播完）。两种情况会话都已结束，用 playback_failed 区分结束原因。
        if self.player.is_finished() {
            // 设备在会话进行中消失（拔出/被移除）：先把会话迁移到系统默认端点
            // 续播，而不是终止——用户拔掉 USB 耳机时，歌应当无缝落到扬声器上
            // 继续，而不是停在“播放中断”的报错上。迁移是 async（重放走播放
            // 后端的 future），tick 做不了，先标记、由 run 循环紧接着处理；
            // 若循环里先到了新命令（切歌/停止），标记自然作废，按新命令走。
            if self.player.playback_failed()
                && self.player.device_lost()
                && self.requested_device.is_some()
                && !self.pending_device_migration
            {
                self.pending_device_migration = true;
                return;
            }

            let reason = if self.player.playback_failed() {
                FinishReason::Error
            } else {
                FinishReason::Ended
            };
            self.player.stop();
            self.end_session(reason);
            self.shared_state
                .set_playback_status(PlaybackStatus::Stopped);
            self.shared_state.set_buffering(false);
            // 会话已终结：没有“实际生效音量”可言，与 abort_session 一样复位，
            // 残留 BitPerfect 锁定标记会让前端误判下一首仍处于直通锁定。
            self.shared_state.set_volume(1.0);
            self.shared_state.set_volume_locked(false);
        }
    }

    /// 设备丢失后的自动迁移（tick 标记、run 循环执行）：把当前会话重放到
    /// 系统默认端点续播。
    ///
    /// 成功路径上会话与 current_source 都保持原样（续播的主体），进度/暂停态
    /// 由 switch_output_device 的重放链路保全，音量按 synced_volume 播种；
    /// wait_finished 的监听者不受扰动，前端无感知。选项固定为“无独占/无
    /// BitPerfect”：系统默认是混音服务器的虚拟端点，带着独占下发重放会被
    /// 原生侧直接拒绝。这是一次性迁移而不是用户选择——下次开播
    /// ensureConfiguredOutputDevice 会按用户持久化的偏好把设备切回去。
    /// 迁移失败（默认端点也起不来）才走错误终止。
    pub(crate) async fn run_pending_device_migration(&mut self) {
        let shared = PlaybackOptions {
            strict_bit_perfect: false,
            exclusive: false,
        };

        eprintln!(
            "[player] output device lost, migrating the current session to the system default"
        );

        if let Err(err) = self.switch_output_device(None, Some(shared)).await {
            eprintln!("[player] migrating to the system default failed: {err}");
            self.abort_session(FinishReason::Error);
        }
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
