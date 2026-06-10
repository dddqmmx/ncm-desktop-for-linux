use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;

use super::backend::{PlayerBackend, PlayerFactory};
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{
    AudioDeviceInfo, BackendResult, PlaybackSource, PlaybackStatus, duration_to_millis,
    seconds_to_duration, start_secs_to_duration,
};

pub(crate) struct WorkerCore<P, F> {
    pub(crate) player: P,
    factory: F,
    shared_state: Arc<SharedState>,
    pub(crate) current_source: Option<PlaybackSource>,
}

impl<P, F> WorkerCore<P, F>
where
    P: PlayerBackend,
    F: PlayerFactory<Player = P>,
{
    pub(crate) fn new(player: P, factory: F, shared_state: Arc<SharedState>) -> Self {
        Self {
            player,
            factory,
            shared_state,
            current_source: None,
        }
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
                _ = ticker.tick() => self.tick(),
            }
        }
    }

    pub(crate) async fn handle_command(&mut self, cmd: PlayerCommand) {
        match cmd {
            PlayerCommand::PlayFile(path, start_secs, options, reply_tx) => {
                let result = self
                    .play_source(
                        PlaybackSource::File(path, options),
                        start_secs_to_duration(start_secs),
                    )
                    .await;
                if let Err(err) = &result {
                    eprintln!("Play file failed: {}", err);
                }
                if let Some(reply_tx) = reply_tx {
                    let _ = reply_tx.send(result);
                }
            }
            PlayerCommand::PlayUrl(url, start_secs, options, reply_tx) => {
                let result = self
                    .play_source(
                        PlaybackSource::Url(url, options),
                        start_secs_to_duration(start_secs),
                    )
                    .await;
                if let Err(err) = &result {
                    eprintln!("Play URL failed: {}", err);
                }
                if let Some(reply_tx) = reply_tx {
                    let _ = reply_tx.send(result);
                }
            }
            PlayerCommand::PlayUrlCached(request, start_secs, options, reply_tx) => {
                let result = self
                    .play_source(
                        PlaybackSource::CachedUrl(request, options),
                        start_secs_to_duration(start_secs),
                    )
                    .await;
                if let Err(err) = &result {
                    eprintln!("Play cached URL failed: {}", err);
                }
                if let Some(reply_tx) = reply_tx {
                    let _ = reply_tx.send(result);
                }
            }
            PlayerCommand::Pause => {
                self.player.pause();
                self.shared_state
                    .set_playback_status(PlaybackStatus::Paused, Ordering::SeqCst);
            }
            PlayerCommand::Resume => {
                self.player.resume();
                self.shared_state
                    .set_playback_status(PlaybackStatus::Playing, Ordering::SeqCst);
            }
            PlayerCommand::Stop => {
                self.player.stop();
                self.current_source = None;
                self.shared_state.reset_playback();
            }
            PlayerCommand::Seek(time_secs) => {
                self.player.seek(seconds_to_duration(time_secs));
                self.shared_state.set_buffering(true, Ordering::SeqCst);
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
        self.shared_state.set_progress_ms(
            duration_to_millis(start_at.unwrap_or(Duration::ZERO)),
            Ordering::SeqCst,
        );
        self.shared_state.set_buffering(true, Ordering::SeqCst);

        match Self::play_source_on(&mut self.player, &source, start_at).await {
            Ok(()) => {
                self.current_source = Some(source);
                self.shared_state
                    .set_playback_status(PlaybackStatus::Playing, Ordering::SeqCst);
                Ok(())
            }
            Err(err) => {
                self.player.stop();
                self.current_source = None;
                self.shared_state.reset_playback();
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
        player: &P,
        device_name: Option<&str>,
    ) -> BackendResult<bool> {
        let devices = player.output_devices()?;
        let current_device = devices.iter().find(|device| device.is_current);

        match (device_name, current_device) {
            (Some(target_device_id), Some(current_device)) => Ok(current_device.id
                == target_device_id
                || current_device.name == target_device_id),
            (None, Some(current_device)) => Ok(current_device.is_default),
            _ => Ok(false),
        }
    }

    async fn switch_output_device(&mut self, device_name: Option<String>) -> BackendResult<()> {
        if Self::is_requested_output_device_already_active(&self.player, device_name.as_deref())? {
            return Ok(());
        }

        let playback_status = self.shared_state.playback_status();
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
                .set_progress_ms(duration_to_millis(position), Ordering::SeqCst);
        }
        self.shared_state
            .set_buffering(self.player.is_buffering(), Ordering::SeqCst);
        self.shared_state
            .set_playback_status(playback_status, Ordering::SeqCst);

        Ok(())
    }

    pub(crate) fn tick(&mut self) {
        let playback_status = self.shared_state.playback_status();
        if playback_status == PlaybackStatus::Stopped {
            return;
        }

        let progress = self.player.progress();
        self.shared_state
            .set_progress_ms(duration_to_millis(progress), Ordering::Relaxed);
        self.shared_state
            .set_buffering(self.player.is_buffering(), Ordering::Relaxed);

        if playback_status == PlaybackStatus::Playing && self.player.is_finished() {
            self.current_source = None;
            self.shared_state
                .set_playback_status(PlaybackStatus::Stopped, Ordering::SeqCst);
            self.shared_state.set_buffering(false, Ordering::SeqCst);
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
