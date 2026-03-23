use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::error::{CacheError, CacheResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheBucket {
    Song,
    Entity,
    Cover,
    Lyric,
}

impl CacheBucket {
    pub const ALL: [Self; 4] = [Self::Song, Self::Entity, Self::Cover, Self::Lyric];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Song => "song",
            Self::Entity => "entity",
            Self::Cover => "cover",
            Self::Lyric => "lyric",
        }
    }
}

impl TryFrom<&str> for CacheBucket {
    type Error = CacheError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "song" => Ok(Self::Song),
            "entity" => Ok(Self::Entity),
            "cover" => Ok(Self::Cover),
            "lyric" => Ok(Self::Lyric),
            other => Err(CacheError::InvalidBucket(other.to_string())),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub bucket: CacheBucket,
    pub key: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub created_at: u64,
    pub accessed_at: u64,
    pub source_url: Option<String>,
    pub mime_type: Option<String>,
    #[serde(default)]
    pub song_id: Option<i64>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub content_length: Option<u64>,
    #[serde(default)]
    pub is_complete: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheIndexData {
    pub entries: HashMap<String, CacheEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheSettingsData {
    pub max_size_bytes: u64,
    #[serde(default = "default_song_cache_ahead_secs")]
    pub song_cache_ahead_secs: u32,
}

impl Default for CacheSettingsData {
    fn default() -> Self {
        Self {
            max_size_bytes: 512 * 1024 * 1024,
            song_cache_ahead_secs: default_song_cache_ahead_secs(),
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    pub total_bytes: i64,
    pub max_size_bytes: i64,
    pub song_bytes: i64,
    pub song_entries: u32,
    pub entity_bytes: i64,
    pub entity_entries: u32,
    pub cover_bytes: i64,
    pub cover_entries: u32,
    pub lyric_bytes: i64,
    pub lyric_entries: u32,
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct CachedSongSource {
    pub r#type: String,
    pub value: String,
    pub cache_path: Option<String>,
    pub metadata_path: Option<String>,
    pub cache_ahead_secs: Option<u32>,
}

impl CacheStats {
    pub fn with_max_size(max_size_bytes: u64) -> Self {
        Self {
            max_size_bytes: as_i64(max_size_bytes),
            ..Self::default()
        }
    }

    pub fn add_entry(&mut self, entry: &CacheEntry) {
        self.total_bytes = self.total_bytes.saturating_add(as_i64(entry.size_bytes));

        match entry.bucket {
            CacheBucket::Song => {
                self.song_bytes = self.song_bytes.saturating_add(as_i64(entry.size_bytes));
                self.song_entries = self.song_entries.saturating_add(1);
            }
            CacheBucket::Entity => {
                self.entity_bytes = self.entity_bytes.saturating_add(as_i64(entry.size_bytes));
                self.entity_entries = self.entity_entries.saturating_add(1);
            }
            CacheBucket::Cover => {
                self.cover_bytes = self.cover_bytes.saturating_add(as_i64(entry.size_bytes));
                self.cover_entries = self.cover_entries.saturating_add(1);
            }
            CacheBucket::Lyric => {
                self.lyric_bytes = self.lyric_bytes.saturating_add(as_i64(entry.size_bytes));
                self.lyric_entries = self.lyric_entries.saturating_add(1);
            }
        }
    }
}

pub fn composite_key(bucket: CacheBucket, key: &str) -> CacheResult<String> {
    let normalized = key.trim();
    if normalized.is_empty() {
        return Err(CacheError::EmptyKey);
    }

    Ok(format!("{}:{normalized}", bucket.as_str()))
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn as_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn default_song_cache_ahead_secs() -> u32 {
    30
}
