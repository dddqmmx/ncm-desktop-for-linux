//! Isolated manual memory probe: no account, API, downloads, or saved settings.
//! Run this test alone with --ignored --nocapture --test-threads=1.
use super::{
    backend::Event,
    data::Preferences,
    images::{DecodedImage, ImageTarget},
    view::View,
};
use crate::AppWindow;
use serde_json::json;
use slint::ComponentHandle;
use std::{collections::BTreeMap, time::Duration};

fn diagnostic_trim() {
    // Opt-in diagnostic, never a production memory-management policy.
    #[cfg(target_env = "gnu")]
    if std::env::var_os("NCM_MEMORY_TRIM").is_some() {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        unsafe {
            malloc_trim(0);
        }
    }
}

fn snapshot(stage: &str, view: &View) {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let values: BTreeMap<_, _> = rollup
        .lines()
        .filter_map(|line| {
            let (key, rest) = line.split_once(':')?;
            Some((key, rest.split_whitespace().next()?.parse::<u64>().ok()?))
        })
        .collect();
    let mut mappings = BTreeMap::<String, u64>::new();
    let mut name = String::new();
    for line in std::fs::read_to_string("/proc/self/smaps").unwrap().lines() {
        if line.as_bytes().first().is_some_and(u8::is_ascii_hexdigit)
            && line
                .split_whitespace()
                .next()
                .is_some_and(|s| s.contains('-'))
        {
            name = line
                .split_whitespace()
                .nth(5)
                .unwrap_or("[anonymous]")
                .into();
        } else if let Some(rest) = line.strip_prefix("Pss:") {
            *mappings.entry(name.clone()).or_default() += rest
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap();
        }
    }
    let (images, image_bytes) = view.cover_cache_usage();
    println!(
        "MEMORY {}",
        json!({
            "stage": stage, "pid": std::process::id(),
            "renderer": crate::rendering::test_renderer().name(),
            "debug_assertions": cfg!(debug_assertions), "rollup_kib": values,
            "mapping_pss_kib": mappings,
            "cover_count": images, "cover_bytes": image_bytes,
            "models": view.memory_model_counts(),
        })
    );
}

