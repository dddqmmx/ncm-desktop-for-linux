use super::{
    api::ApiResponse,
    backend::Event,
    data::{self, Ids, Preferences, Song, Storage},
    download, library,
    view::View,
};
use crate::AppWindow;
use i_slint_backend_testing::ElementHandle;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};
use std::path::PathBuf;
pub(super) fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ncm-runtime-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn opaque_ids_keep_full_service_identity_and_entity_namespaces() {
    let mut ids = Ids::default();
    let song = ids.intern("song", "9007199254740991");
    let playlist = ids.intern("playlist", "9007199254740991");
    assert_ne!(song, playlist);
    assert_eq!(song, ids.intern("song", "9007199254740991"));
    assert_eq!(ids.get(song, "song"), Some("9007199254740991"));
    assert_eq!(ids.get(song, "playlist"), None);
    assert_eq!(ids.get(-1, "song"), None);
    let s = Song::from_api(
        &json!({"id":4500000001u64,"name":"歌","ar":[{"id":5000000002u64,"name":"歌手"}],"al":{"id":6000000003u64,"name":"专辑"},"dt":1000}),
    );
    assert_eq!(s.id, "4500000001");
    assert_eq!(s.album_id, "6000000003");
    assert_eq!(s.artist_id, "5000000002");
}
#[test]
fn business_errors_do_not_become_http_success() {
    let response = ApiResponse {
        status: 200,
        body: json!({"code":401,"msg":"expired"}),
        cookie: vec![],
    };
    assert_eq!(response.checked().unwrap_err(), "expired");
    let response = ApiResponse {
        status: 200,
        body: json!({"code":200}),
        cookie: vec![
            "MUSIC_U=a; Path=/; HttpOnly".into(),
            "__csrf=b; Path=/".into(),
        ],
    };
    assert_eq!(response.session_cookie(), "MUSIC_U=a; __csrf=b");
}
#[test]
fn download_permissions_require_download_privileges_not_playback_or_source_limits() {
    assert!(
        download::allowed(&json!({}), &json!({"pl":999000,"maxbr":999000}), "lossless").is_err()
    );
    assert!(
        download::allowed(&json!({}), &json!({"dl":128000,"maxbr":999000}), "lossless").is_err()
    );
    assert!(download::allowed(
        &json!({}),
        &json!({"dlLevel":"hires","maxbr":999000}),
        "hires"
    )
    .is_ok());
    assert!(download::allowed(
        &json!({"sq":null}),
        &json!({"dlLevel":"lossless"}),
        "lossless"
    )
    .is_err());
    assert!(download::allowed(
        &json!({}),
        &json!({"dlLevels":["standard"],"dlLevel":"hires"}),
        "hires"
    )
    .is_err());
    assert!(download::allowed(
        &json!({}),
        &json!({"dlLevels":["standard"],"st":-1}),
        "standard"
    )
    .is_err());
}
#[test]
fn lyric_timestamps_support_repeated_tags_and_translations() {
    let lines = data::parse_lyrics(
        &json!({"lrc":{"lyric":"[ti:test]\n[00:02.50][01:02.500]a\n[00:01.00]b\n[00:99.0]invalid"},"tlyric":{"lyric":"[00:02.500]译"}}),
    );
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].ms, 1000);
    assert_eq!(lines[1].ms, 2500);
    assert_eq!(lines[1].translation, "译");
    assert_eq!(lines[2].ms, 62500);
}
#[test]
fn lyric_phrase_markers_do_not_create_blank_rows_or_hide_translations() {
    let lines = data::parse_lyrics(&json!({
        "lrc": {"lyric": "[00:00.00]\n[00:01.00] 第一行 \n[00:02.00]   \n[00:03.00][00:04.00]\t\u{3000}\n[00:05.00]第二行\n[00:06.00]第三行\n[00:07.00]\n"},
        "tlyric": {"lyric": "[00:00.90] \n[00:01.12]第一行译文\n[00:05.349]第二行译文\n[00:06.350]超出匹配范围"},
        "romalrc": {"lyric": "[00:00.651]first line\n[00:04.650]too early\n[00:06.00]\t"}
    }));
    assert_eq!(
        lines
            .iter()
            .map(|l| (l.ms, l.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(1000, "第一行"), (5000, "第二行"), (6000, "第三行")]
    );
    assert_eq!(lines[0].translation, "第一行译文");
    assert_eq!(lines[0].pronunciation, "first line");
    assert_eq!(lines[1].translation, "第二行译文");
    assert!(lines[1].pronunciation.is_empty());
    assert!(lines[2].translation.is_empty());
    assert!(lines[2].pronunciation.is_empty());
    assert!(data::parse_lyrics(&json!({"lrc": {"lyric": "[00:00.00]\n[00:01.00] \t"}})).is_empty());
}
#[test]
fn session_is_private_atomic_and_remember_false_removes_prior_cookie() {
    let path = temp("storage");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    storage.session("test-cookie", true).unwrap();
    assert_eq!(
        storage.read::<String>("session.json").unwrap(),
        "test-cookie"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path.join("session.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    storage.session("new-cookie", false).unwrap();
    assert!(!path.join("session.json").exists());
    std::fs::write(path.join("preferences.json"), "{corrupt").unwrap();
    assert!(storage.read::<Preferences>("preferences.json").is_err());
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn local_scan_deduplicates_roots_and_never_follows_symlinks_or_hidden_files() {
    let path = temp("library");
    std::fs::write(path.join("a.FLAC"), b"test").unwrap();
    std::fs::write(path.join(".hidden.mp3"), b"test").unwrap();
    std::fs::write(path.join("readme.txt"), b"test").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&path, path.join("loop")).unwrap();
    let (songs, truncated) = library::scan(&[path.clone(), path.clone()], vec![]).unwrap();
    assert_eq!(songs.len(), 1);
    assert!(!truncated);
    assert!(songs[0].id.starts_with("local:"));
    assert!(library::scan(&[path.join("missing")], songs).is_err());
    std::fs::remove_dir_all(path).unwrap();
}
fn response(name: &str, token: u64, epoch: u64, data: Value) -> Event {
    Event {
        name: name.into(),
        token,
        epoch,
        data,
        error: None,
    }
}

#[test]
fn decoded_images_update_existing_rows_and_discard_stale_account_and_qr_results() {
    use super::images::{DecodedImage, ImageTarget};
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    let home_state = app.global::<crate::HomeState>();
    let login_state = app.global::<crate::LoginState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.apply(
        &app,
        response(
            "home",
            0,
            0,
            json!({"body": {
                "playlists":{"result":[
                    {"id":1,"name":"first","picUrl":"https://example.test/one.jpg"},
                    {"id":2,"name":"second","picUrl":"https://example.test/two.jpg"}
                ]}, "songs":{"result":[]}, "categories":{"sub":[]}
            }}),
        ),
    );
    let rows = home_state.get_recommend_playlists();
    let pixels = || slint::SharedPixelBuffer::clone_from_slice(&[12u8, 34, 56, 255], 1, 1);
    let cover = |epoch| DecodedImage {
        palette: None,
        target: ImageTarget::Cover("https://example.test/one.jpg".into(), 800),
        epoch,
        pixels: pixels(),
    };
    view.apply_images(&app, [cover(1)].into_iter());
    assert_eq!(rows.row_data(0).unwrap().cover.size().width, 0);
    view.apply_images(&app, [cover(0)].into_iter());
    assert_eq!(
        home_state.get_recommend_playlists(),
        rows,
        "cover delivery rebuilt the list"
    );
    assert_eq!(rows.row_data(0).unwrap().cover.size().width, 1);
    assert_eq!(rows.row_data(1).unwrap().cover.size().width, 0);

    assert!(
        rx.try_recv().is_err(),
        "receiving a page must not prefetch its covers"
    );
    view.request_covers(
        [
            (rows.row_data(0).unwrap().id, 128),
            (rows.row_data(1).unwrap().id, 128),
        ]
        .into_iter(),
    );
    let request = rx.try_recv().unwrap();
    assert_eq!(request.name, "covers");
    assert_eq!(
        request.data,
        json!([{"cover":"https://example.test/two.jpg", "size":128}])
    );
    assert!(
        rx.try_recv().is_err(),
        "cached covers should not be requested again"
    );

    // The form's unchecked default must not disable QR session persistence.
    assert!(!login_state.get_login_remember());
    view.action(&app, "qr", -1, "");
    let request = rx.try_recv().unwrap();
    assert_eq!(request.name, "qr");
    assert_eq!(request.data["remember"], true);
    let qr = |epoch, token| DecodedImage {
        palette: None,
        target: ImageTarget::LoginQr(token),
        epoch,
        pixels: pixels(),
    };
    view.apply_images(
        &app,
        [qr(0, request.token), qr(1, request.token + 1)].into_iter(),
    );
    assert_eq!(login_state.get_login_qr_image().size().width, 0);
    view.apply_images(&app, [qr(1, request.token)].into_iter());
    let accepted = login_state.get_login_qr_image();
    assert_eq!(accepted.size().width, 1);
    view.action(&app, "cancel-login", -1, "");
    view.apply_images(&app, [qr(1, request.token)].into_iter());
    assert_eq!(login_state.get_login_qr_image(), accepted);
}
#[test]
fn real_view_search_queue_seek_auth_and_stale_response_regression() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    let detail_state = app.global::<crate::DetailState>();
    let player_state = app.global::<crate::PlayerState>();
    let account_state = app.global::<crate::AccountState>();
    let search_state = app.global::<crate::SearchState>();
    let queue_state = app.global::<crate::QueueState>();
    let login_state = app.global::<crate::LoginState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut v = View::new(tx, Preferences::default());
    assert_eq!(detail_state.get_detail_tracks().row_count(), 0);
    assert!(!player_state.get_is_playing());
    assert!(!account_state.get_is_logged_in());
    search_state.set_search_query("first".into());
    v.action(&app, "search", -1, "");
    let first = rx.try_recv().unwrap();
    search_state.set_search_query("second".into());
    v.action(&app, "search", -1, "");
    let second = rx.try_recv().unwrap();
    let page = |title: &str| {
        json!({
        "kind":"search","body":{
        "result":{
        "songCount":2,"songs":[{
        "id":4500000001u64,"name":title,"dt":200000,"ar":[{
        "id":11,"name":"artist"}
        ],"al":{
        "id":12,"name":"album"}
        }
        ,{
        "id":4500000002u64,"name":"Next","dt":180000}
        ]}
        }
        ,"images":{
        }
        }
        )
    };

    v.apply(&app, response("page", first.token, 0, page("stale")));
    assert_eq!(detail_state.get_detail_tracks().row_count(), 0);
    v.apply(&app, response("page", second.token, 0, page("current")));
    assert_eq!(
        detail_state.get_detail_tracks().row_data(0).unwrap().name,
        "current"
    );
    let handle = detail_state.get_detail_tracks().row_data(0).unwrap().id;
    v.action(&app, "play", handle, "");
    let play = rx.try_recv().unwrap();
    assert_eq!(play.name, "play");
    assert_eq!(play.data["song"]["id"], "4500000001");
    assert_eq!(queue_state.get_queue_songs().row_count(), 2);
    assert!(!player_state.get_is_playing());
    assert!(player_state.get_is_loading());
    while rx.try_recv().is_ok() {}
    v.seek(&app, 25.0);
    assert_eq!(rx.try_recv().unwrap().data["seconds"], 50.0);
    v.apply(
        &app,
        Event {
            name: "play".into(),
            token: play.token,
            epoch: 0,
            data: Value::Null,
            error: Some("no source".into()),
        },
    );
    assert!(!player_state.get_is_playing());
    assert!(!player_state.get_is_loading());
    v.action(&app, "step", 1, "");
    let next = rx.try_recv().unwrap();
    assert_eq!(next.data["song"]["id"], "4500000002");
    while rx.try_recv().is_ok() {}
    login_state.set_login_phone("13000000000".into());
    login_state.set_login_password("not-a-real-secret".into());
    v.action(&app, "login", -1, "");
    let login = rx.try_recv().unwrap();
    assert_eq!(login.name, "login");
    assert_eq!(login_state.get_login_password(), "not-a-real-secret");
    v.apply(&app, response("page", second.token, 0, page("old account")));
    assert_eq!(
        detail_state.get_detail_tracks().row_data(0).unwrap().name,
        "current"
    );
    v.apply(
        &app,
        Event {
            name: "login".into(),
            token: login.token,
            epoch: 1,
            data: Value::Null,
            error: Some("bad code".into()),
        },
    );
    assert!(!account_state.get_is_logged_in());
    assert_eq!(login_state.get_login_error(), "bad code");
    assert!(!login_state.get_login_busy());
}

#[test]
fn attached_runtime_updates_both_windows_and_shuts_down_without_node() {
    use super::api::{ApiFuture, ApiResponse, MusicApi};
    struct Provider(std::thread::ThreadId);
    impl MusicApi for Provider {
        fn call(&self, operation: &str, _params: Value, _cookie: &str) -> ApiFuture<'_> {
            assert_ne!(
                std::thread::current().id(),
                self.0,
                "API invoked on UI thread"
            );
            let body = match operation {
                "personalized" | "recommend_resource" => {
                    json!({"code":200,"result":[],"recommend":[]})
                }
                "personalized_newsong" => {
                    json!({"code":200,"result":[{"id":4500000001u64,"name":"Native runtime song","song":{"id":4500000001u64,"name":"Native runtime song","dt":10000}}]})
                }
                "recommend_songs" => json!({"code":200,"data":{"dailySongs":[]}}),
                "playlist_catlist" => json!({"code":200,"sub":[{"name":"电子"}]}),
                "banner" => json!({"code":200,"banners":[]}),
                "login" => json!({"code":200,"cookie":"mock-cookie"}),
                "user_account" => {
                    json!({"code":200,"profile":{"userId":6500000001u64,"nickname":"Mock account"}})
                }
                "user_playlist" => json!({"code":200,"playlist":[]}),
                "likelist" => json!({"code":200,"ids":[]}),
                _ => json!({"code":200}),
            };
            Box::pin(async move {
                Ok(ApiResponse {
                    status: 200,
                    body,
                    cookie: vec![],
                })
            })
        }
    }
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let path = temp("attached");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    let runtime = super::ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        storage.clone(),
        Preferences::default(),
        Some(std::sync::Arc::new(Provider(std::thread::current().id()))),
    )
    .unwrap();
    let home_state = app.global::<crate::HomeState>();
    let settings_state = app.global::<crate::SettingsState>();
    let audio_state = app.global::<crate::AudioState>();
    let login_state = app.global::<crate::LoginState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let account_state = app.global::<crate::AccountState>();
    let pump = || {
        std::thread::sleep(std::time::Duration::from_millis(10));
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(60));
    };
    for _ in 0..150 {
        pump();
        if !home_state.get_home_songs_loading() {
            break;
        }
    }
    assert!(!home_state.get_home_songs_loading());
    assert_eq!(
        home_state.get_daily_songs().row_data(0).unwrap().title,
        "Native runtime song"
    );
    let settings = &windows._settings;
    settings_state.invoke_open_settings_window();
    settings
        .global::<crate::SettingsState>()
        .set_settings_tab("audio".into());
    pump();
    i_slint_backend_testing::ElementHandle::find_by_element_id(
        &windows._settings,
        "AudioSettingsTab::quality-select",
    )
    .next()
    .unwrap()
    .mock_single_click(slint::platform::PointerEventButton::Left);
    pump();
    assert_eq!(
        settings
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "lossless"
    );
    i_slint_backend_testing::ElementHandle::find_by_accessible_label(
        &windows._settings,
        "标准 (128kbps)",
    )
    .next()
    .unwrap()
    .mock_single_click(slint::platform::PointerEventButton::Left);
    settings
        .global::<crate::AudioState>()
        .set_volume_percent(42.0);
    settings
        .global::<crate::SettingsState>()
        .set_settings_tab("general".into());
    pump();
    ElementHandle::find_by_element_id(&windows._settings, "GeneralSettingsTab::renderer-select")
        .next()
        .unwrap()
        .mock_single_click(slint::platform::PointerEventButton::Left);
    pump();
    ElementHandle::find_by_accessible_label(&windows._settings, "软件渲染（关闭 GPU 加速）")
        .next()
        .unwrap()
        .mock_single_click(slint::platform::PointerEventButton::Left);
    for _ in 0..80 {
        pump();
        if storage
            .read::<Preferences>("preferences.json")
            .is_ok_and(|p| {
                p.quality == "standard"
                    && (p.volume - 0.42).abs() < 0.001
                    && p.renderer == crate::rendering::Renderer::Software
            })
        {
            break;
        }
    }
    let saved = storage.read::<Preferences>("preferences.json").unwrap();
    assert_eq!(saved.quality, "standard");
    assert!((saved.volume - 0.42).abs() < 0.001);
    assert_eq!(audio_state.get_setting_sound_quality(), "standard");
    assert_eq!(saved.renderer, crate::rendering::Renderer::Software);
    assert_eq!(settings_state.get_setting_renderer(), "software");
    assert!(settings_state.get_renderer_restart_required());
    let restored = AppWindow::new().unwrap();
    super::view::initialize(&restored, &saved);
    assert_eq!(
        restored
            .global::<crate::SettingsState>()
            .get_setting_renderer(),
        "software"
    );
    assert_eq!(
        restored
            .global::<crate::SettingsState>()
            .get_startup_renderer(),
        "software"
    );
    assert!(!restored
        .global::<crate::SettingsState>()
        .get_renderer_restart_required());
    assert_eq!(
        restored
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "standard"
    );
    assert_eq!(
        restored
            .global::<crate::AudioState>()
            .get_setting_sound_quality_label(),
        "标准 (128kbps)"
    );
    login_state.set_is_login_open(true);
    pump();
    login_state.set_login_phone("test".into());
    login_state.set_login_password("test".into());
    login_state.set_login_remember(false);
    runtime_state.invoke_runtime_action("login".into(), -1, "".into());
    for _ in 0..150 {
        pump();
        if account_state.get_is_logged_in() && account_state.get_user_nickname() == "Mock account" {
            break;
        }
    }
    assert!(
        account_state.get_is_logged_in(),
        "{}",
        login_state.get_login_error()
    );
    assert_eq!(account_state.get_user_nickname(), "Mock account");
    assert!(!login_state.get_is_login_open());
    assert!(!path.join("session.json").exists());
    runtime_state.invoke_runtime_action("logout".into(), -1, "".into());
    pump();
    assert!(!account_state.get_is_logged_in());
    // Close immediately, before the next 50ms preferences timer fires.
    settings_state.set_setting_renderer("opengl".into());
    drop(runtime);
    assert_eq!(
        storage.preferences().unwrap().renderer,
        crate::rendering::Renderer::OpenGl
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn renderer_preferences_default_to_vulkan_and_round_trip_without_losing_audio_settings() {
    use crate::rendering::Renderer;
    let path = temp("renderer-preferences");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    assert_eq!(storage.preferences().unwrap().renderer, Renderer::Vulkan);
    // Migrate a configuration written before the rendering setting existed.
    storage
        .save(
            "preferences.json",
            &json!({"quality":"hires","volume":0.42}),
        )
        .unwrap();
    let mut preferences = storage.preferences().unwrap();
    assert_eq!(preferences.renderer, Renderer::Vulkan);
    for renderer in [Renderer::Software, Renderer::OpenGl, Renderer::Vulkan] {
        preferences.renderer = renderer;
        storage.save("preferences.json", &preferences).unwrap();
        let restored = storage.preferences().unwrap();
        assert_eq!(restored, preferences);
        assert_eq!(restored.quality, "hires");
        assert_eq!(restored.volume, 0.42);
    }
    storage
        .save(
            "preferences.json",
            &json!({"renderer":"future-mode","quality":"hires"}),
        )
        .unwrap();
    let restored = storage.preferences().unwrap();
    assert_eq!(restored.renderer, Renderer::Vulkan);
    assert_eq!(restored.quality, "hires");
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn bitperfect_volume_stays_disabled_across_restart_and_player_snapshots() {
    i_slint_backend_testing::init_no_event_loop();
    let path = temp("bitperfect-volume");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    storage
        .save(
            "preferences.json",
            &Preferences {
                strict: true,
                software_volume: true,
                volume: 0.42,
                ..Preferences::default()
            },
        )
        .unwrap();
    let restored = storage.preferences().unwrap();
    assert_eq!(restored.effective_volume(), 1.0);
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &restored);
    let audio_state = app.global::<crate::AudioState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, restored);
    assert!(!audio_state.get_can_adjust_volume());

    // Idle/unlocked snapshots cannot override the user's BitPerfect setting;
    // a locked snapshot still blocks volume while the setting is turning off.
    for (strict, software, locked, can_adjust, output_volume) in [
        (true, true, false, false, 1.0),
        (true, true, true, false, 1.0),
        (false, true, true, false, 0.42),
        (false, true, false, true, 0.42),
        (false, false, false, false, 1.0),
        (true, false, false, false, 1.0),
    ] {
        audio_state.set_setting_strict_bitperfect(strict);
        audio_state.set_setting_software_volume(software);
        view.apply(&app, response("snapshot", 0, 0, json!({"locked": locked})));
        assert_eq!(audio_state.get_can_adjust_volume(), can_adjust);
        assert_eq!(audio_state.get_session_volume_locked(), locked);
        assert_eq!(audio_state.get_volume_percent(), 42.0);
        view.save_preferences_if_changed(&app);
        while let Ok(command) = rx.try_recv() {
            if command.name == "preferences" {
                let saved: Preferences = serde_json::from_value(command.data).unwrap();
                assert_eq!(saved.strict, strict);
                assert_eq!(saved.software_volume, software);
                assert_eq!(saved.volume, 0.42);
                assert_eq!(saved.effective_volume(), output_volume);
            }
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn unavailable_outputs_allow_disabling_saved_bitperfect_and_exclusive_settings() {
    use slint::platform::PointerEventButton;
    use std::time::Duration;

    i_slint_backend_testing::init_no_event_loop();
    for software_volume in [false, true] {
        for device_state in ["detecting", "empty", "default", "fallback"] {
            let prefs = Preferences {
                strict: true,
                exclusive: true,
                software_volume,
                volume: 0.42,
                ..Preferences::default()
            };
            let app = AppWindow::new().unwrap();
            super::view::initialize(&app, &prefs);
            let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
            let settings = &windows._settings;
            let audio = app.global::<crate::AudioState>();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut view = View::new(tx, prefs);
            if device_state == "fallback" {
                view.apply(
                    &app,
                    response(
                        "devices",
                        0,
                        0,
                        json!([
                            {"id":"hw:CARD=UsbDac,DEV=0","name":"USB DAC","current":true}
                        ]),
                    ),
                );
                assert!(audio.get_setting_supports_exclusive());
            }
            match device_state {
                "empty" => view.apply(&app, response("devices", 0, 0, json!([]))),
                "default" | "fallback" => view.apply(
                    &app,
                    response(
                        "devices",
                        0,
                        0,
                        json!([
                            {"id":"","name":"系统默认输出","default":true,"current":true}
                        ]),
                    ),
                ),
                _ => {}
            }
            assert!(!audio.get_setting_supports_exclusive());
            assert!(audio.get_setting_strict_bitperfect());
            assert!(audio.get_setting_exclusive_mode());
            app.show().unwrap();
            app.global::<crate::SettingsState>()
                .invoke_open_settings_window();
            settings
                .window()
                .set_size(slint::PhysicalSize::new(1000, 1400));
            settings
                .global::<crate::SettingsState>()
                .set_settings_tab("audio".into());
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(600));
            let click = |id: &str| {
                ElementHandle::find_by_element_id(settings, id)
                    .next()
                    .unwrap()
                    .mock_single_click(PointerEventButton::Left);
                i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(600));
            };

            // BitPerfect still forces exclusive and blocks software volume.
            click("AudioSettingsTab::exclusive-switch");
            click("AudioSettingsTab::software-volume-switch");
            assert!(audio.get_setting_exclusive_mode());
            assert_eq!(audio.get_setting_software_volume(), software_volume);

            // Unsupported hardware must never prevent turning a saved option off.
            click("AudioSettingsTab::bitperfect-switch");
            assert!(!audio.get_setting_strict_bitperfect(), "{device_state}");
            assert_eq!(audio.get_can_adjust_volume(), software_volume);
            click("AudioSettingsTab::exclusive-switch");
            assert!(!audio.get_setting_exclusive_mode(), "{device_state}");
            assert!(!settings
                .global::<crate::AudioState>()
                .get_setting_strict_bitperfect());
            assert!(!settings
                .global::<crate::AudioState>()
                .get_setting_exclusive_mode());

            // Once off, neither hardware option can be enabled on a virtual output.
            click("AudioSettingsTab::bitperfect-switch");
            click("AudioSettingsTab::exclusive-switch");
            assert!(!audio.get_setting_strict_bitperfect());
            assert!(!audio.get_setting_exclusive_mode());
            if !software_volume {
                click("AudioSettingsTab::software-volume-switch");
            }
            assert!(audio.get_can_adjust_volume());
            assert_eq!(audio.get_volume_percent(), 42.0);
            view.save_preferences_if_changed(&app);
            let command = rx.try_recv().unwrap();
            assert_eq!(command.name, "preferences");
            let saved: Preferences = serde_json::from_value(command.data).unwrap();
            assert!(!saved.strict && !saved.exclusive && saved.software_volume);
            assert_eq!(saved.effective_volume(), 0.42);

            // A hardware output makes both options available again.
            view.apply(
                &app,
                response(
                    "devices",
                    0,
                    0,
                    json!([
                        {"id":"hw:CARD=UsbDac,DEV=0","name":"USB DAC","current":true}
                    ]),
                ),
            );
            click("AudioSettingsTab::exclusive-switch");
            assert!(audio.get_setting_exclusive_mode());
            click("AudioSettingsTab::bitperfect-switch");
            assert!(audio.get_setting_strict_bitperfect());
            settings.hide().unwrap();
            app.hide().unwrap();
        }
    }
}

#[test]
fn virtual_outputs_disable_hardware_options_without_erasing_preferences() {
    let prefs = Preferences {
        strict: true,
        exclusive: true,
        ..Preferences::default()
    };
    assert_eq!(prefs.output_options(None), (false, false));
    assert_eq!(prefs.output_options(Some("default")), (false, false));
    assert_eq!(prefs.output_options(Some("pipewire")), (false, false));
    assert_eq!(
        prefs.output_options(Some("hw:CARD=UsbDac,DEV=0")),
        (true, true)
    );
    assert!(prefs.strict && prefs.exclusive);
}

#[test]
fn cover_eviction_releases_playlist_rows_and_scrolling_back_can_reload() {
    use super::images::{DecodedImage, ImageTarget};
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    let playlist_state = app.global::<crate::PlaylistState>();
    let player_state = app.global::<crate::PlayerState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.apply(&app, response("page", 0, 0, json!({"kind":"playlist","id":"1","body":{"playlist":{
        "name":"test","tracks":(0..1_100).map(|i|json!({"id":i+1,"name":"song","al":{"picUrl":format!("https://example.test/{i}.jpg")}})).collect::<Vec<_>>()
    }}})));
    let rows = playlist_state.get_playlist_tracks();
    let cover = |index, size| DecodedImage {
        palette: None,
        target: ImageTarget::Cover(format!("https://example.test/{index}.jpg"), size),
        epoch: 0,
        pixels: slint::SharedPixelBuffer::new(size, size),
    };
    for i in 0..1_100 {
        view.apply_images(&app, [cover(i, 128)].into_iter());
    }
    assert_eq!(
        playlist_state.get_playlist_tracks(),
        rows,
        "eviction rebuilt the model"
    );
    assert_eq!(
        rows.row_data(0).unwrap().cover.size().width,
        0,
        "evicted pixels are still retained by the row"
    );
    assert_eq!(rows.row_data(1_099).unwrap().cover.size().width, 128);
    while rx.try_recv().is_ok() {}
    let id = rows.row_data(0).unwrap().id;
    view.request_covers([(id, 128)].into_iter());
    assert_eq!(
        rx.try_recv().unwrap().data,
        json!([{"cover":"https://example.test/0.jpg","size":128}])
    );
    view.apply_images(&app, [cover(0, 128)].into_iter());
    assert_eq!(rows.row_data(0).unwrap().cover.size().width, 128);
    view.request_covers([(id, 800)].into_iter());
    assert_eq!(rx.try_recv().unwrap().data[0]["size"], 800);
    view.apply_images(&app, [cover(0, 800)].into_iter());
    view.apply_images(&app, [cover(0, 128)].into_iter());
    assert_eq!(
        rows.row_data(0).unwrap().cover.size().width,
        800,
        "late thumbnail downgraded artwork"
    );
    view.action(&app, "play", id, "");
    while rx.try_recv().is_ok() {}
    view.request_covers([(id, 1600)].into_iter());
    assert_eq!(rx.try_recv().unwrap().data[0]["size"], 1600);
    view.apply_images(&app, [cover(0, 1600)].into_iter());
    view.apply_images(&app, [cover(0, 800), cover(0, 128)].into_iter());
    assert_eq!(
        player_state.get_current_song_cover().size().width,
        1600,
        "late card/list image replaced fullscreen artwork"
    );
}

#[test]
fn lyrics_follow_playback_with_offset_and_reset_on_seek() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let player_state = app.global::<crate::PlayerState>();
    super::view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.apply(
        &app,
        response(
            "lyrics",
            0,
            0,
            json!([
                {"ms":1000,"text":"first","translation":"","pronunciation":""},
                {"ms":4000,"text":"second","translation":"","pronunciation":""}
            ]),
        ),
    );
    assert_eq!(
        player_view_state.get_active_lyric_index(),
        -1,
        "first lyric highlighted before its timestamp"
    );
    let reset = player_view_state.get_lyric_position_reset();
    view.apply(&app, response("snapshot", 0, 0, json!({"progress":949})));
    assert_eq!(player_view_state.get_active_lyric_index(), -1);
    view.apply(&app, response("snapshot", 0, 0, json!({"progress":950})));
    assert_eq!(player_view_state.get_active_lyric_index(), 0);
    assert_eq!(player_view_state.get_lyric_position_reset(), reset);
    view.apply(&app, response("snapshot", 0, 0, json!({"progress":4050})));
    assert_eq!(player_view_state.get_active_lyric_index(), 1);
    assert!(player_view_state.get_lyric_position_reset() > reset);
    player_state.set_is_loading(true);
    view.action(&app, "lyric-seek", 0, "");
    assert!(rx.try_recv().is_err(), "loading song accepted lyric seek");
    player_state.set_is_loading(false);
    view.action(&app, "lyric-seek", -1, "");
    assert!(
        rx.try_recv().is_err(),
        "invalid lyric index sought the first line"
    );
    view.action(&app, "lyric-seek", 0, "");
    assert_eq!(rx.try_recv().unwrap().data["seconds"], 1.0);
    assert_eq!(player_view_state.get_active_lyric_index(), 0);
}
