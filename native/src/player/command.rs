use tokio::sync::oneshot;

use super::types::{AudioDeviceInfo, BackendResult, FinishReason, PlaybackOptions, PlaybackSource};

pub(crate) enum PlayerCommand {
    Play(
        PlaybackSource,
        Option<f64>,
        Option<oneshot::Sender<BackendResult<()>>>,
    ),
    Pause,
    Resume,
    Stop,
    Seek(f64),
    SetVolume(f64),
    /// 切换输出端点。第二个参数是调用方此刻的播放选项：设备切换要在新设备上
    /// 重放当前会话，而会话里那份选项是开播时冻结的——用户中途改了独占/
    /// BitPerfect 开关就会用错。None 表示沿用会话原有选项。
    SwitchOutputDevice(
        Option<String>,
        Option<PlaybackOptions>,
        oneshot::Sender<BackendResult<()>>,
    ),
    GetOutputDevices(oneshot::Sender<BackendResult<Vec<AudioDeviceInfo>>>),
    WaitFinished(oneshot::Sender<FinishReason>),
}
