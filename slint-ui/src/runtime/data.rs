use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub fn text(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_number() {
            v.to_string()
        } else {
            String::new()
        }
    })
}
pub fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}
pub fn time(ms: u64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub artist_id: String,
    pub album: String,
    pub album_id: String,
    pub cover: String,
    pub duration: u64,
    pub path: Option<PathBuf>,
    pub root: Option<PathBuf>,
}
impl Song {
    pub fn from_api(v: &Value) -> Self {
        let v = v.get("song").unwrap_or(v);
        let artists = v.get("ar").or(v.get("artists")).unwrap_or(&Value::Null);
        let album = v.get("al").or(v.get("album")).unwrap_or(&Value::Null);
        Self {
            id: text(&v["id"]),
            name: text(&v["name"]),
            artist: items(artists)
                .iter()
                .map(|a| text(&a["name"]))
                .collect::<Vec<_>>()
                .join(" / "),
            artist_id: text(&artists[0]["id"]),
            album: text(&album["name"]),
            album_id: text(&album["id"]),
            cover: text(&album["picUrl"]),
            duration: v["dt"].as_u64().or(v["duration"].as_u64()).unwrap_or(0),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub renderer: crate::rendering::Renderer,
    pub quality: String,
    pub device: Option<String>,
    pub strict: bool,
    pub exclusive: bool,
    pub software_volume: bool,
    pub volume: f64,
    pub cache_limit_mb: u64,
    pub cache_ahead_mb: u64,
    pub folders: Vec<PathBuf>,
    pub visual: Value,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            renderer: crate::rendering::Renderer::default(),
            quality: "lossless".into(),
            device: None,
            strict: false,
            exclusive: false,
            software_volume: true,
            volume: 0.76,
            cache_limit_mb: 2048,
            cache_ahead_mb: 32,
            folders: vec![],
            visual: Value::Null,
        }
    }
}
impl Preferences {
    pub fn effective_volume(&self) -> f64 {
        if self.software_volume && !self.strict {
            self.volume
        } else {
            1.0
        }
    }
    pub fn output_options(&self, device: Option<&str>) -> (bool, bool) {
        let hardware = device.is_some_and(|d| {
            d.starts_with("hw:") || d.starts_with("plughw:") || d.starts_with("alsa:hw:")
        });
        (
            hardware && self.strict,
            hardware && (self.exclusive || self.strict),
        )
    }
    pub fn normalize(&mut self) {
        self.volume = self.volume.clamp(0.0, 1.0);
        self.cache_limit_mb = self.cache_limit_mb.clamp(64, 1048576);
        self.cache_ahead_mb = self.cache_ahead_mb.clamp(1, 1024);
        if ![
            "standard", "exhigh", "lossless", "hires", "jyeffect", "sky", "jymaster",
        ]
        .contains(&self.quality.as_str())
        {
            self.quality = "lossless".into();
        }
    }
}
#[derive(Clone)]
pub struct Storage {
    pub config: PathBuf,
    pub cache: PathBuf,
}
impl Storage {
    pub fn preferences(&self) -> Result<Preferences, String> {
        let mut preferences: Preferences = self.read("preferences.json")?;
        preferences.normalize();
        Ok(preferences)
    }

    pub fn discover() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("无法确定用户主目录")?;
        let config = std::env::var_os("NCM_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".config"))
                    .join("ncm-slint")
            });
        let cache = std::env::var_os("NCM_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("XDG_CACHE_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".cache"))
                    .join("ncm-slint")
            });
        for p in [&config, &cache] {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self { config, cache })
    }
    pub fn read<T: serde::de::DeserializeOwned + Default>(&self, name: &str) -> Result<T, String> {
        match std::fs::read(self.config.join(name)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| format!("配置文件 {name} 格式错误；请先修复或移走该文件")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(format!("无法读取 {name}: {e}")),
        }
    }
    pub fn save(&self, name: &str, data: &impl Serialize) -> Result<(), String> {
        atomic_write(
            &self.config.join(name),
            &serde_json::to_vec(data).map_err(|e| e.to_string())?,
        )
    }
    pub fn session(&self, cookie: &str, remember: bool) -> Result<(), String> {
        if remember && !cookie.is_empty() {
            self.save("session.json", &cookie)
        } else {
            match std::fs::remove_file(self.config.join("session.json")) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}
// Slint's int is i32. All service identities stay opaque strings; handles only index the view.
#[derive(Default)]
pub struct Ids {
    forward: HashMap<(String, String), i32>,
    reverse: Vec<(String, String)>,
}
impl Ids {
    pub fn intern(&mut self, kind: &str, id: &str) -> i32 {
        if id.is_empty() {
            return -1;
        }
        let key = (kind.to_owned(), id.to_owned());
        if let Some(handle) = self.forward.get(&key) {
            return *handle;
        }
        let handle = i32::try_from(self.reverse.len() + 1).expect("UI identity capacity exceeded");
        self.reverse.push(key.clone());
        self.forward.insert(key, handle);
        handle
    }
    pub fn get(&self, handle: i32, kind: &str) -> Option<&str> {
        handle
            .checked_sub(1)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.reverse.get(i))
            .filter(|(k, _)| k == kind)
            .map(|(_, id)| id.as_str())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lyric {
    pub ms: u64,
    pub text: String,
    pub translation: String,
    pub pronunciation: String,
}
pub fn parse_lyrics(body: &Value) -> Vec<Lyric> {
    fn parse(s: &str) -> Vec<(u64, String)> {
        let mut out = vec![];
        for line in s.lines() {
            let mut rest = line;
            let mut times = vec![];
            while let Some(after) = rest.strip_prefix('[') {
                let Some((tag, tail)) = after.split_once(']') else {
                    break;
                };
                if let Some((m, s)) = tag.split_once(':') {
                    if let (Ok(m), Ok(s)) = (m.parse::<u64>(), s.parse::<f64>()) {
                        if s.is_finite() && (0.0..60.0).contains(&s) {
                            times.push(m.saturating_mul(60000).saturating_add((s * 1000.0) as u64));
                        }
                    }
                }
                rest = tail;
            }
            let text = rest.trim();
            // LRC files often use empty timed lines as phrase/end markers. They
            // must not become blank, full-height rows in the lyric viewport.
            if text.is_empty() {
                continue;
            }
            for t in times {
                out.push((t, text.to_owned()));
            }
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }
    fn matched_extra(extras: &[(u64, String)], ms: u64) -> String {
        // Match Electron's first nonempty extra within 350 ms. Translation
        // files frequently round timestamps differently from the main LRC.
        let index = extras.partition_point(|(time, _)| ms.saturating_sub(*time) >= 350);
        extras
            .get(index)
            .filter(|(time, _)| time.abs_diff(ms) < 350)
            .map(|(_, text)| text.clone())
            .unwrap_or_default()
    }
    let translations = parse(&text(&body["tlyric"]["lyric"]));
    let pronunciations = parse(&text(&body["romalrc"]["lyric"]));
    parse(&text(&body["lrc"]["lyric"]))
        .into_iter()
        .map(|(ms, text)| Lyric {
            ms,
            text,
            translation: matched_extra(&translations, ms),
            pronunciation: matched_extra(&pronunciations, ms),
        })
        .collect()
}
