use super::data::{Song, Storage};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
const EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "ape", "flac", "m4a", "mp3", "ogg", "opus", "wav", "webm", "wma",
];
pub fn scan(roots: &[PathBuf], previous: Vec<Song>) -> Result<(Vec<Song>, bool), String> {
    fn visit(
        path: &Path,
        root: &Path,
        depth: usize,
        seen: &mut HashSet<PathBuf>,
        out: &mut Vec<Song>,
        truncated: &mut bool,
    ) -> Result<(), String> {
        if depth > 12 || out.len() >= 50000 {
            *truncated = true;
            return Ok(());
        }
        let entries =
            std::fs::read_dir(path).map_err(|e| format!("无法扫描 {}: {e}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_symlink() {
                continue;
            }
            let p = entry.path();
            if ft.is_dir() {
                visit(&p, root, depth + 1, seen, out, truncated)?;
            } else if ft.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                && seen.insert(p.clone())
            {
                if out.len() >= 50000 {
                    *truncated = true;
                    break;
                }
                out.push(Song {
                    id: format!("local:{}", p.display()),
                    name: p
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    artist: "本地音乐".into(),
                    path: Some(p),
                    root: Some(root.to_owned()),
                    ..Song::default()
                });
            }
        }
        Ok(())
    }
    let mut out = vec![];
    let mut seen = HashSet::new();
    let mut truncated = false;
    for root in roots {
        // Propagate permission/IO failures; never interpret them as deleted audio.
        visit(root, root, 0, &mut seen, &mut out, &mut truncated)?;
    }
    let previous: std::collections::HashMap<_, _> =
        previous.into_iter().map(|s| (s.id.clone(), s)).collect();
    for song in &mut out {
        if let Some(old) = previous.get(&song.id) {
            song.duration = old.duration;
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((out, truncated))
}
pub async fn rescan(storage: Storage, roots: Vec<PathBuf>) -> Result<(Vec<Song>, bool), String> {
    let previous = storage.read("library.json")?;
    let (mut songs, truncated) = tokio::task::spawn_blocking(move || scan(&roots, previous))
        .await
        .map_err(|e| e.to_string())??;
    // Bounded duration probing, through the same Rust decoder used by playback.
    use futures_util::{stream, StreamExt};
    let pending: Vec<_> = songs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.duration == 0)
        .map(|(i, s)| (i, s.path.clone().unwrap()))
        .collect();
    let durations = stream::iter(pending)
        .map(|(i, p)| async move {
            (
                i,
                native::audio::probe_file_duration_ms(p).await.unwrap_or(0),
            )
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
    for (i, d) in durations {
        songs[i].duration = d;
    }
    storage.save("library.json", &songs)?;
    Ok((songs, truncated))
}

/// Retain records on permission/IO errors; only confirmed missing paths are removed.
pub fn load_existing(storage: &Storage) -> Result<Vec<Song>, String> {
    let mut songs: Vec<Song> = storage.read("library.json")?;
    let original = songs.len();
    songs.retain(|song| {
        song.path
            .as_ref()
            .is_some_and(|path| match std::fs::metadata(path) {
                Ok(metadata) => metadata.is_file(),
                Err(error) => !matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ),
            })
    });
    if songs.len() != original {
        storage.save("library.json", &songs)?;
    }
    Ok(songs)
}
