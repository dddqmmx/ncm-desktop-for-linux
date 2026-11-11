use super::View;
use crate::AppWindow;
use serde_json::json;
use slint::ComponentHandle;

impl View {
    pub(super) fn toggle_song_favorite(&mut self, app: &AppWindow, handle: i32) {
        let account_state = app.global::<crate::AccountState>();
        let login_state = app.global::<crate::LoginState>();
        if !account_state.get_is_logged_in() {
            login_state.set_is_login_open(true);
            return;
        }
        if let Some(id) = self.handle_for_song(handle) {
            if self.songs.get(&id).is_some_and(|song| song.path.is_none())
                && self.pending_likes.insert(id.clone())
            {
                self.send_mutation("like", json!({"id": id, "like": !self.likes.contains(&id)}));
            }
        }
    }

    pub(super) fn add_song_to_playlist(&mut self, playlist: i32, song: i32) {
        if let (Some(pid), Some(song)) = (
            self.ids.get(playlist, "playlist").map(str::to_owned),
            self.handle_for_song(song),
        ) {
            self.send_mutation("playlist_track_add", json!({"pid": pid, "tracks": [song]}));
        }
    }

    pub(super) fn remove_song_from_playlist(&mut self, app: &AppWindow, handle: i32) {
        if !app
            .global::<crate::PlaylistState>()
            .get_playlist_can_manage()
        {
            return;
        }
        if let Some(song) = self.handle_for_song(handle) {
            self.send_mutation(
                "playlist_track_delete",
                json!({"pid": self.active_playlist, "tracks": [song]}),
            );
        }
    }

    pub(super) fn remove_local_song(&mut self, app: &AppWindow, handle: i32) {
        if let Some(id) = self.handle_for_song(handle) {
            if self.local.contains(&id) {
                app.global::<crate::RuntimeState>()
                    .set_runtime_message("正在移除本地歌曲…".into());
                self.send("local", json!({"action": "remove-track", "id": id}));
            }
        }
    }
}
