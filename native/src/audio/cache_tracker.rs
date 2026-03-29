use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use stream_download::{StreamPhase, StreamState};
use crate::cache::song::SongStreamCacheMeta;

#[derive(Clone)]
pub struct SongCacheTracker {
    metadata_path: Arc<PathBuf>,
    inner: Arc<Mutex<SongCacheTrackerState>>,
}

struct SongCacheTrackerState {
    meta: SongStreamCacheMeta,
    last_persisted_bytes: u64,
}

impl SongCacheTracker {
    const PERSIST_STEP_BYTES: u64 = 128 * 1024;

    pub fn new(metadata_path: impl Into<PathBuf>) -> io::Result<Self> {
        let metadata_path = metadata_path.into();
        let raw = fs::read_to_string(&metadata_path)?;
        let meta = serde_json::from_str::<SongStreamCacheMeta>(&raw)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let last_persisted_bytes = meta.downloaded_bytes();

        Ok(Self {
            metadata_path: Arc::new(metadata_path),
            inner: Arc::new(Mutex::new(SongCacheTrackerState {
                meta,
                last_persisted_bytes,
            })),
        })
    }

    pub fn set_content_length(&self, content_length: Option<u64>) -> io::Result<()> {
        let mut state = self.inner.lock().unwrap();
        if state.meta.content_length == content_length {
            return Ok(());
        }

        state.meta.set_content_length(content_length);
        self.persist_locked(&mut state)
    }

    pub fn record_progress(&self, progress: StreamState, content_length: Option<u64>) {
        if let Err(err) = self.try_record_progress(progress, content_length) {
            eprintln!("[cache] failed to persist song stream metadata: {err}");
        }
    }

    fn try_record_progress(
        &self,
        progress: StreamState,
        content_length: Option<u64>,
    ) -> io::Result<()> {
        let mut state = self.inner.lock().unwrap();
        let previous_bytes = state.meta.downloaded_bytes();

        if state.meta.content_length != content_length {
            state.meta.set_content_length(content_length);
        }

        state.meta.add_range(progress.current_chunk.clone());
        if matches!(progress.phase, StreamPhase::Complete) {
            state.meta.mark_complete();
        }

        let downloaded_bytes = state.meta.downloaded_bytes();
        if matches!(progress.phase, StreamPhase::Complete)
            || downloaded_bytes.saturating_sub(state.last_persisted_bytes)
                >= Self::PERSIST_STEP_BYTES
            || downloaded_bytes < previous_bytes
        {
            self.persist_locked(&mut state)?;
        }

        Ok(())
    }

    fn persist_locked(&self, state: &mut SongCacheTrackerState) -> io::Result<()> {
        let path = self.metadata_path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_vec_pretty(&state.meta)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, serialized)?;
        fs::rename(&tmp_path, path)?;
        state.last_persisted_bytes = state.meta.downloaded_bytes();
        Ok(())
    }
}
