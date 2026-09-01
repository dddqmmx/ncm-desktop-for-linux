use std::future::Future;
use std::pin::Pin;

use napi_derive::napi;

use crate::audio::OutputDeviceInfo;

pub(crate) type BackendResult<T> = std::result::Result<T, String>;
pub(crate) type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = BackendResult<T>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackSource {
    File(String, PlaybackOptions),
    Url(String, PlaybackOptions),
    CachedUrl(CachedUrlPlaybackRequest, PlaybackOptions),
}

impl PlaybackSource {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::File(..) => "file",
            Self::Url(..) => "URL",
            Self::CachedUrl(..) => "cached URL",
        }
    }

    /// 换一份播放选项，源本身不动。设备切换要在新端点上重放当前会话，而会话里
    /// 那份选项是开播那一刻冻结的：用户之后改了独占/BitPerfect 开关、或换到的
    /// 端点根本不支持独占（系统默认输出是虚拟端点），照搬旧选项就会重放失败。
    pub(crate) fn with_options(&self, options: PlaybackOptions) -> Self {
        match self {
            Self::File(path, _) => Self::File(path.clone(), options),
            Self::Url(url, _) => Self::Url(url.clone(), options),
            Self::CachedUrl(request, _) => Self::CachedUrl(request.clone(), options),
        }
    }
}

/// 播放会话结束的原因，随 wait_finished 传给前端；前端据此区分
/// “自然播完自动切歌”与“中断/替换无需处理”，不再靠进度启发式猜测。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinishReason {
    /// 曲目自然播完（或注册监听时已无会话）。
    Ended,
    /// 前端显式 Stop 结束了会话。
    Stopped,
    /// 新的播放请求替换了当前会话。
    Replaced,
    /// 底层流致命错误终止了播放。
    Error,
}

impl FinishReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::Stopped => "stopped",
            Self::Replaced => "replaced",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlaybackOptions {
    pub(crate) strict_bit_perfect: bool,
    /// 独占输出：预留设备（Linux 上通过 pw-reserve 让 PipeWire 释放声卡）后
    /// 直接打开硬件端点。BitPerfect 隐含独占，另外还要求精确样本格式。
    pub(crate) exclusive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CachedUrlPlaybackRequest {
    pub(crate) url: String,
    pub(crate) cache_path: String,
    pub(crate) metadata_path: String,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) cache_ahead_secs: Option<u32>,
    pub(crate) max_cache_ahead_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

impl PlaybackStatus {
    pub(crate) fn as_u32(self) -> u32 {
        match self {
            Self::Stopped => 0,
            Self::Playing => 1,
            Self::Paused => 2,
        }
    }

    pub(crate) fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::Playing,
            2 => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_current: bool,
}

impl From<OutputDeviceInfo> for AudioDeviceInfo {
    fn from(value: OutputDeviceInfo) -> Self {
        Self {
            id: value.id,
            name: value.name,
            is_default: value.is_default,
            is_current: value.is_current,
        }
    }
}

pub(crate) fn seconds_to_duration(seconds: f64) -> std::time::Duration {
    if !seconds.is_finite() || seconds <= 0.0 {
        return std::time::Duration::ZERO;
    }

    std::time::Duration::from_secs_f64(seconds)
}

pub(crate) fn start_secs_to_duration(start_secs: Option<f64>) -> Option<std::time::Duration> {
    start_secs.map(seconds_to_duration)
}

pub(crate) fn duration_to_millis(duration: std::time::Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}
