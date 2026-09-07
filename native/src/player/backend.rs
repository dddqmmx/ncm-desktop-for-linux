use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::audio::{AudioPlayer, OutputDeviceInfo};

use super::types::{BackendFuture, BackendResult, CachedUrlPlaybackRequest, PlaybackOptions};

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
    fn progress(&self) -> Duration;
    fn is_buffering(&self) -> bool;
    fn is_finished(&self) -> bool;
    /// is_finished 为真时区分结束原因：true = 致命流错误终止，false = 自然播完。
    fn playback_failed(&self) -> bool;
    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>>;
    /// 当前会话实际生效的音量（0.0..=1.0），供 UI 回读校对。
    fn session_volume(&self) -> f64;
    /// 当前会话是否被严格 BitPerfect 锁定为 1.0 直通。
    fn session_volume_locked(&self) -> bool;
    /// 输出设备是否在会话进行中消失（设备类流错误置位）。worker 据此触发
    /// “迁移到系统默认续播”而不是错误终止。
    fn device_lost(&self) -> bool;
}

pub(crate) trait PlayerFactory: Send + Sync + 'static {
    type Player: PlayerBackend;

    fn create(&self, device_name: Option<&str>) -> BackendResult<Self::Player>;
}

#[derive(Clone, Copy)]
pub(crate) struct AudioPlayerFactory;

pub(crate) struct NativePlayer(AudioPlayer);

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

    fn progress(&self) -> Duration {
        self.0.progress()
    }

    fn is_buffering(&self) -> bool {
        self.0.get_state().waiting_for_seek.load(Ordering::Relaxed)
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn playback_failed(&self) -> bool {
        self.0.playback_failed()
    }

    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>> {
        self.0.output_devices().map_err(|err| err.to_string())
    }

    fn session_volume(&self) -> f64 {
        self.0.get_state().volume() as f64
    }

    fn session_volume_locked(&self) -> bool {
        self.0.get_state().volume_locked()
    }

    fn device_lost(&self) -> bool {
        self.0.get_state().device_lost.load(Ordering::SeqCst)
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
