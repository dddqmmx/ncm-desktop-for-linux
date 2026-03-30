use serde::{Deserialize, Serialize};
use std::ops::Range;

use crate::cache::types::now_unix_secs;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn new(start: u64, end: u64) -> Option<Self> {
        if end <= start {
            return None;
        }

        Some(Self { start, end })
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

impl From<Range<u64>> for ByteRange {
    fn from(value: Range<u64>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongStreamCacheMeta {
    pub song_id: i64,
    pub quality: String,
    pub source_url: String,
    pub content_length: Option<u64>,
    pub downloaded_ranges: Vec<ByteRange>,
    pub is_complete: bool,
    pub updated_at: u64,
}

impl SongStreamCacheMeta {
    pub fn new(song_id: i64, quality: &str, source_url: &str) -> Self {
        Self {
            song_id,
            quality: quality.trim().to_string(),
            source_url: source_url.trim().to_string(),
            content_length: None,
            downloaded_ranges: Vec::new(),
            is_complete: false,
            updated_at: now_unix_secs(),
        }
    }

    pub fn set_content_length(&mut self, content_length: Option<u64>) {
        self.content_length = content_length;
        self.updated_at = now_unix_secs();
    }

    pub fn add_range(&mut self, range: Range<u64>) {
        let Some(next) = ByteRange::new(range.start, range.end) else {
            return;
        };

        self.downloaded_ranges.push(next);
        self.downloaded_ranges.sort_by_key(|item| item.start);

        let mut merged: Vec<ByteRange> = Vec::with_capacity(self.downloaded_ranges.len());
        for range in self.downloaded_ranges.drain(..) {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
                continue;
            }

            merged.push(range);
        }

        self.downloaded_ranges = merged;
        self.updated_at = now_unix_secs();
    }

    pub fn downloaded_bytes(&self) -> u64 {
        self.downloaded_ranges.iter().map(ByteRange::len).sum()
    }

    pub fn mark_complete(&mut self) {
        if let Some(content_length) = self.content_length {
            if self.downloaded_bytes() < content_length {
                println!("[cache] 警告：尝试标记完成，但下载字节数（{}）小于内容长度（{}）", self.downloaded_bytes(), content_length);
                return;
            }
        }
        self.is_complete = true;
        self.updated_at = now_unix_secs();
    }

    pub fn is_fully_downloaded(&self) -> bool {
        if let Some(content_length) = self.content_length {
            return self.downloaded_ranges.len() == 1
                && self.downloaded_ranges[0].start == 0
                && self.downloaded_ranges[0].end >= content_length;
        }

        self.is_complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fully_downloaded_logic() {
        let mut meta = SongStreamCacheMeta::new(1, "lossless", "http://example.com/1.flac");
        meta.set_content_length(Some(1000));
        
        // Simulate incomplete download
        meta.add_range(0..500);
        meta.mark_complete(); 
        
        assert!(!meta.is_complete, "Should not mark complete if incomplete");
        assert!(!meta.is_fully_downloaded(), "Should not be fully downloaded if ranges are incomplete");
        
        // Simulate complete download but mark_complete NOT called yet
        meta.add_range(500..1000);
        assert!(meta.is_fully_downloaded(), "Should be fully downloaded now even if is_complete is false");
        
        meta.mark_complete();
        assert!(meta.is_complete);
    }

    #[test]
    fn test_persistent_cache_resumption_and_no_truncation() {
        use std::fs;
        use std::io::{Read, Write, Seek};
        let temp_dir = std::env::temp_dir().join("music_cache_resumption_test");
        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        fs::create_dir_all(&temp_dir).unwrap();
        let data_path = temp_dir.join("song.mp3");
        let meta_path = temp_dir.join("song.json");

        // 1. Initial download session (incomplete)
        let mut meta = SongStreamCacheMeta::new(1, "lossless", "url");
        meta.set_content_length(Some(100));
        meta.add_range(0..50);
        fs::write(&data_path, vec![0xA5u8; 50]).unwrap(); // Original data
        fs::write(&meta_path, serde_json::to_string(&meta).unwrap()).unwrap();

        // 2. Interruption: New session starts (simulating switch_output_device)
        // Ensure NO truncation!
        fs::OpenOptions::new().write(true).truncate(false).create(true).open(&data_path).unwrap();
        
        // New tracker loads existing meta
        let mut new_meta: SongStreamCacheMeta = serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert_eq!(new_meta.downloaded_bytes(), 50);

        // New tracker downloads the rest (50..100)
        new_meta.add_range(50..100);
        
        // Simulating writing the rest at offset 50
        let mut f = fs::OpenOptions::new().write(true).read(true).open(&data_path).unwrap();
        f.seek(std::io::SeekFrom::Start(50)).unwrap();
        f.write_all(&[0x5Au8; 50]).unwrap();
        
        new_meta.mark_complete();
        assert!(new_meta.is_complete);

        let file_size = fs::metadata(&data_path).unwrap().len();
        assert_eq!(file_size, 100);
        
        // VERIFY: First 50 bytes are PRESERVED!
        let mut first_50 = vec![0u8; 50];
        f.seek(std::io::SeekFrom::Start(0)).unwrap();
        f.read_exact(&mut first_50).unwrap();
        assert_eq!(first_50, vec![0xA5u8; 50], "Data from first session must be preserved");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
