//! Demo adapter for the same queue operations used by the production controller.
use crate::{
    runtime::queue::{PlaybackQueue, Removal},
    AppWindow, QueueSongItem,
};
use slint::{ComponentHandle, Model};
use std::collections::HashMap;

fn songs(app: &AppWindow) -> HashMap<String, QueueSongItem> {
    let queue_state = app.global::<crate::QueueState>();
    let detail_state = app.global::<crate::DetailState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let library_state = app.global::<crate::LibraryState>();
    let player_state = app.global::<crate::PlayerState>();
    let mut songs: HashMap<_, _> = queue_state
        .get_queue_songs()
        .iter()
        .map(|song| (song.id.to_string(), song))
        .collect();
    for tracks in [
        detail_state.get_detail_tracks(),
        playlist_state.get_playlist_tracks(),
        library_state.get_local_tracks(),
    ] {
        for track in tracks.iter() {
            songs.entry(track.id.to_string()).or_insert(QueueSongItem {
                id: track.id,
                name: track.name,
                artist: track.artist,
                cover: track.cover,
                is_local: track.is_local,
                has_cover: track.has_cover,
                ..Default::default()
            });
        }
    }
    let current = player_state.get_current_song_id();
    if current >= 0 {
        songs.entry(current.to_string()).or_insert(QueueSongItem {
            id: current,
            name: player_state.get_current_song_title(),
            artist: player_state.get_current_song_artist(),
            cover: player_state.get_current_song_cover(),
            is_local: player_state.get_current_song_is_local(),
            has_cover: player_state.get_current_song_has_cover(),
            ..Default::default()
        });
    }
    songs
}

fn show_song(state: &crate::PlayerState<'_>, song: &QueueSongItem) {
    state.invoke_play_song(
        song.id,
        song.name.clone(),
        song.artist.clone(),
        song.cover.clone(),
        song.is_local,
        song.has_cover,
    );
    state.set_progress_percent(0.0);
    state.set_current_time_str("0:00".into());
}

fn action(app: &AppWindow, name: &str, id: i32) {
    let queue_state = app.global::<crate::QueueState>();
    let player_state = app.global::<crate::PlayerState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let songs = songs(app);
    let mut queue = PlaybackQueue::default();
    queue.replace(
        queue_state
            .get_queue_songs()
            .iter()
            .map(|song| song.id.to_string()),
    );
    if player_state.get_current_song_id() >= 0 {
        queue.select(player_state.get_current_song_id().to_string());
    }
    let id = id.to_string();
    match name {
        "queue-next" => {
            let Some(song) = songs.get(&id) else {
                return;
            };
            if !queue.add_next(id) {
                return;
            }
            runtime_state.set_runtime_message(format!("已将「{}」添加到下一曲", song.name).into());
        }
        "queue-play" => {
            let Some(song) = songs.get(&id) else {
                return;
            };
            queue.select(id);
            show_song(&player_state, song);
        }
        "remove-queue" => match queue.remove(&id) {
            Removal::Missing => return,
            Removal::Removed => {}
            Removal::Current(Some(next)) => show_song(&player_state, &songs[&next]),
            Removal::Current(None) => {
                crate::playback_ui::clear(&player_state, &app.global::<crate::PlayerViewState>())
            }
        },
        "clear-queue" => {
            queue.clear();
            crate::playback_ui::clear(&player_state, &app.global::<crate::PlayerViewState>());
        }
        _ => return,
    }
    queue_state.set_queue_songs(super::model(
        queue
            .ids()
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                songs.get(id).cloned().map(|mut song| {
                    song.index = index as i32;
                    song
                })
            })
            .collect(),
    ));
}

pub(super) fn attach(app: &AppWindow) {
    let weak = app.as_weak();
    app.global::<crate::QueueState>()
        .on_demo_queue_action(move |name, id| {
            if let Some(app) = weak.upgrade() {
                action(&app, &name, id);
            }
        });
}
