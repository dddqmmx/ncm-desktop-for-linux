use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use stream_download::storage::StorageProvider;
use symphonia::core::io::MediaSource;

pub struct SeekableSource<R> {
    inner: R,
    len: Option<u64>,
}

impl<R: Read + Seek + Send + Sync> SeekableSource<R> {
    pub fn new(inner: R, len: Option<u64>) -> Self {
        Self { inner, len }
    }
}

impl<R: Read + Seek + Send + Sync> Read for SeekableSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read + Seek + Send + Sync> Seek for SeekableSource<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<R: Read + Seek + Send + Sync> MediaSource for SeekableSource<R> {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

type WriteProgressCallback = Arc<dyn Fn(Range<u64>) + Send + Sync>;

#[derive(Clone)]
pub struct PersistentFileStorageProvider {
    pub(crate) path: PathBuf,
    pub(crate) max_write_ahead_bytes: Option<u64>,
    on_write: Option<WriteProgressCallback>,
}

impl PersistentFileStorageProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_write_ahead_bytes: None,
            on_write: None,
        }
    }

    pub fn max_write_ahead_bytes(mut self, max_write_ahead_bytes: Option<u64>) -> Self {
        self.max_write_ahead_bytes = max_write_ahead_bytes;
        self
    }

    pub fn on_write<F>(mut self, on_write: F) -> Self
    where
        F: Fn(Range<u64>) + Send + Sync + 'static,
    {
        self.on_write = Some(Arc::new(on_write));
        self
    }
}

impl StorageProvider for PersistentFileStorageProvider {
    type Reader = ThrottledStorageReader;
    type Writer = ThrottledStorageWriter;

    fn into_reader_writer(
        self,
        content_length: Option<u64>,
    ) -> io::Result<(Self::Reader, Self::Writer)> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let writer = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.path)?;
        let reader = BufReader::new(OpenOptions::new().read(true).open(&self.path)?);
        let writer_position = writer
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let shared = Arc::new((
            Mutex::new(StorageReadState {
                reader_position: 0,
                writer_position,
                throttle_anchor_position: 0,
                content_length,
                writer_closed: false,
            }),
            Condvar::new(),
        ));

        Ok((reader, writer)).map(|(reader, writer)| {
            (
                ThrottledStorageReader {
                    inner: reader,
                    shared: Arc::clone(&shared),
                },
                ThrottledStorageWriter {
                    inner: writer,
                    shared,
                    max_write_ahead_bytes: self.max_write_ahead_bytes,
                    on_write: self.on_write,
                },
            )
        })
    }
}

#[derive(Debug)]
struct StorageReadState {
    reader_position: u64,
    writer_position: u64,
    throttle_anchor_position: u64,
    content_length: Option<u64>,
    writer_closed: bool,
}

pub struct ThrottledStorageReader {
    inner: BufReader<File>,
    shared: Arc<(Mutex<StorageReadState>, Condvar)>,
}

impl ThrottledStorageReader {
    fn update_position(&self, position: u64) {
        let (lock, condvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.reader_position = position;
            state.throttle_anchor_position = position;
            condvar.notify_all();
        }
    }
}

impl Read for ThrottledStorageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            let read = self.inner.read(buf)?;
            let position = self.inner.stream_position()?;
            self.update_position(position);
            return Ok(read);
        }

        loop {
            let read = self.inner.read(buf)?;
            let position = self.inner.stream_position()?;
            self.update_position(position);
            if read > 0 {
                return Ok(read);
            }

            let (lock, condvar) = &*self.shared;
            let state = lock.lock().unwrap();
            if state
                .content_length
                .is_some_and(|length| position >= length)
            {
                return Ok(0);
            }
            if state.writer_closed {
                return Ok(0);
            }
            if state.writer_position > position {
                drop(state);
                continue;
            }

            let _ = condvar
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap();
        }
    }
}

impl Seek for ThrottledStorageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let position = self.inner.seek(pos)?;
        self.update_position(position);
        Ok(position)
    }
}

pub struct ThrottledStorageWriter {
    inner: File,
    shared: Arc<(Mutex<StorageReadState>, Condvar)>,
    max_write_ahead_bytes: Option<u64>,
    on_write: Option<WriteProgressCallback>,
}

impl ThrottledStorageWriter {
    fn read_position(&self) -> u64 {
        self.shared
            .0
            .lock()
            .map(|state| state.throttle_anchor_position)
            .unwrap_or_default()
    }

    fn update_position(&self, position: u64) {
        let (lock, condvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.writer_position = position;
            condvar.notify_all();
        }
    }

    fn update_seek_position(&self, position: u64) {
        let (lock, condvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.writer_position = position;
            state.throttle_anchor_position = state.reader_position.max(position);
            condvar.notify_all();
        }
    }

    fn record_write(&self, start: u64, end: u64) {
        if end > start
            && let Some(on_write) = self.on_write.as_ref()
        {
            on_write(start..end);
        }
    }
}

