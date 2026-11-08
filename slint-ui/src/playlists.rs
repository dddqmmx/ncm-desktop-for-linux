//! Offline playlist interactions for --demo and visual regression tests.
//! The production controller in runtime/view.rs commits edits through MusicApi.
use crate::{AppState, AppWindow, NavPlaylistItem, TrackItem};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};

const MAX_NAME: usize = 40;
const MAX_DESCRIPTION: usize = 1000;

// DOM maxlength and JS string.length count UTF-16 code units. Preserve whole
// Unicode scalars at the boundary, so pasted emoji cannot become invalid text.
pub(crate) fn limit_text(value: &str, max: usize) -> String {
    let mut length = 0;
    value
        .chars()
        .take_while(|ch| {
            length += ch.len_utf16();
            length <= max
        })
        .collect()
}

pub(crate) fn normalize_name(value: &str) -> String {
    let collapsed = value
        .split(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    limit_text(&collapsed, MAX_NAME)
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    Rc::new(VecModel::from(items)).into()
}

struct Playlist {
    nav: NavPlaylistItem,
    description: SharedString,
    tracks: Vec<TrackItem>,
    private: bool,
    subscribers: SharedString,
    created_label: SharedString,
}

struct Library {
    playlists: Vec<Playlist>,
    editing_id: Option<i32>,
    next_id: i32,
}

impl Library {
    fn refresh_sidebar(&self, state: &AppState<'_>) {
        state.set_created_playlists(model(
            self.playlists.iter().map(|p| p.nav.clone()).collect(),
        ));
    }

    fn open(&mut self, app: &AppWindow, id: i32) -> bool {
        let state = app.global::<AppState>();
        let Some(playlist) = self.playlists.iter().find(|p| p.nav.id == id) else {
            return false;
        };
        self.editing_id = None;
        state.set_playlist_editor_mode("".into());
        state.set_detail_search_query("".into());
        state.set_active_playlist_id(id);
        state.set_playlist_title(playlist.nav.name.clone());
        state.set_playlist_description(playlist.description.clone());
        state.set_playlist_cover(playlist.nav.cover.clone());
        state.set_playlist_owned(true);
        state.set_playlist_special_type(0);
        state.set_playlist_is_private(playlist.private);
        state.set_playlist_track_count(playlist.nav.track_count);
        state.set_playlist_subscribed_count(playlist.subscribers.clone());
        state.set_playlist_tracks(model(playlist.tracks.clone()));
        let mut meta = vec![
            format!("{} 首歌曲", playlist.nav.track_count).into(),
            format!("{} 次播放", playlist.nav.play_count).into(),
        ];
        if !playlist.created_label.is_empty() {
            meta.insert(0, playlist.created_label.clone());
        }
        state.set_playlist_meta(model(meta));
        state.set_current_route("playlist".into());
        true
    }

    fn create(&mut self, app: &AppWindow) {
        let state = app.global::<AppState>();
        if !state.get_is_logged_in() {
            state.set_is_login_open(true);
            return;
        }
        self.editing_id = None;
        state.set_playlist_editor_name("".into());
        state.set_playlist_editor_description("".into());
        state.set_playlist_editor_private(false);
        state.set_playlist_editor_error("".into());
        state.set_playlist_editor_mode("create".into());
        self.validate(app);
    }

    fn edit(&mut self, app: &AppWindow) {
        let state = app.global::<AppState>();
        if !state.get_playlist_can_manage() || state.get_current_route() != "playlist" {
            return;
        }
        let Some(playlist) = self
            .playlists
            .iter()
            .find(|p| p.nav.id == state.get_active_playlist_id())
        else {
            return;
        };
        self.editing_id = Some(playlist.nav.id);
        state.set_playlist_editor_name(playlist.nav.name.clone());
        state.set_playlist_editor_description(playlist.description.clone());
        state.set_playlist_editor_private(playlist.private);
        state.set_playlist_editor_error("".into());
        state.set_playlist_editor_mode("edit".into());
        self.validate(app);
    }

    fn close(&mut self, app: &AppWindow) {
        self.editing_id = None;
        let state = app.global::<AppState>();
        state.set_playlist_restore_focus(if state.get_playlist_editor_mode() == "create" {
            1
        } else {
            2
        });
        state.set_playlist_editor_mode("".into());
        state.set_playlist_editor_can_submit(false);
        state.set_playlist_editor_error("".into());
    }

    fn validate(&mut self, app: &AppWindow) {
        let state = app.global::<AppState>();
        let name = limit_text(&state.get_playlist_editor_name(), MAX_NAME);
        let description = limit_text(&state.get_playlist_editor_description(), MAX_DESCRIPTION);
        state.set_playlist_editor_name(name.clone().into());
        state.set_playlist_editor_description(description.clone().into());
        state.set_playlist_editor_name_length(name.encode_utf16().count() as i32);
        state.set_playlist_editor_description_length(description.encode_utf16().count() as i32);
        let name = normalize_name(&name);
        let changed = match self.editing_id {
            Some(id) => {
                state.get_playlist_editor_mode() == "edit"
                    && state.get_playlist_can_manage()
                    && state.get_active_playlist_id() == id
                    && state.get_current_route() == "playlist"
                    && self.playlists.iter().any(|p| {
                        p.nav.id == id
                            && (p.nav.name != name || p.description != description.trim())
                    })
            }
            None => state.get_playlist_editor_mode() == "create",
        };
        state.set_playlist_editor_can_submit(
            state.get_is_logged_in() && !name.is_empty() && changed,
        );
    }

    fn submit(&mut self, app: &AppWindow) {
        self.validate(app);
        let state = app.global::<AppState>();
        if !state.get_playlist_editor_can_submit() {
            return;
        }
        let name: SharedString = normalize_name(&state.get_playlist_editor_name()).into();
        let description: SharedString = state.get_playlist_editor_description().trim().into();
        let id = if let Some(id) = self.editing_id {
            let Some(playlist) = self.playlists.iter_mut().find(|p| p.nav.id == id) else {
                return;
            };
            playlist.nav.name = name;
            playlist.description = description;
            id
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.playlists.push(Playlist {
                nav: NavPlaylistItem {
                    id,
                    name,
                    play_count: "0".into(),
                    ..Default::default()
                },
                description,
                tracks: Vec::new(),
                private: state.get_playlist_editor_private(),
                subscribers: "0".into(),
                created_label: "刚刚创建".into(),
            });
            id
        };
        self.close(app);
        self.refresh_sidebar(&state);
        // Keep cards already on the home page in sync with metadata changes.
        let cards = state
            .get_recommend_playlists()
            .iter()
            .map(|mut card| {
                if card.id == id {
                    if let Some(p) = self.playlists.iter().find(|p| p.nav.id == id) {
                        card.title = p.nav.name.clone();
                    }
                }
                card
            })
            .collect();
        state.set_recommend_playlists(model(cards));
        self.open(app, id);
    }

    fn request_delete(&mut self, app: &AppWindow) {
        let state = app.global::<AppState>();
        if !state.get_playlist_can_manage() || state.get_current_route() != "playlist" {
            return;
        }
        let Some(playlist) = self
            .playlists
            .iter()
            .find(|p| p.nav.id == state.get_active_playlist_id())
        else {
            return;
        };
        state.set_playlist_delete_id(playlist.nav.id);
        state.set_dialog_title("删除歌单".into());
        state.set_dialog_message(
            format!("确定删除「{}」吗？删除后无法恢复。", playlist.nav.name).into(),
        );
        state.set_dialog_mode("confirm-cancel".into());
        state.set_dialog_confirm_text("删除".into());
        state.set_dialog_cancel_text("取消".into());
        state.set_is_dialog_open(true);
    }

    fn confirm_delete(&mut self, app: &AppWindow) {
        let state = app.global::<AppState>();
        let id = state.get_playlist_delete_id();
        if !state.get_is_dialog_open()
            || !state.get_playlist_can_manage()
            || state.get_current_route() != "playlist"
            || state.get_active_playlist_id() != id
        {
            return;
        }
        self.playlists.retain(|p| p.nav.id != id);
        self.refresh_sidebar(&state);
        state.set_recommend_playlists(model(
            state
                .get_recommend_playlists()
                .iter()
                .filter(|p| p.id != id)
                .collect(),
        ));
        self.close(app);
        state.set_playlist_delete_id(-1);
        state.set_is_dialog_open(false);
        state.set_playlist_tracks(model(Vec::new()));
        state.set_active_playlist_id(-1);
        state.set_playlist_owned(false);
        state.set_current_route("home".into());
    }
}

pub fn attach(app: &AppWindow) {
    let state = app.global::<AppState>();
    let tracks: Vec<_> = state.get_detail_tracks().iter().collect();
    let library = Rc::new(RefCell::new(Library {
        next_id: state
            .get_created_playlists()
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0)
            + 1,
        editing_id: None,
        playlists: state
            .get_created_playlists()
            .iter()
            .map(|nav| Playlist {
                description: if nav.id == state.get_active_playlist_id() {
                    state.get_playlist_description()
                } else {
                    "".into()
                },
                subscribers: if nav.id == state.get_active_playlist_id() {
                    state.get_playlist_subscribed_count()
                } else {
                    "0".into()
                },
                created_label: state.get_playlist_meta().row_data(0).unwrap_or_default(),
                nav,
                tracks: tracks.clone(),
                private: false,
            })
            .collect(),
    }));
    macro_rules! bind {
        ($callback:ident, $method:ident) => {{
            let weak = app.as_weak();
            let library = library.clone();
            state.$callback(move || {
                if let Some(app) = weak.upgrade() {
                    library.borrow_mut().$method(&app);
                }
            });
        }};
    }
    bind!(on_open_create_playlist, create);
    bind!(on_open_edit_playlist, edit);
    bind!(on_close_playlist_editor, close);
    bind!(on_playlist_input_changed, validate);
    bind!(on_submit_playlist, submit);
    bind!(on_request_delete_playlist, request_delete);
    bind!(on_confirm_playlist_delete, confirm_delete);
    let weak = app.as_weak();
    state.on_open_playlist(move |id| {
        weak.upgrade()
            .is_some_and(|app| library.borrow_mut().open(&app, id))
    });
}
