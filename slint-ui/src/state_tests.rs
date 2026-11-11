use crate::{
    windows::ApplicationWindows, AccountState, AppWindow, AudioState, DetailState, DialogState,
    LibraryState, NavigationState, PlayerState, PlaylistState, QueueState, RuntimeState,
    SettingsState,
};
use i_slint_backend_testing::mock_elapsed_time;
use slint::{ComponentHandle, Model};
use std::time::Duration;

fn settle() {
    mock_elapsed_time(Duration::from_millis(100));
}

#[test]
fn replacing_detail_results_does_not_replace_playlist_library_or_queue() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let playlist = app.global::<PlaylistState>();
    let library = app.global::<LibraryState>();
    let queue = app.global::<QueueState>();
    let original_playlist = playlist.get_playlist_tracks();
    let original_library = library.get_local_tracks();
    let original_queue = queue.get_queue_songs();
    assert!(original_playlist.row_count() > 0);

    // Search and album results share a detail view, but no longer act as the
    // live default binding for the independently loaded playlist page.
    app.global::<DetailState>()
        .set_detail_tracks(Default::default());
    settle();
    assert_eq!(playlist.get_playlist_tracks(), original_playlist);
    assert_eq!(library.get_local_tracks(), original_library);
    assert_eq!(queue.get_queue_songs(), original_queue);
}

#[test]
fn account_and_route_changes_invalidate_only_the_related_playlist_actions() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let account = app.global::<AccountState>();
    let navigation = app.global::<NavigationState>();
    let playlist = app.global::<PlaylistState>();
    let dialog = app.global::<DialogState>();
    navigation.set_current_route("playlist".into());
    settle();
    playlist.set_is_playlist_actions_open(true);
    navigation.set_current_route("home".into());
    settle();
    assert!(!playlist.get_is_playlist_actions_open());

    playlist.set_playlist_editor_mode("create".into());
    playlist.set_playlist_editor_can_submit(true);
    playlist.set_is_playlist_actions_open(true);
    dialog.set_playlist_delete_id(101);
    dialog.set_is_dialog_open(true);
    settle();
    account.set_is_logged_in(false);
    settle();
    assert!(!playlist.get_playlist_can_manage());
    assert!(!playlist.get_is_playlist_actions_open());
    assert_eq!(playlist.get_playlist_editor_mode(), "");
    assert!(!playlist.get_playlist_editor_can_submit());
    assert!(!dialog.get_is_dialog_open());
    assert_eq!(dialog.get_playlist_delete_id(), -1);

    account.set_is_logged_in(true);
    settle();
    dialog.invoke_request_local_remove(201, "本地歌曲".into());
    settle();
    account.set_is_logged_in(false);
    settle();
    assert!(
        dialog.get_is_dialog_open(),
        "local removal does not require an account"
    );
    assert_eq!(dialog.get_local_remove_id(), 201);
    dialog.set_is_dialog_open(false);
    settle();
    assert_eq!(dialog.get_local_remove_id(), -1);
    assert_eq!(dialog.get_local_dialog_action(), "");
}

#[test]
fn windows_sync_audio_preferences_and_status_without_copying_page_state() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    let settings = &windows._settings;
    let main_preferences = app.global::<SettingsState>();
    let window_preferences = settings.global::<SettingsState>();
    let main_audio = app.global::<AudioState>();
    let window_audio = settings.global::<AudioState>();
    let main_runtime = app.global::<RuntimeState>();
    let window_runtime = settings.global::<RuntimeState>();

    main_audio.set_volume_percent(31.0);
    window_preferences.set_setting_acrylic(false);
    main_runtime.set_runtime_message("设备已连接".into());
    app.global::<PlayerState>().set_current_song_id(1234);
    app.global::<NavigationState>()
        .set_current_route("artist".into());
    app.global::<DialogState>().invoke_request_local_clear();
    window_preferences.set_settings_tab("audio".into());
    settle();
    assert_eq!(window_audio.get_volume_percent(), 31.0);
    assert!(!main_preferences.get_setting_acrylic());
    assert_eq!(window_runtime.get_runtime_message(), "设备已连接");
    assert_eq!(main_preferences.get_settings_tab(), "general");
    assert_eq!(
        settings.global::<NavigationState>().get_current_route(),
        "home"
    );
    assert!(!settings.global::<DialogState>().get_is_dialog_open());
    assert_ne!(settings.global::<PlayerState>().get_current_song_id(), 1234);

    window_runtime.set_runtime_message("".into());
    window_audio.set_setting_strict_bitperfect(true);
    main_preferences.set_setting_accent_hex("#22c55e".into());
    settle();
    assert_eq!(main_runtime.get_runtime_message(), "");
    assert!(!main_audio.get_can_adjust_volume());
    assert_eq!(window_preferences.get_setting_accent_hex(), "#22c55e");
}
