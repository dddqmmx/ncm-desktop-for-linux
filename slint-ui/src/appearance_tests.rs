use crate::{windows::ApplicationWindows, AppWindow, SettingsState, SettingsWindow, Theme};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{
    platform::{Key, PointerEventButton, WindowEvent},
    ComponentHandle, LogicalPosition, Model,
};
use std::time::Duration;

fn settle(window: &impl ComponentHandle) {
    let _ = window.root_element().size();
    mock_elapsed_time(Duration::from_millis(1600));
    let _ = window.root_element().size();
}

fn element(window: &impl ComponentHandle, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(window, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}

fn midpoint(item: &ElementHandle) -> LogicalPosition {
    let position = item.absolute_position();
    let size = item.size();
    LogicalPosition::new(position.x + size.width / 2., position.y + size.height / 2.)
}

fn button(window: &SettingsWindow, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(window, label)
        .next()
        .unwrap()
}

fn drag(window: &SettingsWindow, label: &str, to: LogicalPosition) {
    let from = midpoint(&button(window, label));
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: from });
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: to });
    let _ = window.root_element().size();
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: to,
            button: PointerEventButton::Left,
        });
    settle(window);
}

fn sides(state: &SettingsState<'_>) -> (Vec<String>, Vec<String>) {
    let ids = |items: slint::ModelRc<crate::AuxButtonItem>| {
        items.iter().map(|item| item.id.to_string()).collect()
    };
    (
        ids(state.get_fullscreen_aux_left()),
        ids(state.get_fullscreen_aux_right()),
    )
}

fn open_settings(app: &AppWindow, settings: &SettingsWindow) {
    app.show().unwrap();
    app.global::<SettingsState>().invoke_open_settings_window();
    settings
        .global::<SettingsState>()
        .set_settings_tab("appearance".into());
    settle(settings);
}

#[test]
fn appearance_drag_reorders_crosses_docks_and_preserves_visibility() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    let settings = &windows._settings;
    open_settings(&app, settings);
    let state = settings.global::<SettingsState>();
    let preview = element(settings, "AppearanceSettingsTab::player-preview");
    let position = preview.absolute_position();
    let size = preview.size();
    let docks = ElementHandle::find_by_element_type_name(settings, "AuxDock").collect::<Vec<_>>();
    let left_end = LogicalPosition::new(
        docks[0].absolute_position().x + docks[0].size().width - 2.,
        midpoint(&docks[0]).y,
    );
    let right_end = LogicalPosition::new(
        docks[1].absolute_position().x + docks[1].size().width - 2.,
        midpoint(&docks[1]).y,
    );

    drag(settings, "喜欢", left_end);
    assert_eq!(sides(&state).0, ["comment", "favorite"]);
    drag(settings, "喜欢", right_end);
    assert_eq!(
        sides(&state),
        (
            vec!["comment".into()],
            vec!["playMode".into(), "volume".into(), "favorite".into()]
        )
    );
    drag(settings, "评论", right_end);
    assert!(state.get_fullscreen_aux_left().row_count() == 0);
    assert_eq!(state.get_fullscreen_aux_right().row_count(), 4);
    for item in state.get_fullscreen_aux_right().iter() {
        assert!(item.visible, "drag toggled {}", item.id);
    }
    // An empty dock must remain a valid target.
    let empty_left = LogicalPosition::new(position.x + 24., position.y + size.height / 2.);
    drag(settings, "喜欢", empty_left);
    assert_eq!(sides(&state).0, ["favorite"]);
    assert_eq!(sides(&app.global::<SettingsState>()), sides(&state));

    // A real click immediately after a drag still toggles; a hidden button can move.
    button(settings, "喜欢").mock_single_click(PointerEventButton::Left);
    settle(settings);
    assert!(!state.get_fullscreen_aux_left().row_data(0).unwrap().visible);
    drag(settings, "喜欢", right_end);
    assert!(
        !state
            .get_fullscreen_aux_right()
            .row_data(3)
            .unwrap()
            .visible
    );
    let before = sides(&state);
    drag(
        settings,
        "喜欢",
        LogicalPosition::new(position.x + 30., position.y - 20.),
    );
    assert_eq!(sides(&state), before, "dropping outside moved a button");
    assert!(
        !state
            .get_fullscreen_aux_right()
            .row_data(3)
            .unwrap()
            .visible
    );
}

