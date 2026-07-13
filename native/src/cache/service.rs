use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::CONTENT_TYPE;

use crate::audio::player::{AudioPlayer, SongCacheDownloadControl};
use crate::cache::catalog::{CacheCatalog, CacheEntryUpsert};
use crate::cache::error::{CacheError, CacheResult};
use crate::cache::eviction::EvictionPlanner;
use crate::cache::settings::CacheSettingsStore;
use crate::cache::song::SongStreamCacheMeta;
use crate::cache::storage::CacheFileStore;
use crate::cache::types::{CacheBucket, CacheStats, CachedSongSource, SongCacheProgress};
use crate::runtime::native_runtime;

struct CacheState {
    settings: CacheSettingsStore,
    catalog: CacheCatalog,
}

impl CacheState {
    fn stats(&self) -> CacheStats {
        self.catalog.stats(self.settings.max_size_bytes())
    }

    fn song_cache_ahead_secs(&self) -> u32 {
        self.settings.song_cache_ahead_secs()
    }

    fn song_max_cache_ahead_bytes(&self) -> u64 {
        self.settings.song_max_cache_ahead_bytes()
    }
}

pub struct NativeCacheService {
    files: CacheFileStore,
    state: Mutex<CacheState>,
    active_song_downloads: Mutex<HashMap<String, Arc<SongCacheDownloadControl>>>,
    http_client: reqwest::Client,
}

impl NativeCacheService {
    pub fn new(root_dir: impl AsRef<Path>, fallback_max_size_bytes: u64) -> CacheResult<Self> {
        let files = CacheFileStore::new(root_dir)?;
        let settings = CacheSettingsStore::load(files.settings_path(), fallback_max_size_bytes)?;
        settings.persist()?;

        let catalog = CacheCatalog::load(files.index_path())?;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        let service = Self {
            files,
            state: Mutex::new(CacheState { settings, catalog }),
            active_song_downloads: Mutex::new(HashMap::new()),
            http_client,
        };

        service.reconcile_missing_files()?;
        service.enforce_limit()?;

        Ok(service)
    }

    pub fn get_stats(&self) -> CacheResult<CacheStats> {
        let state = self.lock_state()?;
        Ok(state.stats())
    }

