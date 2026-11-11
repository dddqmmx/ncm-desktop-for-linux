use crate::{playlists, AppWindow, Theme};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::Value;
use slint::{
    platform::{Key, PointerEventButton, WindowEvent},
    ComponentHandle, Model, PhysicalSize,
};
use std::time::Duration;

fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("Missing {id}"))
}

fn click(app: &AppWindow, id: &str) {
    element(app, id).mock_single_click(PointerEventButton::Left);
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(250));
}

fn key(app: &AppWindow, key: Key) {
    let text: slint::SharedString = key.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    mock_elapsed_time(Duration::from_millis(10));
}

fn check_rect(app: &AppWindow, gt: &Value, view: &str, name: &str, id: &str) {
    let item = element(app, id);
    if view.starts_with("detail") {
        let owner = element(
            app,
            if name == "header" {
                "MediaDetailView::detail-container"
            } else if ["play", "search", "subscribers", "more"].contains(&name) {
                "MediaDetailHeader::detail-action-bar"
            } else {
                "MediaDetailView::detail-header"
            },
        );
        crate::responsive_tests::assert_inside(&owner, &item, &format!("{view}.{name}"));
        return;
    }
    if view == "menu" {
        let owner = if name == "card" {
            app.root_element()
        } else {
            element(app, "PlaylistActionsOverlay::action-menu")
        };
        crate::responsive_tests::assert_inside(&owner, &item, name);
        return;
    }
    let p = item.absolute_position();
    let s = item.size();
    let actual = [p.x, p.y, s.width, s.height];
    let expected = &gt[view][name]["rect"];
    for (i, field) in ["x", "y", "width", "height"].iter().enumerate() {
        let expected = expected[field].as_f64().unwrap() as f32;
        assert!(
            (actual[i] - expected).abs() <= 0.5,
            "{view}.{name}.{field}: Slint {} != Electron {expected}",
            actual[i]
        );
    }
}

fn check_editor(app: &AppWindow, _gt: &Value, view: &str, editing: bool) {
    use crate::responsive_tests::{assert_disjoint, assert_inside};
    let card = element(app, "PlaylistEditor::editor-card");
    assert_inside(&app.root_element(), &card, view);
    let header = element(app, "PlaylistEditorContent::editor-header");
    let footer = element(app, "PlaylistEditorContent::editor-footer");
    let name = element(app, "PlaylistEditorForm::name-box");
    for id in [
        "PlaylistEditorContent::editor-header",
        "PlaylistEditorContent::editor-footer",
        "PlaylistEditorHeader::close-button",
        "PlaylistEditorForm::name-box",
        "PlaylistEditorFooter::cancel-button",
        "PlaylistEditorFooter::submit-button",
    ] {
        assert_inside(&card, &element(app, id), view);
    }
    assert_disjoint(&header, &name, view);
    assert_disjoint(&name, &footer, view);
    assert_disjoint(
        &element(app, "PlaylistEditorFooter::cancel-button"),
        &element(app, "PlaylistEditorFooter::submit-button"),
        view,
    );
    let last_field = element(
        app,
        if editing {
            "PlaylistDescriptionField::description-box"
        } else {
            "PlaylistEditorForm::privacy-row"
        },
    );
    assert_inside(&card, &last_field, view);
    assert_disjoint(&name, &last_field, view);
    assert_disjoint(&last_field, &footer, view);
    if editing {
        let cover = element(app, "PlaylistEditorForm::preview-cover");
        assert_inside(&card, &cover, view);
        assert_disjoint(&cover, &name, view);
    } else {
        assert_inside(
            &last_field,
            &element(app, "PlaylistEditorForm::privacy-toggle"),
            view,
        );
    }
}

// Optional headless screenshots for inspection, without adding a PNG dependency.
fn snapshot(app: &AppWindow, name: &str) {
    if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = app.window().take_snapshot().unwrap();
        let mut data = format!("P6\n{} {}\n255\n", pixels.width(), pixels.height()).into_bytes();
        for pixel in pixels.as_slice() {
            data.extend([pixel.r, pixel.g, pixel.b]);
        }
        std::fs::write(dir.join(format!("{name}.ppm")), data).unwrap();
    }
}