#[test]
fn appearance_palette_selects_hsv_syncs_windows_and_reopens_at_current_color() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    let settings = &windows._settings;
    open_settings(&app, settings);
    let picker = || element(settings, "AppearanceSettingsTab::accent-picker");
    app.global::<SettingsState>()
        .invoke_set_accent_hex("#ff0000".into());
    settle(settings);
    // ElementHandle click maps popup coordinates.
    picker().mock_single_click(PointerEventButton::Left);
    settle(settings);
    crate::window_tests::snapshot(settings, "appearance-palette");
    element(settings, "ColorPalette::hue-field").mock_single_click(PointerEventButton::Left);
    settle(settings);
    assert_eq!(
        settings.global::<SettingsState>().get_setting_accent_hex(),
        "#00ffff"
    );
    element(settings, "ColorPalette::saturation-field").mock_single_click(PointerEventButton::Left);
    settle(settings);
    assert_eq!(
        settings.global::<SettingsState>().get_setting_accent_hex(),
        "#407f7f"
    );
    assert_eq!(
        app.global::<Theme>().get_theme_color(),
        slint::Color::from_rgb_u8(64, 127, 127)
    );
    assert_eq!(
        app.global::<Theme>().get_theme_color_strong(),
        slint::Color::from_rgb_u8(52, 104, 104)
    );
    let escape: slint::SharedString = Key::Escape.into();
    settings.window().dispatch_event(WindowEvent::KeyPressed {
        text: escape.clone(),
    });
    settings
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: escape });
    settle(settings);
    assert!(
        settings.window().is_visible(),
        "palette Escape closed settings"
    );
    assert!(
        ElementHandle::find_by_element_id(settings, "ColorPalette::saturation-field")
            .next()
            .is_none()
    );
    app.global::<SettingsState>()
        .invoke_set_accent_hex("#123456".into());
    settle(settings);
    picker().mock_single_click(PointerEventButton::Left);
    settle(settings);
    assert_eq!(
        element(settings, "ColorPalette::hex-input")
            .accessible_value()
            .as_deref(),
        Some("#123456")
    );
}

#[test]
fn appearance_accent_rejects_invalid_hex_without_changing_theme() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let _windows = ApplicationWindows::attach(&app).unwrap();
    let state = app.global::<SettingsState>();
    let theme = app.global::<Theme>();
    let original = theme.get_theme_color();
    for hex in ["#fff", "ffffff", "#1234567", "#ffgg00", "#💚💚"] {
        assert!(!state.invoke_set_accent_hex(hex.into()));
        assert_eq!(theme.get_theme_color(), original);
    }
    assert!(state.invoke_set_accent_hex(" #AbC123 ".into()));
    assert_eq!(state.get_setting_accent_hex(), "#abc123");
    assert_eq!(
        theme.get_theme_color(),
        slint::Color::from_rgb_u8(171, 193, 35)
    );
}

#[test]
fn appearance_fullscreen_aux_buttons_fit_when_all_moved_to_one_side() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let _windows = ApplicationWindows::attach(&app).unwrap();
    let state = app.global::<SettingsState>();
    app.global::<crate::AudioState>()
        .set_setting_software_volume(true);
    app.global::<crate::AudioState>()
        .set_setting_strict_bitperfect(false);
    app.global::<crate::PlayerViewState>()
        .set_is_fullscreen(true);
    app.show().unwrap();
    for left in [true, false] {
        for id in ["favorite", "comment", "playMode", "volume"] {
            state.invoke_move_fullscreen_aux(id.into(), left, 4);
        }
        settle(&app);
        let group = ElementHandle::find_by_element_type_name(&app, "FullscreenAuxGroup")
            .find(|group| {
                group
                    .query_descendants()
                    .match_id("FullscreenAuxGroup::aux-button")
                    .find_all()
                    .len()
                    == 4
            })
            .unwrap();
        let mut buttons = group
            .query_descendants()
            .match_id("FullscreenAuxGroup::aux-button")
            .find_all();
        buttons.sort_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x));
        for button in &buttons {
            assert!(button.absolute_position().x >= group.absolute_position().x - 0.5);
            assert!(
                button.absolute_position().x + button.size().width
                    <= group.absolute_position().x + group.size().width + 0.5
            );
        }
        for pair in buttons.windows(2) {
            assert!(
                pair[0].absolute_position().x + pair[0].size().width
                    <= pair[1].absolute_position().x + 0.5
            );
        }
    }
}