    pub fn get_json(&self, bucket: CacheBucket, key: &str) -> CacheResult<Option<String>> {
        let relative_path = {
            let mut state = self.lock_state()?;
            let entry = match state.catalog.touch(bucket, key)? {
                Some(entry) if !entry.is_expired() => entry,
                Some(_) => {
                    state.catalog.remove(bucket, key)?;
                    state.catalog.persist()?;
                    return Ok(None);
                }
                None => return Ok(None),
            };

            if !self.files.exists(&entry.relative_path) {
                state.catalog.remove(bucket, key)?;
                state.catalog.persist()?;
                return Ok(None);
            }

            entry.relative_path
        };

        let bytes = self.files.read_bytes(&relative_path)?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    pub fn put_json(&self, bucket: CacheBucket, key: &str, value: &str) -> CacheResult<CacheStats> {
        self.store_bytes(
            bucket,
            key,
            value.as_bytes(),
            "json",
            None,
            Some("application/json"),
            bucket.default_ttl_secs(),
        )?;
        self.get_stats()
    }

    pub fn set_max_size_bytes(&self, max_size_bytes: u64) -> CacheResult<CacheStats> {
        {
            let mut state = self.lock_state()?;
            state.settings.set_max_size_bytes(max_size_bytes)?;
        }

        self.enforce_limit()?;
        self.get_stats()
    }

    pub fn get_song_cache_ahead_secs(&self) -> CacheResult<u32> {
        let state = self.lock_state()?;
        Ok(state.song_cache_ahead_secs())
    }

    pub fn set_song_cache_ahead_secs(&self, song_cache_ahead_secs: u32) -> CacheResult<u32> {
        let mut state = self.lock_state()?;
        state
            .settings
            .set_song_cache_ahead_secs(song_cache_ahead_secs)?;
        Ok(state.song_cache_ahead_secs())
    }

    pub fn get_song_max_cache_ahead_bytes(&self) -> CacheResult<u64> {
        let state = self.lock_state()?;
        Ok(state.song_max_cache_ahead_bytes())
    }

    pub fn set_song_max_cache_ahead_bytes(
        &self,
        song_max_cache_ahead_bytes: u64,
    ) -> CacheResult<u64> {
        let mut state = self.lock_state()?;
        state
            .settings
            .set_song_max_cache_ahead_bytes(song_max_cache_ahead_bytes)?;
        Ok(state.song_max_cache_ahead_bytes())
    }

    pub fn clear(&self) -> CacheResult<CacheStats> {
        self.cancel_all_song_cache_downloads();
        {
            let mut state = self.lock_state()?;
            state.catalog.clear();
            state.catalog.persist()?;
        }

        for bucket in CacheBucket::ALL {
            self.files.clear_bucket(bucket)?;
        }

        self.get_stats()
    }

    pub async fn cache_remote_file(
        &self,
        bucket: CacheBucket,
        key: &str,
        url: &str,
    ) -> CacheResult<Option<PathBuf>> {
        if let Some(path) = self.get_existing_path(bucket, key)? {
            return Ok(Some(path));
        }

        let response = self.http_client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(CacheError::HttpStatus(status.as_u16()));
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await?;
        let extension = infer_extension(bucket, url, content_type.as_deref());

        self.store_bytes(
            bucket,
            key,
            bytes.as_ref(),
            &extension,
            Some(url.to_string()),
            content_type.as_deref(),
            bucket.default_ttl_secs(),
        )
    }

    pub fn prepare_song_source(
        &self,
        song_id: i64,
        quality: &str,
        url: &str,
        expected_bytes: Option<u64>,
    ) -> CacheResult<CachedSongSource> {
        let normalized_url = url.trim();
        if normalized_url.is_empty() {
            return Ok(CachedSongSource {
                r#type: "url".to_string(),
                value: String::new(),
                cache_path: None,
                metadata_path: None,
                cache_ahead_secs: None,
                max_cache_ahead_bytes: None,
            });
        }

        let normalized_quality = quality.trim().to_ascii_lowercase();
        let cache_key = song_stream_key(song_id, &normalized_quality);

        let (existing_source, cache_ahead_secs, max_cache_ahead_bytes) = {
            let mut state = self.lock_state()?;

            let (same_quality_entry, variant_victims, cache_ahead_secs, max_cache_ahead_bytes) = {
                let same_quality_entry = state.catalog.entries().find_map(|(_, entry)| {
                    if entry.bucket != CacheBucket::Song {
                        return None;
                    }

                    if entry.song_id == Some(song_id)
                        && entry.quality.as_deref() == Some(normalized_quality.as_str())
                    {
                        return Some(entry.clone());
                    }

                    (entry.song_id.is_none()
                        && is_legacy_audio_key_for_song(&entry.key, song_id)
                        && legacy_audio_key_matches_quality(&entry.key, &normalized_quality))
                    .then_some(entry.clone())
                });
                let variant_victims = state
                    .catalog
                    .entries()
                    .filter_map(|(composite_key, entry)| {
                        (entry.bucket == CacheBucket::Song
                            && ((entry.song_id == Some(song_id)
                                && entry.quality.as_deref() != Some(normalized_quality.as_str()))
                                || (entry.song_id.is_none()
                                    && is_legacy_audio_key_for_song(&entry.key, song_id)
                                    && !legacy_audio_key_matches_quality(
                                        &entry.key,
                                        &normalized_quality,
                                    ))))
                        .then_some(composite_key.clone())
                    })
                    .collect::<Vec<_>>();
                (
                    same_quality_entry,
                    variant_victims,
                    state.song_cache_ahead_secs(),
                    state.song_max_cache_ahead_bytes(),
                )
            };

            for victim in variant_victims {
                if let Some(entry) = state.catalog.remove_by_composite_key(&victim) {
                    self.remove_entry_files(&entry)?;
                }
            }

            let mut existing_source = None;
            if let Some(entry) = same_quality_entry {
                if let Some(expected_bytes) = expected_bytes {
                    let cached_bytes = entry
                        .content_length
                        .or_else(|| self.files.file_size(&entry.relative_path).ok().flatten())
                        .unwrap_or(entry.size_bytes);

                    if cached_bytes != expected_bytes {
                        if let Some(entry) = state.catalog.remove(entry.bucket, &entry.key)? {
                            self.remove_entry_files(&entry)?;
                        }
                    } else {
                        existing_source = self.prepare_existing_song_source_locked(
                            &mut state,
                            entry,
                            song_id,
                            &normalized_quality,
                            normalized_url,
                            &cache_key,
                            cache_ahead_secs,
                            max_cache_ahead_bytes,
                        )?;
                    }
                } else {
                    existing_source = self.prepare_existing_song_source_locked(
                        &mut state,
                        entry,
                        song_id,
                        &normalized_quality,
                        normalized_url,
                        &cache_key,
                        cache_ahead_secs,
                        max_cache_ahead_bytes,
                    )?;
                }
            }

            state.catalog.persist()?;
            (existing_source, cache_ahead_secs, max_cache_ahead_bytes)
        };

        if let Some(source) = existing_source {
            return Ok(source);
        }

        let extension = infer_extension(CacheBucket::Song, normalized_url, None);
        let relative_path =
            self.files
                .build_relative_path(CacheBucket::Song, &cache_key, &extension);
        let absolute_path = self.files.ensure_song_file(&relative_path)?;
        let mut meta = SongStreamCacheMeta::new(song_id, &normalized_quality, normalized_url);
        meta.set_content_length(expected_bytes);
        let metadata_path = self.files.write_song_meta(&relative_path, &meta)?;

        {
            let mut state = self.lock_state()?;
            state.catalog.upsert(CacheEntryUpsert {
                bucket: CacheBucket::Song,
                key: &cache_key,
                relative_path,
                size_bytes: 0,
                source_url: Some(normalized_url.to_string()),
                mime_type: None,
                song_id: Some(song_id),
                quality: Some(normalized_quality),
                content_length: expected_bytes,
                is_complete: false,
                ttl_secs: None,
            })?;
            state.catalog.persist()?;
        }

        Ok(CachedSongSource {
            r#type: "url".to_string(),
            value: normalized_url.to_string(),
            cache_path: Some(absolute_path.to_string_lossy().into_owned()),
            metadata_path: Some(metadata_path.to_string_lossy().into_owned()),
            cache_ahead_secs: Some(cache_ahead_secs),
            max_cache_ahead_bytes: Some(max_cache_ahead_bytes.min(i64::MAX as u64) as i64),
        })
    }

    pub fn spawn_song_cache_download(
        self_arc: Arc<Self>,
        song_id: i64,
        quality: String,
        url: String,
        expected_bytes: Option<u64>,
        duration_ms: Option<u64>,
    ) -> CacheResult<CachedSongSource> {
        let normalized_quality = quality.trim().to_ascii_lowercase();
        let source =
            self_arc.prepare_song_source(song_id, &normalized_quality, &url, expected_bytes)?;

        if source.r#type == "file" {
            return Ok(source);
        }

        let cache_path = source
            .cache_path
            .clone()
            .ok_or_else(|| CacheError::InvalidState("missing cache_path".to_string()))?;
        let metadata_path = source
            .metadata_path
            .clone()
            .ok_or_else(|| CacheError::InvalidState("missing metadata_path".to_string()))?;
        let cache_ahead_secs = source.cache_ahead_secs.map(|value| value as u32);
        let max_cache_ahead_bytes = source.max_cache_ahead_bytes.map(|value| value as u64);
        let download_control = Arc::new(SongCacheDownloadControl::new());
        self_arc.register_song_cache_download(&metadata_path, Arc::clone(&download_control))?;

        println!(
            "[cache] 启动后台流式缓存: song_id={} quality={} cache_path={}",
            song_id, normalized_quality, cache_path
        );

        let cache_path_for_task = cache_path.clone();
        let url_for_task = url.clone();
        let quality_for_task = normalized_quality.clone();
        native_runtime().spawn(async move {
            println!(
                "[cache] 后台下载开始: song_id={} quality={}",
                song_id, quality_for_task
            );

            let download_outcome = match AudioPlayer::download_song_for_cache(
                &url_for_task,
                &cache_path_for_task,
                &metadata_path,
                duration_ms,
                cache_ahead_secs,
                max_cache_ahead_bytes,
                Arc::clone(&download_control),
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(err) => {
                    eprintln!("[cache] 后台下载失败: song_id={} err={}", song_id, err);
                    if let Err(cleanup_err) =
                        self_arc.remove_song_cache_entry(song_id, &quality_for_task)
                    {
                        eprintln!(
                            "[cache] 清理失败缓存条目失败: song_id={} err={}",
                            song_id, cleanup_err
                        );
                    }
                    self_arc.finish_song_cache_download(&metadata_path, &download_control);
                    return;
                }
            };

            if !download_outcome.is_complete {
                if let Err(err) = self_arc.mark_song_cache_partial(
                    song_id,
                    &quality_for_task,
                    &cache_path_for_task,
                    download_outcome.downloaded_bytes,
                    &url_for_task,
                ) {
                    eprintln!(
                        "[cache] 记录歌曲预下载失败: song_id={} err={}",
                        song_id, err
                    );
                } else {
                    println!(
                        "[cache] 歌曲预下载完成: song_id={} quality={} size={}",
                        song_id, quality_for_task, download_outcome.downloaded_bytes
                    );
                }
                self_arc.finish_song_cache_download(&metadata_path, &download_control);
                return;
            }

            if let Err(err) = self_arc.mark_song_cache_complete(
                song_id,
                &quality_for_task,
                &cache_path_for_task,
                download_outcome.downloaded_bytes,
                &url_for_task,
            ) {
                eprintln!("[cache] 标记缓存完成失败: song_id={} err={}", song_id, err);
            } else {
                println!(
                    "[cache] 后台下载完成: song_id={} quality={} size={}",
                    song_id, quality_for_task, download_outcome.downloaded_bytes
                );
            }
            self_arc.finish_song_cache_download(&metadata_path, &download_control);
        });

        Ok(source)
    }