#[test]
fn playlist_management_flows_with_content_and_handles_user_actions() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    playlists::attach(&app);
    app.window().set_size(PhysicalSize::new(1435, 1000));
    app.show().unwrap();
    let playlist_state = app.global::<crate::PlaylistState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let detail_state = app.global::<crate::DetailState>();
    let queue_state = app.global::<crate::QueueState>();
    let dialog_state = app.global::<crate::DialogState>();
    let account_state = app.global::<crate::AccountState>();
    let login_state = app.global::<crate::LoginState>();
    let original_title = playlist_state.get_playlist_title();
    let original_description = playlist_state.get_playlist_description();
    navigation_state.set_window_width(1435.0);
    navigation_state.set_window_height(1000.0);
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    let gt: Value = serde_json::from_str(include_str!(
        "../tests/playlist-management-ground-truth.json"
    ))
    .unwrap();
    check_rect(
        &app,
        &gt,
        "sidebar",
        "header",
        "SidebarNavigation::nav-group-header",
    );
    check_rect(
        &app,
        &gt,
        "sidebar",
        "create",
        "SidebarNavigation::create-playlist-btn",
    );
    let initial_count = playlist_state.get_created_playlists().row_count();
    click(&app, "SidebarNavigation::create-playlist-btn");
    assert_eq!(playlist_state.get_playlist_editor_mode(), "create");
    assert!(!playlist_state.get_playlist_editor_can_submit());
    check_editor(&app, &gt, "create", false);
    snapshot(&app, "create-light");
    playlist_state.set_playlist_editor_name(" \t\n　 ".into());
    playlist_state.invoke_playlist_input_changed();
    playlist_state.invoke_submit_playlist();
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count
    );
    // Escape and outside clicks discard drafts.
    key(&app, Key::Escape);
    assert_eq!(playlist_state.get_playlist_editor_mode(), "");
    click(&app, "SidebarNavigation::create-playlist-btn");
    playlist_state.set_playlist_editor_name("  夜晚　  🎵  ".into());
    playlist_state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditorForm::privacy-toggle");
    click(&app, "PlaylistEditorFooter::submit-button");
    assert_eq!(playlist_state.get_playlist_editor_mode(), "");
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count + 1
    );
    assert_eq!(playlist_state.get_playlist_title(), "夜晚 🎵");
    assert!(playlist_state.get_playlist_is_private());
    assert_eq!(playlist_state.get_playlist_tracks().row_count(), 0);
    assert_eq!(
        detail_state.get_detail_tracks().row_count(),
        8,
        "playlist navigation must not empty albums/favorites"
    );
    let created_id = playlist_state.get_active_playlist_id();
    playlist_state.invoke_submit_playlist();
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count + 1,
        "duplicate submissions must not create another list"
    );

    playlist_state.invoke_open_playlist(101);
    mock_elapsed_time(Duration::from_millis(350));
    for (name, id) in [
        ("play", "detail-play-main-btn"),
        ("search", "detail-search-box"),
        ("subscribers", "detail-secondary-btn"),
        ("more", "playlist-actions-trigger"),
    ] {
        check_rect(
            &app,
            &gt,
            "detail",
            name,
            &format!("MediaDetailActions::{id}"),
        );
    }
    click(&app, "MediaDetailActions::playlist-actions-trigger");
    for (name, id) in [
        ("card", "action-menu"),
        ("edit", "edit-button"),
        ("divider", "menu-divider"),
        ("delete", "delete-button"),
    ] {
        check_rect(
            &app,
            &gt,
            "menu",
            name,
            &format!("PlaylistActionsOverlay::{id}"),
        );
    }
    snapshot(&app, "actions-light");
    key(&app, Key::Escape);
    assert!(!playlist_state.get_is_playlist_actions_open());
    click(&app, "MediaDetailActions::playlist-actions-trigger");
    click(&app, "PlaylistActionsOverlay::edit-button");
    assert_eq!(playlist_state.get_playlist_editor_mode(), "edit");
    assert!(
        !playlist_state.get_playlist_editor_can_submit(),
        "unchanged metadata cannot be saved"
    );
    check_editor(&app, &gt, "edit", true);
    snapshot(&app, "edit-light");
    // The name field is focused and selected when the editor opens.
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: "改名".into(),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: "改名".into(),
    });
    assert_eq!(playlist_state.get_playlist_editor_name(), "改名");
    assert!(playlist_state.get_playlist_editor_can_submit());
    key(&app, Key::Tab);
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: "键盘简介".into(),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: "键盘简介".into(),
    });
    assert!(
        playlist_state
            .get_playlist_editor_description()
            .contains("键盘简介"),
        "Tab must move focus to the description"
    );
    playlist_state.set_playlist_editor_description(" 草稿 ".into());
    playlist_state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditorFooter::cancel-button");
    assert_ne!(playlist_state.get_playlist_title(), "改名");
    playlist_state.invoke_open_edit_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(50));
    playlist_state.set_playlist_editor_name("  深夜   循环  ".into());
    playlist_state.set_playlist_editor_description(" 新的简介 \n".into());
    playlist_state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditorFooter::submit-button");
    assert_eq!(playlist_state.get_playlist_title(), "深夜 循环");
    assert_eq!(playlist_state.get_playlist_description(), "新的简介");
    assert_eq!(
        playlist_state
            .get_created_playlists()
            .row_data(0)
            .unwrap()
            .name,
        "深夜 循环"
    );
    playlist_state.invoke_open_playlist(created_id);
    playlist_state.invoke_open_playlist(101);
    assert_eq!(playlist_state.get_playlist_description(), "新的简介");

    // Deletion requires confirmation and is tied to the original route/id.
    let queue_count = queue_state.get_queue_songs().row_count();
    playlist_state.invoke_request_delete_playlist();
    mock_elapsed_time(Duration::from_millis(250));
    key(&app, Key::Escape);
    assert!(!dialog_state.get_is_dialog_open());
    playlist_state.invoke_confirm_playlist_delete();
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count + 1
    );
    playlist_state.invoke_request_delete_playlist();
    playlist_state.invoke_open_playlist(created_id);
    playlist_state.invoke_confirm_playlist_delete();
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count + 1
    );
    dialog_state.set_is_dialog_open(false);
    playlist_state.invoke_request_delete_playlist();
    mock_elapsed_time(Duration::from_millis(250));
    click(&app, "DialogContent::dialog-primary-btn");
    assert_eq!(navigation_state.get_current_route(), "home");
    assert_eq!(
        playlist_state.get_created_playlists().row_count(),
        initial_count
    );
    assert_eq!(queue_state.get_queue_songs().row_count(), queue_count);
    assert!(!playlist_state
        .get_created_playlists()
        .iter()
        .any(|p| p.id == created_id));

    // Permission changes and logout cannot mutate a hidden editor's target.
    playlist_state.invoke_open_playlist(101);
    playlist_state.set_playlist_owned(false);
    playlist_state.invoke_open_edit_playlist();
    assert_eq!(playlist_state.get_playlist_editor_mode(), "");
    playlist_state.set_playlist_owned(true);
    playlist_state.set_playlist_special_type(5);
    playlist_state.invoke_request_delete_playlist();
    assert!(!dialog_state.get_is_dialog_open());
    playlist_state.set_playlist_special_type(0);
    playlist_state.invoke_open_edit_playlist();
    playlist_state.set_playlist_editor_name("不能保存".into());
    account_state.set_is_logged_in(false);
    playlist_state.invoke_submit_playlist();
    assert_eq!(playlist_state.get_playlist_title(), "深夜 循环");
    playlist_state.invoke_open_create_playlist();
    assert!(login_state.get_is_login_open());
    login_state.set_is_login_open(false);
    account_state.set_is_logged_in(true);
    playlist_state.invoke_close_playlist_editor();

    // Compact windows and dark theme retain the same measured dialog geometry.
    app.window().set_size(PhysicalSize::new(620, 720));
    navigation_state.set_window_width(620.0);
    navigation_state.set_window_height(720.0);
    playlist_state.set_playlist_title(original_title);
    playlist_state.set_playlist_description(original_description);
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    for (name, id) in [
        ("header", "detail-header"),
        ("cover", "detail-cover-wrapper"),
        ("title", "detail-title"),
        ("actions", "detail-action-bar"),
        ("play", "detail-play-main-btn"),
        ("search", "detail-search-box"),
        ("subscribers", "detail-secondary-btn"),
        ("more", "playlist-actions-trigger"),
    ] {
        let component = match name {
            "play" | "search" | "subscribers" | "more" => "MediaDetailActions",
            "header" => "MediaDetailView",
            _ => "MediaDetailHeader",
        };
        check_rect(
            &app,
            &gt,
            "detailCompact",
            name,
            &format!("{component}::{id}"),
        );
    }
    snapshot(&app, "detail-compact");
    // The floating player overlaps the bottom of a tall header, just as it does
    // in Electron. Scroll the page before reaching the second action row.
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(450.0, 400.0),
        delta_x: 0.0,
        delta_y: -160.0,
    });
    mock_elapsed_time(Duration::from_millis(250));
    click(&app, "MediaDetailActions::playlist-actions-trigger");
    assert!(playlist_state.get_is_playlist_actions_open());
    key(&app, Key::Escape);
    playlist_state.invoke_open_playlist(101);
    app.global::<Theme>().set_is_dark(true);
    playlist_state.invoke_open_edit_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    check_editor(&app, &gt, "editCompact", true);
    snapshot(&app, "edit-compact-dark");
    playlist_state.invoke_close_playlist_editor();
    playlist_state.invoke_open_create_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    check_editor(&app, &gt, "createCompact", false);
    snapshot(&app, "create-compact-dark");
    playlist_state.set_playlist_editor_name("🎵".repeat(25).into());
    playlist_state.set_playlist_editor_description("文".repeat(1001).into());
    playlist_state.invoke_playlist_input_changed();
    assert_eq!(playlist_state.get_playlist_editor_name_length(), 40);
    assert_eq!(
        playlist_state.get_playlist_editor_description_length(),
        1000
    );
    app.hide().unwrap();
}

