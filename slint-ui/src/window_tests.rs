use crate::{windows::ApplicationWindows, AppState, AppWindow, SettingsWindow, Theme};
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
        ("navIndicator", "SettingsModal::settings-nav-indicator"),
        ("navItem", "SettingsModal::settings-nav-general"),
        ("mainHeader", "SettingsModal::settings-main-header"),
        ("closeBtn", "SettingsModal::settings-close-btn"),
        ("contentScroll", "SettingsModal::settings-content-scroll"),
        ("controlCard", "SettingGroupCard::control-card-box"),
        ("controlRow", "SettingRowItem::control-row-box"),
        ("modernSwitch", "SettingsModal::modern-switch-auto-launch"),
    ] {
        let item = element(window, id);
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
        "SettingsModal::cache-card",
        "SettingsModal::library-path-row",
    ] {
        for card in ElementHandle::find_by_element_id(window, id) {
            card.visit_descendants::<()>(|child| {
                contains(&card, &child, tab);
                std::ops::ControlFlow::Continue(())
            });
        }
    }
    if tab == "general" {
        let gt: serde_json::Value =
            serde_json::from_str(include_str!("../tests/settings-window-ground-truth.json"))
                .unwrap();
        let width = window.window().size().width;
        let size = if width == 1000 {
            "1000x800"
        } else {
            "1200x900"
        };
        for (name, id) in [
            ("titles", "SettingRowItem::row-title"),
            ("descriptions", "SettingRowItem::row-description"),
        ] {
            // The original two general rows retain Electron geometry. New
            // rendering controls are covered by the containment checks above.
            for (index, item) in ElementHandle::find_by_element_id(window, id)
                .take(gt["tabs"][size][tab][name].as_array().unwrap().len())
                .enumerate()
            {
                let expected = &gt["tabs"][size][tab][name][index];
                // Intrinsic text widths differ by font. The line boxes and spacing must agree.
                assert!(
                    (item.absolute_position().y - expected["y"].as_f64().unwrap() as f32).abs()
                        < 0.5,
                    "{name}[{index}] has wrong vertical position: {:?}",
                    item.absolute_position()
                );
                assert!(
                    (item.size().height - expected["height"].as_f64().unwrap() as f32).abs() < 0.5,
                    "{name}[{index}] has wrong line height: {:?}",
                    item.size()
                );
            }
        }
    }
}

