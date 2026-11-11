use super::{
    api::{ApiFuture, ApiResponse, MusicApi},
    data::{Preferences, Song, Storage},
    library,
    tests::temp,
    ApplicationRuntime,
};
use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::{json, Value};
use slint::{platform::PointerEventButton, ComponentHandle, Model};
use std::{sync::Arc, time::Duration};

fn settle(app: &AppWindow) {
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(300));
    let _ = app.root_element().size();
}

fn click(app: &AppWindow, id: &str, index: usize) {
    ElementHandle::find_by_element_id(app, id)
        .nth(index)
        .unwrap_or_else(|| panic!("missing {id}[{index}]"))
        .mock_single_click(PointerEventButton::Left);
    settle(app);
}

fn request_remove(app: &AppWindow, index: usize) {
    let library_state = app.global::<crate::LibraryState>();
    let dialog_state = app.global::<crate::DialogState>();
    let target = library_state.get_local_tracks().row_data(index).unwrap();
    click(app, "SongRowActions::more-ta", index);
    assert!(dialog_state.get_is_dialog_open());
    assert_eq!(dialog_state.get_dialog_title(), "移除本地歌曲");
    assert_eq!(dialog_state.get_dialog_confirm_text(), "移除");
    assert_eq!(dialog_state.get_dialog_mode(), "confirm-cancel");
    assert!(dialog_state
        .get_dialog_message()
        .contains(target.name.as_str()));
    assert_eq!(dialog_state.get_local_remove_id(), target.id);
}

fn wait_for(app: &AppWindow, count: usize) {
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(10));
        mock_elapsed_time(Duration::from_millis(60));
        if app
            .global::<crate::LibraryState>()
            .get_local_tracks()
            .row_count()
            == count
        {
            settle(app);
            return;
        }
    }
    panic!("local library did not reach {count} songs");
}

#[test]
fn local_song_removal_and_clear_have_separate_confirmation_targets() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    crate::playlists::attach(&app);
    app.show().unwrap();
    let navigation_state = app.global::<crate::NavigationState>();
    let library_state = app.global::<crate::LibraryState>();
    let dialog_state = app.global::<crate::DialogState>();
    navigation_state.set_current_route("local-music".into());
    settle(&app);
    let original: Vec<_> = library_state
        .get_local_tracks()
        .iter()
        .map(|song| song.id)
        .collect();

    request_remove(&app, 1);
    click(&app, "DialogContent::dialog-secondary-btn", 0);
    assert_eq!(library_state.get_local_tracks().row_count(), original.len());
    assert_eq!(dialog_state.get_local_remove_id(), -1);
    assert!(dialog_state.get_local_dialog_action().is_empty());

    click(&app, "LocalLibraryActions::local-clear-btn", 0);
    assert_eq!(dialog_state.get_dialog_title(), "清空本地音乐");
    assert_eq!(dialog_state.get_dialog_confirm_text(), "清空");
    assert_eq!(dialog_state.get_local_remove_id(), -1);
    click(&app, "DialogContent::dialog-secondary-btn", 0);

    // A previous dialog's text or mode must never choose the destructive action.
    dialog_state.set_dialog_mode("confirm".into());
    request_remove(&app, 1);
    click(&app, "DialogContent::dialog-primary-btn", 0);
    let remaining: Vec<_> = library_state
        .get_local_tracks()
        .iter()
        .map(|song| song.id)
        .collect();
    assert_eq!(remaining, vec![original[0], original[2], original[3]]);
    assert_eq!(
        library_state
            .get_local_tracks()
            .row_data(1)
            .unwrap()
            .index_str,
        "02"
    );

    // Reused rows must target their current song after the list shrinks.
    request_remove(&app, 1);
    click(&app, "DialogContent::dialog-primary-btn", 0);
    assert_eq!(
        library_state
            .get_local_tracks()
            .iter()
            .map(|song| song.id)
            .collect::<Vec<_>>(),
        vec![original[0], original[3]]
    );
    click(&app, "LocalLibraryActions::local-clear-btn", 0);
    assert_eq!(dialog_state.get_dialog_title(), "清空本地音乐");
    assert_eq!(dialog_state.get_dialog_confirm_text(), "清空");
    click(&app, "DialogContent::dialog-primary-btn", 0);
    assert_eq!(library_state.get_local_tracks().row_count(), 0);
}