#[test]
fn demo_queue_actions_share_ordering_and_reset_behavior_with_runtime() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    playlists::attach(&app);
    let queue_state = app.global::<crate::QueueState>();
    let player_state = app.global::<crate::PlayerState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    queue_state.set_queue_songs(
        std::rc::Rc::new(slint::VecModel::from(
            [11, 22, 33]
                .map(|id| crate::QueueSongItem {
                    id,
                    name: format!("song-{id}").into(),
                    ..Default::default()
                })
                .to_vec(),
        ))
        .into(),
    );
    player_state.set_current_song_id(22);
    player_state.set_progress_percent(42.0);
    player_state.set_current_time_str("1:15".into());
    queue_state.invoke_queue_action("queue-next".into(), 11);
    queue_state.invoke_queue_action("queue-next".into(), 11);
    assert_eq!(
        queue_state
            .get_queue_songs()
            .iter()
            .map(|song| song.id)
            .collect::<Vec<_>>(),
        [22, 11, 33]
    );
    assert_eq!(player_state.get_current_song_id(), 22);
    assert_eq!(player_state.get_progress_percent(), 42.0);
    assert_eq!(player_state.get_current_time_str(), "1:15");
    queue_state.invoke_queue_action("remove-queue".into(), 22);
    assert_eq!(player_state.get_current_song_id(), 11);
    assert_eq!(player_state.get_progress_percent(), 0.0);
    queue_state.invoke_queue_action("clear-queue".into(), -1);
    assert_eq!(queue_state.get_queue_songs().row_count(), 0);
    assert_eq!(player_state.get_current_song_id(), -1);
    assert_eq!(player_state.get_current_time_str(), "0:00");
    assert!(!player_state.get_is_playing());
    assert!(!player_state.get_is_loading());
    let target = playlist_state.get_playlist_tracks().row_data(0).unwrap().id;
    queue_state.invoke_queue_action("queue-next".into(), target);
    assert_eq!(queue_state.get_queue_songs().row_count(), 1);
    assert_eq!(
        queue_state.get_queue_songs().row_data(0).unwrap().id,
        target
    );
    assert_eq!(player_state.get_current_song_id(), -1);
    queue_state.invoke_queue_action("queue-play".into(), target);
    assert_eq!(player_state.get_current_song_id(), target);
    assert!(player_state.get_is_playing());
}

#[test]
fn demo_recommendations_open_through_the_playlist_controller() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    playlists::attach(&app);
    let home_state = app.global::<crate::HomeState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let detail_state = app.global::<crate::DetailState>();
    let recommendation = crate::RecommendPlaylistItem {
        id: 1_000_000,
        title: "推荐歌单".into(),
        desc: "介绍".into(),
        ..Default::default()
    };
    home_state.set_recommend_playlists(
        std::rc::Rc::new(slint::VecModel::from(vec![recommendation])).into(),
    );
    assert!(playlist_state.invoke_open_playlist(1_000_000));
    assert_eq!(navigation_state.get_current_route(), "playlist");
    assert_eq!(playlist_state.get_playlist_title(), "推荐歌单");
    assert!(!playlist_state.get_playlist_owned());
    assert_eq!(
        playlist_state.get_playlist_tracks().row_count(),
        detail_state.get_detail_tracks().row_count()
    );
    assert!(!playlist_state.invoke_open_playlist(-1));
}
