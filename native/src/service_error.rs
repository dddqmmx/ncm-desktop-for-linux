//! The services are usable without loading a JavaScript runtime.
#[cfg(feature = "node")]
pub use napi::{Error, Result};

#[cfg(not(feature = "node"))]
#[derive(Debug)]
pub struct Error(String);
#[cfg(not(feature = "node"))]
impl Error {
    pub fn from_reason(reason: impl Into<String>) -> Self { Self(reason.into()) }
}
#[cfg(not(feature = "node"))]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
}
#[cfg(not(feature = "node"))]
impl std::error::Error for Error {}
#[cfg(not(feature = "node"))]
pub type Result<T> = std::result::Result<T, Error>;
