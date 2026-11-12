use crate::{windows::ApplicationWindows, AppWindow, SettingsWindow, Theme};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{
    platform::{Key, PointerEventButton, WindowEvent},
    ComponentHandle, LogicalSize, Model, PhysicalSize,
};
use std::time::Duration;

fn settle() {
    mock_elapsed_time(Duration::from_millis(600));
}

fn element(window: &impl ComponentHandle, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(window, id)
        .next()
        .unwrap_or_else(|| panic!("Missing {id}"))
}

fn click(window: &impl ComponentHandle, id: &str) {
    element(window, id).mock_single_click(PointerEventButton::Left);
    let _ = window.root_element().size();
    settle();
}

fn check_settings_geometry(window: &SettingsWindow, size: &str) {
    let gt: serde_json::Value =
        serde_json::from_str(include_str!("../tests/settings-window-ground-truth.json")).unwrap();
    for (name, id) in [
        ("window", "SettingsModal::settings-window"),
        ("sidebar", "SettingsModal::settings-sidebar"),
        ("navIndicator", "SettingsSidebar::settings-nav-indicator"),
        ("navItem", "SettingsSidebar::settings-nav-general"),
        ("mainHeader", "SettingsMain::settings-main-header"),
        ("closeBtn", "SettingsHeader::settings-close-btn"),
        ("contentScroll", "SettingsMain::settings-content-scroll"),
        ("controlCard", "SettingGroupCard::control-card-box"),
        ("controlRow", "SettingRowItem::control-row-box"),
        (
            "modernSwitch",
            "GeneralSettingsTab::modern-switch-auto-launch",
        ),
    ] {
        let item = element(window, id);
        let parent = match name {
            "controlCard" => Some("SettingsMain::settings-content-scroll"),
            "controlRow" => Some("SettingGroupCard::control-card-box"),
            "modernSwitch" => Some("SettingRowItem::control-row-box"),
            _ => None,
        };
        if let Some(parent) = parent {
            contains(&element(window, parent), &item, size);
            continue;
        }
        let pos = item.absolute_position();
        let bounds = item.size();
        for (field, value) in [
            ("x", pos.x),
            ("y", pos.y),
            ("width", bounds.width),
            ("height", bounds.height),
        ] {
            let expected = gt[size][name]["rect"][field].as_f64().unwrap() as f32;
            assert!(
                (value - expected).abs() <= 0.5,
                "settings {size} {name}.{field}: Slint {value} != Electron {expected}"
            );
        }
    }
}

fn contains(outer: &ElementHandle, inner: &ElementHandle, context: &str) {
    let a = outer.absolute_position();
    let b = inner.absolute_position();
    let aw = outer.size();
    let bw = inner.size();
    if bw.width <= 0.0 || bw.height <= 0.0 {
        return;
    }
    assert!(
        b.x >= a.x - 0.5
            && b.y >= a.y - 0.5
            && b.x + bw.width <= a.x + aw.width + 0.5
            && b.y + bw.height <= a.y + aw.height + 0.5,
        "{context}: {:?} at {b:?} {bw:?} overflows {a:?} {aw:?}",
        inner.id()
    );
}

fn check_settings_contents(window: &SettingsWindow, tab: &str) {
    for row in ElementHandle::find_by_element_id(window, "SettingRowItem::control-row-box") {
        let child = |id| row.query_descendants().match_id(id).find_first().unwrap();
        let info = child("SettingRowItem::row-info");
        let actions = child("SettingRowItem::row-actions");
        contains(&row, &info, tab);
        contains(&row, &actions, tab);
        assert!(
            info.absolute_position().x + info.size().width + 8.0 <= actions.absolute_position().x,
            "{tab}: text overlaps actions"
        );
        for area in [&info, &actions] {
            area.visit_descendants::<()>(|child| {
                contains(area, &child, tab);
                std::ops::ControlFlow::Continue(())
            });
        }
    }
    for id in [
        "CacheSettingsTab::cache-card",
        "LibrarySettingsTab::library-path-row",
    ] {
        for card in ElementHandle::find_by_element_id(window, id) {
            card.visit_descendants::<()>(|child| {
                contains(&card, &child, tab);
                std::ops::ControlFlow::Continue(())
            });
        }
    }
    for row in ElementHandle::find_by_element_id(window, "SettingRowItem::control-row-box") {
        let title = row
            .query_descendants()
            .match_id("SettingRowItem::row-title")
            .find_first()
            .unwrap();
        if let Some(description) = row
            .query_descendants()
            .match_id("SettingRowItem::row-description")
            .find_first()
        {
            assert!(
                (description.absolute_position().y
                    - title.absolute_position().y
                    - title.size().height
                    - 2.0)
                    .abs()
                    < 0.5,
                "{tab}: description should flow 2px below the measured title"
            );
        }
    }
}

