use std::time::Duration;

use crate::audio::OutputDeviceInfo;
use crate::audio::session::SessionOutcome;

use super::types::{BackendFuture, BackendResult, CachedUrlPlaybackRequest, PlaybackOptions};

/// 会话快照：位置、缓冲态、音量、终局**一次取回**。
///
/// 分成多个 getter 读是安全的假象——中途一次失败落地，`position → is_finished →
/// playback_failed → device_lost` 这串读就会拼出"已结束、但原因是上一瞬间的
/// 那个"的组合。worker 靠这份快照决定迁移还是终止，拼错即误判。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SessionReport {
    pub(crate) position: Duration,
    /// 输出侧正在等缓冲（UI 显示 loading，也意味着此刻没有声音）。
    pub(crate) buffering: bool,
    /// 当前会话实际生效的音量（0.0..=1.0），供 UI 回读校对。
    pub(crate) volume: f64,
    /// 当前会话是否被严格 BitPerfect 锁定为 1.0 直通。
    pub(crate) volume_locked: bool,
    /// `None` = 会话仍在进行。
    pub(crate) outcome: Option<SessionOutcome>,
}

/// 播放后端：一个端点上的播放器。
///
/// 接口刻意只有三件"事"——放、控、看快照。所有跨线程时序（seek 换代、缓冲
/// 门控、终局判定）都封在 `audio::session` 里，实现这一 trait 的人不需要、也
/// 不该重新推导它们。
pub(crate) trait PlayerBackend: Send {
    fn play_file<'a>(
        &'a mut self,
        path: &'a str,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()>;
    fn play_url<'a>(
        &'a mut self,
        url: &'a str,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()>;
    fn play_url_cached<'a>(
        &'a mut self,
        request: &'a CachedUrlPlaybackRequest,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()>;

    fn pause(&self);
    fn resume(&self);
    fn stop(&mut self);
    fn seek(&self, target: Duration);
    /// 软件音量（0.0..=1.0）：更新开播种子并应用到当前会话；
    /// BitPerfect 会话内由底层直通忽略。
    fn set_volume(&mut self, volume: f64);

    fn session_report(&self) -> SessionReport;
    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>>;
}

pub(crate) trait PlayerFactory: Send + Sync + 'static {
    type Player: PlayerBackend;

    fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player>;
}

#[derive(Clone, Copy)]
pub(crate) struct AudioPlayerFactory;

pub(crate) struct NativePlayer(crate::audio::AudioPlayer);

impl PlayerBackend for NativePlayer {
    fn play_file<'a>(
        &'a mut self,
        path: &'a str,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_file(
                    path,
                    start_at,
                    options.strict_bit_perfect,
                    options.exclusive,
                )
                .await
                .map_err(|err| err.to_string())
        })
    }

    fn play_url<'a>(
        &'a mut self,
        url: &'a str,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_url(url, start_at, options.strict_bit_perfect, options.exclusive)
                .await
                .map_err(|err| err.to_string())
        })
    }

    fn play_url_cached<'a>(
        &'a mut self,
        request: &'a CachedUrlPlaybackRequest,
        start_at: Option<Duration>,
        options: PlaybackOptions,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.0
                .play_url_cached(
                    &request.url,
                    &request.cache_path,
                    &request.metadata_path,
                    request.duration_ms,
                    request.cache_ahead_secs,
                    request.max_cache_ahead_bytes,
                    start_at,
                    options.strict_bit_perfect,
                    options.exclusive,
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

    fn set_volume(&mut self, volume: f64) {
        self.0.set_volume(volume as f32);
    }

    fn session_report(&self) -> SessionReport {
        SessionReport {
            position: self.0.progress(),
            buffering: self.0.is_buffering(),
            volume: f64::from(self.0.session_volume()),
            volume_locked: self.0.session_volume_locked(),
            outcome: self.0.session_outcome(),
        }
    }

    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>> {
        self.0.output_devices().map_err(|err| err.to_string())
    }
}

impl PlayerFactory for AudioPlayerFactory {
    type Player = NativePlayer;

    fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player> {
        crate::audio::AudioPlayer::new(device_name)
            .map(NativePlayer)
            .map_err(|err| err.to_string())
    }
}