    pub fn update_song_cache_playback_position(
        &self,
        metadata_path: &str,
        playback_position_ms: u64,
    ) -> CacheResult<bool> {
        let downloads = self
            .active_song_downloads
            .lock()
            .map_err(|_| CacheError::Poisoned)?;
        let Some(control) = downloads.get(metadata_path) else {
            return Ok(false);
        };
        control.update_playback_position(playback_position_ms);
        Ok(true)
    }

    pub fn cancel_song_cache_download(&self, metadata_path: &str) -> CacheResult<bool> {
        let control = self
            .active_song_downloads
            .lock()
            .map_err(|_| CacheError::Poisoned)?
            .remove(metadata_path);
        let Some(control) = control else {
            return Ok(false);
        };
        control.cancel();
        control.wait_finished();
        Ok(true)
    }

    fn register_song_cache_download(
        &self,
        metadata_path: &str,
        control: Arc<SongCacheDownloadControl>,
    ) -> CacheResult<()> {
        let previous = self
            .active_song_downloads
            .lock()
            .map_err(|_| CacheError::Poisoned)?
            .insert(metadata_path.to_string(), control);
        if let Some(previous) = previous {
            previous.cancel();
            previous.wait_finished();
        }
        Ok(())
    }

    fn unregister_song_cache_download(
        &self,
        metadata_path: &str,
        control: &Arc<SongCacheDownloadControl>,
    ) {
        let Ok(mut downloads) = self.active_song_downloads.lock() else {
            return;
        };
        if downloads
            .get(metadata_path)
            .is_some_and(|active| Arc::ptr_eq(active, control))
        {
            downloads.remove(metadata_path);
        }
    }

