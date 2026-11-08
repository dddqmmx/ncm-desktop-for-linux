use crate::{AppState, AppWindow, SettingsWindow, Theme};
use slint::{winit_030::WinitWindowAccessor, CloseRequestResponse, ComponentHandle};

// Copy only the changed preference: copying a whole snapshot would overwrite
// unrelated changes made in the other window during the same event-loop turn.
// Navigation, focus, dialog state, and window dimensions always remain local.
macro_rules! copy_properties {
    ($source:ident, $target:ident, $field:ident, $($name:literal => $get:ident => $set:ident),+ $(,)?) => {
        $(if $field.is_none_or(|field| field == $name) && $source.$get() != $target.$get() {
            $target.$set($source.$get());
        })+
    };
}

fn sync_preferences(source: AppState<'_>, target: AppState<'_>, field: Option<&str>) {
    copy_properties!(source, target, field,
        "runtime-message" => get_runtime_message => set_runtime_message,
        "setting-cache-details" => get_setting_cache_details => set_setting_cache_details,
        "output-devices" => get_output_devices => set_output_devices,
        "setting-output-device-index" => get_setting_output_device_index => set_setting_output_device_index,
        "setting-auto-launch" => get_setting_auto_launch => set_setting_auto_launch,
        "setting-tray-minimize" => get_setting_tray_minimize => set_setting_tray_minimize,
        "setting-renderer" => get_setting_renderer => set_setting_renderer,
        "startup-renderer" => get_startup_renderer => set_startup_renderer,
        "setting-sound-quality" => get_setting_sound_quality => set_setting_sound_quality,
        "setting-audio-engine" => get_setting_audio_engine => set_setting_audio_engine,
        "setting-software-volume" => get_setting_software_volume => set_setting_software_volume,
        "setting-output-device-label" => get_setting_output_device_label => set_setting_output_device_label,
        "setting-supports-exclusive" => get_setting_supports_exclusive => set_setting_supports_exclusive,
        "setting-exclusive-mode" => get_setting_exclusive_mode => set_setting_exclusive_mode,
        "setting-strict-bitperfect" => get_setting_strict_bitperfect => set_setting_strict_bitperfect,
        "setting-acrylic" => get_setting_acrylic => set_setting_acrylic,
        "setting-accent-hex" => get_setting_accent_hex => set_setting_accent_hex,
        "setting-lyric-debug" => get_setting_lyric_debug => set_setting_lyric_debug,
        "setting-cache-usage-percent" => get_setting_cache_usage_percent => set_setting_cache_usage_percent,
        "setting-cache-usage-text" => get_setting_cache_usage_text => set_setting_cache_usage_text,
        "setting-cache-limit-mb" => get_setting_cache_limit_mb => set_setting_cache_limit_mb,
        "setting-cache-ahead-mb" => get_setting_cache_ahead_mb => set_setting_cache_ahead_mb,
        "setting-lib-paths" => get_setting_lib_paths => set_setting_lib_paths,
        "fullscreen-aux-left" => get_fullscreen_aux_left => set_fullscreen_aux_left,
        "fullscreen-aux-right" => get_fullscreen_aux_right => set_fullscreen_aux_right,
        "volume-percent" => get_volume_percent => set_volume_percent,
        "can-adjust-volume" => get_can_adjust_volume => set_can_adjust_volume,
        "session-volume-locked" => get_session_volume_locked => set_session_volume_locked,
    );
}

pub(crate) fn sync_theme(source: Theme<'_>, target: Theme<'_>, field: Option<&str>) {
    copy_properties!(source, target, field,
        "is-dark" => get_is_dark => set_is_dark,
        "theme-mode" => get_theme_mode => set_theme_mode,
        "theme-color" => get_theme_color => set_theme_color,
        "theme-color-soft" => get_theme_color_soft => set_theme_color_soft,
        "theme-color-muted" => get_theme_color_muted => set_theme_color_muted,
        "theme-color-strong" => get_theme_color_strong => set_theme_color_strong,
        "theme-color-tint" => get_theme_color_tint => set_theme_color_tint,
    );
}

