//! Playback diagnostics with bounded, asynchronous file writes.
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
static LOG: OnceLock<SyncSender<String>> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> u64 {
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

struct LogFile {
    path: PathBuf,
    file: File,
    bytes: u64,
}

impl LogFile {
    fn open(path: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path: path.into(),
            file,
            bytes,
        })
    }

    fn write(&mut self, line: &[u8]) -> io::Result<()> {
        if self.bytes + line.len() as u64 > MAX_LOG_BYTES {
            std::fs::rename(&self.path, self.path.with_extension("jsonl.1"))?;
            *self = Self::open(&self.path)?;
        }
        self.file.write_all(line)?;
        self.file.flush()?;
        self.bytes += line.len() as u64;
        Ok(())
    }
}

pub fn init(path: &Path) -> io::Result<()> {
    let mut log = LogFile::open(path)?;
    let (sender, receiver) = mpsc::sync_channel::<String>(1024);
    std::thread::Builder::new()
        .name("diagnostic-log".into())
        .spawn(move || {
            for line in receiver {
                let _ = io::stderr().write_all(line.as_bytes());
                if let Err(error) = log.write(line.as_bytes()) {
                    let _ = writeln!(io::stderr(), "[diagnostics] failed to write log: {error}");
                }
            }
        })?;
    let _ = LOG.set(sender);
    Ok(())
}

/// Fields must be explicitly selected; never pass cookies, credentials or API bodies.
pub fn record(level: &str, event: &str, fields: Value) {
    let time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    let mut line = json!({
        "time_ms": time_ms, "pid": std::process::id(),
        "level": level, "event": event, "fields": fields, "dropped_records": dropped,
    })
    .to_string();
    line.push('\n');
    if let Some(sender) = LOG.get() {
        // A full queue must not stall audio or playback control threads.
        if sender.try_send(line).is_err() {
            DROPPED.fetch_add(dropped + 1, Ordering::Relaxed);
        }
    } else {
        let _ = io::stderr().write_all(line.as_bytes());
    }
}

/// Strip complete URLs (including signed media paths) from dependency error strings.
pub fn redact_urls(message: &str) -> String {
    message
        .split_whitespace()
        .map(|word| {
            if word.contains("://") {
                "[url redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn error_chain(error: &dyn std::error::Error) -> Vec<String> {
    let mut chain = vec![redact_urls(&error.to_string())];
    let mut source = error.source();
    while let Some(error) = source {
        chain.push(redact_urls(&error.to_string()));
        source = error.source();
    }
    chain
}

pub(crate) fn stage_error(
    stage: &str,
    error: &dyn std::error::Error,
) -> Box<dyn std::error::Error> {
    let chain = error_chain(error);
    record(
        "error",
        "audio.stage_failed",
        json!({"stage": stage, "causes": chain}),
    );
    format!("{stage}失败：{}", chain.join("；原因：")).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_urls_are_removed_from_error_chains() {
        let message = redact_urls(
            "request failed (https://user:secret@music.test/private-token/song?key=secret): connection reset",
        );
        assert_eq!(message, "request failed [url redacted] connection reset");
    }

    #[test]
    fn stage_errors_preserve_the_os_cause() {
        let error = io::Error::from_raw_os_error(2);
        let message = stage_error("打开音频文件", &error).to_string();
        assert!(message.starts_with("打开音频文件失败："));
        assert!(message.contains("os error 2"));
    }

    #[test]
    fn log_rotation_retains_previous_records() {
        let path = std::env::temp_dir().join(format!("ncm-log-test-{}.jsonl", std::process::id()));
        let mut log = LogFile::open(&path).unwrap();
        log.write(b"previous\n").unwrap();
        log.bytes = MAX_LOG_BYTES;
        log.write(b"current\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"current\n");
        assert_eq!(
            std::fs::read(path.with_extension("jsonl.1")).unwrap(),
            b"previous\n"
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("jsonl.1")).unwrap();
    }
}