    fn finish_song_cache_download(
        &self,
        metadata_path: &str,
        control: &Arc<SongCacheDownloadControl>,
    ) {
        self.unregister_song_cache_download(metadata_path, control);
        control.finish();
    }

    fn cancel_all_song_cache_downloads(&self) {
        let downloads = self
            .active_song_downloads
            .lock()
            .map(|mut downloads| {
                downloads
                    .drain()
                    .map(|(_, control)| control)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for control in downloads {
            control.cancel();
            control.wait_finished();
        }
    }

    fn mark_song_cache_complete(
        &self,
        song_id: i64,
        normalized_quality: &str,
        cache_path: &str,
        file_size: u64,
        url: &str,
    ) -> CacheResult<()> {
        let cache_key = song_stream_key(song_id, normalized_quality);
        let cache_path_buf = PathBuf::from(cache_path);
        let relative_path = self
            .files
            .relative_path(&cache_path_buf)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| cache_path.to_string());

        let mut state = self.lock_state()?;
        state.catalog.upsert(CacheEntryUpsert {
            bucket: CacheBucket::Song,
            key: &cache_key,
            relative_path,
            size_bytes: file_size,
            source_url: Some(url.to_string()),
            mime_type: None,
            song_id: Some(song_id),
            quality: Some(normalized_quality.to_string()),
            content_length: Some(file_size),
            is_complete: true,
            ttl_secs: None,
        })?;
        state.catalog.persist()?;
        Ok(())
    }

    fn mark_song_cache_partial(
        &self,
        song_id: i64,
        normalized_quality: &str,
        cache_path: &str,
        downloaded_bytes: u64,
        url: &str,
    ) -> CacheResult<()> {
        let cache_key = song_stream_key(song_id, normalized_quality);
        let cache_path_buf = PathBuf::from(cache_path);
        let relative_path = self
            .files
            .relative_path(&cache_path_buf)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| cache_path.to_string());
        let content_length = self
            .files
            .read_song_meta(&relative_path)?
            .and_then(|meta| meta.content_length);

        {
            let mut state = self.lock_state()?;
            state.catalog.upsert(CacheEntryUpsert {
                bucket: CacheBucket::Song,
                key: &cache_key,
                relative_path,
                size_bytes: downloaded_bytes,
                source_url: Some(url.to_string()),
                mime_type: None,
                song_id: Some(song_id),
                quality: Some(normalized_quality.to_string()),
                content_length,
                is_complete: false,
                ttl_secs: None,
            })?;
            state.catalog.persist()?;
        }

        self.enforce_limit()
    }

