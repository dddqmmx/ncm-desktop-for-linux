use sha2::{Digest, Sha256};
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::cache::error::CacheResult;
use crate::cache::song::SongStreamCacheMeta;
use crate::cache::types::CacheBucket;

pub struct CacheFileStore {
    root_dir: PathBuf,
}

impl CacheFileStore {
    pub fn new(root_dir: impl AsRef<Path>) -> CacheResult<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root_dir)?;

        let store = Self { root_dir };
        for bucket in CacheBucket::ALL {
            fs::create_dir_all(store.bucket_dir(bucket))?;
        }

        Ok(store)
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root_dir.join("settings.json")
    }

    pub fn index_path(&self) -> PathBuf {
        self.root_dir.join("index.json")
    }

    pub fn bucket_dir(&self, bucket: CacheBucket) -> PathBuf {
        self.root_dir.join(bucket.as_str())
    }

    pub fn build_relative_path(&self, bucket: CacheBucket, key: &str, extension: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bucket.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(key.trim().as_bytes());
        let hash = hex::encode(hasher.finalize());
        let normalized_ext = normalize_extension(extension);
        format!("{}/{}.{}", bucket.as_str(), hash, normalized_ext)
    }

    pub fn absolute_path(&self, relative_path: &str) -> PathBuf {
        self.root_dir.join(relative_path)
    }

    pub fn song_meta_path(&self, relative_path: &str) -> PathBuf {
        self.absolute_path(&format!("{relative_path}.meta.json"))
    }

    pub fn read_bytes(&self, relative_path: &str) -> CacheResult<Vec<u8>> {
        Ok(fs::read(self.absolute_path(relative_path))?)
    }

    pub fn write_bytes(&self, relative_path: &str, bytes: &[u8]) -> CacheResult<PathBuf> {
        let absolute_path = self.absolute_path(relative_path);
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&absolute_path, bytes)?;
        Ok(absolute_path)
    }

    pub fn remove(&self, relative_path: &str) -> CacheResult<()> {
        let absolute_path = self.absolute_path(relative_path);
        if absolute_path.exists() {
            fs::remove_file(absolute_path)?;
        }
        Ok(())
    }

    pub fn exists(&self, relative_path: &str) -> bool {
        self.absolute_path(relative_path).exists()
    }

    pub fn file_size(&self, relative_path: &str) -> CacheResult<Option<u64>> {
        let absolute_path = self.absolute_path(relative_path);
        if !absolute_path.exists() {
            return Ok(None);
        }

        Ok(Some(fs::metadata(absolute_path)?.len()))
    }

    pub fn read_song_meta(&self, relative_path: &str) -> CacheResult<Option<SongStreamCacheMeta>> {
        let meta_path = self.song_meta_path(relative_path);
        if !meta_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(meta_path)?;
        Ok(Some(serde_json::from_str::<SongStreamCacheMeta>(&raw)?))
    }

    pub fn write_song_meta(
        &self,
        relative_path: &str,
        meta: &SongStreamCacheMeta,
    ) -> CacheResult<PathBuf> {
        let meta_path = self.song_meta_path(relative_path);
        if let Some(parent) = meta_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = meta_path.with_extension("meta.json.tmp");
        let serialized = serde_json::to_vec_pretty(meta)?;
        fs::write(&tmp_path, serialized)?;
        fs::rename(&tmp_path, &meta_path)?;
        Ok(meta_path)
    }

    pub fn ensure_song_file(&self, relative_path: &str) -> CacheResult<PathBuf> {
        let absolute_path = self.absolute_path(relative_path);
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)?;
        }

        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&absolute_path)?;

        Ok(absolute_path)
    }

    pub fn remove_song_meta(&self, relative_path: &str) -> CacheResult<()> {
        let meta_path = self.song_meta_path(relative_path);
        if meta_path.exists() {
            fs::remove_file(meta_path)?;
        }
        Ok(())
    }

    pub fn clear_bucket(&self, bucket: CacheBucket) -> CacheResult<()> {
        let bucket_dir = self.bucket_dir(bucket);
        if bucket_dir.exists() {
            fs::remove_dir_all(&bucket_dir)?;
        }
        fs::create_dir_all(bucket_dir)?;
        Ok(())
    }
}

fn normalize_extension(extension: &str) -> &str {
    let trimmed = extension.trim().trim_start_matches('.');
    if trimmed.is_empty() { "bin" } else { trimmed }
}
