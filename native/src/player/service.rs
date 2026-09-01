use std::sync::Arc;

use napi::{Error, Result};
use napi_derive::napi;
use tokio::sync::{mpsc, oneshot};

use crate::runtime::native_runtime;

use super::backend::AudioPlayerFactory;
use super::command::PlayerCommand;
use super::state::SharedState;
use super::types::{AudioDeviceInfo, CachedUrlPlaybackRequest, PlaybackOptions, PlaybackSource};
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

        let worker = WorkerCore::new(AudioPlayerFactory, Arc::clone(&shared_state))
            .map_err(Error::from_reason)?;

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
        exclusive: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::Play(
                PlaybackSource::File(path, playback_options(strict_bit_perfect, exclusive)),
                start_secs,
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
        exclusive: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::Play(
                PlaybackSource::Url(url, playback_options(strict_bit_perfect, exclusive)),
                start_secs,
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
        exclusive: Option<bool>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::Play(
                PlaybackSource::CachedUrl(
                    CachedUrlPlaybackRequest {
                        url,
                        cache_path,
                        metadata_path,
                        duration_ms: duration_ms.map(|value| value.max(0) as u64),
                        cache_ahead_secs,
                        max_cache_ahead_bytes: max_cache_ahead_bytes
                            .map(|value| value.max(0) as u64),
                    },
                    playback_options(strict_bit_perfect, exclusive),
                ),
                start_secs,
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

    /// 设置应用级软件音量（0.0..=1.0）。仅作用于本应用的输出，不改系统音量；
    /// 严格 BitPerfect 会话内底层保持 1.0 直通。
    #[napi]
    pub fn set_volume(&self, volume: f64) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::SetVolume(volume));
        Ok(())
    }

    /// 切换输出端点。`strict_bit_perfect` / `exclusive` 是调用方此刻的设置：
    /// 会话里那份是开播时冻结的，用户中途改了开关、或切到的端点压根不支持独占
    /// （系统默认输出是虚拟端点）时，照搬旧的会让重放被拒绝、切换失败。
    /// 两者都不传则沿用会话原有选项。
    #[napi]
    pub async fn switch_output_device(
        &self,
        device_id: Option<String>,
        strict_bit_perfect: Option<bool>,
        exclusive: Option<bool>,
    ) -> Result<()> {
        let options = (strict_bit_perfect.is_some() || exclusive.is_some())
            .then(|| playback_options(strict_bit_perfect, exclusive));
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::SwitchOutputDevice(device_id, options, tx))
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

    /// 阻塞到当前播放会话结束，返回结束原因：
    /// "ended" 自然播完 / "stopped" 显式停止 / "replaced" 被新播放替换 / "error" 流致命错误。
    #[napi]
    pub async fn wait_finished(&self) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PlayerCommand::WaitFinished(tx))
            .map_err(|_| Error::from_reason("Worker shutdown"))?;
        rx.await
            .map(|reason| reason.as_str().to_string())
            .map_err(|_| Error::from_reason("Playback task interrupted"))
    }
}

fn playback_options(strict_bit_perfect: Option<bool>, exclusive: Option<bool>) -> PlaybackOptions {
    let strict_bit_perfect = strict_bit_perfect.unwrap_or(false);
    PlaybackOptions {
        strict_bit_perfect,
        // BitPerfect 隐含独占：前端即使漏传 exclusive 也要预留设备。
        exclusive: strict_bit_perfect || exclusive.unwrap_or(false),
    }
}
