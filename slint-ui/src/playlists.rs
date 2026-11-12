//! Offline playlist interactions for --demo and visual regression tests.
//! The production controller in runtime/view.rs commits edits through MusicApi.
mod queue;
use crate::{AppWindow, NavPlaylistItem, TrackItem};
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
    fn refresh_sidebar(&self, state: &crate::PlaylistState<'_>) {
        state.set_created_playlists(model(
            self.playlists.iter().map(|p| p.nav.clone()).collect(),
        ));
    }

    fn open(&mut self, app: &AppWindow, id: i32) -> bool {
        let playlist_state = app.global::<crate::PlaylistState>();
        let detail_state = app.global::<crate::DetailState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let Some(playlist) = self.playlists.iter().find(|p| p.nav.id == id) else {
            return self.open_recommendation(app, id);
        };
        self.editing_id = None;
        playlist_state.set_playlist_editor_mode("".into());
        detail_state.set_detail_search_query("".into());
        playlist_state.set_active_playlist_id(id);
        playlist_state.set_playlist_title(playlist.nav.name.clone());
        playlist_state.set_playlist_description(playlist.description.clone());
        playlist_state.set_playlist_cover(playlist.nav.cover.clone());
        playlist_state.set_playlist_owned(true);
        playlist_state.set_playlist_special_type(0);
        playlist_state.set_playlist_is_private(playlist.private);
        playlist_state.set_playlist_track_count(playlist.nav.track_count);
        playlist_state.set_playlist_subscribed_count(playlist.subscribers.clone());
        playlist_state.set_playlist_tracks(model(playlist.tracks.clone()));
        let mut meta = vec![
            format!("{} 首歌曲", playlist.nav.track_count).into(),
            format!("{} 次播放", playlist.nav.play_count).into(),
        ];
        if !playlist.created_label.is_empty() {
            meta.insert(0, playlist.created_label.clone());
        }
        playlist_state.set_playlist_meta(model(meta));
        navigation_state.set_current_route("playlist".into());
        true
    }

    fn open_recommendation(&self, app: &AppWindow, id: i32) -> bool {
        let home_state = app.global::<crate::HomeState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let detail_state = app.global::<crate::DetailState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let Some(item) = home_state
            .get_recommend_playlists()
            .iter()
            .find(|item| item.id == id)
            .or_else(|| {
                home_state
                    .get_home_charts()
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| crate::RecommendPlaylistItem {
                        id: item.id,
                        title: item.title,
                        cover: item.cover,
                        desc: item.update,
                    })
            })
        else {
            return false;
        };
        playlist_state.set_active_playlist_id(item.id);
        playlist_state.set_playlist_title(item.title);
        playlist_state.set_playlist_cover(item.cover);
        playlist_state.set_playlist_description(item.desc);
        playlist_state.set_playlist_owned(false);
        playlist_state.set_playlist_tracks(detail_state.get_detail_tracks());
        navigation_state.set_current_route("playlist".into());
        true
    }

    fn create(&mut self, app: &AppWindow) {
        let account_state = app.global::<crate::AccountState>();
        let login_state = app.global::<crate::LoginState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        if !account_state.get_is_logged_in() {
            login_state.set_is_login_open(true);
            return;
        }
        self.editing_id = None;
        playlist_state.set_playlist_editor_name("".into());
        playlist_state.set_playlist_editor_description("".into());
        playlist_state.set_playlist_editor_private(false);
        playlist_state.set_playlist_editor_error("".into());
        playlist_state.set_playlist_editor_mode("create".into());
        self.validate(app);
    }

    fn edit(&mut self, app: &AppWindow) {
        let playlist_state = app.global::<crate::PlaylistState>();
        let navigation_state = app.global::<crate::NavigationState>();
        if !playlist_state.get_playlist_can_manage()
            || navigation_state.get_current_route() != "playlist"
        {
            return;
        }
        let Some(playlist) = self
            .playlists
            .iter()
            .find(|p| p.nav.id == playlist_state.get_active_playlist_id())
        else {
            return;
        };
        self.editing_id = Some(playlist.nav.id);
        playlist_state.set_playlist_editor_name(playlist.nav.name.clone());
        playlist_state.set_playlist_editor_description(playlist.description.clone());
        playlist_state.set_playlist_editor_private(playlist.private);
        playlist_state.set_playlist_editor_error("".into());
        playlist_state.set_playlist_editor_mode("edit".into());
        self.validate(app);
    }

    fn close(&mut self, app: &AppWindow) {
        self.editing_id = None;
        let playlist_state = app.global::<crate::PlaylistState>();
        playlist_state.set_playlist_restore_focus(
            if playlist_state.get_playlist_editor_mode() == "create" {
                1
            } else {
                2
            },
        );
        playlist_state.set_playlist_editor_mode("".into());
        playlist_state.set_playlist_editor_can_submit(false);
        playlist_state.set_playlist_editor_error("".into());
    }

    fn validate(&mut self, app: &AppWindow) {
        let playlist_state = app.global::<crate::PlaylistState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let account_state = app.global::<crate::AccountState>();
        let name = limit_text(&playlist_state.get_playlist_editor_name(), MAX_NAME);
        let description = limit_text(
            &playlist_state.get_playlist_editor_description(),
            MAX_DESCRIPTION,
        );
        playlist_state.set_playlist_editor_name(name.clone().into());
        playlist_state.set_playlist_editor_description(description.clone().into());
        playlist_state.set_playlist_editor_name_length(name.encode_utf16().count() as i32);
        playlist_state
            .set_playlist_editor_description_length(description.encode_utf16().count() as i32);
        let name = normalize_name(&name);
        let changed = match self.editing_id {
            Some(id) => {
                playlist_state.get_playlist_editor_mode() == "edit"
                    && playlist_state.get_playlist_can_manage()
                    && playlist_state.get_active_playlist_id() == id
                    && navigation_state.get_current_route() == "playlist"
                    && self.playlists.iter().any(|p| {
                        p.nav.id == id
                            && (p.nav.name != name || p.description != description.trim())
                    })
            }
            None => playlist_state.get_playlist_editor_mode() == "create",
        };
        playlist_state.set_playlist_editor_can_submit(
            account_state.get_is_logged_in() && !name.is_empty() && changed,
        );
    }

    fn submit(&mut self, app: &AppWindow) {
        self.validate(app);
        let playlist_state = app.global::<crate::PlaylistState>();
        let home_state = app.global::<crate::HomeState>();
        if !playlist_state.get_playlist_editor_can_submit() {
            return;
        }
        let name: SharedString = normalize_name(&playlist_state.get_playlist_editor_name()).into();
        let description: SharedString = playlist_state
            .get_playlist_editor_description()
            .trim()
            .into();
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
                private: playlist_state.get_playlist_editor_private(),
                subscribers: "0".into(),
                created_label: "刚刚创建".into(),
            });
            id
        };
        self.close(app);
        self.refresh_sidebar(&playlist_state);
        // Keep cards already on the home page in sync with metadata changes.
        let cards = home_state
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
        home_state.set_recommend_playlists(model(cards));
        self.open(app, id);
    }

    fn request_delete(&mut self, app: &AppWindow) {
        let playlist_state = app.global::<crate::PlaylistState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let dialog_state = app.global::<crate::DialogState>();
        if !playlist_state.get_playlist_can_manage()
            || navigation_state.get_current_route() != "playlist"
        {
            return;
        }
        let Some(playlist) = self
            .playlists
            .iter()
            .find(|p| p.nav.id == playlist_state.get_active_playlist_id())
        else {
            return;
        };
        dialog_state.set_playlist_delete_id(playlist.nav.id);
        dialog_state.set_dialog_title("删除歌单".into());
        dialog_state.set_dialog_message(
            format!("确定删除「{}」吗？删除后无法恢复。", playlist.nav.name).into(),
        );
        dialog_state.set_dialog_mode("confirm-cancel".into());
        dialog_state.set_dialog_confirm_text("删除".into());
        dialog_state.set_dialog_cancel_text("取消".into());
        dialog_state.set_is_dialog_open(true);
    }

    fn confirm_delete(&mut self, app: &AppWindow) {
        let dialog_state = app.global::<crate::DialogState>();
        let playlist_state = app.global::<crate::PlaylistState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let home_state = app.global::<crate::HomeState>();
        let id = dialog_state.get_playlist_delete_id();
        if !dialog_state.get_is_dialog_open()
            || !playlist_state.get_playlist_can_manage()
            || navigation_state.get_current_route() != "playlist"
            || playlist_state.get_active_playlist_id() != id
        {
            return;
        }
        self.playlists.retain(|p| p.nav.id != id);
        self.refresh_sidebar(&playlist_state);
        home_state.set_recommend_playlists(model(
            home_state
                .get_recommend_playlists()
                .iter()
                .filter(|p| p.id != id)
                .collect(),
        ));
        self.close(app);
        dialog_state.set_playlist_delete_id(-1);
        dialog_state.set_is_dialog_open(false);
        playlist_state.set_playlist_tracks(model(Vec::new()));
        playlist_state.set_active_playlist_id(-1);
        playlist_state.set_playlist_owned(false);
        navigation_state.set_current_route("home".into());
    }
}