impl Write for ThrottledStorageWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(max_write_ahead_bytes) = self.max_write_ahead_bytes else {
            let start = self.inner.stream_position()?;
            let written = self.inner.write(buf)?;
            let position = self.inner.stream_position()?;
            self.update_position(position);
            self.record_write(start, position);
            return Ok(written);
        };

        let writer_position = self.inner.stream_position()?;
        let read_position = self.read_position();
        let allowed_until = read_position.saturating_add(max_write_ahead_bytes);
        if writer_position >= allowed_until {
            return Ok(0);
        }

        let remaining = (allowed_until - writer_position) as usize;
        let written = self.inner.write(&buf[..buf.len().min(remaining)])?;
        let position = self.inner.stream_position()?;
        self.update_position(position);
        self.record_write(writer_position, position);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for ThrottledStorageWriter {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let position = self.inner.seek(pos)?;
        if matches!(pos, SeekFrom::Current(0)) {
            self.update_position(position);
        } else {
            self.update_seek_position(position);
        }
        Ok(position)
    }
}

impl Drop for ThrottledStorageWriter {
    fn drop(&mut self) {
        let (lock, condvar) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.writer_closed = true;
            condvar.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn persistent_file_storage_provider_keeps_reader_and_writer_offsets_independent() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (mut reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .into_reader_writer(None)
            .unwrap();

        writer.write_all(b"ID3abcdef").unwrap();
        writer.flush().unwrap();
        writer.seek(SeekFrom::End(0)).unwrap();

        let mut header = [0u8; 3];
        reader.read_exact(&mut header).unwrap();

        assert_eq!(&header, b"ID3");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persistent_file_reader_waits_for_more_cached_bytes_before_eof() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-wait-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (mut reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .into_reader_writer(Some(6))
            .unwrap();

        writer.write_all(b"abc").unwrap();
        writer.flush().unwrap();

        let mut first = [0u8; 3];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"abc");

        let reader_thread = thread::spawn(move || {
            let mut second = [0u8; 3];
            reader.read_exact(&mut second).unwrap();
            second
        });

        thread::sleep(Duration::from_millis(100));
        writer.write_all(b"def").unwrap();
        writer.flush().unwrap();

        assert_eq!(&reader_thread.join().unwrap(), b"def");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persistent_file_reader_returns_eof_when_download_ends_early() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-early-eof-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (mut reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .into_reader_writer(Some(6))
            .unwrap();

        writer.write_all(b"abc").unwrap();
        writer.flush().unwrap();

        let reader_thread = thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            bytes
        });

        thread::sleep(Duration::from_millis(100));
        drop(writer);

        assert_eq!(reader_thread.join().unwrap(), b"abc");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persistent_file_reader_returns_eof_for_unknown_length_after_writer_closes() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-unknown-length-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (mut reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .into_reader_writer(None)
            .unwrap();

        writer.write_all(b"abc").unwrap();
        writer.flush().unwrap();
        drop(writer);

        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, b"abc");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn writer_seek_moves_throttle_anchor_to_avoid_forward_seek_deadlock() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-seek-throttle-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (_reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .max_write_ahead_bytes(Some(4))
            .into_reader_writer(Some(256))
            .unwrap();

        let (tx, rx) = mpsc::channel();
        let writer_thread = thread::spawn(move || {
            writer.seek(SeekFrom::Start(128)).unwrap();
            let result = writer.write(b"abcd");
            let _ = tx.send(result);
        });

        let result = rx.recv_timeout(Duration::from_millis(300)).expect(
            "a far forward source seek must write the requested range without waiting for the stale reader position",
        );
        assert_eq!(result.unwrap(), 4);
        writer_thread.join().unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn writer_returns_zero_at_write_ahead_limit_instead_of_blocking_prefetch() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-prefetch-throttle-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let (_reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .max_write_ahead_bytes(Some(4))
            .into_reader_writer(Some(256))
            .unwrap();

        assert_eq!(writer.write(b"abcd").unwrap(), 4);

        let (tx, rx) = mpsc::channel();
        let writer_thread = thread::spawn(move || {
            let result = writer.write(b"efgh");
            let _ = tx.send(result);
        });

        let result = rx
            .recv_timeout(Duration::from_millis(300))
            .expect("prefetch writes must return instead of waiting for a reader");
        assert_eq!(result.unwrap(), 0);
        writer_thread.join().unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn querying_writer_position_does_not_advance_write_ahead_limit() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-position-throttle-{}-{}-{}.bin",
            std::process::id(),
            std::thread::current().name().unwrap_or("test"),
            crate::cache::types::now_unix_secs()
        ));
        let (_reader, mut writer) = PersistentFileStorageProvider::new(&path)
            .max_write_ahead_bytes(Some(4))
            .into_reader_writer(Some(256))
            .unwrap();

        assert_eq!(writer.write(b"abcd").unwrap(), 4);
        assert_eq!(writer.stream_position().unwrap(), 4);
        assert_eq!(writer.write(b"efgh").unwrap(), 0);

        let _ = std::fs::remove_file(path);
    }
}
