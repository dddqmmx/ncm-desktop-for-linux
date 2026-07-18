use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use stream_download::storage::StorageProvider;
use symphonia::core::io::MediaSource;

/// 逃逸预算下限：保证足够写完 in-flight chunk（hyper 默认读缓冲上限约 512KB），
/// 让下载任务能回到事件循环处理 seek 消息。
const MIN_ESCAPE_BUDGET_BYTES: u64 = 1024 * 1024;

pub struct SeekableSource<R> {
    inner: R,
    len: Option<u64>,
    storage_state: Option<SharedStorageState>,
}

impl<R: Read + Seek + Send + Sync> SeekableSource<R> {
    pub fn new(inner: R, len: Option<u64>) -> Self {
        Self {
            inner,
            len,
            storage_state: None,
        }
    }

    pub fn with_storage_state(mut self, storage_state: SharedStorageState) -> Self {
        self.storage_state = Some(storage_state);
        self
    }
}

impl<R: Read + Seek + Send + Sync> Read for SeekableSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return self.inner.read(buf);
        }

        // 无缓存状态（普通 URL/文件）：保持原有临时 EOF 重试语义。
        let Some(storage_state) = self.storage_state.clone() else {
            return read_with_temporary_eof_retry(&mut self.inner, buf, self.len);
        };

        // 流式缓存：仅对“写端身后的空洞”做物理覆盖门控。
        // tip 等待交给 StreamDownload::read（wait_for_position）——它在写端
        // 尚未越过读端时工作正确；只在写端已越过但物理区间未写（稀疏空洞）
        // 时需要我们介入，避免读到 0 字节垃圾。
        let (pos, writer_pos, covers) = {
            let state = storage_state
                .lock()
                .map_err(|_| io::Error::other("storage state poisoned"))?;
            let pos = state.reader_position;
            let Some(len) = self.len else {
                drop(state);
                return read_with_temporary_eof_retry(&mut self.inner, buf, None);
            };
            if pos >= len {
                return Ok(0);
            }
            let need_end = (pos + buf.len() as u64).min(len);
            let covers = ranges_cover(&state.written_ranges, pos, need_end);
            (pos, state.writer_position, covers)
        };

        if !covers && pos < writer_pos {
            // 身后空洞：先等已有 seek/下载回填（回退 seek 后任务通常已在回填）。
            // 绝不能立刻再发 StreamDownload::seek——seek 通道容量为 1，二次
            // try_send 会静默丢弃，并把任务状态弄乱，最终 "stream failed"。
            // 仅当一段时间仍无覆盖时，才补发一次 seek 触发 range 回填。
            let wait_start = std::time::Instant::now();
            let mut reseeked = false;
            loop {
                let covered = {
                    let state = storage_state
                        .lock()
                        .map_err(|_| io::Error::other("storage state poisoned"))?;
                    let pos = state.reader_position;
                    let need_end = self
                        .len
                        .map(|len| (pos + buf.len() as u64).min(len))
                        .unwrap_or(pos + buf.len() as u64);
                    ranges_cover(&state.written_ranges, pos, need_end)
                };
                if covered {
                    break;
                }
                if !reseeked && wait_start.elapsed() >= Duration::from_millis(50) {
                    prepare_blocking_seek(&mut self.inner, &storage_state, pos);
                    let _ = self.inner.seek(SeekFrom::Start(pos));
                    reseeked = true;
                } else {
                    // 唤醒可能因节流睡眠的下载任务（不发新 seek）。
                    let _ = self.inner.read(&mut [])?;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        read_with_temporary_eof_retry(&mut self.inner, buf, self.len)
    }
}

impl<R: Read + Seek + Send + Sync> Seek for SeekableSource<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        // SeekFrom::Current(0) 只是位置查询，不触发任何下载协调。
        if !matches!(pos, SeekFrom::Current(0)) {
            let target = match pos {
                SeekFrom::Start(position) => Some(position),
                SeekFrom::Current(offset) => self
                    .inner
                    .stream_position()
                    .ok()
                    .map(|current| current.saturating_add_signed(offset)),
                SeekFrom::End(offset) => self.len.map(|len| len.saturating_add_signed(offset)),
            };
            if let (Some(storage_state), Some(target)) = (self.storage_state.as_ref(), target) {
                prepare_blocking_seek(&mut self.inner, storage_state, target);
            }
        }
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

fn read_with_temporary_eof_retry<R: Read + Seek>(
    inner: &mut R,
    buf: &mut [u8],
    len: Option<u64>,
) -> io::Result<usize> {
    loop {
        let read = inner.read(buf)?;
        if read > 0 {
            return Ok(read);
        }

        let Some(len) = len else {
            return Ok(0);
        };
        if inner.stream_position()? >= len {
            return Ok(0);
        }

        // A streaming storage reader can temporarily reach the physical cache EOF.
        // Retry here so the inner StreamDownload can notify and resume its writer.
        std::thread::yield_now();
    }
}

type WriteProgressCallback = Arc<dyn Fn(Range<u64>) + Send + Sync>;

#[derive(Clone)]
pub struct PersistentFileStorageProvider {
    pub(crate) path: PathBuf,
    pub(crate) max_write_ahead_bytes: Option<u64>,
    on_write: Option<WriteProgressCallback>,
    shared: SharedStorageState,
}

impl PersistentFileStorageProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_write_ahead_bytes: None,
            on_write: None,
            shared: Arc::new(Mutex::new(StorageReadState {
                reader_position: 0,
                writer_position: 0,
                throttle_anchor_position: 0,
                written_ranges: Vec::new(),
                budget_bytes: MIN_ESCAPE_BUDGET_BYTES,
                seek_escape_budget_end: None,
            })),
        }
    }

    pub fn max_write_ahead_bytes(mut self, max_write_ahead_bytes: Option<u64>) -> Self {
        self.max_write_ahead_bytes = max_write_ahead_bytes;
        if let Ok(mut state) = self.shared.lock() {
            state.budget_bytes = max_write_ahead_bytes
                .unwrap_or(MIN_ESCAPE_BUDGET_BYTES)
                .max(MIN_ESCAPE_BUDGET_BYTES);
        }
        self
    }

    pub fn on_write<F>(mut self, on_write: F) -> Self
    where
        F: Fn(Range<u64>) + Send + Sync + 'static,
    {
        self.on_write = Some(Arc::new(on_write));
        self
    }

    pub fn shared_state(&self) -> SharedStorageState {
        Arc::clone(&self.shared)
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
        let shared = self.shared;

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

pub(crate) type SharedStorageState = Arc<Mutex<StorageReadState>>;

#[derive(Debug)]
pub(crate) struct StorageReadState {
    reader_position: u64,
    writer_position: u64,
    throttle_anchor_position: u64,
    /// 物理写入区间（半开 [start, end)），已合并排序。
    written_ranges: Vec<(u64, u64)>,
    /// 逃逸预算大小（字节）。
    budget_bytes: u64,
    /// 逃逸预算终点：武装后写端在到达此位置前可突破常规 write-ahead 节流。
    ///
    /// 背景：写端被节流时返回 0，stream-download 下载任务睡在 `wait_for_read`；
    /// 期间无法处理 seek 消息。预算让任务写完当前 in-flight 块、回到事件循环
    /// 处理 seek（向目标发起 range 请求）；写端 seek 时解除武装。
    seek_escape_budget_end: Option<u64>,
}

/// 武装写端逃逸预算并唤醒可能因节流而睡眠的下载任务。
///
/// 仅当 `target` 尚未物理写入时才武装预算，避免后台缓存 driver 每次
/// 跟随播放位置 seek 到已缓存区间时把节流窗口撑大。
/// 预算从当前写端位置起算，与目标方向无关，因此回退 seek 到未缓存空洞时同样有效。
pub(crate) fn prepare_blocking_seek<R: Read>(
    reader: &mut R,
    storage_state: &SharedStorageState,
    target: u64,
) {
    if let Ok(mut state) = storage_state.lock() {
        let needs_download = !ranges_cover(&state.written_ranges, target, target.saturating_add(1));
        if needs_download {
            let end = state.writer_position.saturating_add(state.budget_bytes);
            state.seek_escape_budget_end = Some(
                state
                    .seek_escape_budget_end
                    .map_or(end, |prev| prev.max(end)),
            );
        }
    }
    // 唤醒读失败被有意忽略——随后的真正 seek/读会正确上报错误。
    let _ = reader.read(&mut []);
}

/// 判断 `written_ranges`（已合并排序的半开区间）是否完整覆盖 `[start, end)`。
fn ranges_cover(ranges: &[(u64, u64)], start: u64, end: u64) -> bool {
    if end <= start {
        return true;
    }
    let mut cursor = start;
    for &(range_start, range_end) in ranges {
        if range_end <= cursor {
            continue;
        }
        if range_start > cursor {
            return false;
        }
        cursor = range_end;
        if cursor >= end {
            return true;
        }
    }
    false
}

/// 把 `[start, end)` 合并进已排序的半开区间列表。
fn insert_written_range(ranges: &mut Vec<(u64, u64)>, start: u64, end: u64) {
    if end <= start {
        return;
    }
    ranges.push((start, end));
    ranges.sort_by_key(|(s, _)| *s);
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges.drain(..) {
        if let Some((_, prev_end)) = merged.last_mut()
            && start <= *prev_end
        {
            *prev_end = (*prev_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    *ranges = merged;
}

pub struct ThrottledStorageReader {
    inner: BufReader<File>,
    shared: Arc<Mutex<StorageReadState>>,
}

impl ThrottledStorageReader {
    fn update_position(&self, position: u64) {
        if let Ok(mut state) = self.shared.lock() {
            state.reader_position = position;
            state.throttle_anchor_position = position;
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
    on_write: Option<WriteProgressCallback>,
}

impl ThrottledStorageWriter {
    /// 返回当前生效的 write-ahead 上限。
    /// 逃逸预算武装期间放宽到 `seek_escape_budget_end`；越过预算终点后自动解除。
    fn allowed_write_until(&self, writer_position: u64) -> u64 {
        let Some(max_write_ahead_bytes) = self.max_write_ahead_bytes else {
            return u64::MAX;
        };

        let mut state = match self.shared.lock() {
            Ok(state) => state,
            Err(_) => return max_write_ahead_bytes,
        };

        if let Some(budget_end) = state.seek_escape_budget_end {
            if writer_position >= budget_end {
                state.seek_escape_budget_end = None;
            } else {
                let normal = state
                    .throttle_anchor_position
                    .saturating_add(max_write_ahead_bytes);
                return normal.max(budget_end);
            }
        }

        state
            .throttle_anchor_position
            .saturating_add(max_write_ahead_bytes)
    }

    fn disarm_seek_escape(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.seek_escape_budget_end = None;
        }
    }

    fn update_seek_position(&self, position: u64) {
        if let Ok(mut state) = self.shared.lock() {
            state.writer_position = position;
            state.throttle_anchor_position = state.reader_position.max(position);
        }
    }

    fn record_write(&self, start: u64, end: u64) {
        if end <= start {
            return;
        }
        if let Ok(mut state) = self.shared.lock() {
            state.writer_position = end;
            insert_written_range(&mut state.written_ranges, start, end);
        }
        if let Some(on_write) = self.on_write.as_ref() {
            on_write(start..end);
        }
    }
}

impl Write for ThrottledStorageWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.max_write_ahead_bytes.is_none() {
            let start = self.inner.stream_position()?;
            let written = self.inner.write(buf)?;
            let position = self.inner.stream_position()?;
            self.record_write(start, position);
            return Ok(written);
        }

        let writer_position = self.inner.stream_position()?;
        let allowed_until = self.allowed_write_until(writer_position);
        if writer_position >= allowed_until {
            return Ok(0);
        }

        let remaining = allowed_until.saturating_sub(writer_position);
        let remaining = usize::try_from(remaining).unwrap_or(usize::MAX);
        let written = self.inner.write(&buf[..buf.len().min(remaining)])?;
        let position = self.inner.stream_position()?;
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
        if !matches!(pos, SeekFrom::Current(0)) {
            // 下载任务处理 seek 请求时会 seek 写端：逃逸目的达成，解除武装，
            // 并以新的写入位置为锚恢复常规节流。
            self.disarm_seek_escape();
            self.update_seek_position(position);
        }
        Ok(position)
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
    fn ranges_cover_and_insert_merge_adjacent_intervals() {
        let mut ranges = Vec::new();
        insert_written_range(&mut ranges, 0, 100);
        insert_written_range(&mut ranges, 100, 200);
        insert_written_range(&mut ranges, 300, 400);
        assert_eq!(ranges, vec![(0, 200), (300, 400)]);
        assert!(ranges_cover(&ranges, 0, 200));
        assert!(!ranges_cover(&ranges, 150, 250));
        assert!(ranges_cover(&ranges, 350, 400));
        assert!(!ranges_cover(&ranges, 200, 300));
    }

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
    fn persistent_file_reader_returns_at_cache_eof_and_can_read_later_bytes() {
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

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut at_eof = [0u8; 3];
            let result = reader.read(&mut at_eof);
            let _ = tx.send((result, reader));
        });
        let (at_eof, mut reader) = rx
            .recv_timeout(Duration::from_millis(300))
            .expect("the storage reader must not block at the physical cache EOF");
        assert_eq!(at_eof.unwrap(), 0);

        writer.write_all(b"def").unwrap();
        writer.flush().unwrap();

        let mut second = [0u8; 3];
        reader.read_exact(&mut second).unwrap();
        assert_eq!(&second, b"def");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn seekable_source_retries_temporary_eof_before_known_content_end() {
        struct TemporaryEofReader {
            cursor: io::Cursor<Vec<u8>>,
            returned_temporary_eof: bool,
        }

        impl Read for TemporaryEofReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if !self.returned_temporary_eof {
                    self.returned_temporary_eof = true;
                    return Ok(0);
                }
                self.cursor.read(buf)
            }
        }

        impl Seek for TemporaryEofReader {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                self.cursor.seek(pos)
            }
        }

        let reader = TemporaryEofReader {
            cursor: io::Cursor::new(b"abcdef".to_vec()),
            returned_temporary_eof: false,
        };
        let mut source = SeekableSource::new(reader, Some(6));
        let mut bytes = [0u8; 6];

        source.read_exact(&mut bytes).unwrap();

        assert_eq!(&bytes, b"abcdef");
        assert_eq!(source.read(&mut bytes).unwrap(), 0);
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
    fn armed_seek_escape_budget_bypasses_throttle_then_self_clears() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-escape-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let provider = PersistentFileStorageProvider::new(&path).max_write_ahead_bytes(Some(4));
        let shared = provider.shared_state();
        // 测试用小预算，覆盖 in-flight 块即可。
        shared.lock().unwrap().budget_bytes = 8;
        let (_reader, mut writer) = provider.into_reader_writer(Some(256)).unwrap();

        assert_eq!(writer.write(b"abcd").unwrap(), 4);
        assert_eq!(writer.write(b"efgh").unwrap(), 0, "writer is throttled");

        // 武装预算：写端可再写 8 字节。
        if let Ok(mut state) = shared.lock() {
            state.seek_escape_budget_end = Some(state.writer_position + state.budget_bytes);
        }
        assert_eq!(writer.write(b"efgh").unwrap(), 4);
        assert_eq!(writer.write(b"ijkl").unwrap(), 4);

        // 预算耗尽后自动解除，恢复常规节流。
        assert_eq!(
            writer.write(b"mnop").unwrap(),
            0,
            "throttle must resume once the escape budget is exhausted"
        );
        assert!(
            shared.lock().unwrap().seek_escape_budget_end.is_none(),
            "escape budget must self-clear after the budget end"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn writer_seek_disarms_escape_and_reanchors_throttle() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-escape-seek-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let provider = PersistentFileStorageProvider::new(&path).max_write_ahead_bytes(Some(4));
        let shared = provider.shared_state();
        let (_reader, mut writer) = provider.into_reader_writer(Some(256)).unwrap();

        if let Ok(mut state) = shared.lock() {
            state.seek_escape_budget_end = Some(200);
        }
        // 下载任务处理 seek 请求时 seek 写端：解除逃逸并以新位置为节流锚点。
        writer.seek(SeekFrom::Start(64)).unwrap();
        assert!(shared.lock().unwrap().seek_escape_budget_end.is_none());

        assert_eq!(writer.write(b"abcd").unwrap(), 4);
        assert_eq!(
            writer.write(b"efgh").unwrap(),
            0,
            "throttle must re-engage relative to the new writer position"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn seekable_source_seek_arms_escape_and_wakes_with_zero_read() {
        struct ProbeReader {
            position: u64,
            zero_length_reads: usize,
        }

        impl Read for ProbeReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if buf.is_empty() {
                    self.zero_length_reads += 1;
                    return Ok(0);
                }
                Ok(0)
            }
        }

        impl Seek for ProbeReader {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                if let SeekFrom::Start(position) = pos {
                    self.position = position;
                }
                Ok(self.position)
            }
        }

        let shared: SharedStorageState = Arc::new(Mutex::new(StorageReadState {
            reader_position: 0,
            writer_position: 10,
            throttle_anchor_position: 0,
            written_ranges: Vec::new(),
            budget_bytes: 8,
            seek_escape_budget_end: None,
        }));
        let mut source =
            SeekableSource::new(ProbeReader { position: 0, zero_length_reads: 0 }, Some(256))
                .with_storage_state(Arc::clone(&shared));

        source.seek(SeekFrom::Start(128)).unwrap();

        assert_eq!(
            shared.lock().unwrap().seek_escape_budget_end,
            Some(18),
            "seek must arm the escape budget from the current writer position"
        );
        assert_eq!(
            source.inner.zero_length_reads, 1,
            "seek must poke a zero-length read to wake the download task"
        );

        // 位置查询（Current(0)）不得触发逃逸/唤醒。
        #[allow(clippy::seek_from_current)]
        source.seek(SeekFrom::Current(0)).unwrap();
        assert_eq!(source.inner.zero_length_reads, 1);
    }

    #[test]
    fn writer_records_physical_written_ranges() {
        let path = std::env::temp_dir().join(format!(
            "stream-cache-written-ranges-{}-{}.bin",
            std::process::id(),
            crate::cache::types::now_unix_secs()
        ));
        let provider = PersistentFileStorageProvider::new(&path).max_write_ahead_bytes(Some(16));
        let shared = provider.shared_state();
        let (_reader, mut writer) = provider.into_reader_writer(Some(256)).unwrap();

        assert_eq!(writer.write(b"abcdefgh").unwrap(), 8);
        writer.seek(SeekFrom::Start(32)).unwrap();
        assert_eq!(writer.write(b"ijkl").unwrap(), 4);

        let state = shared.lock().unwrap();
        assert!(ranges_cover(&state.written_ranges, 0, 8));
        assert!(ranges_cover(&state.written_ranges, 32, 36));
        assert!(!ranges_cover(&state.written_ranges, 8, 32));
        assert_eq!(state.writer_position, 36);

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