pub fn attach(app: &AppWindow) {
    crate::detail_search::attach(app);
    queue::attach(app);
    let library_state = app.global::<crate::LibraryState>();
    let detail_state = app.global::<crate::DetailState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let weak = app.as_weak();
    library_state.on_remove_local_track(move |id| {
        if let Some(app) = weak.upgrade() {
            remove_local_track(&app.global::<crate::LibraryState>(), id);
        }
    });
    let weak = app.as_weak();
    library_state.on_clear_local_library(move || {
        if let Some(app) = weak.upgrade() {
            clear_local_library(&app.global::<crate::LibraryState>());
        }
    });
    let tracks: Vec<_> = detail_state.get_detail_tracks().iter().collect();
    let library = Rc::new(RefCell::new(Library {
        next_id: playlist_state
            .get_created_playlists()
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0)
            + 1,
        editing_id: None,
        playlists: playlist_state
            .get_created_playlists()
            .iter()
            .map(|nav| Playlist {
                description: if nav.id == playlist_state.get_active_playlist_id() {
                    playlist_state.get_playlist_description()
                } else {
                    "".into()
                },
                subscribers: if nav.id == playlist_state.get_active_playlist_id() {
                    playlist_state.get_playlist_subscribed_count()
                } else {
                    "0".into()
                },
                created_label: playlist_state
                    .get_playlist_meta()
                    .row_data(0)
                    .unwrap_or_default(),
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
            playlist_state.$callback(move || {
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
    playlist_state.on_open_playlist(move |id| {
        weak.upgrade()
            .is_some_and(|app| library.borrow_mut().open(&app, id))
    });
}

fn remove_local_track(state: &crate::LibraryState<'_>, id: i32) {
    let tracks = state
        .get_local_tracks()
        .iter()
        .filter(|track| track.id != id)
        .enumerate()
        .map(|(index, mut track)| {
            track.index_str = format!("{:02}", index + 1).into();
            track
        })
        .collect();
    state.set_local_tracks(model(tracks));
}

fn clear_local_library(state: &crate::LibraryState<'_>) {
    state.set_local_tracks(model(Vec::new()));
}
