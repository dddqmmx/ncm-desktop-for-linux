use crate::{playlists, AppState, AppWindow, Theme};
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

fn check_editor(app: &AppWindow, gt: &Value, view: &str, editing: bool) {
    for (name, id) in [
        ("card", "editor-card"),
        ("header", "editor-header"),
        ("close", "close-button"),
        ("name", "name-box"),
        ("cancel", "cancel-button"),
        ("submit", "submit-button"),
    ] {
        check_rect(app, gt, view, name, &format!("PlaylistEditor::{id}"));
    }
    let extra = if editing {
        [
            ("cover", "preview-cover"),
            ("description", "description-box"),
            ("footer", "editor-footer"),
        ]
        .as_slice()
    } else {
        [("privacy", "privacy-row"), ("toggle", "privacy-toggle")].as_slice()
    };
    for (name, id) in extra {
        check_rect(app, gt, view, name, &format!("PlaylistEditor::{id}"));
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
fn playlist_management_matches_electron_and_handles_user_actions() {
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
    let state = app.global::<AppState>();
    let original_title = state.get_playlist_title();
    let original_description = state.get_playlist_description();
    state.set_window_width(1435.0);
    state.set_window_height(1000.0);
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
        "SideNavBar::nav-group-header",
    );
    check_rect(
        &app,
        &gt,
        "sidebar",
        "create",
        "SideNavBar::create-playlist-btn",
    );
    let initial_count = state.get_created_playlists().row_count();
    click(&app, "SideNavBar::create-playlist-btn");
    assert_eq!(state.get_playlist_editor_mode(), "create");
    assert!(!state.get_playlist_editor_can_submit());
    check_editor(&app, &gt, "create", false);
    snapshot(&app, "create-light");
    state.set_playlist_editor_name(" \t\n　 ".into());
    state.invoke_playlist_input_changed();
    state.invoke_submit_playlist();
    assert_eq!(state.get_created_playlists().row_count(), initial_count);
    // Escape and outside clicks discard drafts.
    key(&app, Key::Escape);
    assert_eq!(state.get_playlist_editor_mode(), "");
    click(&app, "SideNavBar::create-playlist-btn");
    state.set_playlist_editor_name("  夜晚　  🎵  ".into());
    state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditor::privacy-toggle");
    click(&app, "PlaylistEditor::submit-button");
    assert_eq!(state.get_playlist_editor_mode(), "");
    assert_eq!(state.get_created_playlists().row_count(), initial_count + 1);
    assert_eq!(state.get_playlist_title(), "夜晚 🎵");
    assert!(state.get_playlist_is_private());
    assert_eq!(state.get_playlist_tracks().row_count(), 0);
    assert_eq!(
        state.get_detail_tracks().row_count(),
        8,
        "playlist navigation must not empty albums/favorites"
    );
    let created_id = state.get_active_playlist_id();
    state.invoke_submit_playlist();
    assert_eq!(
        state.get_created_playlists().row_count(),
        initial_count + 1,
        "duplicate submissions must not create another list"
    );

    state.invoke_open_playlist(101);
    mock_elapsed_time(Duration::from_millis(350));
    for (name, id) in [
        ("play", "detail-play-main-btn"),
        ("search", "detail-search-box"),
        ("subscribers", "detail-secondary-btn"),
        ("more", "playlist-actions-trigger"),
    ] {
        check_rect(&app, &gt, "detail", name, &format!("MediaDetailView::{id}"));
    }
    click(&app, "MediaDetailView::playlist-actions-trigger");
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
    assert!(!state.get_is_playlist_actions_open());
    click(&app, "MediaDetailView::playlist-actions-trigger");
    click(&app, "PlaylistActionsOverlay::edit-button");
    assert_eq!(state.get_playlist_editor_mode(), "edit");
    assert!(
        !state.get_playlist_editor_can_submit(),
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
    assert_eq!(state.get_playlist_editor_name(), "改名");
    assert!(state.get_playlist_editor_can_submit());
    key(&app, Key::Tab);
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: "键盘简介".into(),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: "键盘简介".into(),
    });
    assert!(
        state.get_playlist_editor_description().contains("键盘简介"),
        "Tab must move focus to the description"
    );
    state.set_playlist_editor_description(" 草稿 ".into());
    state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditor::cancel-button");
    assert_ne!(state.get_playlist_title(), "改名");
    state.invoke_open_edit_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(50));
    state.set_playlist_editor_name("  深夜   循环  ".into());
    state.set_playlist_editor_description(" 新的简介 \n".into());
    state.invoke_playlist_input_changed();
    click(&app, "PlaylistEditor::submit-button");
    assert_eq!(state.get_playlist_title(), "深夜 循环");
    assert_eq!(state.get_playlist_description(), "新的简介");
    assert_eq!(
        state.get_created_playlists().row_data(0).unwrap().name,
        "深夜 循环"
    );
    state.invoke_open_playlist(created_id);
    state.invoke_open_playlist(101);
    assert_eq!(state.get_playlist_description(), "新的简介");

    // Deletion requires confirmation and is tied to the original route/id.
    let queue_count = state.get_queue_songs().row_count();
    state.invoke_request_delete_playlist();
    mock_elapsed_time(Duration::from_millis(250));
    key(&app, Key::Escape);
    assert!(!state.get_is_dialog_open());
    state.invoke_confirm_playlist_delete();
    assert_eq!(state.get_created_playlists().row_count(), initial_count + 1);
    state.invoke_request_delete_playlist();
    state.invoke_open_playlist(created_id);
    state.invoke_confirm_playlist_delete();
    assert_eq!(state.get_created_playlists().row_count(), initial_count + 1);
    state.set_is_dialog_open(false);
    state.invoke_request_delete_playlist();
    mock_elapsed_time(Duration::from_millis(250));
    click(&app, "AppDialogModal::dialog-primary-btn");
    assert_eq!(state.get_current_route(), "home");
    assert_eq!(state.get_created_playlists().row_count(), initial_count);
    assert_eq!(state.get_queue_songs().row_count(), queue_count);
    assert!(!state
        .get_created_playlists()
        .iter()
        .any(|p| p.id == created_id));

    // Permission changes and logout cannot mutate a hidden editor's target.
    state.invoke_open_playlist(101);
    state.set_playlist_owned(false);
    state.invoke_open_edit_playlist();
    assert_eq!(state.get_playlist_editor_mode(), "");
    state.set_playlist_owned(true);
    state.set_playlist_special_type(5);
    state.invoke_request_delete_playlist();
    assert!(!state.get_is_dialog_open());
    state.set_playlist_special_type(0);
    state.invoke_open_edit_playlist();
    state.set_playlist_editor_name("不能保存".into());
    state.set_is_logged_in(false);
    state.invoke_submit_playlist();
    assert_eq!(state.get_playlist_title(), "深夜 循环");
    state.invoke_open_create_playlist();
    assert!(state.get_is_login_open());
    state.set_is_login_open(false);
    state.set_is_logged_in(true);
    state.invoke_close_playlist_editor();

    // Compact windows and dark theme retain the same measured dialog geometry.
    app.window().set_size(PhysicalSize::new(620, 720));
    state.set_window_width(620.0);
    state.set_window_height(720.0);
    state.set_playlist_title(original_title);
    state.set_playlist_description(original_description);
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
        check_rect(
            &app,
            &gt,
            "detailCompact",
            name,
            &format!("MediaDetailView::{id}"),
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
    click(&app, "MediaDetailView::playlist-actions-trigger");
    assert!(state.get_is_playlist_actions_open());
    key(&app, Key::Escape);
    state.invoke_open_playlist(101);
    app.global::<Theme>().set_is_dark(true);
    state.invoke_open_edit_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    check_editor(&app, &gt, "editCompact", true);
    snapshot(&app, "edit-compact-dark");
    state.invoke_close_playlist_editor();
    state.invoke_open_create_playlist();
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(1000));
    check_editor(&app, &gt, "createCompact", false);
    snapshot(&app, "create-compact-dark");
    state.set_playlist_editor_name("🎵".repeat(25).into());
    state.set_playlist_editor_description("文".repeat(1001).into());
    state.invoke_playlist_input_changed();
    assert_eq!(state.get_playlist_editor_name_length(), 40);
    assert_eq!(state.get_playlist_editor_description_length(), 1000);
    app.hide().unwrap();
}
