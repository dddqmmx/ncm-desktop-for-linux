use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::error::CacheResult;
use crate::cache::types::CacheSettingsData;

pub struct CacheSettingsStore {
    path: PathBuf,
    settings: CacheSettingsData,
}

impl CacheSettingsStore {
    pub fn load(path: impl AsRef<Path>, fallback_max_size_bytes: u64) -> CacheResult<Self> {
        let path = path.as_ref().to_path_buf();
        let settings = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str::<CacheSettingsData>(&raw)?
        } else {
            CacheSettingsData {
                max_size_bytes: fallback_max_size_bytes,
                ..CacheSettingsData::default()
            }
        };

        Ok(Self { path, settings })
    }

    pub fn max_size_bytes(&self) -> u64 {
        self.settings.max_size_bytes
    }

    pub fn set_max_size_bytes(&mut self, max_size_bytes: u64) -> CacheResult<()> {
        self.settings.max_size_bytes = max_size_bytes;
        self.persist()
    }

    pub fn song_cache_ahead_secs(&self) -> u32 {
        self.settings.song_cache_ahead_secs
    }

    pub fn set_song_cache_ahead_secs(&mut self, song_cache_ahead_secs: u32) -> CacheResult<()> {
        self.settings.song_cache_ahead_secs = song_cache_ahead_secs.clamp(5, 300);
        self.persist()
    }

    pub fn persist(&self) -> CacheResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_vec_pretty(&self.settings)?;
        fs::write(&self.path, serialized)?;
        Ok(())
    }
}
