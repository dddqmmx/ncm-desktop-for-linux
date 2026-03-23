use std::fmt::{Display, Formatter};
use std::io;

#[derive(Debug)]
pub enum CacheError {
    InvalidBucket(String),
    EmptyKey,
    Io(io::Error),
    Json(serde_json::Error),
    Http(reqwest::Error),
    HttpStatus(u16),
    Poisoned,
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBucket(bucket) => write!(f, "Unsupported cache bucket: {bucket}"),
            Self::EmptyKey => write!(f, "Cache key must not be empty"),
            Self::Io(err) => write!(f, "Cache I/O error: {err}"),
            Self::Json(err) => write!(f, "Cache JSON error: {err}"),
            Self::Http(err) => write!(f, "Cache HTTP error: {err}"),
            Self::HttpStatus(status) => write!(f, "Remote download failed with status {status}"),
            Self::Poisoned => write!(f, "Cache state lock was poisoned"),
        }
    }
}

impl std::error::Error for CacheError {}

impl From<io::Error> for CacheError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<reqwest::Error> for CacheError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

pub type CacheResult<T> = std::result::Result<T, CacheError>;