pub(crate) fn snapshot(window: &impl ComponentHandle, name: &str) {
    if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = window.window().take_snapshot().unwrap();
        let mut data = format!("P6\n{} {}\n255\n", pixels.width(), pixels.height()).into_bytes();
        for pixel in pixels.as_slice() {
            data.extend([pixel.r, pixel.g, pixel.b]);
        }
        std::fs::write(dir.join(format!("{name}.ppm")), data).unwrap();
    }
}

#[test]
fn settings_drag_does_not_capture_later_clicks() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    let settings = &windows._settings;
    app.show().unwrap();
    app.global::<crate::SettingsState>()
        .invoke_open_settings_window();
    settle();

    // A native window move can consume the release. The next click must reach
    // its target even if the compositor sends neither release nor pointer exit.
    for (area, tab, dx, dy, released, exited) in [
        ("settings-main-header", "general", 80.0, 10.0, false, false),
        ("settings-main-header", "general", 80.0, 10.0, false, true),
        ("settings-main-header", "general", 80.0, 10.0, true, false),
        (
            "settings-content-scroll",
            "general",
            0.0,
            -120.0,
            true,
            false,
        ),
        ("settings-content-scroll", "audio", 0.0, -120.0, true, false),
        ("settings-content-scroll", "audio", 80.0, 0.0, true, false),
    ] {
        click(settings, &format!("SettingsSidebar::settings-nav-{tab}"));
        let origin = element(settings, &format!("SettingsMain::{area}")).absolute_position();
        let start = slint::LogicalPosition::new(origin.x + 20.0, origin.y + 40.0);
        let end = slint::LogicalPosition::new(start.x + dx, start.y + dy);
        settings
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position: start });
        settings
            .window()
            .dispatch_event(WindowEvent::PointerPressed {
                position: start,
                button: PointerEventButton::Left,
            });
        mock_elapsed_time(Duration::from_millis(30));
        settings
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position: end });
        if released {
            settings
                .window()
                .dispatch_event(WindowEvent::PointerReleased {
                    position: end,
                    button: PointerEventButton::Left,
                });
        }
        if exited {
            settings.window().dispatch_event(WindowEvent::PointerExited);
        }

        let prefs = settings;
        click(settings, "SettingsSidebar::settings-nav-debug");
        assert_eq!(
            prefs.global::<crate::SettingsState>().get_settings_tab(),
            "debug",
            "navigation was captured after {area} drag in {tab} (released={released}, exited={exited})"
        );
        click(settings, "SettingsSidebar::settings-nav-general");
        assert_eq!(
            prefs.global::<crate::SettingsState>().get_settings_tab(),
            "general"
        );
        let auto_launch = prefs
            .global::<crate::SettingsState>()
            .get_setting_auto_launch();
        click(settings, "GeneralSettingsTab::modern-switch-auto-launch");
        assert_eq!(
            prefs
                .global::<crate::SettingsState>()
                .get_setting_auto_launch(),
            !auto_launch
        );
        assert_eq!(
            app.global::<crate::SettingsState>()
                .get_setting_auto_launch(),
            !auto_launch
        );
        click(settings, "GeneralSettingsTab::renderer-select");
        assert_eq!(
            ElementHandle::find_by_element_id(settings, "DropdownMenu::option").count(),
            3
        );
        ElementHandle::find_by_accessible_label(settings, "软件渲染（关闭 GPU 加速）")
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        settle();
        assert_eq!(
            prefs
                .global::<crate::SettingsState>()
                .get_setting_renderer(),
            "software"
        );
        assert_eq!(
            ElementHandle::find_by_element_id(settings, "DropdownMenu::option").count(),
            0
        );
        click(settings, "SettingsHeader::settings-close-btn");
        assert!(
            !settings.window().is_visible(),
            "close was captured after drag"
        );
        app.global::<crate::SettingsState>()
            .invoke_open_settings_window();
        settle();
    }
}

