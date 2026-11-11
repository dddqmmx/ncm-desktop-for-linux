use super::data::Song;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Store song identities and metadata, never transient UI handles or stream URLs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackState {
    pub queue: Vec<Song>,
    pub current_id: Option<String>,
    pub position_ms: u64,
}

impl PlaybackState {
    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        self.queue
            .retain(|song| !song.id.is_empty() && seen.insert(song.id.clone()));
        if !self.current_id.as_ref().is_some_and(|id| seen.contains(id)) {
            self.current_id = self.queue.first().map(|song| song.id.clone());
            self.position_ms = 0;
        }
        if let Some(song) = self
            .queue
            .iter()
            .find(|song| Some(&song.id) == self.current_id.as_ref())
        {
            self.position_ms = clamp_position(song, self.position_ms);
        } else {
            self.position_ms = 0;
        }
    }
}

pub fn clamp_position(song: &Song, position_ms: u64) -> u64 {
    position_ms.min(if song.duration > 0 {
        song.duration
    } else {
        u32::MAX as u64
    })
}
