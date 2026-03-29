use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;

pub(crate) struct SharedState {
    pub(crate) is_paused: AtomicBool,
    pub(crate) current_frame: AtomicU64,
    pub(crate) sample_rate: AtomicU32,
    pub(crate) has_seek_request: AtomicBool,
    pub(crate) seek_request: Mutex<Option<Duration>>,
    pub(crate) is_terminating: AtomicBool,
    pub(crate) discard_buffer: AtomicBool,
    pub(crate) decoder_done: AtomicBool,
    pub(crate) is_finished: AtomicBool,
    pub(crate) finish_notify: Notify,
    pub(crate) buffered_frames: AtomicU64,
    pub(crate) waiting_for_seek: AtomicBool,
}