    fn remove_song_cache_entry(&self, song_id: i64, normalized_quality: &str) -> CacheResult<()> {
        let cache_key = song_stream_key(song_id, normalized_quality);
        let mut state = self.lock_state()?;
        if let Some(entry) = state.catalog.remove(CacheBucket::Song, &cache_key)? {
            self.remove_entry_files(&entry)?;
        }
        state.catalog.persist()?;
        Ok(())
    }

    pub fn get_song_cache_progress(&self, metadata_path: &str) -> CacheResult<SongCacheProgress> {
        let path = PathBuf::from(metadata_path);
        if !self.files.is_inside_root(&path) {
            return Ok(SongCacheProgress::default());
        }

        if !path.exists() {
            return Ok(SongCacheProgress::default());
        }

        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Ok(SongCacheProgress::default()),
        };

        let meta: SongStreamCacheMeta = match serde_json::from_str(&raw) {
            Ok(meta) => meta,
            Err(_) => return Ok(SongCacheProgress::default()),
        };

        let downloaded_bytes = meta.downloaded_bytes();
        let total_bytes = meta.content_length.unwrap_or(0);
        let percent = if total_bytes > 0 {
            ((downloaded_bytes as f64 / total_bytes as f64) * 100.0).min(100.0)
        } else {
            0.0
        };

