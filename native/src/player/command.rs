use tokio::sync::oneshot;

use super::types::{AudioDeviceInfo, BackendResult, CachedUrlPlaybackRequest, PlaybackOptions};

pub(crate) enum PlayerCommand {
    PlayFile(
        String,
        Option<f64>,
        PlaybackOptions,
        Option<oneshot::Sender<BackendResult<()>>>,
    ),
    PlayUrl(
        String,
        Option<f64>,
        PlaybackOptions,
        Option<oneshot::Sender<BackendResult<()>>>,
    ),
    PlayUrlCached(
        CachedUrlPlaybackRequest,
        Option<f64>,
        PlaybackOptions,
        Option<oneshot::Sender<BackendResult<()>>>,
    ),
    Pause,
    Resume,
    Stop,
    Seek(f64),
    SwitchOutputDevice(Option<String>, oneshot::Sender<BackendResult<()>>),
    GetOutputDevices(oneshot::Sender<BackendResult<Vec<AudioDeviceInfo>>>),
    WaitFinished(oneshot::Sender<()>),
}
