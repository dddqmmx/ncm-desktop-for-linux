use std::future::Future;
use std::pin::Pin;

use napi_derive::napi;

use crate::audio::OutputDeviceInfo;

pub(crate) type BackendResult<T> = std::result::Result<T, String>;
pub(crate) type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = BackendResult<T>> + Send + 'a>>;
pub(crate) type SignalFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackSource {
    File(String),
    Url(String),
    CachedUrl(CachedUrlPlaybackRequest),
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