        Ok(SongCacheProgress {
            downloaded_bytes: downloaded_bytes.min(i64::MAX as u64) as i64,
            total_bytes: total_bytes.min(i64::MAX as u64) as i64,
            percent,
            is_complete: meta.is_fully_downloaded(),
        })
    }

    fn prepare_existing_song_source_locked(
        &self,
        state: &mut CacheState,
        entry: crate::cache::types::CacheEntry,
        song_id: i64,
        normalized_quality: &str,
        normalized_url: &str,
        cache_key: &str,
        cache_ahead_secs: u32,
        max_cache_ahead_bytes: u64,
    ) -> CacheResult<Option<CachedSongSource>> {
        if entry.song_id.is_none() && self.files.exists(&entry.relative_path) {
            let file_size = self.files.file_size(&entry.relative_path)?.unwrap_or(0);
            let mut meta = SongStreamCacheMeta::new(song_id, normalized_quality, normalized_url);
            meta.set_content_length(Some(file_size));
            meta.add_range(0..file_size);
            meta.mark_complete();
            self.files.write_song_meta(&entry.relative_path, &meta)?;

            let _ = state.catalog.remove(CacheBucket::Song, &entry.key)?;
            state.catalog.upsert(CacheEntryUpsert {
                bucket: CacheBucket::Song,
                key: cache_key,
                relative_path: entry.relative_path.clone(),
                size_bytes: file_size,
                source_url: Some(normalized_url.to_string()),
                mime_type: entry.mime_type.clone(),
                song_id: Some(song_id),
                quality: Some(normalized_quality.to_string()),
                content_length: Some(file_size),
                is_complete: true,
                ttl_secs: None,
            })?;

            return Ok(Some(CachedSongSource {
                r#type: "file".to_string(),
                value: self
                    .files
                    .absolute_path(&entry.relative_path)
                    .to_string_lossy()
                    .into_owned(),
                cache_path: None,
                metadata_path: None,
                cache_ahead_secs: Some(cache_ahead_secs),
                max_cache_ahead_bytes: Some(max_cache_ahead_bytes.min(i64::MAX as u64) as i64),
            }));
        }

        if entry.is_complete && self.files.exists(&entry.relative_path) {
            let _ = state.catalog.touch(CacheBucket::Song, cache_key)?;
            return Ok(Some(CachedSongSource {
                r#type: "file".to_string(),
                value: self
                    .files
                    .absolute_path(&entry.relative_path)
                    .to_string_lossy()
                    .into_owned(),
                cache_path: None,
                metadata_path: None,
                cache_ahead_secs: Some(cache_ahead_secs),
                max_cache_ahead_bytes: Some(max_cache_ahead_bytes.min(i64::MAX as u64) as i64),
            }));
        }

        if let Some(entry) = state.catalog.remove(entry.bucket, &entry.key)? {
            self.remove_entry_files(&entry)?;
        }

        Ok(None)
    }

    fn get_existing_path(&self, bucket: CacheBucket, key: &str) -> CacheResult<Option<PathBuf>> {
        let relative_path = {
            let mut state = self.lock_state()?;
            let entry = match state.catalog.touch(bucket, key)? {
                Some(entry) if !entry.is_expired() => entry,
                Some(_) => {
                    state.catalog.remove(bucket, key)?;
                    state.catalog.persist()?;
                    return Ok(None);
                }
                None => return Ok(None),
            };

            if !self.files.exists(&entry.relative_path) {
                state.catalog.remove(bucket, key)?;
                state.catalog.persist()?;
                return Ok(None);
            }

            entry.relative_path
        };

        Ok(Some(self.files.absolute_path(&relative_path)))
    }

    fn store_bytes(
        &self,
        bucket: CacheBucket,
        key: &str,
        bytes: &[u8],
        extension: &str,
        source_url: Option<String>,
        mime_type: Option<&str>,
        ttl_secs: Option<u64>,
    ) -> CacheResult<Option<PathBuf>> {
        let relative_path = self.files.build_relative_path(bucket, key, extension);
        let absolute_path = self.files.write_bytes(&relative_path, bytes)?;

        {
            let mut state = self.lock_state()?;
            let previous_path = state.catalog.upsert(CacheEntryUpsert {
                bucket,
                key,
                relative_path: relative_path.clone(),
                size_bytes: bytes.len() as u64,
                source_url,
                mime_type: mime_type.map(str::to_string),
                song_id: None,
                quality: None,
                content_length: None,
                is_complete: false,
                ttl_secs,
            })?;

            if let Some(previous_path) = previous_path
                && previous_path != relative_path
            {
                self.files.remove(&previous_path)?;
                self.files.remove_song_meta(&previous_path)?;
            }

            state.catalog.persist()?;
        }

        self.enforce_limit()?;

        if self.get_existing_path(bucket, key)?.is_some() {
            Ok(Some(absolute_path))
        } else {
            Ok(None)
        }
    }

    fn reconcile_missing_files(&self) -> CacheResult<()> {
        self.refresh_catalog_from_disk()?;

        let victims = {
            let state = self.lock_state()?;
            state
                .catalog
                .entries()
                .filter_map(|(composite_key, entry)| {
                    if self.files.exists(&entry.relative_path) {
                        None
                    } else {
                        Some(composite_key.clone())
                    }
                })
                .collect::<Vec<_>>()
        };

        if victims.is_empty() {
            return Ok(());
        }

        let mut state = self.lock_state()?;
        for victim in victims {
            if let Some(entry) = state.catalog.remove_by_composite_key(&victim) {
                self.remove_entry_files(&entry)?;
            }
        }
        state.catalog.persist()
    }

    fn enforce_limit(&self) -> CacheResult<()> {
        let victims = {
            let state = self.lock_state()?;
            let stats = state.stats();
            EvictionPlanner::select_victims(
                state.catalog.entries(),
                stats.total_bytes.max(0) as u64,
                state.settings.max_size_bytes(),
            )
        };

        if victims.is_empty() {
            return Ok(());
        }

        let mut state = self.lock_state()?;
        for victim in victims {
            if let Some(entry) = state.catalog.remove_by_composite_key(&victim) {
                self.remove_entry_files(&entry)?;
            }
        }
        state.catalog.persist()
    }

    fn refresh_catalog_from_disk(&self) -> CacheResult<()> {
        let mut state = self.lock_state()?;

        let snapshots: Vec<_> = state
            .catalog
            .entries()
            .map(|(composite_key, entry)| (composite_key.clone(), entry.clone()))
            .collect();

        if snapshots.is_empty() {
            return Ok(());
        }

        let mut dirty = false;
        let mut victims = Vec::new();

        for (composite_key, snapshot) in snapshots {
            if !self.files.exists(&snapshot.relative_path) {
                victims.push(composite_key);
                dirty = true;
                continue;
            }

            if snapshot.song_id.is_none() {
                continue;
            }

            let Some(entry) = state.catalog.entry_mut(&composite_key) else {
                continue;
            };

            if let Some(meta) = self.files.read_song_meta(&entry.relative_path)? {
                let downloaded_bytes = meta.downloaded_bytes();
                let is_complete = meta.is_fully_downloaded();
                if entry.size_bytes != downloaded_bytes
                    || entry.content_length != meta.content_length
                    || entry.is_complete != is_complete
                    || entry.source_url.as_deref() != Some(meta.source_url.as_str())
                    || entry.quality.as_deref() != Some(meta.quality.as_str())
                {
                    entry.size_bytes = downloaded_bytes;
                    entry.content_length = meta.content_length;
                    entry.is_complete = is_complete;
                    entry.source_url = Some(meta.source_url);
                    entry.quality = Some(meta.quality);
                    dirty = true;
                }
            } else if let Some(size_bytes) = self.files.file_size(&entry.relative_path)?
                && (entry.size_bytes != size_bytes
                    || entry.content_length.is_some()
                    || entry.is_complete)
            {
                entry.size_bytes = size_bytes;
                entry.content_length = None;
                entry.is_complete = false;
                dirty = true;
            }
        }

        if !victims.is_empty() {
            for victim in victims {
                if let Some(entry) = state.catalog.remove_by_composite_key(&victim) {
                    self.remove_entry_files(&entry)?;
                }
            }
        }

        if dirty {
            state.catalog.persist()?;
        }

        Ok(())
    }

    fn remove_entry_files(&self, entry: &crate::cache::types::CacheEntry) -> CacheResult<()> {
        self.files.remove(&entry.relative_path)?;
        if entry.song_id.is_some() {
            self.files.remove_song_meta(&entry.relative_path)?;
        }
        Ok(())
    }

    fn lock_state(&self) -> CacheResult<std::sync::MutexGuard<'_, CacheState>> {
        self.state.lock().map_err(|_| CacheError::Poisoned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestCacheRoot {
        path: PathBuf,
    }

    impl TestCacheRoot {
        fn new(label: &str) -> Self {
            let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "music_native_cache_service_{label}_{}_{}",
                std::process::id(),
                id
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp cache root");
            Self { path }
        }
    }

    impl Drop for TestCacheRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn complete_song_source(source: &CachedSongSource, song_id: i64, quality: &str, bytes: u64) {
        let cache_path = PathBuf::from(source.cache_path.as_ref().expect("cache path"));
        let metadata_path = PathBuf::from(source.metadata_path.as_ref().expect("metadata path"));
        fs::write(&cache_path, vec![0xA5; bytes as usize]).expect("write cached song bytes");

        let mut meta = SongStreamCacheMeta::new(song_id, quality, &source.value);
        meta.set_content_length(Some(bytes));
        meta.add_range(0..bytes);
        meta.mark_complete();
        fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&meta).expect("serialize song meta"),
        )
        .expect("write song meta");
    }

    fn read_meta(path: &str) -> SongStreamCacheMeta {
        serde_json::from_str(&fs::read_to_string(path).expect("read song meta"))
            .expect("parse song meta")
    }

    #[test]
    fn prepare_song_source_replaces_other_quality_for_same_song() {
        let root = TestCacheRoot::new("replace_quality");
        let service = NativeCacheService::new(&root.path, 1024 * 1024).expect("create service");

        let lossless = service
            .prepare_song_source(
                2628631968,
                "lossless",
                "https://example.com/song-lossless.flac",
                Some(100),
            )
            .expect("prepare lossless");
        let old_cache_path = lossless.cache_path.clone().expect("lossless cache path");
        let old_meta_path = lossless.metadata_path.clone().expect("lossless meta path");
        complete_song_source(&lossless, 2628631968, "lossless", 100);

        let jymaster = service
            .prepare_song_source(
                2628631968,
                "jymaster",
                "https://example.com/song-jymaster.flac",
                Some(1000),
            )
            .expect("prepare jymaster");

        assert_eq!(jymaster.r#type, "url");
        assert!(!Path::new(&old_cache_path).exists());
        assert!(!Path::new(&old_meta_path).exists());

        let new_meta = read_meta(jymaster.metadata_path.as_ref().expect("jymaster meta path"));
        assert_eq!(new_meta.song_id, 2628631968);
        assert_eq!(new_meta.quality, "jymaster");
        assert_eq!(new_meta.content_length, Some(1000));

        let stats = service.get_stats().expect("get stats");
        assert_eq!(stats.song_entries, 1);
    }

    #[test]
    fn prepare_song_source_replaces_same_quality_when_expected_size_changes() {
        let root = TestCacheRoot::new("replace_size");
        let service = NativeCacheService::new(&root.path, 1024 * 1024).expect("create service");

        let first = service
            .prepare_song_source(
                2628631968,
                "jymaster",
                "https://example.com/song-jymaster-v1.flac",
                Some(100),
            )
            .expect("prepare first jymaster");
        complete_song_source(&first, 2628631968, "jymaster", 100);

        let second = service
            .prepare_song_source(
                2628631968,
                "jymaster",
                "https://example.com/song-jymaster-v2.flac",
                Some(200),
            )
            .expect("prepare replacement jymaster");

        assert_eq!(second.r#type, "url");
        let cache_path = second.cache_path.as_ref().expect("replacement cache path");
        assert_eq!(fs::metadata(cache_path).expect("replacement file").len(), 0);

        let meta = read_meta(
            second
                .metadata_path
                .as_ref()
                .expect("replacement meta path"),
        );
        assert_eq!(meta.quality, "jymaster");
        assert_eq!(meta.source_url, "https://example.com/song-jymaster-v2.flac");
        assert_eq!(meta.content_length, Some(200));
        assert!(!meta.is_complete);

        let stats = service.get_stats().expect("get stats");
        assert_eq!(stats.song_entries, 1);
        assert_eq!(stats.song_bytes, 0);
    }
}

fn infer_extension(bucket: CacheBucket, url: &str, content_type: Option<&str>) -> String {
    if let Some(extension) = extension_from_url(url) {
        return extension;
    }

    if let Some(content_type) = content_type {
        let normalized = content_type.trim().to_ascii_lowercase();
        if normalized.contains("mpeg") || normalized.contains("mp3") {
            return "mp3".to_string();
        }
        if normalized.contains("flac") {
            return "flac".to_string();
        }
        if normalized.contains("wav") {
            return "wav".to_string();
        }
        if normalized.contains("ogg") {
            return "ogg".to_string();
        }
        if normalized.contains("aac") {
            return "aac".to_string();
        }
        if normalized.contains("png") {
            return "png".to_string();
        }
        if normalized.contains("jpeg") || normalized.contains("jpg") {
            return "jpg".to_string();
        }
        if normalized.contains("webp") {
            return "webp".to_string();
        }
    }

    match bucket {
        CacheBucket::Song => "mp3".to_string(),
        CacheBucket::Cover => "jpg".to_string(),
        CacheBucket::Entity | CacheBucket::Lyric => "json".to_string(),
    }
}

fn extension_from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let file_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    let extension = file_name
        .split('?')
        .next()
        .and_then(|segment| segment.rsplit('.').next())?
        .trim()
        .trim_start_matches('.');

    if extension.is_empty() || extension.len() > 5 {
        return None;
    }

    Some(extension.to_ascii_lowercase())
}

fn song_stream_key(song_id: i64, quality: &str) -> String {
    format!(
        "audio-stream:{song_id}:{}",
        quality.trim().to_ascii_lowercase()
    )
}

fn is_legacy_audio_key_for_song(key: &str, song_id: i64) -> bool {
    key.contains("\"scope\":\"audio\"") && key.contains(&format!("\"songId\":{song_id}"))
}

fn legacy_audio_key_matches_quality(key: &str, quality: &str) -> bool {
    key.contains(&format!(
        "\"quality\":\"{}\"",
        quality.trim().to_ascii_lowercase()
    ))
}
