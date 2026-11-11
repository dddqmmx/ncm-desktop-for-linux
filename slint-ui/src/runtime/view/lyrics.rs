use super::{model, time, Song, View};
use crate::{AppWindow, LyricLineItem, PlayerViewState};
use serde_json::{json, Value};
use slint::ComponentHandle;

impl View {
    // Lyrics belong to the selected track, including a restored, paused track.
    // Audio startup must not be a prerequisite for loading or positioning them.
    pub(super) fn load_song_lyrics(&mut self, app: &AppWindow, song: &Song, position_ms: u64) {
        *self.tokens.entry("lyrics".into()).or_default() += 1;
        self.lyrics.clear();
        app.global::<PlayerViewState>().set_lyrics(model(vec![]));
        self.update_lyric_position(app, position_ms, true);
        if song.path.is_none() {
            self.send("lyrics", json!({"id": song.id}));
        }
    }

    pub(super) fn apply_lyrics(&mut self, app: &AppWindow, data: Value) {
        self.lyrics = serde_json::from_value(data).unwrap_or_default();
        app.global::<PlayerViewState>().set_lyrics(model(
            self.lyrics
                .iter()
                .map(|line| LyricLineItem {
                    time_str: time(line.ms).into(),
                    text: line.text.clone().into(),
                    translation: line.translation.clone().into(),
                    pronunciation: line.pronunciation.clone().into(),
                    ..Default::default()
                })
                .collect(),
        ));
        self.update_lyric_position(app, self.lyric_progress_ms, true);
    }

    pub(super) fn update_lyric_position(&mut self, app: &AppWindow, position_ms: u64, snap: bool) {
        let jumped = self.lyric_progress_ms.abs_diff(position_ms) > 1000;
        self.lyric_progress_ms = position_ms;
        let state = app.global::<PlayerViewState>();
        state.set_active_lyric_index(
            self.lyrics
                .partition_point(|line| line.ms <= position_ms.saturating_add(50))
                as i32
                - 1,
        );
        if snap || jumped {
            state.set_lyric_position_reset(state.get_lyric_position_reset().wrapping_add(1));
        }
    }
}
