use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reqwest::header::CONTENT_TYPE;

use crate::cache::catalog::{CacheCatalog, CacheEntryUpsert};
use crate::cache::error::{CacheError, CacheResult};
use crate::cache::eviction::EvictionPlanner;
use crate::cache::settings::CacheSettingsStore;
use crate::cache::song::SongStreamCacheMeta;
use crate::cache::storage::CacheFileStore;
use crate::cache::types::{CacheBucket, CacheStats, CachedSongSource};

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
}

pub struct NativeCacheService {
    files: CacheFileStore,
    state: Mutex<CacheState>,
}

impl NativeCacheService {
    pub fn new(root_dir: impl AsRef<Path>, fallback_max_size_bytes: u64) -> CacheResult<Self> {
        let files = CacheFileStore::new(root_dir)?;
        let settings = CacheSettingsStore::load(files.settings_path(), fallback_max_size_bytes)?;
        settings.persist()?;

        let catalog = CacheCatalog::load(files.index_path())?;

        let service = Self {
            files,
            state: Mutex::new(CacheState { settings, catalog }),
        };

        service.reconcile_missing_files()?;
        service.enforce_limit()?;

        Ok(service)
    }

    pub fn get_stats(&self) -> CacheResult<CacheStats> {
        self.refresh_catalog_from_disk()?;
        let state = self.lock_state()?;
        Ok(state.stats())
    }

    pub fn get_json(&self, bucket: CacheBucket, key: &str) -> CacheResult<Option<String>> {
        let relative_path = {
            let mut state = self.lock_state()?;
            let entry = match state.catalog.touch(bucket, key)? {
                Some(entry) => entry,
                None => return Ok(None),
            };

            if !self.files.exists(&entry.relative_path) {
                state.catalog.remove(bucket, key)?;
                state.catalog.persist()?;
                return Ok(None);
            }

            state.catalog.persist()?;
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

    pub fn clear(&self) -> CacheResult<CacheStats> {
        {
            let mut state = self.lock_state()?;
            let existing_entries = state
                .catalog
                .entries()
                .map(|(_, entry)| entry.clone())
                .collect::<Vec<_>>();

            for entry in &existing_entries {
                self.remove_entry_files(entry)?;
            }

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

        let response = reqwest::get(url).await?;
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
        )
    }

    pub fn prepare_song_source(
        &self,
        song_id: i64,
        quality: &str,
        url: &str,
    ) -> CacheResult<CachedSongSource> {
        let normalized_url = url.trim();
        if normalized_url.is_empty() {
            return Ok(CachedSongSource {
                r#type: "url".to_string(),
                value: String::new(),
                cache_path: None,
                metadata_path: None,
                cache_ahead_secs: None,
            });
        }

        self.refresh_catalog_from_disk()?;

        let normalized_quality = quality.trim().to_ascii_lowercase();
        let cache_key = song_stream_key(song_id, &normalized_quality);

        let (same_quality_entry, variant_victims, cache_ahead_secs) = {
            let state = self.lock_state()?;
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
            )
        };

        if !variant_victims.is_empty() {
            let mut state = self.lock_state()?;
            for victim in variant_victims {
                if let Some(entry) = state.catalog.remove_by_composite_key(&victim) {
                    self.remove_entry_files(&entry)?;
                }
            }
            state.catalog.persist()?;
        }

        if let Some(entry) = same_quality_entry {
            if entry.song_id.is_none() && self.files.exists(&entry.relative_path) {
                let file_size = self.files.file_size(&entry.relative_path)?.unwrap_or(0);
                let mut meta =
                    SongStreamCacheMeta::new(song_id, &normalized_quality, normalized_url);
                meta.set_content_length(Some(file_size));
                meta.add_range(0..file_size);
                meta.mark_complete();
                self.files.write_song_meta(&entry.relative_path, &meta)?;

                let mut state = self.lock_state()?;
                let _ = state.catalog.remove(CacheBucket::Song, &entry.key)?;
                state.catalog.upsert(CacheEntryUpsert {
                    bucket: CacheBucket::Song,
                    key: &cache_key,
                    relative_path: entry.relative_path.clone(),
                    size_bytes: file_size,
                    source_url: Some(normalized_url.to_string()),
                    mime_type: entry.mime_type.clone(),
                    song_id: Some(song_id),
                    quality: Some(normalized_quality.clone()),
                    content_length: Some(file_size),
                    is_complete: true,
                })?;
                state.catalog.persist()?;

                return Ok(CachedSongSource {
                    r#type: "file".to_string(),
                    value: self
                        .files
                        .absolute_path(&entry.relative_path)
                        .to_string_lossy()
                        .into_owned(),
                    cache_path: None,
                    metadata_path: None,
                    cache_ahead_secs: Some(cache_ahead_secs),
                });
            }

            if entry.is_complete && self.files.exists(&entry.relative_path) {
                let mut state = self.lock_state()?;
                let _ = state.catalog.touch(CacheBucket::Song, &cache_key)?;
                state.catalog.persist()?;
                return Ok(CachedSongSource {
                    r#type: "file".to_string(),
                    value: self
                        .files
                        .absolute_path(&entry.relative_path)
                        .to_string_lossy()
                        .into_owned(),
                    cache_path: None,
                    metadata_path: None,
                    cache_ahead_secs: Some(cache_ahead_secs),
                });
            }

            let mut state = self.lock_state()?;
            if let Some(entry) = state.catalog.remove(CacheBucket::Song, &cache_key)? {
                self.remove_entry_files(&entry)?;
            }
            state.catalog.persist()?;
        }

        let extension = infer_extension(CacheBucket::Song, normalized_url, None);
        let relative_path =
            self.files
                .build_relative_path(CacheBucket::Song, &cache_key, &extension);
        let absolute_path = self.files.ensure_song_file(&relative_path)?;
        let meta = SongStreamCacheMeta::new(song_id, &normalized_quality, normalized_url);
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
                content_length: None,
                is_complete: false,
            })?;
            state.catalog.persist()?;
        }

        Ok(CachedSongSource {
            r#type: "url".to_string(),
            value: normalized_url.to_string(),
            cache_path: Some(absolute_path.to_string_lossy().into_owned()),
            metadata_path: Some(metadata_path.to_string_lossy().into_owned()),
            cache_ahead_secs: Some(cache_ahead_secs),
        })
    }

    fn get_existing_path(&self, bucket: CacheBucket, key: &str) -> CacheResult<Option<PathBuf>> {
        let relative_path = {
            let mut state = self.lock_state()?;
            let entry = match state.catalog.touch(bucket, key)? {
                Some(entry) => entry,
                None => return Ok(None),
            };

            if !self.files.exists(&entry.relative_path) {
                state.catalog.remove(bucket, key)?;
                state.catalog.persist()?;
                return Ok(None);
            }

            state.catalog.persist()?;
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
        self.refresh_catalog_from_disk()?;

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
        let snapshots = {
            let state = self.lock_state()?;
            state
                .catalog
                .entries()
                .map(|(composite_key, entry)| (composite_key.clone(), entry.clone()))
                .collect::<Vec<_>>()
        };

        if snapshots.is_empty() {
            return Ok(());
        }

        let mut dirty = false;
        let mut victims = Vec::new();
        let mut state = self.lock_state()?;

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
