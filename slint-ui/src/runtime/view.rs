mod lyrics;
mod song_actions;

use super::{
    backend::{Command, Event},
    covers::{cover_model, update_covers, CoverCache},
    data::{items, text, time, Ids, Lyric, Preferences, Song},
    playback::{clamp_position, PlaybackState},
    queue::{PlaybackQueue, Removal},
};
use crate::*;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};
pub fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    Rc::new(VecModel::from(v)).into()
}
fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    model(v.into_iter().map(Into::into).collect())
}
fn cancel_playlist_loading(app: &AppWindow) {
    let home_state = app.global::<crate::HomeState>();
    let artist_state = app.global::<crate::ArtistState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    // Public discovery responses also carry an epoch. Allow lazy loaders to
    // restart any request invalidated by a login/logout transition.
    if home_state.get_home_albums_loading() {
        home_state.set_home_albums_loading(false);
        home_state.set_home_albums_loaded(false);
    }
    if home_state.get_home_charts_loading() {
        home_state.set_home_charts_loading(false);
        home_state.set_home_charts_loaded(false);
    }
    if artist_state.get_artist_loading() {
        artist_state.set_artist_loading(false);
        artist_state.set_artist_load_error("登录状态已变化，请重新加载歌手".into());
    }
    if playlist_state.get_playlist_loading() {
        playlist_state.set_playlist_loading(false);
        playlist_state.set_playlist_metadata_loading(false);
        playlist_state.set_playlist_load_error("登录状态已变化，请重新加载歌单".into());
    }
}
fn artist_is_single(album: &Value) -> bool {
    let kind = format!("{} {}", text(&album["type"]), text(&album["subType"])).to_lowercase();
    kind.contains("single") || kind.contains("ep") || kind.contains("单曲")
}
fn artist_release_year(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        return value
            .as_bytes()
            .windows(4)
            .find(|year| year.iter().all(u8::is_ascii_digit))
            .map(|year| String::from_utf8_lossy(year).into_owned())
            .unwrap_or_else(|| {
                if value.is_empty() {
                    "--".into()
                } else {
                    value.into()
                }
            });
    }
    use chrono::{Datelike, TimeZone};
    value
        .as_i64()
        .filter(|ms| *ms != 0)
        .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
        .map(|date| date.year().to_string())
        .unwrap_or_else(|| "--".into())
}
pub struct View {
    #[cfg(test)]
    pub(super) cover_probe: CoverProbe,
    pub sender: tokio::sync::mpsc::UnboundedSender<Command>,
    ids: Ids,
    songs: HashMap<String, Song>,
    images: CoverCache,
    cover_handles: HashMap<String, HashSet<i32>>,
    cover_refs: HashMap<i32, String>,
    image_targets: HashMap<String, String>,
    likes: HashSet<String>,
    queue: PlaybackQueue,
    position_ms: u64,
    resume_pending: bool,
    seek_target: Option<(u64, Instant)>,
    playback_dirty: bool,
    last_playback_save: Instant,
    saved_playback: Option<PlaybackState>,
    displayed: Vec<String>,
    local: Vec<String>,
    daily: Vec<String>,
    lyrics: Vec<Lyric>,
    lyric_progress_ms: u64,
    tokens: HashMap<String, u64>,
    epoch: u64,
    route: String,
    active_playlist: String,
    album_artist: String,
    devices: Vec<Value>,
    login_waiting: bool,
    editor_pending: bool,
    pending_likes: HashSet<String>,
    downloads: HashSet<String>,
    pub preferences: Preferences,
    last_preferences: Value,
    loading: bool,
    paused: bool,
    rng: u64,
}
#[cfg(test)]
#[derive(Default)]
pub(super) struct CoverProbe {
    pub requests: HashMap<(String, u32), usize>,
    pub deliveries: HashMap<(String, u32), usize>,
    pub evictions: usize,
}
impl View {
    #[cfg(test)]
    pub(super) fn cover_cache_usage(&self) -> (usize, usize) {
        self.images.usage()
    }
    #[cfg(test)]
    pub(super) fn memory_model_counts(&self) -> serde_json::Value {
        serde_json::json!({
            "songs": self.songs.len(),
            "cover_refs": self.cover_refs.len(),
            "cover_urls": self.cover_handles.len(),
            "displayed": self.displayed.len(),
            "queue": self.queue.ids().len(),
        })
    }
    pub fn new(
        sender: tokio::sync::mpsc::UnboundedSender<Command>,
        preferences: Preferences,
    ) -> Self {
        Self {
            #[cfg(test)]
            cover_probe: CoverProbe::default(),
            sender,
            ids: Ids::default(),
            songs: HashMap::new(),
            images: CoverCache::default(),
            cover_handles: HashMap::new(),
            cover_refs: HashMap::new(),
            image_targets: HashMap::new(),
            likes: HashSet::new(),
            queue: PlaybackQueue::default(),
            position_ms: 0,
            resume_pending: false,
            seek_target: None,
            playback_dirty: false,
            last_playback_save: Instant::now(),
            saved_playback: Some(PlaybackState::default()),
            displayed: vec![],
            local: vec![],
            daily: vec![],
            lyrics: vec![],
            lyric_progress_ms: 0,
            tokens: HashMap::new(),
            epoch: 0,
            route: "home".into(),
            active_playlist: String::new(),
            album_artist: String::new(),
            devices: vec![],
            login_waiting: false,
            editor_pending: false,
            pending_likes: HashSet::new(),
            downloads: HashSet::new(),
            last_preferences: serde_json::to_value(&preferences).unwrap(),
            preferences,
            loading: false,
            paused: false,
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
        }
    }
    fn send(&mut self, name: &str, data: Value) {
        let token = self.tokens.entry(name.into()).or_default();
        *token += 1;
        let _ = self.sender.send(Command {
            name: name.into(),
            data,
            token: *token,
        });
    }
    fn send_mutation(&mut self, op: &str, params: Value) {
        let _ = self.sender.send(Command {
            name: "mutation".into(),
            data: json!({ "op" : op, "params" : params }),
            token: 0,
        });
    }
    fn image(&self, url: &str) -> slint::Image {
        self.images.get(url).cloned().unwrap_or_default()
    }
    fn request_cover_urls(&mut self, urls: Vec<(String, u32)>) {
        let urls: Vec<_> = urls
            .into_iter()
            .map(|(url, size)| (url, super::images::cover_size(size)))
            .filter(|(url, size)| {
                !self.images.contains(url, *size)
                    && (url.starts_with("https://") || url.starts_with("http://"))
            })
            .map(|(url, size)| json!({"cover": url, "size": size}))
            .collect();
        if !urls.is_empty() {
            #[cfg(test)]
            for value in &urls {
                *self
                    .cover_probe
                    .requests
                    .entry((
                        text(&value["cover"]),
                        value["size"].as_u64().unwrap() as u32,
                    ))
                    .or_default() += 1;
            }
            self.send("covers", json!(urls));
        }
    }
    pub fn request_covers(&mut self, ids: impl Iterator<Item = (i32, u32)>) {
        let urls = ids
            .filter_map(|(id, size)| self.cover_refs.get(&id).map(|url| (url.clone(), size)))
            .collect();
        self.request_cover_urls(urls);
    }
    fn register_cover(&mut self, handle: i32, url: String) {
        if let Some(old) = self.cover_refs.insert(handle, url.clone()) {
            if old != url {
                if let Some(handles) = self.cover_handles.get_mut(&old) {
                    handles.remove(&handle);
                }
            }
        }
        self.cover_handles.entry(url).or_default().insert(handle);
    }
    fn register(&mut self, song: Song) -> i32 {
        let handle = self.ids.intern("song", &song.id);
        self.register_cover(handle, song.cover.clone());
        self.songs.insert(song.id.clone(), song);
        handle
    }
    fn tracks(&mut self, songs: Vec<Song>) -> ModelRc<TrackItem> {
        let out = songs
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let item = TrackItem {
                    id: self.ids.intern("song", &s.id),
                    index_str: format!("{:02}", i + 1).into(),
                    name: s.name.clone().into(),
                    artist: s.artist.clone().into(),
                    album: s.album.clone().into(),
                    duration: time(s.duration).into(),
                    cover: self.image(&s.cover),
                    has_cover: !s.cover.is_empty(),
                    is_local: s.path.is_some(),
                    is_favorite: self.likes.contains(&s.id),
                };
                self.register(s);
                item
            })
            .collect();
        cover_model(out)
    }
    fn api_songs(&mut self, v: &Value) -> Vec<Song> {
        items(v)
            .iter()
            .map(Song::from_api)
            .filter(|s| !s.id.is_empty())
            .collect()
    }
    pub fn apply_images(
        &mut self,
        app: &AppWindow,
        images: impl Iterator<Item = super::images::DecodedImage>,
    ) {
        let login_state = app.global::<crate::LoginState>();
        let mut changed = HashSet::new();
        for image in images {
            if image.epoch != self.epoch {
                continue;
            }
            match image.target {
                super::images::ImageTarget::Cover(url, size) => {
                    #[cfg(test)]
                    {
                        *self
                            .cover_probe
                            .deliveries
                            .entry((url.clone(), size))
                            .or_default() += 1;
                    }
                    // A request can finish while another is still in the UI
                    // queue. Re-delivery (or a late thumbnail after an artwork
                    // upgrade) must not invalidate rows that already have it.
                    if self.images.contains(&url, size) {
                        continue;
                    }
                    changed.insert(url.clone());
                    let mut pinned: HashSet<_> = self.image_targets.values().cloned().collect();
                    if let Some(song) = self.queue.current().and_then(|id| self.songs.get(id)) {
                        pinned.insert(song.cover.clone());
                    }
                    let evicted = self.images.insert(
                        url,
                        size,
                        slint::Image::from_rgba8(image.pixels),
                        image.palette,
                        &pinned,
                    );
                    #[cfg(test)]
                    {
                        self.cover_probe.evictions += evicted.len();
                    }
                    changed.extend(evicted);
                }
                super::images::ImageTarget::LoginQr(token) => {
                    if self.login_waiting && self.tokens.get("qr").copied() == Some(token) {
                        login_state.set_login_qr_image(slint::Image::from_rgba8(image.pixels));
                        login_state.set_login_qr_ready(true);
                    }
                }
            }
        }
        if !changed.is_empty() {
            self.refresh_images(app, &changed);
        }
    }
    fn entity_cover(&mut self, kind: &str, id: &str, url: &str) -> i32 {
        let handle = self.ids.intern(kind, id);
        self.register_cover(handle, url.into());
        handle
    }
    fn target_image(&mut self, key: &str, url: &str) -> slint::Image {
        self.image_targets.insert(key.into(), url.into());
        self.request_cover_urls(vec![(
            url.into(),
            if key == "user" || key == "creator" {
                128
            } else {
                800
            },
        )]);
        self.image(url)
    }
    fn nav(&mut self, v: &Value, uid: &str) -> ModelRc<NavPlaylistItem> {
        cover_model(
            items(v)
                .iter()
                .filter(|p| text(&p["creator"]["userId"]) == uid)
                .map(|p| NavPlaylistItem {
                    id: self.entity_cover("playlist", &text(&p["id"]), &text(&p["coverImgUrl"])),
                    name: text(&p["name"]).into(),
                    track_count: p["trackCount"].as_i64().unwrap_or(0).min(i32::MAX as i64) as i32,
                    play_count: text(&p["playCount"]).into(),
                    cover: self.image(&text(&p["coverImgUrl"])),
                    already_added: false,
                })
                .collect(),
        )
    }
    fn refresh_images(&self, app: &AppWindow, changed: &HashSet<String>) {
        let detail_state = app.global::<crate::DetailState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let library_state = app.global::<crate::LibraryState>();
        let home_state = app.global::<crate::HomeState>();
        let artist_state = app.global::<crate::ArtistState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        let queue_state = app.global::<crate::QueueState>();
        let player_state = app.global::<crate::PlayerState>();
        let account_state = app.global::<crate::AccountState>();
        let mut changes = HashMap::new();
        for url in changed {
            if let Some(handles) = self.cover_handles.get(url) {
                let image = self.image(url);
                for &id in handles {
                    changes.insert(id, image.clone());
                }
            }
        }
        let cover = |id: i32| changes.get(&id).cloned();
        update_covers(detail_state.get_detail_tracks(), &changes);
        update_covers(playlist_state.get_playlist_tracks(), &changes);
        update_covers(library_state.get_local_tracks(), &changes);
        update_covers(home_state.get_daily_songs(), &changes);
        update_covers(home_state.get_recommend_playlists(), &changes);
        update_covers(home_state.get_home_new_albums(), &changes);
        update_covers(home_state.get_home_charts(), &changes);
        update_covers(playlist_state.get_created_playlists(), &changes);
        update_covers(artist_state.get_artist_top_songs(), &changes);
        update_covers(artist_state.get_artist_latest(), &changes);
        update_covers(artist_state.get_artist_albums(), &changes);
        update_covers(artist_state.get_artist_eps(), &changes);
        update_covers(artist_state.get_artist_mvs(), &changes);
        update_covers(player_view_state.get_comments(), &changes);
        update_covers(queue_state.get_queue_songs(), &changes);
        if let Some(image) = cover(player_state.get_current_song_id()) {
            player_state.set_current_song_cover(image);
            let palette = self
                .queue
                .current()
                .and_then(|id| self.songs.get(id))
                .and_then(|song| self.images.palette(&song.cover));
            crate::fullscreen_palette::apply(&player_view_state, palette);
        }
        for (key, url) in &self.image_targets {
            if !changed.contains(url) {
                continue;
            }
            let image = self.image(url);
            match key.as_str() {
                "user" => account_state.set_user_avatar(image),
                "playlist" => playlist_state.set_playlist_cover(image),
                "creator" => playlist_state.set_playlist_creator_avatar(image),
                "album" => detail_state.set_album_cover(image),
                "artist" => artist_state.set_artist_avatar(image),
                _ => {}
            }
        }
    }
    fn queue_model(&mut self, state: &crate::QueueState<'_>) {
        state.set_queue_songs(cover_model(
            self.queue
                .ids()
                .iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    self.songs.get(id).map(|s| QueueSongItem {
                        id: self.ids.intern("song", id),
                        index: i as i32,
                        name: s.name.clone().into(),
                        artist: s.artist.clone().into(),
                        cover: self.images.get(&s.cover).cloned().unwrap_or_default(),
                        is_local: s.path.is_some(),
                        has_cover: !s.cover.is_empty(),
                    })
                })
                .collect(),
        ));
    }
    pub fn restore_playback(&mut self, app: &AppWindow, mut saved: PlaybackState) {
        saved.normalize();
        self.queue.clear();
        self.queue
            .replace(saved.queue.iter().map(|song| song.id.clone()));
        for song in &saved.queue {
            self.register(song.clone());
        }
        if let Some(id) = saved.current_id.clone() {
            self.queue.select(id);
        }
        self.position_ms = saved.position_ms;
        self.resume_pending = self.queue.current().is_some();
        self.loading = false;
        self.paused = false;
        self.saved_playback = Some(saved);
        let player_state = app.global::<crate::PlayerState>();
        player_state.set_is_playing(false);
        player_state.set_is_loading(false);
        if let Some(song) = self
            .queue
            .current()
            .and_then(|id| self.songs.get(id))
            .cloned()
        {
            self.show_current(app, &song, self.position_ms);
        } else {
            self.queue_model(&app.global::<crate::QueueState>());
        }
    }

    pub fn save_playback_if_changed(&mut self, _app: &AppWindow, force: bool) {
        // Queue edits and seeks are saved on the next UI tick. During playback,
        // serialize the queue at most once per two seconds, not every frame.
        if !force
            && !self.playback_dirty
            && self.last_playback_save.elapsed() < Duration::from_secs(2)
        {
            return;
        }
        self.last_playback_save = Instant::now();
        self.playback_dirty = false;
        let saved = PlaybackState {
            queue: self
                .queue
                .ids()
                .iter()
                .filter_map(|id| self.songs.get(id).cloned())
                .collect(),
            current_id: self.queue.current().cloned(),
            position_ms: self.position_ms,
        };
        if self.saved_playback.as_ref() != Some(&saved) {
            self.send("save-playback", serde_json::to_value(&saved).unwrap());
            self.saved_playback = Some(saved);
        }
    }

    fn play(&mut self, app: &AppWindow, id: String, replace: bool) {
        self.play_from(app, id, replace, 0);
    }

    fn queue_next(&mut self, app: &AppWindow, id: String) {
        let Some(song) = self.songs.get(&id) else {
            return;
        };
        let message = format!("已将「{}」添加到下一曲", song.name);
        if !self.queue.add_next(id) {
            return;
        }
        self.playback_dirty = true;
        let runtime_state = app.global::<crate::RuntimeState>();
        self.queue_model(&app.global::<crate::QueueState>());
        runtime_state.set_runtime_message(message.into());
    }

    fn clear_queue(&mut self, app: &AppWindow) {
        self.queue.clear();
        self.position_ms = 0;
        self.resume_pending = false;
        self.seek_target = None;
        self.playback_dirty = true;
        self.paused = false;
        self.loading = false;
        self.send("stop", Value::Null);
        *self.tokens.entry("play".into()).or_default() += 1;
        *self.tokens.entry("lyrics".into()).or_default() += 1;
        *self.tokens.entry("comments".into()).or_default() += 1;
        self.queue_model(&app.global::<crate::QueueState>());
        crate::playback_ui::clear(
            &app.global::<crate::PlayerState>(),
            &app.global::<crate::PlayerViewState>(),
        );
    }

    fn remove_from_queue(&mut self, app: &AppWindow, id: &str) {
        match self.queue.remove(id) {
            Removal::Missing => return,
            Removal::Removed => self.queue_model(&app.global::<crate::QueueState>()),
            Removal::Current(Some(next)) => self.play(app, next, false),
            Removal::Current(None) => self.clear_queue(app),
        }
        self.playback_dirty = true;
    }

    fn play_from(&mut self, app: &AppWindow, id: String, replace: bool, position_ms: u64) {
        let Some(song) = self.songs.get(&id).cloned() else {
            return;
        };
        let position_ms = clamp_position(&song, position_ms);
        *self.tokens.entry("comments".into()).or_default() += 1;
        if replace {
            let ids = if self.route == "home" && self.daily.contains(&id) {
                self.daily.clone()
            } else if self.route == "local-music" && self.local.contains(&id) {
                self.local.clone()
            } else if self.displayed.contains(&id) {
                self.displayed.clone()
            } else {
                vec![id.clone()]
            };
            self.queue.replace(ids);
        }
        self.queue.select(id);
        self.resume_pending = false;
        self.seek_target = None;
        self.playback_dirty = true;
        self.loading = true;
        self.paused = false;
        let player_state = app.global::<crate::PlayerState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        player_state.set_is_loading(true);
        player_state.set_is_playing(false);
        // Queue playback before the artwork and lyric metadata requests.
        self.send("play", json!({ "song": song, "position_ms": position_ms }));
        self.show_current(app, &song, position_ms);
        if song.path.is_none() && player_view_state.get_is_comment_open() {
            self.send("comments", json!({ "id" : song.id, "limit" : 50 }));
        }
    }

    fn show_current(&mut self, app: &AppWindow, song: &Song, position_ms: u64) {
        let player_state = app.global::<crate::PlayerState>();
        let runtime_state = app.global::<crate::RuntimeState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        self.position_ms = position_ms;
        player_state.set_progress_percent(if song.duration > 0 {
            (position_ms as f64 / song.duration as f64 * 100.0) as f32
        } else {
            0.0
        });
        player_state.set_current_time_str(time(position_ms).into());
        player_state.set_buffered_percent(0.0);
        player_state.set_duration_time_str(time(song.duration).into());
        player_state.set_current_song_id(self.ids.intern("song", &song.id));
        player_state.set_current_song_title(song.name.clone().into());
        player_state.set_current_song_artist(song.artist.clone().into());
        player_state.set_current_song_cover(self.image(&song.cover));
        crate::fullscreen_palette::apply(&player_view_state, self.images.palette(&song.cover));
        self.request_cover_urls(vec![(song.cover.clone(), 800)]);
        player_state.set_current_song_is_local(song.path.is_some());
        player_state.set_current_song_has_cover(!song.cover.is_empty());
        player_state.set_current_song_favorite(self.likes.contains(&song.id));
        runtime_state.set_runtime_message("".into());
        self.load_song_lyrics(app, song, position_ms);
        player_view_state.set_comments(cover_model(vec![]));
        self.queue_model(&app.global::<crate::QueueState>());
    }
    fn step(&mut self, app: &AppWindow, delta: i32, natural: bool) {
        let mode = app.global::<crate::PlayerState>().get_play_mode();
        if mode == "random" {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
        }
        if let Some(id) = self.queue.step(delta, natural, &mode, self.rng).cloned() {
            self.play(app, id, false);
        }
    }
    fn page(&mut self, app: &AppWindow, kind: &str, id: &str, autoplay: bool) {
        self.route = kind.into();
        let artist_state = app.global::<crate::ArtistState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let home_state = app.global::<crate::HomeState>();
        let detail_state = app.global::<crate::DetailState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let runtime_state = app.global::<crate::RuntimeState>();
        let search_state = app.global::<crate::SearchState>();
        artist_state.set_artist_loading(kind == "artist");
        artist_state.set_artist_load_error("".into());
        if kind == "artist" {
            artist_state.set_active_artist_id(self.ids.intern("artist", id));
            self.image_targets.remove("artist");
            artist_state.set_artist_avatar(slint::Image::default());
            artist_state.set_artist_name("".into());
            artist_state.set_artist_summary("".into());
            artist_state.set_artist_pills(strings(vec![]));
            artist_state.set_artist_latest(cover_model(vec![]));
            artist_state.set_artist_albums(cover_model(vec![]));
            artist_state.set_artist_eps(cover_model(vec![]));
            artist_state.set_artist_mvs(cover_model(vec![]));
            artist_state.set_artist_top_songs(cover_model(vec![]));
        }
        playlist_state.set_playlist_loading(kind == "playlist");
        playlist_state.set_playlist_metadata_loading(kind == "playlist");
        playlist_state.set_playlist_load_error("".into());
        if kind == "playlist" {
            self.active_playlist = id.into();
            let handle = self.ids.intern("playlist", id);
            let preview = playlist_state
                .get_created_playlists()
                .iter()
                .find(|p| p.id == handle)
                .map(|p| (p.name, p.cover))
                .or_else(|| {
                    home_state
                        .get_recommend_playlists()
                        .iter()
                        .find(|p| p.id == handle)
                        .map(|p| (p.title, p.cover))
                })
                .or_else(|| {
                    home_state
                        .get_home_charts()
                        .iter()
                        .find(|p| p.id == handle)
                        .map(|p| (p.title, p.cover))
                });
            let (title, cover) = preview.unwrap_or_default();
            // Detach the previous page's images before any late decode arrives.
            self.image_targets.remove("playlist");
            self.image_targets.remove("creator");
            playlist_state.set_active_playlist_id(handle);
            playlist_state.set_playlist_title(title);
            playlist_state.set_playlist_cover(cover);
            if let Some(url) = self.cover_refs.get(&handle).cloned() {
                playlist_state.set_playlist_cover(self.target_image("playlist", &url));
            }
            playlist_state.set_playlist_description("".into());
            playlist_state.set_playlist_creator("".into());
            playlist_state.set_playlist_creator_avatar(slint::Image::default());
            playlist_state.set_playlist_meta(model(vec![]));
            playlist_state.set_playlist_track_count(0);
            playlist_state.set_playlist_subscribed_count("".into());
            playlist_state.set_playlist_special_type(0);
            playlist_state.set_playlist_is_private(false);
            playlist_state.set_is_playlist_actions_open(false);
        }
        if kind == "album" {
            self.image_targets.remove("album");
            let handle = self.ids.intern("album", id);
            let preview = home_state
                .get_home_new_albums()
                .iter()
                .find(|a| a.id == handle);
            detail_state.set_album_title(
                preview
                    .as_ref()
                    .map(|a| a.title.clone())
                    .unwrap_or_default(),
            );
            detail_state.set_album_artist(
                preview
                    .as_ref()
                    .map(|a| a.artist.clone())
                    .unwrap_or_default(),
            );
            detail_state.set_album_cover(preview.map(|a| a.cover).unwrap_or_default());
            detail_state.set_album_description("".into());
            detail_state.set_album_meta(model(vec![]));
            self.album_artist.clear();
            if let Some(url) = self.cover_refs.get(&handle).cloned() {
                detail_state.set_album_cover(self.target_image("album", &url));
            }
        }
        navigation_state.set_current_route(kind.into());
        detail_state.set_detail_search_query("".into());
        detail_state.set_detail_tracks(model(vec![]));
        playlist_state.set_playlist_tracks(model(vec![]));
        playlist_state.set_playlist_owned(false);
        runtime_state.set_runtime_message(
            if kind == "playlist" {
                ""
            } else {
                "正在加载…"
            }
            .into(),
        );
        self.displayed.clear();
        self.send(
            "page",
            json!(
                { "kind" : kind, "id" : id, "query" : search_state.get_search_query().as_str(),
                "autoplay" : autoplay }
            ),
        );
    }
    fn handle_for_song(&self, id: i32) -> Option<String> {
        self.ids.get(id, "song").map(str::to_owned)
    }
    pub fn action(&mut self, app: &AppWindow, name: &str, id: i32, arg: &str) {
        let home_state = app.global::<crate::HomeState>();
        let player_state = app.global::<crate::PlayerState>();
        let artist_state = app.global::<crate::ArtistState>();
        let search_state = app.global::<crate::SearchState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let runtime_state = app.global::<crate::RuntimeState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        let login_state = app.global::<crate::LoginState>();
        let account_state = app.global::<crate::AccountState>();
        let detail_state = app.global::<crate::DetailState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let download_state = app.global::<crate::DownloadState>();
        match name {
            "home-albums" => {
                if !home_state.get_home_albums_loading()
                    && (!home_state.get_home_albums_loaded() || arg == "retry")
                {
                    home_state.set_home_albums_loading(true);
                    home_state.set_home_albums_error("".into());
                    self.send(name, json!({}));
                }
            }
            "home-charts" => {
                if !home_state.get_home_charts_loading()
                    && (!home_state.get_home_charts_loaded() || arg == "retry")
                {
                    home_state.set_home_charts_loading(true);
                    home_state.set_home_charts_error("".into());
                    self.send(name, json!({}));
                }
            }
            "play" | "queue-play" => {
                if let Some(id) = self.handle_for_song(id) {
                    self.play(app, id, name == "play")
                }
            }
            "queue-next" => {
                if let Some(id) = self.handle_for_song(id) {
                    self.queue_next(app, id);
                }
            }
            "toggle" => {
                if self.loading {
                    return;
                }
                if let Some(id) = self.queue.current().cloned() {
                    if !player_state.get_is_playing() && !self.paused {
                        self.play_from(app, id, false, self.position_ms)
                    } else {
                        self.send("toggle", Value::Null)
                    }
                } else if let Some(id) = self.queue.ids().first().cloned() {
                    self.play(app, id, false)
                }
            }
            "step" => self.step(app, id, false),
            "play-all" => {
                if let Some(id) = self.displayed.first().cloned() {
                    self.play(app, id, true)
                }
            }
            "play-playlist" | "playlist" => {
                if let Some(id) = self.ids.get(id, "playlist").map(str::to_owned) {
                    self.page(app, "playlist", &id, name == "play-playlist")
                }
            }
            "song-album" => {
                if let Some(song) = self
                    .handle_for_song(id)
                    .and_then(|id| self.songs.get(&id))
                    .cloned()
                {
                    self.page(app, "album", &song.album_id, false)
                }
            }
            "album" => {
                if let Some(id) = self.ids.get(id, "album").map(str::to_owned) {
                    self.page(app, "album", &id, false)
                }
            }
            "artist" => {
                let artist = if self.route == "album" {
                    self.album_artist.clone()
                } else {
                    self.queue
                        .current()
                        .and_then(|id| self.songs.get(id))
                        .map(|s| s.artist_id.clone())
                        .unwrap_or_default()
                };
                if !artist.is_empty() {
                    self.page(app, "artist", &artist, false)
                }
            }
            "artist-retry" => {
                if let Some(id) = self
                    .ids
                    .get(artist_state.get_active_artist_id(), "artist")
                    .map(str::to_owned)
                {
                    self.page(app, "artist", &id, false);
                }
            }
            "mv" => {
                if let Some(id) = self.ids.get(id, "mv").map(str::to_owned) {
                    self.send("mv", json!({ "id" : id }))
                }
            }
            "more-search" => {
                if self.route == "search" {
                    self.send(
                        "page",
                        json!(
                            { "kind" : "search", "query" : search_state.get_search_query().as_str(),
                            "offset" : self.displayed.len() }
                        ),
                    );
                }
            }
            "search" => {
                if !search_state.get_search_query().trim().is_empty() {
                    search_state.set_search_show_results(true);
                    self.page(app, "search", "", false);
                }
            }
            "route" => {
                if self.route == arg {
                    return;
                }
                self.route = arg.into();
                artist_state.set_artist_loading(false);
                *self.tokens.entry("page".into()).or_default() += 1;
                playlist_state.set_playlist_loading(false);
                playlist_state.set_playlist_metadata_loading(false);
                playlist_state.set_playlist_load_error("".into());
                runtime_state.set_runtime_message("".into());
                match arg {
                    "favorites" => self.page(app, "favorites", "", false),
                    "artist" => self.action(app, "artist", -1, ""),
                    "local-music" => {
                        self.displayed = self.local.clone();
                    }
                    "home" => {
                        self.send("home", json!({}));
                    }
                    _ => {}
                }
            }
            "favorite" => self.toggle_song_favorite(app, id),
            "add-track" => {
                if let Ok(song) = arg.parse::<i32>() {
                    self.add_song_to_playlist(id, song);
                }
            }
            "remove-track" => self.remove_song_from_playlist(app, id),
            "clear-queue" => self.clear_queue(app),
            "remove-queue" => {
                if let Some(id) = self.handle_for_song(id) {
                    self.remove_from_queue(app, &id);
                }
            }
            "lyric-seek" => {
                if !self.loading && !player_state.get_is_loading() && id >= 0 {
                    if let Some(line) = self.lyrics.get(id as usize) {
                        let ms = line.ms;
                        self.seek_to_ms(app, ms);
                        player_view_state.set_active_lyric_index(id);
                    }
                }
            }
            "comments" => {
                if let Some(id) = self
                    .queue
                    .current()
                    .cloned()
                    .filter(|id| self.songs.get(id).is_some_and(|s| s.path.is_none()))
                {
                    self.send("comments", json!({ "id" : id, "limit" : 50 }));
                }
            }
            "login-mode" => {
                if arg == "qr" {
                    self.action(app, "qr", -1, "");
                } else {
                    self.action(app, "cancel-login", -1, "");
                }
            }
            "login" | "qr" => {
                if self.login_waiting && name == "login" {
                    return;
                }
                login_state.set_login_error("".into());
                let password_mode = login_state.get_login_tab() == "password";
                let (phone, secret, country) = if password_mode {
                    (
                        login_state.get_login_phone(),
                        login_state.get_login_password(),
                        login_state.get_login_country_code(),
                    )
                } else {
                    (
                        login_state.get_login_sms_phone(),
                        login_state.get_login_code(),
                        login_state.get_login_sms_country_code(),
                    )
                };
                if name == "login" && (phone.is_empty() || secret.is_empty()) {
                    login_state.set_login_error(
                        if password_mode {
                            "请输入手机号和密码"
                        } else {
                            "请输入手机号和验证码"
                        }
                        .into(),
                    );
                    return;
                }
                self.epoch += 1;
                self.login_waiting = true;
                cancel_playlist_loading(app);
                login_state.set_login_busy(name == "login");
                login_state.set_login_captcha_busy(false);
                if name == "qr" {
                    login_state.set_login_qr_ready(false);
                    login_state.set_login_qr_image(slint::Image::default());
                    login_state.set_login_qr_message("正在加载二维码...".into());
                    // QR login remembers the session independently of the form checkbox.
                    self.send("qr", json!({ "remember" : true }));
                } else {
                    let mut params = json!({ "phone":phone.as_str(), "countrycode":country.as_str(), "rememberMe":login_state.get_login_remember() });
                    params[if password_mode { "password" } else { "captcha" }] =
                        json!(secret.as_str());
                    self.send(
                        "login",
                        json!({ "params":params, "remember":login_state.get_login_remember() }),
                    );
                }
            }
            "captcha" => {
                if login_state.get_login_captcha_busy()
                    || login_state.get_login_captcha_seconds() > 0
                {
                    return;
                }
                login_state.set_login_error("".into());
                if login_state.get_login_sms_phone().is_empty() {
                    login_state.set_login_error("请输入手机号码".into());
                    return;
                }
                login_state.set_login_captcha_busy(true);
                self.send("captcha", json!({ "phone":login_state.get_login_sms_phone().as_str(), "ctcode":login_state.get_login_sms_country_code().as_str() }));
            }
            "close-login" | "cancel-login" => {
                // A closed dialog must not receive a late captcha result on reopening.
                *self.tokens.entry("captcha".into()).or_default() += 1;
                login_state.set_login_captcha_busy(false);
                if self.login_waiting {
                    self.login_waiting = false;
                    self.epoch += 1;
                    cancel_playlist_loading(app);
                    login_state.set_login_busy(false);
                    self.send("cancel-login", Value::Null);
                    self.send("home", json!({}));
                    self.send("devices", json!({}));
                    self.send("cache", json!({}));
                }
            }
            "logout" => {
                self.epoch += 1;
                cancel_playlist_loading(app);
                self.login_waiting = false;
                self.likes.clear();
                self.image_targets.clear();
                self.pending_likes.clear();
                self.clear_queue(app);
                self.displayed.clear();
                self.daily.clear();
                self.send("logout", Value::Null);
                account_state.set_is_logged_in(false);
                account_state.set_user_nickname("未登录".into());
                account_state.set_user_avatar(slint::Image::default());
                playlist_state.set_created_playlists(model(vec![]));
                detail_state.set_detail_tracks(model(vec![]));
                playlist_state.set_playlist_tracks(model(vec![]));
                home_state.set_daily_songs(cover_model(vec![]));
                home_state.set_recommend_playlists(cover_model(vec![]));
                playlist_state.set_playlist_owned(false);
                playlist_state.set_playlist_loading(false);
                playlist_state.set_playlist_metadata_loading(false);
                playlist_state.set_playlist_load_error("".into());
                login_state.set_is_login_open(false);
                navigation_state.set_current_route("home".into());
                self.route = "home".into();
                self.send("home", json!({}));
            }
            "local" => {
                runtime_state.set_runtime_message("正在更新本地曲库…".into());
                self.send("local", json!({ "action" : arg }));
            }
            "remove-folder" => self.send("local", json!({ "action" : "remove", "path" : arg })),
            "remove-local-track" => self.remove_local_song(app, id),
            "clear-cache" => {
                self.loading = false;
                self.send("stop", Value::Null);
                *self.tokens.entry("play".into()).or_default() += 1;
                self.send("cache", json!({ "clear" : true }));
            }
            "devices" => self.send("devices", json!({})),
            "select-device" => self.send("device", json!({ "id" : arg })),
            "apply-cache" => {
                self.save_preferences_if_changed(app);
                self.send("cache", json!({}));
            }
            "next-device" => {
                if self.devices.is_empty() {
                    self.send("devices", json!({}));
                    return;
                }
                let current = self
                    .devices
                    .iter()
                    .position(|d| d["current"] == true)
                    .unwrap_or(0);
                let next = &self.devices[(current + 1) % self.devices.len()];
                self.send("device", json!({ "id" : next["id"] }));
            }
            "download" | "save-download" => {
                if let Some(song) = self
                    .handle_for_song(id)
                    .and_then(|id| self.songs.get(&id))
                    .cloned()
                {
                    if name == "download" {
                        runtime_state.set_runtime_message("正在检查下载权限…".into());
                        download_state.set_download_song_id(id);
                        download_state.set_download_song_name(song.name.clone().into());
                        download_state.set_download_qualities(model(vec![]));
                        download_state.set_is_download_open(true);
                        self.send("download-options", json!({"song":song}));
                    } else if self.downloads.insert(song.id.clone()) {
                        download_state.set_is_download_open(false);
                        runtime_state.set_runtime_message("正在下载…".into());
                        self.send("download", json!({"song":song,"quality":arg}));
                    }
                }
            }
            _ => {}
        }
    }
    pub fn seek(&mut self, app: &AppWindow, percent: f32) {
        if !percent.is_finite() {
            return;
        }
        if let Some(song) = self.queue.current().and_then(|id| self.songs.get(id)) {
            let ms = (song.duration as f64 * percent.clamp(0.0, 100.0) as f64 / 100.0) as u64;
            self.seek_to_ms(app, ms);
        }
    }
    fn seek_to_ms(&mut self, app: &AppWindow, ms: u64) {
        self.position_ms = ms;
        self.update_lyric_position(app, ms, true);
        self.playback_dirty = true;
        let player_state = app.global::<crate::PlayerState>();
        player_state.set_current_time_str(time(ms).into());
        if let Some(song) = self.queue.current().and_then(|id| self.songs.get(id)) {
            player_state.set_progress_percent(if song.duration > 0 {
                (ms as f64 / song.duration as f64 * 100.0) as f32
            } else {
                0.0
            });
        }
        if !self.resume_pending {
            self.seek_target = Some((ms, Instant::now()));
            self.send("seek", json!({ "seconds": ms as f64 / 1000.0 }));
        }
    }
    pub fn editor(&mut self, app: &AppWindow, action: &str) {
        let account_state = app.global::<crate::AccountState>();
        let login_state = app.global::<crate::LoginState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let dialog_state = app.global::<crate::DialogState>();
        match action {
            "create" => {
                if !account_state.get_is_logged_in() {
                    login_state.set_is_login_open(true);
                    return;
                }
                playlist_state.set_playlist_editor_name("".into());
                playlist_state.set_playlist_editor_description("".into());
                playlist_state.set_playlist_editor_private(false);
                playlist_state.set_playlist_editor_mode("create".into());
                playlist_state.set_playlist_editor_error("".into());
            }
            "edit" => {
                if !playlist_state.get_playlist_can_manage() {
                    return;
                }
                playlist_state.set_playlist_editor_name(playlist_state.get_playlist_title());
                playlist_state
                    .set_playlist_editor_description(playlist_state.get_playlist_description());
                playlist_state.set_playlist_editor_mode("edit".into());
                playlist_state.set_playlist_editor_error("".into());
            }
            "close" => {
                if self.editor_pending {
                    return;
                }
                playlist_state.set_playlist_restore_focus(
                    if playlist_state.get_playlist_editor_mode() == "create" {
                        1
                    } else {
                        2
                    },
                );
                playlist_state.set_playlist_editor_mode("".into());
            }
            "delete" => {
                if !playlist_state.get_playlist_can_manage() {
                    return;
                }
                dialog_state.set_playlist_delete_id(playlist_state.get_active_playlist_id());
                dialog_state.set_dialog_title("删除歌单".into());
                dialog_state.set_dialog_message(
                    format!(
                        "确定删除「{}」吗？删除后无法恢复。",
                        playlist_state.get_playlist_title()
                    )
                    .into(),
                );
                dialog_state.set_dialog_mode("confirm-cancel".into());
                dialog_state.set_dialog_confirm_text("删除".into());
                dialog_state.set_dialog_cancel_text("取消".into());
                dialog_state.set_is_dialog_open(true);
            }
            "confirm-delete" => {
                if playlist_state.get_playlist_can_manage()
                    && dialog_state.get_playlist_delete_id()
                        == playlist_state.get_active_playlist_id()
                {
                    self.send_mutation("playlist_delete", json!({ "id" : self.active_playlist }));
                }
            }
            "submit" => {
                if self.editor_pending || !playlist_state.get_playlist_editor_can_submit() {
                    return;
                }
                self.editor_pending = true;
                playlist_state.set_playlist_editor_can_submit(false);
                let name =
                    crate::playlists::normalize_name(&playlist_state.get_playlist_editor_name());
                if playlist_state.get_playlist_editor_mode() == "create" {
                    self.send_mutation(
                        "playlist_create",
                        json!(
                            { "name" : name, "privacy" : if playlist_state.get_playlist_editor_private() { 10 } else { 0 } }
                        ),
                    );
                } else {
                    self.send_mutation(
                        "playlist_update",
                        json!(
                            { "id" : self.active_playlist, "name" : name, "desc" : playlist_state.get_playlist_editor_description().trim() }
                        ),
                    );
                }
            }
            _ => {}
        }
        let name = crate::playlists::limit_text(&playlist_state.get_playlist_editor_name(), 40);
        let desc =
            crate::playlists::limit_text(&playlist_state.get_playlist_editor_description(), 1000);
        playlist_state.set_playlist_editor_name(name.clone().into());
        playlist_state.set_playlist_editor_description(desc.clone().into());
        playlist_state.set_playlist_editor_name_length(name.encode_utf16().count() as i32);
        playlist_state.set_playlist_editor_description_length(desc.encode_utf16().count() as i32);
        playlist_state.set_playlist_editor_can_submit(
            !self.editor_pending
                && !crate::playlists::normalize_name(&name).is_empty()
                && account_state.get_is_logged_in()
                && (playlist_state.get_playlist_editor_mode() == "create"
                    || (playlist_state.get_playlist_can_manage()
                        && (playlist_state.get_playlist_title() != name
                            || playlist_state.get_playlist_description() != desc))),
        );
    }
    fn update_likes(&self, app: &AppWindow) {
        let detail_state = app.global::<crate::DetailState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let player_state = app.global::<crate::PlayerState>();
        let update = |source: ModelRc<TrackItem>| {
            cover_model(
                source
                    .iter()
                    .map(|mut t| {
                        t.is_favorite = self
                            .ids
                            .get(t.id, "song")
                            .is_some_and(|id| self.likes.contains(id));
                        t
                    })
                    .collect(),
            )
        };
        detail_state.set_detail_tracks(update(detail_state.get_detail_tracks()));
        playlist_state.set_playlist_tracks(update(playlist_state.get_playlist_tracks()));
        player_state.set_current_song_favorite(
            self.queue
                .current()
                .is_some_and(|id| self.likes.contains(id)),
        );
    }
    pub fn apply(&mut self, app: &AppWindow, event: Event) {
        if event.name == "finished" && self.tokens.get("play").copied().unwrap_or(0) != event.token
        {
            return;
        }
        if event.epoch != self.epoch {
            return;
        }
        let token_key = if event.name == "playlist-metadata" {
            "page"
        } else {
            &event.name
        };
        if event.token > 0
            && !matches!(event.name.as_str(), "finished" | "mutation" | "download")
            && self.tokens.get(token_key).copied().unwrap_or(0) != event.token
        {
            return;
        }
        let home_state = app.global::<crate::HomeState>();
        let login_state = app.global::<crate::LoginState>();
        let player_state = app.global::<crate::PlayerState>();
        let artist_state = app.global::<crate::ArtistState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let download_state = app.global::<crate::DownloadState>();
        let runtime_state = app.global::<crate::RuntimeState>();
        let search_state = app.global::<crate::SearchState>();
        let account_state = app.global::<crate::AccountState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let audio_state = app.global::<crate::AudioState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        let library_state = app.global::<crate::LibraryState>();
        let settings_state = app.global::<crate::SettingsState>();
        if let Some(error) = event.error {
            match event.name.as_str() {
                "home-albums" => {
                    home_state.set_home_albums_loading(false);
                    home_state.set_home_albums_loaded(true);
                    home_state.set_home_albums_error("新碟暂时加载失败，请重试".into());
                    return;
                }
                "home-charts" => {
                    home_state.set_home_charts_loading(false);
                    home_state.set_home_charts_loaded(true);
                    home_state.set_home_charts_error("榜单暂时加载失败，请重试".into());
                    return;
                }
                "captcha" => {
                    login_state.set_login_captcha_busy(false);
                    login_state.set_login_error(error.clone().into());
                }
                "login" | "qr" => {
                    self.login_waiting = false;
                    login_state.set_login_busy(false);
                    login_state.set_login_error(error.clone().into());
                    login_state.set_login_qr_message(error.clone().into());
                    self.send("home", json!({}));
                }
                "play" => {
                    self.loading = false;
                    self.resume_pending = self.queue.current().is_some();
                    player_state.set_is_loading(false);
                    player_state.set_is_playing(false);
                }
                "save-playback" => {
                    // A failed disk write must be retried on the next save interval.
                    self.saved_playback = None;
                }
                "home" => {
                    home_state.set_home_playlists_loading(false);
                    home_state.set_home_songs_loading(false);
                }
                "page" if self.route == "artist" => {
                    artist_state.set_artist_loading(false);
                    artist_state.set_artist_load_error(error.clone().into());
                }
                "page" if self.route == "playlist" => {
                    playlist_state.set_playlist_loading(false);
                    playlist_state.set_playlist_metadata_loading(false);
                    playlist_state.set_playlist_load_error(error.clone().into());
                }
                "mutation" => {
                    self.editor_pending = false;
                    self.pending_likes.clear();
                    playlist_state.set_playlist_editor_error(error.clone().into());
                    self.editor(app, "validate");
                }
                "download-options" => download_state.set_is_download_open(false),
                _ => {}
            }
            runtime_state.set_runtime_message(error.into());
            return;
        }
        let data = event.data;
        match event.name.as_str() {
            "home-albums" => {
                home_state.set_home_new_albums(cover_model(
                    items(&data["albums"])
                        .iter()
                        .filter(|a| !text(&a["id"]).is_empty() && !text(&a["name"]).is_empty())
                        .take(24)
                        .map(|a| {
                            let artists = items(&a["artists"])
                                .iter()
                                .map(|v| text(&v["name"]))
                                .filter(|v| !v.is_empty())
                                .collect::<Vec<_>>()
                                .join(" / ");
                            DailySongItem {
                                id: self.entity_cover(
                                    "album",
                                    &text(&a["id"]),
                                    &text(&a["picUrl"]),
                                ),
                                title: text(&a["name"]).into(),
                                artist: if artists.is_empty() {
                                    text(&a["artist"]["name"])
                                } else {
                                    artists
                                }
                                .into(),
                                cover: self.image(&text(&a["picUrl"])),
                            }
                        })
                        .collect(),
                ));
                home_state.set_home_albums_loading(false);
                home_state.set_home_albums_loaded(true);
                home_state.set_home_albums_error("".into());
            }
            "home-charts" => {
                home_state.set_home_charts(cover_model(
                    items(&data["list"])
                        .iter()
                        .filter(|p| {
                            !text(&p["id"]).is_empty()
                                && !text(&p["name"]).is_empty()
                                && !items(&p["tracks"]).is_empty()
                        })
                        .take(6)
                        .map(|p| HomeChartItem {
                            id: self.entity_cover(
                                "playlist",
                                &text(&p["id"]),
                                &text(&p["coverImgUrl"]),
                            ),
                            title: text(&p["name"]).into(),
                            update: text(&p["updateFrequency"]).into(),
                            cover: self.image(&text(&p["coverImgUrl"])),
                            tracks: strings(
                                items(&p["tracks"])
                                    .iter()
                                    .take(3)
                                    .map(|t| {
                                        let name = text(&t["first"]);
                                        let artist = text(&t["second"]);
                                        if artist.is_empty() {
                                            name
                                        } else {
                                            format!("{name} · {artist}")
                                        }
                                    })
                                    .collect(),
                            ),
                        })
                        .collect(),
                ));
                home_state.set_home_charts_loading(false);
                home_state.set_home_charts_loaded(true);
                home_state.set_home_charts_error("".into());
            }
            "home" => {
                let b = &data["body"];
                let pls = b["playlists"]
                    .get("recommend")
                    .or(b["playlists"].get("result"))
                    .unwrap_or(&Value::Null);
                home_state.set_recommend_playlists(cover_model(
                    items(pls)
                        .iter()
                        .map(|p| RecommendPlaylistItem {
                            id: self.entity_cover(
                                "playlist",
                                &text(&p["id"]),
                                &text(p.get("picUrl").unwrap_or(&p["coverImgUrl"])),
                            ),
                            title: text(&p["name"]).into(),
                            desc: text(p.get("copywriter").unwrap_or(&p["description"])).into(),
                            cover: self.image(&text(p.get("picUrl").unwrap_or(&p["coverImgUrl"]))),
                        })
                        .collect(),
                ));
                let songlist = b["songs"]["data"]
                    .get("dailySongs")
                    .or(b["songs"].get("result"))
                    .unwrap_or(&Value::Null);
                let songs = self.api_songs(songlist);
                self.daily = songs.iter().map(|s| s.id.clone()).collect();
                home_state.set_daily_songs(cover_model(
                    songs
                        .into_iter()
                        .map(|song| {
                            let row = DailySongItem {
                                id: self.ids.intern("song", &song.id),
                                title: song.name.clone().into(),
                                artist: song.artist.clone().into(),
                                cover: self.image(&song.cover),
                            };
                            self.register(song);
                            row
                        })
                        .collect(),
                ));
                let colors = [
                    "purple", "orange", "red", "blue", "green", "dark", "pink", "indigo",
                ];
                search_state.set_browse_categories(model(
                    items(&b["categories"]["sub"])
                        .iter()
                        .enumerate()
                        .map(|(i, c)| CategoryItem {
                            name: text(&c["name"]).into(),
                            color_type: colors[i % colors.len()].into(),
                        })
                        .collect(),
                ));
                home_state.set_home_playlists_loading(false);
                home_state.set_home_songs_loading(false);
            }
            "login" => {
                if data.is_null() {
                    return;
                }
                self.login_waiting = false;
                login_state.set_login_busy(false);
                account_state.set_is_logged_in(true);
                account_state.set_user_nickname("正在获取用户信息…".into());
                login_state.set_is_login_open(false);
                runtime_state.set_runtime_message("登录成功".into());
                self.send("account", json!({}));
                self.send("home", json!({}));
                self.send("configure-xeapi", json!({}));
            }
            "account" => {
                if data.is_null() {
                    return;
                }
                let b = &data["body"];
                let uid = text(&b["profile"]["userId"]);
                account_state.set_is_logged_in(true);
                account_state.set_user_nickname(text(&b["profile"]["nickname"]).into());
                account_state
                    .set_user_avatar(self.target_image("user", &text(&b["profile"]["avatarUrl"])));
                playlist_state.set_created_playlists(self.nav(&b["playlists"], &uid));
                self.likes = items(&b["likes"]).iter().map(text).collect();
                self.update_likes(app);
            }
            "page" => self.apply_page(app, &data),
            "playlist-metadata" => {
                self.apply_playlist_metadata(app, &text(&data["id"]), &data["playlist"]);
            }
            "mutation" => {
                let op = text(&data["op"]);
                let p = &data["request"]["params"];
                match op.as_str() {
                    "like" => {
                        let id = text(&p["id"]);
                        self.pending_likes.remove(&id);
                        if p["like"] == true {
                            self.likes.insert(id);
                        } else {
                            self.likes.remove(&id);
                        }
                        self.update_likes(app);
                    }
                    "playlist_create" | "playlist_update" => {
                        self.editor_pending = false;
                        playlist_state.set_playlist_editor_mode("".into());
                        playlist_state.set_playlist_editor_can_submit(false);
                        let id = if op == "playlist_create" {
                            text(&data["body"]["playlist"]["id"])
                        } else {
                            text(&p["id"])
                        };
                        if !id.is_empty() {
                            self.page(app, "playlist", &id, false)
                        }
                        self.send("account", json!({}));
                    }
                    "playlist_delete" => {
                        if text(&p["id"]) == self.active_playlist {
                            navigation_state.set_current_route("home".into());
                            playlist_state.set_playlist_tracks(model(vec![]));
                            playlist_state.set_playlist_owned(false);
                        }
                        self.send("account", json!({}));
                    }
                    "playlist_track_add" | "playlist_track_delete" => {
                        if self.route == "playlist" && text(&p["pid"]) == self.active_playlist {
                            let id = self.active_playlist.clone();
                            self.page(app, "playlist", &id, false);
                        }
                        self.send("account", json!({}));
                    }
                    _ => {}
                }
                runtime_state.set_runtime_message("已保存".into());
            }
            "play" => {
                if !data.is_null() {
                    self.loading = false;
                    player_state.set_is_loading(false);
                    if let Some(notice) =
                        data["notice"].as_str().filter(|notice| !notice.is_empty())
                    {
                        runtime_state.set_runtime_message(notice.into());
                    }
                }
            }
            "finished" => {
                if data["reason"] == "ended" && !self.loading {
                    self.step(app, 1, true)
                } else if data["reason"] == "error" {
                    runtime_state.set_runtime_message("音频流播放失败，请重试或切换音质".into());
                    player_state.set_is_playing(false);
                }
            }
            "snapshot" => {
                let progress = data["progress"].as_u64().unwrap_or(0);
                let owns_snapshot = !self.resume_pending
                    && data["play_token"].as_u64().is_none_or(|token| {
                        token != 0 && token == self.tokens.get("play").copied().unwrap_or(0)
                    });
                let seek_ready = self.seek_target.is_none_or(|(target, since)| {
                    target.abs_diff(progress) <= 1000 || since.elapsed() >= Duration::from_secs(2)
                });
                if self.loading {
                    player_state.set_is_playing(false);
                    player_state.set_is_loading(true);
                } else if owns_snapshot {
                    self.paused = data["paused"] == true;
                    player_state.set_is_playing(data["playing"] == true);
                    player_state.set_is_loading(data["buffering"] == true);
                }
                audio_state.set_session_volume_locked(data["locked"] == true);
                let update_progress =
                    !self.loading && owns_snapshot && seek_ready && data["active"] != false;
                if update_progress {
                    self.seek_target = None;
                    if let Some(song) = self.queue.current().and_then(|id| self.songs.get(id)) {
                        self.position_ms = clamp_position(song, progress);
                        player_state.set_current_time_str(time(progress).into());
                        player_state.set_progress_percent(if song.duration > 0 {
                            (progress as f64 / song.duration as f64 * 100.0).clamp(0.0, 100.0)
                                as f32
                        } else {
                            0.0
                        });
                        player_state
                            .set_buffered_percent(data["buffered"].as_f64().unwrap_or(0.0) as f32);
                    }
                }
                if update_progress {
                    self.update_lyric_position(app, progress, false);
                }
            }
            "lyrics" => self.apply_lyrics(app, data),
            "comments" => {
                let b = &data["body"];
                let mut rows = vec![];
                for (key, hot) in [("hotComments", true), ("comments", false)] {
                    for (i, c) in items(&b[key]).iter().enumerate() {
                        rows.push(CommentItem {
                            id: self.entity_cover(
                                "comment",
                                &text(&c["commentId"]),
                                &text(&c["user"]["avatarUrl"]),
                            ),
                            nickname: text(&c["user"]["nickname"]).into(),
                            avatar: self.image(&text(&c["user"]["avatarUrl"])),
                            is_vip: c["user"]["vipType"].as_i64().unwrap_or(0) > 0,
                            time: text(&c["timeStr"]).into(),
                            content: text(&c["content"]).into(),
                            has_reply: !items(&c["beReplied"]).is_empty(),
                            reply_nickname: text(&c["beReplied"][0]["user"]["nickname"]).into(),
                            reply_content: text(&c["beReplied"][0]["content"]).into(),
                            liked: text(&c["likedCount"]).into(),
                            is_hot_header: hot && i == 0,
                            is_recent_header: !hot && i == 0,
                        });
                    }
                }
                player_view_state.set_comments(cover_model(rows));
            }
            "local" => {
                if data.is_null() {
                    return;
                }
                let songs: Vec<Song> =
                    serde_json::from_value(data["songs"].clone()).unwrap_or_default();
                self.local = songs.iter().map(|s| s.id.clone()).collect();
                if self.route == "local-music" {
                    self.displayed = self.local.clone();
                }
                library_state.set_local_tracks(self.tracks(songs));
                let folders: Vec<std::path::PathBuf> =
                    serde_json::from_value(data["folders"].clone()).unwrap_or_default();
                self.preferences.folders = folders.clone();
                settings_state.set_setting_lib_paths(strings(
                    folders
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                ));
                library_state.set_local_description(
                    format!(
                        "{} 首本地歌曲{}",
                        self.local.len(),
                        if data["truncated"] == true {
                            "（已达扫描深度或数量上限）"
                        } else {
                            ""
                        }
                    )
                    .into(),
                );
                if event.token > 0 {
                    runtime_state.set_runtime_message("本地曲库已更新".into());
                }
            }
            "devices" | "device" => {
                if data.is_null() {
                    return;
                }
                self.devices = items(&data).to_vec();
                audio_state.set_output_devices(model(
                    self.devices
                        .iter()
                        .map(|d| OutputDeviceItem {
                            id: text(&d["id"]).into(),
                            name: text(&d["name"]).into(),
                            current: d["current"] == true,
                        })
                        .collect(),
                ));
                let active = self
                    .devices
                    .iter()
                    .position(|d| d["current"] == true)
                    .or_else(|| self.devices.iter().position(|d| d["default"] == true));
                audio_state
                    .set_setting_output_device_index(active.map_or(-1, |index| index as i32));
                if let Some(index) = active {
                    let d = &self.devices[index];
                    audio_state.set_setting_output_device_label(text(&d["name"]).into());
                    audio_state.set_setting_supports_exclusive(text(&d["id"]).starts_with("hw:"));
                    if event.name == "device" {
                        self.preferences.device = d["id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .map(str::to_owned);
                    }
                } else {
                    audio_state.set_setting_output_device_label("无可用输出设备".into());
                    audio_state.set_setting_supports_exclusive(false);
                }
            }
            "cache" => {
                if data.is_null() {
                    return;
                }
                settings_state.set_setting_cache_details(strings(
                    [
                        ("song", "song_count"),
                        ("entity", "entity_count"),
                        ("cover", "cover_count"),
                        ("lyric", "lyric_count"),
                    ]
                    .iter()
                    .flat_map(|(size, count)| {
                        vec![
                            format!("{:.1} MB", data[*size].as_f64().unwrap_or(0.0) / 1048576.0),
                            format!("{} 个", data[*count].as_u64().unwrap_or(0)),
                        ]
                    })
                    .collect(),
                ));
                let total = data["total"].as_f64().unwrap_or(0.0);
                let max = data["max"].as_f64().unwrap_or(1.0);
                settings_state
                    .set_setting_cache_usage_percent((total / max.max(1.0) * 100.0) as f32);
                settings_state.set_setting_cache_usage_text(
                    format!("{:.1} MB / {:.2} GB", total / 1048576.0, max / 1073741824.0).into(),
                );
            }
            "qr" => {
                if let Some(message) = data["message"].as_str() {
                    login_state.set_login_qr_message(message.into());
                }
            }
            "captcha" => {
                login_state.set_login_captcha_busy(false);
                login_state.set_login_captcha_seconds(60);
                login_state.set_login_error("".into());
            }
            "download-options" => {
                download_state.set_download_qualities(model(
                    items(&data)
                        .iter()
                        .map(|q| DownloadQualityItem {
                            quality: text(&q["quality"]).into(),
                            label: text(&q["label"]).into(),
                            available: q["available"] == true,
                            reason: text(&q["reason"]).into(),
                        })
                        .collect(),
                ));
                runtime_state.set_runtime_message("".into());
            }
            "download" => {
                self.downloads.remove(&text(&data["song_id"]));
                if let Some(error) = data["error"].as_str() {
                    runtime_state.set_runtime_message(error.into());
                } else if data["cancelled"] != true {
                    runtime_state
                        .set_runtime_message(format!("已下载到 {}", text(&data["path"])).into());
                } else {
                    runtime_state.set_runtime_message("已取消下载".into());
                }
            }
            "logout" => runtime_state.set_runtime_message("已退出登录".into()),
            _ => {}
        }
    }
    fn apply_playlist_metadata(&mut self, app: &AppWindow, id: &str, p: &Value) {
        let playlist_state = app.global::<crate::PlaylistState>();
        playlist_state.set_active_playlist_id(self.ids.intern("playlist", id));
        playlist_state.set_playlist_title(text(&p["name"]).into());
        playlist_state.set_playlist_description(text(&p["description"]).into());
        playlist_state.set_playlist_cover(self.target_image("playlist", &text(&p["coverImgUrl"])));
        playlist_state.set_playlist_creator(text(&p["creator"]["nickname"]).into());
        playlist_state.set_playlist_creator_avatar(
            self.target_image("creator", &text(&p["creator"]["avatarUrl"])),
        );
        playlist_state.set_playlist_owned(
            playlist_state
                .get_created_playlists()
                .iter()
                .any(|p| self.ids.get(p.id, "playlist") == Some(id)),
        );
        playlist_state.set_playlist_special_type(p["specialType"].as_i64().unwrap_or(0) as i32);
        playlist_state.set_playlist_is_private(p["privacy"] == 10);
        let count = p["trackCount"]
            .as_u64()
            .unwrap_or(items(&p["tracks"]).len() as u64)
            .min(i32::MAX as u64) as i32;
        playlist_state.set_playlist_track_count(count);
        playlist_state.set_playlist_subscribed_count(text(&p["subscribedCount"]).into());
        playlist_state.set_playlist_meta(strings(vec![
            format!("{count} 首歌曲"),
            format!("{} 次播放", text(&p["playCount"])),
        ]));
        playlist_state.set_playlist_metadata_loading(false);
    }
    fn apply_page(&mut self, app: &AppWindow, data: &Value) {
        let search_state = app.global::<crate::SearchState>();
        let detail_state = app.global::<crate::DetailState>();
        let artist_state = app.global::<crate::ArtistState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let runtime_state = app.global::<crate::RuntimeState>();
        let b = &data["body"];
        let kind = text(&data["kind"]);
        let id = text(&data["id"]);
        let songs = match kind.as_str() {
            "search" => {
                search_state.set_search_result_count(
                    b["result"]["songCount"]
                        .as_i64()
                        .unwrap_or(0)
                        .min(i32::MAX as i64) as i32,
                );
                self.api_songs(&b["result"]["songs"])
            }
            "playlist" => {
                let p = &b["playlist"];
                self.apply_playlist_metadata(app, &id, p);
                self.api_songs(&p["tracks"])
            }
            "album" => {
                let a = &b["album"];
                detail_state.set_album_title(text(&a["name"]).into());
                detail_state.set_album_description(text(&a["description"]).into());
                detail_state.set_album_cover(self.target_image("album", &text(&a["picUrl"])));
                detail_state.set_album_artist(text(&a["artist"]["name"]).into());
                self.album_artist = text(&a["artist"]["id"]);
                detail_state.set_album_meta(strings(vec![
                    format!("{} 首歌曲", items(&b["songs"]).len()),
                    text(&a["company"]),
                ]));
                self.api_songs(&b["songs"])
            }
            "favorites" => {
                self.likes = items(&b["ids"]).iter().map(text).collect();
                self.api_songs(&b["songs"])
            }
            "artist" => {
                let a = &b["detail"]["data"]["artist"];
                artist_state.set_artist_loading(false);
                if !a.is_object() {
                    artist_state.set_artist_load_error("未获取到歌手详情".into());
                    return;
                }
                artist_state.set_artist_load_error("".into());
                artist_state.set_artist_name(a["name"].as_str().unwrap_or("未知歌手").into());
                let avatar = text(&a["avatar"]);
                artist_state.set_artist_avatar(self.target_image(
                    "artist",
                    &if avatar.is_empty() {
                        text(&a["cover"])
                    } else {
                        avatar
                    },
                ));
                let mut raw_albums: Vec<_> = items(&b["album"]["hotAlbums"]).iter().collect();
                raw_albums
                    .sort_by_key(|a| std::cmp::Reverse(a["publishTime"].as_i64().unwrap_or(0)));
                let albums: Vec<_> = raw_albums
                    .iter()
                    .map(|a| ArtistReleaseItem {
                        id: self.entity_cover("album", &text(&a["id"]), &text(&a["picUrl"])),
                        title: text(&a["name"]).into(),
                        kind: if artist_is_single(a) {
                            "EP / Single"
                        } else {
                            "Album"
                        }
                        .into(),
                        year: artist_release_year(&a["publishTime"]).into(),
                        cover: self.image(&text(&a["picUrl"])),
                    })
                    .collect();
                let eps: Vec<_> = albums
                    .iter()
                    .filter(|a| a.kind == "EP / Single")
                    .cloned()
                    .collect();
                let full_albums: Vec<_> = albums
                    .iter()
                    .filter(|a| a.kind == "Album")
                    .cloned()
                    .collect();
                let mvs: Vec<_> = items(&b["mv"]["mvs"])
                    .iter()
                    .map(|m| {
                        let wide = text(&m["imgurl16v9"]);
                        let cover = if wide.is_empty() {
                            text(&m["imgurl"])
                        } else {
                            wide
                        };
                        ArtistReleaseItem {
                            id: self.entity_cover("mv", &text(&m["id"]), &cover),
                            title: text(&m["name"]).into(),
                            kind: "Music Video".into(),
                            year: artist_release_year(&m["publishTime"]).into(),
                            cover: self.image(&cover),
                        }
                    })
                    .collect();
                artist_state.set_artist_pills(strings(vec![
                    format!(
                        "{} 张专辑",
                        a["albumSize"].as_u64().unwrap_or(full_albums.len() as u64)
                    ),
                    format!("{} 张 EP / 单曲", eps.len()),
                    format!("{} 支 MV", a["mvSize"].as_u64().unwrap_or(mvs.len() as u64)),
                ]));
                let songs = self.api_songs(&b["top"]["songs"]);
                let summary = text(&a["briefDesc"]);
                artist_state.set_artist_summary(
                    if !summary.trim().is_empty() {
                        summary.trim().to_owned()
                    } else if let Some(latest) = albums.first() {
                        format!(
                            "最新发布《{}》，当前已收录 {} 首热门歌曲。",
                            latest.title,
                            songs.len()
                        )
                    } else {
                        format!(
                            "当前已收录 {} 首热门歌曲、{} 张专辑。",
                            songs.len(),
                            full_albums.len()
                        )
                    }
                    .into(),
                );
                artist_state.set_artist_latest(cover_model(
                    albums
                        .iter()
                        .take(2)
                        .cloned()
                        .map(|mut a| {
                            if a.kind == "EP / Single" {
                                a.kind = "Single / EP".into();
                            }
                            a
                        })
                        .collect(),
                ));
                artist_state.set_artist_albums(cover_model(full_albums));
                artist_state.set_artist_eps(cover_model(eps));
                artist_state.set_artist_mvs(cover_model(mvs));
                artist_state.set_artist_top_songs(cover_model(
                    songs
                        .iter()
                        .enumerate()
                        .map(|(i, song)| ArtistSongItem {
                            id: self.ids.intern("song", &song.id),
                            index_str: format!("{:02}", i + 1).into(),
                            title: song.name.clone().into(),
                            album: song.album.clone().into(),
                            duration: time(song.duration).into(),
                            cover: self.image(&song.cover),
                            badge: match i {
                                0 => "hot",
                                1 => "rec",
                                _ => "",
                            }
                            .into(),
                        })
                        .collect(),
                ));
                songs
            }
            "cloud" => items(&b["data"])
                .iter()
                .map(|s| Song::from_api(&s["simpleSong"]))
                .collect(),
            _ => vec![],
        };
        let mut songs = songs;
        if kind == "search" && data["offset"].as_u64().unwrap_or(0) > 0 {
            let mut previous: Vec<_> = self
                .displayed
                .iter()
                .filter_map(|id| self.songs.get(id).cloned())
                .collect();
            previous.extend(songs);
            songs = previous;
        }
        self.displayed = songs.iter().map(|s| s.id.clone()).collect();
        let tracks = self.tracks(songs);
        if kind == "playlist" {
            playlist_state.set_playlist_tracks(tracks);
            playlist_state.set_playlist_loading(false);
            playlist_state.set_playlist_load_error("".into());
        } else {
            detail_state.set_detail_tracks(tracks)
        }
        runtime_state.set_runtime_message("".into());
        if data["autoplay"] == true {
            if let Some(id) = self.displayed.first().cloned() {
                self.play(app, id, true)
            }
        }
    }
    pub fn save_preferences_if_changed(&mut self, app: &AppWindow) {
        let settings_state = app.global::<crate::SettingsState>();
        let audio_state = app.global::<crate::AudioState>();
        let player_state = app.global::<crate::PlayerState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        let theme = app.global::<Theme>();
        self.preferences.renderer =
            crate::rendering::Renderer::from_name(&settings_state.get_setting_renderer());
        self.preferences.quality = audio_state.get_setting_sound_quality().into();
        self.preferences.strict = audio_state.get_setting_strict_bitperfect();
        self.preferences.exclusive = audio_state.get_setting_exclusive_mode();
        self.preferences.software_volume = audio_state.get_setting_software_volume();
        self.preferences.volume = audio_state.get_volume_percent() as f64 / 100.0;
        self.preferences.cache_limit_mb =
            settings_state.get_setting_cache_limit_mb().max(64.0) as u64;
        self.preferences.cache_ahead_mb =
            settings_state.get_setting_cache_ahead_mb().max(1.0) as u64;
        let aux = |model: ModelRc<AuxButtonItem>| {
            model
                .iter()
                .map(|a| json!({"id":a.id.as_str(),"visible":a.visible}))
                .collect::<Vec<_>>()
        };
        self.preferences.visual = json!({
            "color": theme.get_theme_color().as_argb_encoded(),
            "soft": theme.get_theme_color_soft().as_argb_encoded(),
            "muted": theme.get_theme_color_muted().as_argb_encoded(),
            "strong": theme.get_theme_color_strong().as_argb_encoded(),
            "tint": theme.get_theme_color_tint().as_argb_encoded(),
            "play_mode": player_state.get_play_mode().as_str(),
            "pronunciation": player_view_state.get_show_pronunciation(),
            "translation": player_view_state.get_show_translation(),
            "dark": theme.get_is_dark(),
            "mode": theme.get_theme_mode().as_str(),
            "accent": settings_state.get_setting_accent_hex().as_str(),
            "acrylic": settings_state.get_setting_acrylic(),
            "lyric_debug": settings_state.get_setting_lyric_debug(),
            "auto_launch": settings_state.get_setting_auto_launch(),
            "left": aux(settings_state.get_fullscreen_aux_left()),
            "right": aux(settings_state.get_fullscreen_aux_right()),
        });
        let next = serde_json::to_value(&self.preferences).unwrap();
        if next != self.last_preferences {
            self.last_preferences = next.clone();
            self.send("preferences", next);
        }
    }
}
pub fn initialize(app: &AppWindow, prefs: &Preferences) {
    crate::detail_search::attach(app);
    let account_state = app.global::<crate::AccountState>();
    let player_state = app.global::<crate::PlayerState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let search_state = app.global::<crate::SearchState>();
    let home_state = app.global::<crate::HomeState>();
    let detail_state = app.global::<crate::DetailState>();
    let library_state = app.global::<crate::LibraryState>();
    let queue_state = app.global::<crate::QueueState>();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let artist_state = app.global::<crate::ArtistState>();
    let settings_state = app.global::<crate::SettingsState>();
    let audio_state = app.global::<crate::AudioState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    account_state.set_is_logged_in(false);
    account_state.set_user_nickname("未登录".into());
    account_state.set_user_avatar(slint::Image::default());
    player_state.set_is_playing(false);
    player_state.set_is_loading(false);
    player_state.set_current_song_id(-1);
    player_state.set_current_song_title("未在播放".into());
    player_state.set_current_song_artist("".into());
    player_state.set_current_song_cover(slint::Image::default());
    crate::fullscreen_palette::apply(&player_view_state, None);
    player_state.set_current_song_favorite(false);
    player_state.set_progress_percent(0.0);
    player_state.set_buffered_percent(0.0);
    player_state.set_current_time_str("0:00".into());
    player_state.set_duration_time_str("0:00".into());
    playlist_state.set_playlist_owned(false);
    playlist_state.set_playlist_loading(false);
    playlist_state.set_playlist_metadata_loading(false);
    playlist_state.set_playlist_load_error("".into());
    playlist_state.set_active_playlist_id(-1);
    playlist_state.set_playlist_title("".into());
    playlist_state.set_playlist_description("".into());
    playlist_state.set_playlist_meta(model(vec![]));
    search_state.set_search_result_count(0);
    home_state.set_home_playlists_loading(true);
    home_state.set_home_songs_loading(true);
    playlist_state.set_created_playlists(model(vec![]));
    home_state.set_recommend_playlists(cover_model(vec![]));
    home_state.set_daily_songs(cover_model(vec![]));
    home_state.set_home_new_albums(cover_model(vec![]));
    home_state.set_home_charts(cover_model(vec![]));
    home_state.set_home_albums_loaded(false);
    home_state.set_home_charts_loaded(false);
    home_state.set_home_albums_loading(false);
    home_state.set_home_charts_loading(false);
    home_state.set_home_albums_error("".into());
    home_state.set_home_charts_error("".into());
    search_state.set_browse_categories(model(vec![]));
    detail_state.set_detail_tracks(model(vec![]));
    playlist_state.set_playlist_tracks(model(vec![]));
    library_state.set_local_tracks(model(vec![]));
    queue_state.set_queue_songs(cover_model(vec![]));
    player_view_state.set_lyrics(model(vec![]));
    player_view_state.set_comments(cover_model(vec![]));
    artist_state.set_artist_latest(cover_model(vec![]));
    artist_state.set_artist_albums(cover_model(vec![]));
    artist_state.set_artist_eps(cover_model(vec![]));
    artist_state.set_artist_mvs(cover_model(vec![]));
    artist_state.set_artist_top_songs(cover_model(vec![]));
    player_view_state.set_active_lyric_index(-1);
    settings_state.set_setting_renderer(prefs.renderer.name().into());
    settings_state.set_startup_renderer(prefs.renderer.name().into());
    audio_state.set_setting_sound_quality(prefs.quality.clone().into());
    audio_state.set_setting_audio_engine("native".into());
    audio_state.set_setting_exclusive_mode(prefs.exclusive);
    audio_state.set_setting_strict_bitperfect(prefs.strict);
    audio_state.set_setting_software_volume(prefs.software_volume);
    audio_state.set_volume_percent((prefs.volume * 100.0) as f32);
    audio_state.set_output_devices(model(vec![]));
    audio_state.set_setting_output_device_index(-1);
    audio_state.set_setting_output_device_label("正在检测输出设备…".into());
    audio_state.set_setting_supports_exclusive(false);
    settings_state.set_setting_cache_limit_mb(prefs.cache_limit_mb as f32);
    settings_state.set_setting_cache_ahead_mb(prefs.cache_ahead_mb as f32);
    settings_state.set_setting_cache_usage_percent(0.0);
    settings_state.set_setting_cache_usage_text("正在读取…".into());
    settings_state.set_setting_lib_paths(strings(
        prefs
            .folders
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    ));
    settings_state.set_setting_tray_minimize(false);
    let v = &prefs.visual;
    let theme = app.global::<Theme>();
    if let Some(d) = v["dark"].as_bool() {
        theme.set_is_dark(d)
    }
    if let Some(m) = v["mode"].as_str() {
        theme.set_theme_mode(m.into())
    }
    if let Some(a) = v["acrylic"].as_bool() {
        settings_state.set_setting_acrylic(a)
    }
    settings_state.set_setting_auto_launch(v["auto_launch"] == true);
    settings_state.set_setting_lyric_debug(v["lyric_debug"] == true);
    if let Some(hex) = v["accent"].as_str() {
        settings_state.set_setting_accent_hex(hex.into());
        if let Some(hex) = hex
            .strip_prefix('#')
            .and_then(|h| u32::from_str_radix(h, 16).ok())
        {
            theme.set_theme_color(slint::Color::from_rgb_u8(
                (hex >> 16) as u8,
                (hex >> 8) as u8,
                hex as u8,
            ));
        }
    }
    macro_rules! color {
        ($key:literal,$setter:ident) => {
            if let Some(c) = v[$key].as_u64() {
                theme.$setter(slint::Color::from_argb_encoded(c as u32));
            }
        };
    }
    color!("color", set_theme_color);
    color!("soft", set_theme_color_soft);
    color!("muted", set_theme_color_muted);
    color!("strong", set_theme_color_strong);
    color!("tint", set_theme_color_tint);
    // Keep the saved HEX value and all theme variants consistent, including
    // older preferences whose color picker only updated the primary color.
    crate::appearance::apply_accent(&settings_state, &theme, theme.get_theme_color());
    if let Some(mode) = v["play_mode"]
        .as_str()
        .filter(|m| ["loop", "single", "random"].contains(m))
    {
        player_state.set_play_mode(mode.into());
    }
    if let Some(show) = v["pronunciation"].as_bool() {
        player_view_state.set_show_pronunciation(show)
    }
    if let Some(show) = v["translation"].as_bool() {
        player_view_state.set_show_translation(show)
    }
    let aux = |key: &str| {
        model(
            items(&v[key])
                .iter()
                .map(|a| AuxButtonItem {
                    id: text(&a["id"]).into(),
                    visible: a["visible"] == true,
                })
                .collect(),
        )
    };
    if v["left"].is_array() {
        settings_state.set_fullscreen_aux_left(aux("left"));
    }
    if v["right"].is_array() {
        settings_state.set_fullscreen_aux_right(aux("right"));
    }
    runtime_state.set_runtime_enabled(true);
}
