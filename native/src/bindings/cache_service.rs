use std::path::PathBuf;
use std::sync::Arc;

use napi::{Error, Result};
use napi_derive::napi;

use crate::cache::{CacheBucket, CacheStats, CachedSongSource, NativeCacheService};
use crate::runtime::native_runtime;

#[napi]
pub struct CacheService {
    service: Arc<NativeCacheService>,
}

#[napi]
impl CacheService {
    #[napi(constructor)]
    pub fn new(root_dir: String, max_size_bytes: Option<i64>) -> Result<Self> {
        let root_dir = PathBuf::from(root_dir);
        let fallback_max_size_bytes =
            max_size_bytes.unwrap_or((512 * 1024 * 1024) as i64).max(0) as u64;
        let service = NativeCacheService::new(root_dir, fallback_max_size_bytes)
            .map_err(|err| Error::from_reason(err.to_string()))?;

        Ok(Self {
            service: Arc::new(service),
        })
    }

    #[napi]
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let service = Arc::clone(&self.service);
        native_runtime()
            .spawn_blocking(move || {
                service
                    .get_stats()
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn get_json(&self, bucket: String, key: String) -> Result<Option<String>> {
        let service = Arc::clone(&self.service);

        native_runtime()
            .spawn_blocking(move || {
                let bucket = CacheBucket::try_from(bucket.as_str())
                    .map_err(|err| Error::from_reason(err.to_string()))?;

                service
                    .get_json(bucket, &key)
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn put_json(&self, bucket: String, key: String, value: String) -> Result<CacheStats> {
        let service = Arc::clone(&self.service);

        native_runtime()
            .spawn_blocking(move || {
                let bucket = CacheBucket::try_from(bucket.as_str())
                    .map_err(|err| Error::from_reason(err.to_string()))?;

                service
                    .put_json(bucket, &key, &value)
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn set_max_size_bytes(&self, max_size_bytes: i64) -> Result<CacheStats> {
        let service = Arc::clone(&self.service);
        native_runtime()
            .spawn_blocking(move || {
                service
                    .set_max_size_bytes(max_size_bytes.max(0) as u64)
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn get_song_cache_ahead_secs(&self) -> Result<u32> {
        self.service
            .get_song_cache_ahead_secs()
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn set_song_cache_ahead_secs(&self, song_cache_ahead_secs: u32) -> Result<u32> {
        self.service
            .set_song_cache_ahead_secs(song_cache_ahead_secs)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn get_song_max_cache_ahead_bytes(&self) -> Result<i64> {
        self.service
            .get_song_max_cache_ahead_bytes()
            .map(|value| value.min(i64::MAX as u64) as i64)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn set_song_max_cache_ahead_bytes(
        &self,
        song_max_cache_ahead_bytes: i64,
    ) -> Result<i64> {
        self.service
            .set_song_max_cache_ahead_bytes(song_max_cache_ahead_bytes.max(0) as u64)
            .map(|value| value.min(i64::MAX as u64) as i64)
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn clear(&self) -> Result<CacheStats> {
        let service = Arc::clone(&self.service);
        native_runtime()
            .spawn_blocking(move || {
                service
                    .clear()
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn cache_remote_file(
        &self,
        bucket: String,
        key: String,
        url: String,
    ) -> Result<Option<String>> {
        let bucket = CacheBucket::try_from(bucket.as_str())
            .map_err(|err| Error::from_reason(err.to_string()))?;

        self.service
            .cache_remote_file(bucket, &key, &url)
            .await
            .map(|path| path.map(|path| path.to_string_lossy().into_owned()))
            .map_err(|err| Error::from_reason(err.to_string()))
    }

    #[napi]
    pub async fn prepare_song_source(
        &self,
        song_id: i64,
        quality: String,
        url: String,
    ) -> Result<CachedSongSource> {
        let service = Arc::clone(&self.service);
        native_runtime()
            .spawn_blocking(move || {
                service
                    .prepare_song_source(song_id, &quality, &url)
                    .map_err(|err| Error::from_reason(err.to_string()))
            })
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?
    }
}