#[test]
fn bitperfect_disables_software_volume_and_restores_the_previous_preference() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    let settings = &windows._settings;
    app.show().unwrap();
    app.global::<crate::SettingsState>()
        .invoke_open_settings_window();
    settings.window().set_size(PhysicalSize::new(1000, 1400));
    click(settings, "SettingsSidebar::settings-nav-audio");
    let audio_state = app.global::<crate::AudioState>();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let prefs = settings;
    let software_switch = element(settings, "AudioSettingsTab::software-volume-switch");
    let knob = software_switch
        .query_descendants()
        .match_id("ModernSwitch::knob")
        .find_first()
        .unwrap();
    let slider = element(settings, "AudioSettingsTab::application-volume")
        .query_descendants()
        .match_id("VolumeRow::slider")
        .find_first()
        .unwrap();

    for software_enabled in [true, false] {
        audio_state.set_setting_software_volume(software_enabled);
        audio_state.set_volume_percent(37.0);
        player_view_state.set_is_volume_popup_open(software_enabled);
        player_view_state.set_is_fullscreen_volume_open(software_enabled);
        settle();
        click(settings, "AudioSettingsTab::bitperfect-switch");
        assert!(audio_state.get_setting_strict_bitperfect());
        assert!(!audio_state.get_can_adjust_volume());
        assert!(!prefs.global::<crate::AudioState>().get_can_adjust_volume());
        assert!(
            !audio_state.get_session_volume_locked(),
            "only the player sets the session lock"
        );
        assert!(!player_view_state.get_is_volume_popup_open());
        assert!(!player_view_state.get_is_fullscreen_volume_open());
        assert!((software_switch.computed_opacity() - 0.55).abs() < 0.01);
        assert!(
            (knob.absolute_position().x - software_switch.absolute_position().x - 2.0).abs() < 0.1
        );

        click(settings, "AudioSettingsTab::software-volume-switch");
        slider.mock_single_click(PointerEventButton::Left);
        settle();
        assert_eq!(audio_state.get_setting_software_volume(), software_enabled);
        assert_eq!(
            prefs
                .global::<crate::AudioState>()
                .get_setting_software_volume(),
            software_enabled
        );
        assert_eq!(audio_state.get_volume_percent(), 37.0);
        assert_eq!(
            prefs.global::<crate::AudioState>().get_volume_percent(),
            37.0
        );

        click(settings, "AudioSettingsTab::bitperfect-switch");
        assert!(!audio_state.get_setting_strict_bitperfect());
        assert_eq!(audio_state.get_can_adjust_volume(), software_enabled);
        assert_eq!(
            prefs.global::<crate::AudioState>().get_can_adjust_volume(),
            software_enabled
        );
        assert!((software_switch.computed_opacity() - 1.0).abs() < 0.01);
        let expected_x = if software_enabled { 22.0 } else { 2.0 };
        assert!(
            (knob.absolute_position().x - software_switch.absolute_position().x - expected_x).abs()
                < 0.1
        );

        click(settings, "AudioSettingsTab::software-volume-switch");
        assert_eq!(audio_state.get_setting_software_volume(), !software_enabled);
        assert_eq!(audio_state.get_can_adjust_volume(), !software_enabled);
    }

    // A still-locked native session must also block the settings slider until
    // the player confirms that switching out of BitPerfect has completed.
    audio_state.set_session_volume_locked(true);
    settle();
    assert!(prefs
        .global::<crate::AudioState>()
        .get_session_volume_locked());
    slider.mock_single_click(PointerEventButton::Left);
    settle();
    assert_eq!(audio_state.get_volume_percent(), 37.0);
    audio_state.set_session_volume_locked(false);
    settle();
    slider.mock_single_click(PointerEventButton::Left);
    settle();
    assert!((audio_state.get_volume_percent() - 50.0).abs() < 0.1);
}

