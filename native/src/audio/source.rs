use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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

#[derive(Clone, Debug)]
pub struct PersistentFileStorageProvider {
    pub(crate) path: PathBuf,
    pub(crate) max_write_ahead_bytes: Option<u64>,
}

impl PersistentFileStorageProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_write_ahead_bytes: None,
        }
    }

    pub fn max_write_ahead_bytes(mut self, max_write_ahead_bytes: Option<u64>) -> Self {
        self.max_write_ahead_bytes = max_write_ahead_bytes;
        self
    }
}

impl StorageProvider for PersistentFileStorageProvider {
    type Reader = ThrottledStorageReader;
    type Writer = ThrottledStorageWriter;

    fn into_reader_writer(
        self,
        _content_length: Option<u64>,
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
        let shared = Arc::new(Mutex::new(StorageReadState { position: 0 }));

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
                },
            )
        })
    }
}

#[derive(Debug)]
struct StorageReadState {
    position: u64,
}

pub struct ThrottledStorageReader {
    inner: BufReader<File>,
    shared: Arc<Mutex<StorageReadState>>,
}

impl ThrottledStorageReader {
    fn update_position(&self, position: u64) {
        if let Ok(mut state) = self.shared.lock() {
            state.position = position;
        }
    }
}

impl Read for ThrottledStorageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        let position = self.inner.stream_position()?;
        self.update_position(position);
        Ok(read)
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
    shared: Arc<Mutex<StorageReadState>>,
    max_write_ahead_bytes: Option<u64>,
}

impl ThrottledStorageWriter {
    fn read_position(&self) -> u64 {
        self.shared
            .lock()
            .map(|state| state.position)
            .unwrap_or_default()
    }
}

impl Write for ThrottledStorageWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(max_write_ahead_bytes) = self.max_write_ahead_bytes else {
            return self.inner.write(buf);
        };

        let writer_position = self.inner.stream_position()?;
        let read_position = self.read_position();
        let allowed_until = read_position.saturating_add(max_write_ahead_bytes);

        if writer_position >= allowed_until {
            return Ok(0);
        }

        let remaining = (allowed_until - writer_position) as usize;
        self.inner.write(&buf[..buf.len().min(remaining)])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for ThrottledStorageWriter {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

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
}