// Retain one settings window for the lifetime of the main window. All callback
// captures are weak, so closing the application doesn't create a reference cycle.
pub struct ApplicationWindows {
    pub(crate) _settings: SettingsWindow,
}

impl ApplicationWindows {
    pub fn attach(app: &AppWindow) -> Result<Self, slint::PlatformError> {
        let weak = app.as_weak();
        app.on_window_action(move |action| {
            if let Some(app) = weak.upgrade() {
                match action.as_str() {
                    "minimize" => app.window().set_minimized(true),
                    "maximize" => app.window().set_maximized(!app.window().is_maximized()),
                    "close" => {
                        app.window()
                            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
                    }
                    _ => {}
                }
            }
        });
        crate::login_animation::attach(app)?;
        let settings = SettingsWindow::new()?;
        sync_preferences(
            app.global::<AppState>(),
            settings.global::<AppState>(),
            None,
        );
        sync_theme(app.global::<Theme>(), settings.global::<Theme>(), None);

        let weak_main = app.as_weak();
        let weak_settings = settings.as_weak();
        app.global::<AppState>().on_settings_changed(move |field| {
            if let (Some(main), Some(settings)) = (weak_main.upgrade(), weak_settings.upgrade()) {
                sync_preferences(
                    main.global::<AppState>(),
                    settings.global::<AppState>(),
                    Some(field.as_str()),
                );
            }
        });
        let weak_main = app.as_weak();
        let weak_settings = settings.as_weak();
        settings
            .global::<AppState>()
            .on_settings_changed(move |field| {
                if let (Some(main), Some(settings)) = (weak_main.upgrade(), weak_settings.upgrade())
                {
                    sync_preferences(
                        settings.global::<AppState>(),
                        main.global::<AppState>(),
                        Some(field.as_str()),
                    );
                }
            });
        let weak_main = app.as_weak();
        let weak_settings = settings.as_weak();
        app.global::<Theme>().on_theme_changed(move |field| {
            if let (Some(main), Some(settings)) = (weak_main.upgrade(), weak_settings.upgrade()) {
                sync_theme(
                    main.global::<Theme>(),
                    settings.global::<Theme>(),
                    Some(field.as_str()),
                );
            }
        });
        let weak_main = app.as_weak();
        let weak_settings = settings.as_weak();
        settings.global::<Theme>().on_theme_changed(move |field| {
            if let (Some(main), Some(settings)) = (weak_main.upgrade(), weak_settings.upgrade()) {
                sync_theme(
                    settings.global::<Theme>(),
                    main.global::<Theme>(),
                    Some(field.as_str()),
                );
            }
        });

        let weak_settings = settings.as_weak();
        app.global::<AppState>().on_open_settings_window(move || {
            if let Some(settings) = weak_settings.upgrade() {
                if !settings.window().is_visible() {
                    settings
                        .global::<AppState>()
                        .set_settings_tab("general".into());
                    if let Err(error) = settings.show() {
                        eprintln!("无法打开设置窗口: {error}");
                        return;
                    }
                    settings.set_presented(true);
                }
                settings.window().set_minimized(false);
                settings
                    .window()
                    .with_winit_window(|window| window.focus_window());
            }
        });
        let weak_settings = settings.as_weak();
        settings.on_close_settings(move || {
            if let Some(settings) = weak_settings.upgrade() {
                settings.set_presented(false);
                if let Err(error) = settings.hide() {
                    eprintln!("无法关闭设置窗口: {error}");
                }
            }
        });
        let weak_settings = settings.as_weak();
        settings.window().on_close_requested(move || {
            if let Some(settings) = weak_settings.upgrade() {
                settings.set_presented(false);
            }
            CloseRequestResponse::HideWindow
        });
        let weak_settings = settings.as_weak();
        settings.on_drag_window(move || {
            if let Some(settings) = weak_settings.upgrade() {
                settings.window().with_winit_window(|window| {
                    let _ = window.drag_window();
                });
            }
        });
        let weak_settings = settings.as_weak();
        app.window().on_close_requested(move || {
            if let Some(settings) = weak_settings.upgrade() {
                settings.invoke_close_settings();
            }
            CloseRequestResponse::HideWindow
        });
        Ok(Self {
            _settings: settings,
        })
    }
}
