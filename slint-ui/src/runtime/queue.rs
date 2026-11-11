use std::collections::HashSet;

/// The playback order has a single owner. UI handles and audio state live elsewhere.
#[derive(Default)]
pub(crate) struct PlaybackQueue {
    ids: Vec<String>,
    current: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Removal {
    Missing,
    Removed,
    Current(Option<String>),
}

impl PlaybackQueue {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn current(&self) -> Option<&String> {
        self.current.as_ref()
    }

    pub fn replace(&mut self, ids: impl IntoIterator<Item = String>) {
        let mut seen = HashSet::new();
        self.ids = ids
            .into_iter()
            .filter(|id| !id.is_empty() && seen.insert(id.clone()))
            .collect();
        if !self
            .current
            .as_ref()
            .is_some_and(|id| self.ids.contains(id))
        {
            self.current = None;
        }
    }

    pub fn add(&mut self, id: String) {
        if !id.is_empty() && !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }

    pub fn select(&mut self, id: String) {
        if !id.is_empty() {
            self.add(id.clone());
            self.current = Some(id);
        }
    }

    /// Move an existing song, or insert a new one, directly after the current song.
    /// With no current song it goes at the front; selecting it never starts audio.
    pub fn add_next(&mut self, id: String) -> bool {
        if id.is_empty() || self.current.as_ref() == Some(&id) {
            return false;
        }
        self.ids.retain(|queued| queued != &id);
        let index = self.current_index().map_or(0, |index| index + 1);
        self.ids.insert(index, id);
        true
    }

    pub fn remove(&mut self, id: &str) -> Removal {
        let Some(index) = self.ids.iter().position(|queued| queued == id) else {
            return Removal::Missing;
        };
        self.ids.remove(index);
        if self.current.as_deref() == Some(id) {
            self.current = None;
            Removal::Current(
                self.ids
                    .get(index.min(self.ids.len().saturating_sub(1)))
                    .cloned(),
            )
        } else {
            Removal::Removed
        }
    }

    pub fn clear(&mut self) {
        self.ids.clear();
        self.current = None;
    }

    pub fn step(&self, delta: i32, natural: bool, mode: &str, random: u64) -> Option<&String> {
        if self.ids.is_empty() {
            return None;
        }
        let Some(position) = self.current_index() else {
            return if delta < 0 {
                self.ids.last()
            } else {
                self.ids.first()
            };
        };
        let next = if natural && mode == "single" {
            position
        } else if mode == "random" && self.ids.len() > 1 {
            (position + 1 + (random as usize % (self.ids.len() - 1))) % self.ids.len()
        } else {
            (position as i64 + delta as i64).rem_euclid(self.ids.len() as i64) as usize
        };
        self.ids.get(next)
    }

    fn current_index(&self) -> Option<usize> {
        self.current
            .as_ref()
            .and_then(|id| self.ids.iter().position(|queued| queued == id))
    }
}