#[test]
fn settings_is_an_independent_reusable_window_with_shared_preferences() {
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
    app.show().unwrap();
    let navigation_state = app.global::<crate::NavigationState>();
    let settings_state = app.global::<crate::SettingsState>();
    let audio_state = app.global::<crate::AudioState>();
    let prefs = settings;
    let main_theme = app.global::<Theme>();
    let settings_theme = settings.global::<Theme>();
    navigation_state.set_current_route("search".into());
    settle();
    assert_eq!(app.window().size(), PhysicalSize::new(1435, 1000));
    assert!(!settings.window().is_visible());
    assert!(
        ElementHandle::find_by_element_id(&app, "SettingsModal::settings-window")
            .next()
            .is_none()
    );

    app.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(120.0, 940.0),
    });
    settle();
    element(&app, "SidebarProfileMenu::settings-menu-item").invoke_accessible_default_action();
    settle();
    assert!(settings.window().is_visible());
    assert!(app.window().is_visible());
    assert_eq!(settings.window().size(), PhysicalSize::new(1000, 800));
    assert_eq!(
        prefs.global::<crate::SettingsState>().get_settings_tab(),
        "general"
    );
    assert_eq!(navigation_state.get_current_route(), "search");
    assert_eq!(app.window().size(), PhysicalSize::new(1435, 1000));
    assert_eq!(
        settings.root_element().size(),
        LogicalSize::new(1000.0, 800.0)
    );
    check_settings_geometry(settings, "1000x800");
    snapshot(settings, "settings-general-light");

    click(settings, "GeneralSettingsTab::modern-switch-auto-launch");
    assert!(prefs
        .global::<crate::SettingsState>()
        .get_setting_auto_launch());
    assert!(settings_state.get_setting_auto_launch());
    audio_state.set_volume_percent(37.0);
    settle();
    assert_eq!(
        prefs.global::<crate::AudioState>().get_volume_percent(),
        37.0
    );
    prefs.global::<crate::AudioState>().set_volume_percent(62.0);
    settle();
    assert_eq!(audio_state.get_volume_percent(), 62.0);

    // Changes to unrelated preferences in the same event-loop turn must both survive.
    audio_state.set_volume_percent(41.0);
    prefs
        .global::<crate::SettingsState>()
        .set_setting_acrylic(false);
    settle();
    assert_eq!(
        prefs.global::<crate::AudioState>().get_volume_percent(),
        41.0
    );
    assert!(!settings_state.get_setting_acrylic());

    click(settings, "SettingsSidebar::settings-nav-appearance");
    click(settings, "SegmentedSlider3::btn-1");
    assert!(main_theme.get_is_dark());
    assert_eq!(main_theme.get_theme_mode(), "dark");
    assert_eq!(navigation_state.get_current_route(), "search");
    settings_state.invoke_open_settings_window();
    settle();
    assert_eq!(
        prefs.global::<crate::SettingsState>().get_settings_tab(),
        "appearance",
        "focusing an open window preserves its tab"
    );
    snapshot(settings, "settings-appearance-dark");
    main_theme.set_is_dark(false);
    main_theme.set_theme_mode("light".into());
    settle();
    assert!(!settings_theme.get_is_dark());
    assert_eq!(settings_theme.get_theme_mode(), "light");

    // A shared model mutation must also update the main player controls.
    let aux = prefs
        .global::<crate::SettingsState>()
        .get_fullscreen_aux_left();
    let mut favorite = aux.row_data(0).unwrap();
    favorite.visible = false;
    aux.set_row_data(0, favorite);
    settle();
    assert!(
        !settings_state
            .get_fullscreen_aux_left()
            .row_data(0)
            .unwrap()
            .visible
    );

    // Every tab remains reachable at the real minimum window size.
    for tab in [
        "general",
        "audio",
        "appearance",
        "library",
        "cache",
        "shortcuts",
        "debug",
        "about",
    ] {
        click(settings, &format!("SettingsSidebar::settings-nav-{tab}"));
        assert_eq!(
            prefs.global::<crate::SettingsState>().get_settings_tab(),
            tab
        );
        check_settings_contents(settings, tab);
        let close = element(settings, "SettingsHeader::settings-close-btn");
        assert!(close.absolute_position().x + close.size().width <= 1000.0);
        snapshot(settings, &format!("settings-{tab}-light"));
    }
    // Scroll the long audio page, then ensure switching tabs starts at the top.
    click(settings, "SettingsSidebar::settings-nav-audio");
    prefs
        .global::<crate::AudioState>()
        .set_setting_supports_exclusive(false);
    prefs
        .global::<crate::AudioState>()
        .set_setting_output_device_label("Very long USB DAC device name ".repeat(8).into());
    prefs.global::<crate::AudioState>().set_output_devices(
        std::rc::Rc::new(slint::VecModel::from(vec![crate::OutputDeviceItem {
            id: "hw:long-name".into(),
            name: prefs
                .global::<crate::AudioState>()
                .get_setting_output_device_label(),
            current: true,
        }]))
        .into(),
    );
    prefs
        .global::<crate::AudioState>()
        .set_setting_output_device_index(0);
    settle();
    check_settings_contents(settings, "audio");
    settings
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(700.0, 600.0),
            delta_x: 0.0,
            delta_y: -500.0,
        });
    settle();
    snapshot(settings, "settings-audio-scrolled");
    click(settings, "SettingsSidebar::settings-nav-general");
    check_settings_geometry(settings, "1000x800");
    settings.window().set_size(PhysicalSize::new(1200, 900));
    click(settings, "SettingsSidebar::settings-nav-general");
    settle();
    check_settings_geometry(settings, "1200x900");
    for tab in [
        "general",
        "audio",
        "appearance",
        "library",
        "cache",
        "shortcuts",
        "debug",
        "about",
    ] {
        click(settings, &format!("SettingsSidebar::settings-nav-{tab}"));
        check_settings_contents(settings, tab);
    }
    assert_eq!(
        settings.root_element().size(),
        LogicalSize::new(1200.0, 900.0)
    );
    assert_eq!(app.window().size(), PhysicalSize::new(1435, 1000));
    click(settings, "SettingsHeader::settings-close-btn");
    assert!(!settings.window().is_visible());
    assert!(app.window().is_visible());

    settings_state.invoke_open_settings_window();
    settle();
    assert_eq!(
        prefs.global::<crate::SettingsState>().get_settings_tab(),
        "general"
    );
    assert!(prefs
        .global::<crate::SettingsState>()
        .get_setting_auto_launch());
    assert_eq!(
        prefs.global::<crate::AudioState>().get_volume_percent(),
        41.0
    );
    let escape: slint::SharedString = Key::Escape.into();
    settings.window().dispatch_event(WindowEvent::KeyPressed {
        text: escape.clone(),
    });
    settings
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: escape });
    settle();
    assert!(!settings.window().is_visible(), "Escape closes settings");
    assert!(app.window().is_visible());

    settings_state.invoke_open_settings_window();
    settle();
    settings
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(
        !settings.window().is_visible(),
        "native close closes only settings"
    );
    assert!(app.window().is_visible());
    assert_eq!(navigation_state.get_current_route(), "search");
    settings_state.invoke_open_settings_window();
    settle();
    app.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!app.window().is_visible());
    assert!(
        !settings.window().is_visible(),
        "closing main leaves no orphan settings window"
    );
}