fn snapshot(window: &impl ComponentHandle, name: &str) {
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
    let state = app.global::<AppState>();
    let prefs = settings.global::<AppState>();
    let main_theme = app.global::<Theme>();
    let settings_theme = settings.global::<Theme>();
    state.set_current_route("search".into());
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
    element(&app, "SideNavBar::settings-menu-item").invoke_accessible_default_action();
    settle();
    assert!(settings.window().is_visible());
    assert!(app.window().is_visible());
    assert_eq!(settings.window().size(), PhysicalSize::new(1000, 800));
    assert_eq!(prefs.get_settings_tab(), "general");
    assert_eq!(state.get_current_route(), "search");
    assert_eq!(app.window().size(), PhysicalSize::new(1435, 1000));
    assert_eq!(
        settings.root_element().size(),
        LogicalSize::new(1000.0, 800.0)
    );
    check_settings_geometry(settings, "1000x800");
    snapshot(settings, "settings-general-light");

    click(settings, "SettingsModal::modern-switch-auto-launch");
    assert!(prefs.get_setting_auto_launch());
    assert!(state.get_setting_auto_launch());
    state.set_volume_percent(37.0);
    settle();
    assert_eq!(prefs.get_volume_percent(), 37.0);
    prefs.set_volume_percent(62.0);
    settle();
    assert_eq!(state.get_volume_percent(), 62.0);

    // Changes to unrelated preferences in the same event-loop turn must both survive.
    state.set_volume_percent(41.0);
    prefs.set_setting_acrylic(false);
    settle();
    assert_eq!(prefs.get_volume_percent(), 41.0);
    assert!(!state.get_setting_acrylic());

    click(settings, "SettingsModal::settings-nav-appearance");
    click(settings, "SegmentedSlider3::btn-1");
    assert!(main_theme.get_is_dark());
    assert_eq!(main_theme.get_theme_mode(), "dark");
    assert_eq!(state.get_current_route(), "search");
    state.invoke_open_settings_window();
    settle();
    assert_eq!(
        prefs.get_settings_tab(),
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
    let aux = prefs.get_fullscreen_aux_left();
    let mut favorite = aux.row_data(0).unwrap();
    favorite.visible = false;
    aux.set_row_data(0, favorite);
    settle();
    assert!(!state.get_fullscreen_aux_left().row_data(0).unwrap().visible);

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
        click(settings, &format!("SettingsModal::settings-nav-{tab}"));
        assert_eq!(prefs.get_settings_tab(), tab);
        check_settings_contents(settings, tab);
        let close = element(settings, "SettingsModal::settings-close-btn");
        assert!(close.absolute_position().x + close.size().width <= 1000.0);
        snapshot(settings, &format!("settings-{tab}-light"));
    }
    // Scroll the long audio page, then ensure switching tabs starts at the top.
    click(settings, "SettingsModal::settings-nav-audio");
    prefs.set_setting_supports_exclusive(false);
    prefs.set_setting_output_device_label("Very long USB DAC device name ".repeat(8).into());
    prefs.set_output_devices(
        std::rc::Rc::new(slint::VecModel::from(vec![crate::OutputDeviceItem {
            id: "hw:long-name".into(),
            name: prefs.get_setting_output_device_label(),
            current: true,
        }]))
        .into(),
    );
    prefs.set_setting_output_device_index(0);
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
    click(settings, "SettingsModal::settings-nav-general");
    check_settings_geometry(settings, "1000x800");
    settings.window().set_size(PhysicalSize::new(1200, 900));
    click(settings, "SettingsModal::settings-nav-general");
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
        click(settings, &format!("SettingsModal::settings-nav-{tab}"));
        check_settings_contents(settings, tab);
    }
    assert_eq!(
        settings.root_element().size(),
        LogicalSize::new(1200.0, 900.0)
    );
    assert_eq!(app.window().size(), PhysicalSize::new(1435, 1000));
    click(settings, "SettingsModal::settings-close-btn");
    assert!(!settings.window().is_visible());
    assert!(app.window().is_visible());

    state.invoke_open_settings_window();
    settle();
    assert_eq!(prefs.get_settings_tab(), "general");
    assert!(prefs.get_setting_auto_launch());
    assert_eq!(prefs.get_volume_percent(), 41.0);
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

    state.invoke_open_settings_window();
    settle();
    settings
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(
        !settings.window().is_visible(),
        "native close closes only settings"
    );
    assert!(app.window().is_visible());
    assert_eq!(state.get_current_route(), "search");
    state.invoke_open_settings_window();
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
    let state = app.global::<AppState>();
    let prefs = settings.global::<AppState>();
    app.show().unwrap();
    state.invoke_open_settings_window();
    settle();
    assert_eq!(prefs.get_setting_renderer(), "vulkan");
    assert!(!prefs.get_renderer_restart_required());
    click(settings, "SettingsModal::renderer-select");
    assert_eq!(prefs.get_setting_renderer(), "vulkan");
    snapshot(settings, "settings-renderer-dropdown-light");
    ElementHandle::find_by_accessible_label(settings, "软件渲染（关闭 GPU 加速）")
        .next()
        .unwrap()
        .mock_single_click(PointerEventButton::Left);
    settle();
    assert_eq!(prefs.get_setting_renderer(), "software");
    assert_eq!(state.get_setting_renderer(), "software");
    assert_eq!(prefs.get_startup_renderer(), "vulkan");
    assert!(prefs.get_renderer_restart_required());
    click(settings, "SettingsModal::renderer-select");
    ElementHandle::find_by_accessible_label(settings, "Vulkan（默认）")
        .next()
        .unwrap()
        .mock_single_click(PointerEventButton::Left);
    settle();
    assert_eq!(state.get_setting_renderer(), "vulkan");
    assert!(!prefs.get_renderer_restart_required());
    click(settings, "SettingsModal::settings-nav-audio");
    let quality = element(settings, "SettingsModal::quality-select");
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
        || ElementHandle::find_by_element_id(settings, "Dropdown::option").collect::<Vec<_>>();
    assert!(options().is_empty(), "closed dropdown created its options");
    click(settings, "SettingsModal::quality-select");
    assert_eq!(quality.accessible_expanded(), Some(true));
    assert_eq!(
        prefs.get_setting_sound_quality(),
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
        prefs.get_setting_sound_quality(),
        "lossless",
        "navigation committed early"
    );
    key(Key::Escape);
    assert_eq!(quality.accessible_expanded(), Some(false));
    assert!(
        settings.window().is_visible(),
        "Escape closed settings instead of the menu"
    );
    assert_eq!(prefs.get_setting_sound_quality(), "lossless");
    assert!(options().is_empty());

    click(settings, "SettingsModal::quality-select");
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
    assert_eq!(prefs.get_setting_sound_quality(), "lossless");

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
        click(settings, "SettingsModal::quality-select");
        ElementHandle::find_by_accessible_label(settings, label)
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        settle();
        assert_eq!(prefs.get_setting_sound_quality(), value);
        assert_eq!(state.get_setting_sound_quality(), value);
        assert_eq!(quality.accessible_value().unwrap(), label);
        assert_eq!(state.get_setting_sound_quality_label(), label);
        assert_eq!(quality.accessible_expanded(), Some(false));
    }
    key(Key::Return);
    assert_eq!(quality.accessible_expanded(), Some(true));
    key(Key::End);
    key(Key::Return);
    assert_eq!(state.get_setting_sound_quality(), "jymaster");
    assert_eq!(quality.accessible_expanded(), Some(false));

    // The same control handles a changing device model and a long scrollable menu.
    state.set_output_devices(
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
    state.set_setting_output_device_index(30);
    prefs.set_runtime_enabled(true);
    let selected_device = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let selected = selected_device.clone();
    prefs.on_runtime_action(move |action, _, value| {
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
    click(settings, "SettingsModal::device-select");
    let device = element(settings, "SettingsModal::device-select");
    assert_eq!(device.accessible_expanded(), Some(true));
    assert!(
        options().len() <= 12,
        "device menu eagerly created all rows"
    );
    key(Key::End);
    key(Key::Return);
    assert_eq!(*selected_device.borrow(), "hw:49");
    assert_eq!(
        prefs.get_setting_output_device_index(),
        30,
        "device changes await backend confirmation"
    );
    assert_eq!(device.accessible_expanded(), Some(false));
    state.set_output_devices(
        std::rc::Rc::new(slint::VecModel::<crate::OutputDeviceItem>::default()).into(),
    );
    state.set_setting_output_device_index(-1);
    settle();
    click(settings, "SettingsModal::device-select");
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
