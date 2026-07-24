use std::sync::Arc;

use napi::{Error, Result};
use napi_derive::napi;
use tokio::sync::{mpsc, oneshot};

use crate::runtime::native_runtime;

use super::backend::{AudioPlayerFactory, PlayerFactory};
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{AudioDeviceInfo, CachedUrlPlaybackRequest, PlaybackOptions};
use super::worker::WorkerCore;

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
        let shared_state = Arc::new(SharedState::new());

        let factory = AudioPlayerFactory;
        let player = factory.create(None).map_err(Error::from_reason)?;
        let worker = WorkerCore::new(player, factory, Arc::clone(&shared_state));

        native_runtime().spawn(worker.run(rx));

        Ok(Self {
            sender: tx,
            shared_state,
        })
    }

    #[napi]
    pub async fn play_file(
        &self,
        path: String,
        start_secs: Option<f64>,
        strict_bit_perfect: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::PlayFile(
                path,
                start_secs,
                playback_options(strict_bit_perfect),
                Some(tx),
            ))
            .map_err(|_| Error::from_reason("Background worker died"))?;

        rx.await
            .map_err(|_| Error::from_reason("Playback start interrupted"))?
            .map_err(Error::from_reason)
    }

    #[napi]
    pub async fn play_url(
        &self,
        url: String,
        start_secs: Option<f64>,
        strict_bit_perfect: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::PlayUrl(
                url,
                start_secs,
                playback_options(strict_bit_perfect),
                Some(tx),
            ))
            .map_err(|_| Error::from_reason("Background worker died"))?;

        rx.await
            .map_err(|_| Error::from_reason("Playback start interrupted"))?
            .map_err(Error::from_reason)
    }

    #[napi]
    pub async fn play_url_cached(
        &self,
        url: String,
        cache_path: String,
        metadata_path: String,
        duration_ms: Option<i64>,
        cache_ahead_secs: Option<u32>,
        max_cache_ahead_bytes: Option<i64>,
        start_secs: Option<f64>,
        strict_bit_perfect: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::PlayUrlCached(
                CachedUrlPlaybackRequest {
                    url,
                    cache_path,
                    metadata_path,
                    duration_ms: duration_ms.map(|value| value.max(0) as u64),
                    cache_ahead_secs,
                    max_cache_ahead_bytes: max_cache_ahead_bytes.map(|value| value.max(0) as u64),
                },
                start_secs,
                playback_options(strict_bit_perfect),
                Some(tx),
            ))
            .map_err(|_| Error::from_reason("Background worker died"))?;

        rx.await
            .map_err(|_| Error::from_reason("Playback start interrupted"))?
            .map_err(Error::from_reason)
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

    #[napi]
    pub async fn get_file_duration_ms(&self, path: String) -> Result<i64> {
        crate::audio::decoder::probe_file_duration_ms(path)
            .await
            .map(|duration| duration.min(i64::MAX as u64) as i64)
            .map_err(|error| Error::from_reason(error.to_string()))
    }

    #[napi(getter)]
    pub fn progress_ms(&self) -> u32 {
        self.shared_state.progress_ms()
    }

    #[napi(getter)]
    pub fn is_playing(&self) -> bool {
        self.shared_state.is_playing()
    }

    #[napi(getter)]
    pub fn is_buffering(&self) -> bool {
        self.shared_state.is_buffering()
    }

    #[napi(getter)]
    pub fn is_paused(&self) -> bool {
        self.shared_state.is_paused()
    }

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

fn playback_options(strict_bit_perfect: Option<bool>) -> PlaybackOptions {
    PlaybackOptions {
        strict_bit_perfect: strict_bit_perfect.unwrap_or(false),
    }
}