#[test]
fn settings_dropdowns_commit_only_confirmed_choices_and_support_keyboard() {
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
    let settings_state = app.global::<crate::SettingsState>();
    let audio_state = app.global::<crate::AudioState>();
    let prefs = settings;
    app.show().unwrap();
    settings_state.invoke_open_settings_window();
    settle();
    assert_eq!(
        prefs
            .global::<crate::SettingsState>()
            .get_setting_renderer(),
        "vulkan"
    );
    assert!(!prefs
        .global::<crate::SettingsState>()
        .get_renderer_restart_required());
    click(settings, "GeneralSettingsTab::renderer-select");
    assert_eq!(
        prefs
            .global::<crate::SettingsState>()
            .get_setting_renderer(),
        "vulkan"
    );
    snapshot(settings, "settings-renderer-dropdown-light");
    ElementHandle::find_by_accessible_label(settings, "软件渲染（关闭 GPU 加速）")
        .next()
        .unwrap()
        .mock_single_click(PointerEventButton::Left);
    settle();
    assert_eq!(
        prefs
            .global::<crate::SettingsState>()
            .get_setting_renderer(),
        "software"
    );
    assert_eq!(settings_state.get_setting_renderer(), "software");
    assert_eq!(
        prefs
            .global::<crate::SettingsState>()
            .get_startup_renderer(),
        "vulkan"
    );
    assert!(prefs
        .global::<crate::SettingsState>()
        .get_renderer_restart_required());
    click(settings, "GeneralSettingsTab::renderer-select");
    ElementHandle::find_by_accessible_label(settings, "Vulkan（默认）")
        .next()
        .unwrap()
        .mock_single_click(PointerEventButton::Left);
    settle();
    assert_eq!(settings_state.get_setting_renderer(), "vulkan");
    assert!(!prefs
        .global::<crate::SettingsState>()
        .get_renderer_restart_required());
    click(settings, "SettingsSidebar::settings-nav-audio");
    let quality = element(settings, "AudioSettingsTab::quality-select");
    let key = |key: Key| {
        let text: slint::SharedString = key.into();
        settings
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        settings
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        settle();
    };
    let options =
        || ElementHandle::find_by_element_id(settings, "DropdownMenu::option").collect::<Vec<_>>();
    assert!(options().is_empty(), "closed dropdown created its options");
    click(settings, "AudioSettingsTab::quality-select");
    assert_eq!(quality.accessible_expanded(), Some(true));
    assert_eq!(
        prefs
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "lossless",
        "opening changed the quality"
    );
    assert_eq!(options().len(), 7);
    assert_eq!(
        options()
            .iter()
            .filter(|row| row.accessible_item_selected() == Some(true))
            .count(),
        1
    );
    snapshot(settings, "settings-quality-dropdown-light");
    key(Key::DownArrow);
    assert_eq!(
        prefs
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "lossless",
        "navigation committed early"
    );
    key(Key::Escape);
    assert_eq!(quality.accessible_expanded(), Some(false));
    assert!(
        settings.window().is_visible(),
        "Escape closed settings instead of the menu"
    );
    assert_eq!(
        prefs
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "lossless"
    );
    assert!(options().is_empty());

    click(settings, "AudioSettingsTab::quality-select");
    for pressed in [true, false] {
        let position = slint::LogicalPosition::new(510.0, 120.0);
        settings.window().dispatch_event(if pressed {
            WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            }
        } else {
            WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            }
        });
    }
    settle();
    assert_eq!(quality.accessible_expanded(), Some(false));
    assert_eq!(
        prefs
            .global::<crate::AudioState>()
            .get_setting_sound_quality(),
        "lossless"
    );

    // Non-adjacent choices, including all high-quality labels, must map to exact values.
    for (value, label) in [
        ("jymaster", "超清母带 (192khz, 24bit)"),
        ("standard", "标准 (128kbps)"),
        ("sky", "沉浸环绕声 (最高5.1声道)"),
        ("exhigh", "极高 (320kbps)"),
        ("jyeffect", "高清臻音 (96khz, 24bit)"),
        ("hires", "Hi-Res (最高192khz, 24bit)"),
        ("lossless", "无损 (最高48khz, 16bit)"),
    ] {
        click(settings, "AudioSettingsTab::quality-select");
        ElementHandle::find_by_accessible_label(settings, label)
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        settle();
        assert_eq!(
            prefs
                .global::<crate::AudioState>()
                .get_setting_sound_quality(),
            value
        );
        assert_eq!(audio_state.get_setting_sound_quality(), value);
        assert_eq!(quality.accessible_value().unwrap(), label);
        assert_eq!(audio_state.get_setting_sound_quality_label(), label);
        assert_eq!(quality.accessible_expanded(), Some(false));
    }
    key(Key::Return);
    assert_eq!(quality.accessible_expanded(), Some(true));
    key(Key::End);
    key(Key::Return);
    assert_eq!(audio_state.get_setting_sound_quality(), "jymaster");
    assert_eq!(quality.accessible_expanded(), Some(false));

    // The same control handles a changing device model and a long scrollable menu.
    audio_state.set_output_devices(
        std::rc::Rc::new(slint::VecModel::from(
            (0..50)
                .map(|index| crate::OutputDeviceItem {
                    id: format!("hw:{index}").into(),
                    name: format!("Device {index}").into(),
                    current: index == 30,
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    audio_state.set_setting_output_device_index(30);
    prefs
        .global::<crate::RuntimeState>()
        .set_runtime_enabled(true);
    let selected_device = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let selected = selected_device.clone();
    prefs
        .global::<crate::RuntimeState>()
        .on_runtime_action(move |action, _, value| {
            assert_eq!(action, "select-device");
            *selected.borrow_mut() = value.to_string();
        });
    settings
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(800.0, 600.0),
            delta_x: 0.0,
            delta_y: -300.0,
        });
    settle();
    click(settings, "OutputDevicePicker::device-select");
    let device = element(settings, "OutputDevicePicker::device-select");
    assert_eq!(device.accessible_expanded(), Some(true));
    assert!(
        options().len() <= 12,
        "device menu eagerly created all rows"
    );
    key(Key::End);
    key(Key::Return);
    assert_eq!(*selected_device.borrow(), "hw:49");
    assert_eq!(
        prefs
            .global::<crate::AudioState>()
            .get_setting_output_device_index(),
        30,
        "device changes await backend confirmation"
    );
    assert_eq!(device.accessible_expanded(), Some(false));
    audio_state.set_output_devices(
        std::rc::Rc::new(slint::VecModel::<crate::OutputDeviceItem>::default()).into(),
    );
    audio_state.set_setting_output_device_index(-1);
    settle();
    click(settings, "OutputDevicePicker::device-select");
    assert_eq!(device.accessible_enabled(), Some(false));
    assert_eq!(device.accessible_expanded(), Some(false));
}

// Observe the actual constraints Slint supplies to a platform adapter. The standard
// TestingBackend permits arbitrary set_size calls and cannot test OS size limits.
#[test]
fn window_constraints_match_electron() {
    use slint::platform::{
        software_renderer::SoftwareRenderer, LayoutConstraints, Platform, Renderer, WindowAdapter,
        WindowProperties,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::{Rc, Weak},
    };

    struct ProbeWindow {
        window: slint::Window,
        renderer: SoftwareRenderer,
        constraints: Cell<LayoutConstraints>,
    }
    impl WindowAdapter for ProbeWindow {
        fn window(&self) -> &slint::Window {
            &self.window
        }
        fn renderer(&self) -> &dyn Renderer {
            &self.renderer
        }
        fn size(&self) -> PhysicalSize {
            PhysicalSize::new(1435, 1000)
        }
        fn update_window_properties(&self, properties: WindowProperties<'_>) {
            self.constraints.set(properties.layout_constraints());
        }
    }
    struct ProbePlatform(Rc<RefCell<Vec<Rc<ProbeWindow>>>>);
    impl Platform for ProbePlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            let window = Rc::new_cyclic(|weak: &Weak<ProbeWindow>| ProbeWindow {
                window: slint::Window::new(weak.clone()),
                renderer: SoftwareRenderer::default(),
                constraints: Cell::default(),
            });
            self.0.borrow_mut().push(window.clone());
            Ok(window)
        }
    }
    let probes = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(probes.clone()))).unwrap();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let settings = SettingsWindow::new().unwrap();
    settings.show().unwrap();
    let probes = probes.borrow();
    assert_eq!(probes.len(), 2);
    for (probe, minimum, preferred) in [
        (
            &probes[0],
            LogicalSize::new(1340.0, 960.0),
            LogicalSize::new(1435.0, 1000.0),
        ),
        (
            &probes[1],
            LogicalSize::new(1000.0, 800.0),
            LogicalSize::new(1000.0, 800.0),
        ),
    ] {
        let actual = probe.constraints.get();
        assert_eq!(actual.min, Some(minimum));
        assert_eq!(actual.preferred, preferred);
        assert!(
            actual.max.is_none(),
            "Electron permits enlarging both windows without an upper bound"
        );
    }
}
