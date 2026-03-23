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
        self.is_complete = true;
        self.updated_at = now_unix_secs();
    }

    pub fn is_fully_downloaded(&self) -> bool {
        let Some(content_length) = self.content_length else {
            return false;
        };

        self.is_complete
            && self.downloaded_ranges.len() == 1
            && self.downloaded_ranges[0].start == 0
            && self.downloaded_ranges[0].end >= content_length
    }
}