#[test]
fn local_song_removal_persists_only_the_selected_record_and_keeps_files() {
    struct EmptyApi;
    impl MusicApi for EmptyApi {
        fn call(&self, _operation: &str, _params: Value, _cookie: &str) -> ApiFuture<'_> {
            Box::pin(async {
                Ok(ApiResponse {
                    status: 200,
                    body: json!({"code": 200}),
                    cookie: vec![],
                })
            })
        }
    }
    i_slint_backend_testing::init_no_event_loop();
    let path = temp("local-removal");
    let music = path.join("音乐");
    std::fs::create_dir(&music).unwrap();
    for name in ["a.flac", "b 选中歌曲.flac", "c.flac"] {
        std::fs::write(music.join(name), b"original music file").unwrap();
    }
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    let preferences = Preferences {
        folders: vec![music.clone()],
        ..Preferences::default()
    };
    let (songs, _) = library::scan(&preferences.folders, vec![]).unwrap();
    storage.save("library.json", &songs).unwrap();
    storage.save("preferences.json", &preferences).unwrap();
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let runtime = ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        storage.clone(),
        preferences.clone(),
        Some(Arc::new(EmptyApi)),
    )
    .unwrap();
    app.show().unwrap();
    let navigation_state = app.global::<crate::NavigationState>();
    let library_state = app.global::<crate::LibraryState>();
    let settings_state = app.global::<crate::SettingsState>();
    let dialog_state = app.global::<crate::DialogState>();
    navigation_state.set_current_route("local-music".into());
    wait_for(&app, 3);

    request_remove(&app, 1);
    click(&app, "DialogContent::dialog-secondary-btn", 0);
    assert_eq!(storage.read::<Vec<Song>>("library.json").unwrap().len(), 3);
    request_remove(&app, 1);
    click(&app, "DialogContent::dialog-primary-btn", 0);
    wait_for(&app, 2);

    let remaining = library::load_existing(&storage).unwrap();
    assert_eq!(
        remaining.iter().map(|song| &song.id).collect::<Vec<_>>(),
        vec![&songs[0].id, &songs[2].id]
    );
    assert_eq!(
        library_state.get_local_tracks().row_data(0).unwrap().name,
        songs[0].name
    );
    assert_eq!(
        library_state.get_local_tracks().row_data(1).unwrap().name,
        songs[2].name
    );
    assert_eq!(storage.preferences().unwrap().folders, preferences.folders);
    assert_eq!(settings_state.get_setting_lib_paths().row_count(), 1);
    for song in &songs {
        assert_eq!(
            std::fs::read(song.path.as_ref().unwrap()).unwrap(),
            b"original music file"
        );
    }
    assert!(library::remove_track(&storage, "missing-song").is_err());
    assert!(library::remove_track(&storage, "").is_err());
    assert_eq!(storage.read::<Vec<Song>>("library.json").unwrap().len(), 2);

    click(&app, "LocalLibraryActions::local-clear-btn", 0);
    assert_eq!(dialog_state.get_dialog_confirm_text(), "清空");
    click(&app, "DialogContent::dialog-primary-btn", 0);
    wait_for(&app, 0);
    assert!(storage
        .read::<Vec<Song>>("library.json")
        .unwrap()
        .is_empty());
    assert!(storage.preferences().unwrap().folders.is_empty());
    assert!(songs
        .iter()
        .all(|song| song.path.as_ref().unwrap().is_file()));
    drop(runtime);
    std::fs::remove_dir_all(path).unwrap();
}
