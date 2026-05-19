use tokio::sync::oneshot;

use super::types::{AudioDeviceInfo, BackendResult, CachedUrlPlaybackRequest};

pub(crate) enum PlayerCommand {
    PlayFile(String, Option<f64>),
    PlayUrl(String, Option<f64>),
    PlayUrlCached(CachedUrlPlaybackRequest, Option<f64>),
    Pause,
    Resume,
    Stop,
    Seek(f64),
    SwitchOutputDevice(Option<String>, oneshot::Sender<BackendResult<()>>),
    GetOutputDevices(oneshot::Sender<BackendResult<Vec<AudioDeviceInfo>>>),
    WaitFinished(oneshot::Sender<()>),
}
