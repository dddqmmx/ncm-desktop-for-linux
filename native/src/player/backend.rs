use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::audio::{AudioPlayer, OutputDeviceInfo};

use super::types::{
    BackendFuture, BackendResult, CachedUrlPlaybackRequest, PlaybackOptions, SignalFuture,
};

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
    fn progress(&self) -> Duration;
    fn is_buffering(&self) -> bool;
    fn is_finished(&self) -> bool;
    fn wait_finished_signal(&self) -> SignalFuture;
    fn output_devices(&self) -> BackendResult<Vec<OutputDeviceInfo>>;
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
                .play_file(path, start_at, options.strict_bit_perfect)
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
                .play_url(url, start_at, options.strict_bit_perfect)
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

    fn is_buffering(&self) -> bool {
        self.0.get_state().waiting_for_seek.load(Ordering::Relaxed)
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn wait_finished_signal(&self) -> super::types::SignalFuture {
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