#[test]
#[ignore = "manual Linux desktop memory measurement; no accounts or network"]
fn profile_memory_stages() {
    let preferences = Preferences {
        renderer: crate::rendering::test_renderer(),
        ..Preferences::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx.clone(), preferences.clone());
    snapshot("before_backend", &view);
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
        &mut builder,
        true,
    );
    preferences
        .renderer
        .backend_selector()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    snapshot("after_backend", &view);
    let app = AppWindow::new().unwrap();
    snapshot("after_main_constructed", &view);
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    snapshot("after_hidden_windows_constructed", &view);
    super::view::initialize(&app, &preferences);
    app.global::<crate::HomeState>()
        .set_home_playlists_loading(false);
    app.global::<crate::HomeState>()
        .set_home_songs_loading(false);
    app.window()
        .set_size(slint::LogicalSize::new(1435.0, 1000.0));
    app.show().unwrap();
    println!(
        "MEMORY_WINDOW {:?} scale={}",
        app.window().size(),
        app.window().scale_factor()
    );

    let weak = app.as_weak();
    let settings = windows._settings.as_weak();
    let timer = slint::Timer::default();
    let mut stage = 0;
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(3), move || {
        let app = weak.upgrade().unwrap();
        let navigation_state = app.global::<crate::NavigationState>();
        let home_state = app.global::<crate::HomeState>();
        let settings_state = app.global::<crate::SettingsState>();
        let login_state = app.global::<crate::LoginState>();
        match stage {
            0 => {
                println!("MEMORY_WINDOW_SETTLED {:?} scale={}", app.window().size(), app.window().scale_factor());
                snapshot("empty_home", &view);
                navigation_state.set_current_route("playlist".into());
                view.apply(&app, Event {
                    name: "page".into(), token: 0, epoch: 0, error: None,
                    data: json!({"kind":"playlist","id":"1","body":{"playlist":{
                        "name":"内存测试（模拟数据）","tracks":(0..10_000).map(|i| json!({
                            "id":i + 1,"name":format!("曲目 {i}"),"dt":180_000,
                            "ar":[{"name":"测试歌手"}],
                            "al":{"name":"测试专辑","picUrl":format!("https://example.test/{i}.jpg")}
                        })).collect::<Vec<_>>()
                    }}}),
                });
            }
            1 | 2 => {
                snapshot(if stage == 1 { "playlist_10000_no_covers" } else { "playlist_500_covers" }, &view);
                let range = if stage == 1 { 0..500 } else { 500..1500 };
                // Exercise the real cache and model eviction, without network or decoder noise.
                for batch in range.collect::<Vec<_>>().chunks(4) {
                    view.apply_images(&app, batch.iter().map(|i| {
                        let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(128, 128);
                        pixels.make_mut_bytes().fill((i % 255) as u8);
                        DecodedImage { palette: None, target: ImageTarget::Cover(format!("https://example.test/{i}.jpg"), 128), epoch: 0, pixels }
                    }));
                }
            }
            3 => {
                snapshot("playlist_1500_covers_cache_capped", &view);
                super::view::initialize(&app, &preferences);
                navigation_state.set_current_route("home".into());
                home_state.set_home_playlists_loading(false);
                home_state.set_home_songs_loading(false);
                // Explicitly release all model/cache ownership to measure allocator/renderer retention.
                view = View::new(tx.clone(), preferences.clone());
            }
            4 => snapshot("models_and_cache_dropped", &view),
            5 => {
                snapshot("models_and_cache_dropped_idle", &view);
                if std::env::var_os("NCM_MEMORY_TRIM").is_some() {
                    diagnostic_trim();
                    snapshot("after_diagnostic_malloc_trim", &view);
                }
                settings_state.invoke_open_settings_window();
            }
            6 => {
                snapshot("settings_shown", &view);
                settings.upgrade().unwrap().invoke_close_settings();
            }
            7 => {
                snapshot("settings_hidden", &view);
                login_state.set_is_login_open(true);
            }
            8 => {
                snapshot("login_shown", &view);
                login_state.invoke_prepare_login_flip();
            }
            9 => {
                snapshot("login_flip_captured", &view);
                login_state.set_is_login_open(false);
            }
            10 => {
                snapshot("login_hidden", &view);
                app.hide().unwrap();
                slint::quit_event_loop().unwrap();
            }
            _ => unreachable!(),
        }
        stage += 1;
    });
    slint::run_event_loop().unwrap();
    timer.stop();
}

#[test]
#[ignore = "manual data-retention measurement; no desktop, accounts, API, or image decoding"]
fn profile_memory_navigation_retention() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    let preferences = Preferences::default();
    super::view::initialize(&app, &preferences);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, preferences);
    let same = std::env::var_os("NCM_MEMORY_REPEAT_SAME").is_some();
    snapshot("navigation_baseline", &view);
    for page in 0..10 {
        let offset = if same { 0 } else { page * 10_000 };
        view.apply(&app, Event {
            name: "page".into(), token: 0, epoch: 0, error: None,
            data: json!({"kind":"playlist","id":"1","body":{"playlist":{
                "name":"内存测试","tracks":(offset..offset+10_000).map(|i| json!({
                    "id":i+1,"name":format!("歌曲 {i}"),"dt":180_000,
                    "ar":[{"id":1,"name":"测试歌手"}],
                    "al":{"id":2,"name":"测试专辑","picUrl":format!("https://example.test/{i}.jpg")}
                })).collect::<Vec<_>>()
            }}}),
        });
        // Follow the production navigation path; it empties the previous UI model.
        let handle = app
            .global::<crate::PlaylistState>()
            .get_active_playlist_id();
        view.action(&app, "playlist", handle, "");
        while rx.try_recv().is_ok() {}
        diagnostic_trim();
        snapshot(&format!("after_page_{}_model_cleared", page + 1), &view);
    }
}
