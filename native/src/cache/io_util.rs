use std::fs;
use std::path::Path;

use crate::cache::error::CacheResult;

pub fn atomic_write(path: &Path, bytes: &[u8]) -> CacheResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> CacheResult<()> {
    let serialized = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &serialized)
}
