use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::error::CacheResult;
use crate::cache::types::{
    CacheBucket, CacheEntry, CacheIndexData, CacheStats, composite_key, now_unix_secs,
};

pub struct CacheCatalog {
    path: PathBuf,
    data: CacheIndexData,
}

pub struct CacheEntryUpsert<'a> {
    pub bucket: CacheBucket,
    pub key: &'a str,
    pub relative_path: String,
    pub size_bytes: u64,
    pub source_url: Option<String>,
    pub mime_type: Option<String>,
    pub song_id: Option<i64>,
    pub quality: Option<String>,
    pub content_length: Option<u64>,
    pub is_complete: bool,
}

impl CacheCatalog {
    pub fn load(path: impl AsRef<Path>) -> CacheResult<Self> {
        let path = path.as_ref().to_path_buf();
        let data = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str::<CacheIndexData>(&raw)?
        } else {
            CacheIndexData::default()
        };

        Ok(Self { path, data })
    }

    pub fn touch(&mut self, bucket: CacheBucket, key: &str) -> CacheResult<Option<CacheEntry>> {
        let composite = composite_key(bucket, key)?;
        if let Some(entry) = self.data.entries.get_mut(&composite) {
            entry.accessed_at = now_unix_secs();
            return Ok(Some(entry.clone()));
        }

        Ok(None)
    }

    pub fn upsert(&mut self, request: CacheEntryUpsert<'_>) -> CacheResult<Option<String>> {
        let CacheEntryUpsert {
            bucket,
            key,
            relative_path,
            size_bytes,
            source_url,
            mime_type,
            song_id,
            quality,
            content_length,
            is_complete,
        } = request;
        let now = now_unix_secs();
        let composite = composite_key(bucket, key)?;
        let previous_path = self
            .data
            .entries
            .get(&composite)
            .map(|entry| entry.relative_path.clone());
        let created_at = self
            .data
            .entries
            .get(&composite)
            .map(|entry| entry.created_at)
            .unwrap_or(now);

        self.data.entries.insert(
            composite,
            CacheEntry {
                bucket,
                key: key.trim().to_string(),
                relative_path,
                size_bytes,
                created_at,
                accessed_at: now,
                source_url,
                mime_type,
                song_id,
                quality,
                content_length,
                is_complete,
            },
        );

        Ok(previous_path)
    }

    pub fn remove(&mut self, bucket: CacheBucket, key: &str) -> CacheResult<Option<CacheEntry>> {
        let composite = composite_key(bucket, key)?;
        Ok(self.data.entries.remove(&composite))
    }

    pub fn remove_by_composite_key(&mut self, key: &str) -> Option<CacheEntry> {
        self.data.entries.remove(key)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&String, &CacheEntry)> {
        self.data.entries.iter()
    }

    pub fn entry_mut(&mut self, key: &str) -> Option<&mut CacheEntry> {
        self.data.entries.get_mut(key)
    }

    pub fn clear(&mut self) {
        self.data.entries.clear();
    }

    pub fn persist(&self) -> CacheResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_vec_pretty(&self.data)?;
        fs::write(&self.path, serialized)?;
        Ok(())
    }

    pub fn stats(&self, max_size_bytes: u64) -> CacheStats {
        let mut stats = CacheStats::with_max_size(max_size_bytes);
        for entry in self.data.entries.values() {
            stats.add_entry(entry);
        }
        stats
    }
}
