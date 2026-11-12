//! Manual performance measurements; run alone with --ignored --nocapture.
//! Account-backed checks require explicit environment opt-in and an isolated cache.
use super::{
    backend::Event,
    data::Preferences,
    images::{DecodedImage, ImageTarget},
    view::View,
};
use crate::AppWindow;
use serde_json::json;
use slint::{ComponentHandle, Model};
use std::time::{Duration, Instant};

#[test]
#[ignore = "manual playlist performance measurement"]
fn profile_playlist_cover_updates_and_scrolling() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    app.show().unwrap();
    let navigation_state = app.global::<crate::NavigationState>();
    let account_state = app.global::<crate::AccountState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    navigation_state.set_current_route("playlist".into());
    account_state.set_is_logged_in(true);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    for count in [100, 1_000, 10_000] {
        let data = json!({"kind":"playlist","id":"1","body":{"playlist":{
            "name":"性能测试歌单","tracks":(0..count).map(|i| json!({
                "id":i + 1,"name":format!("曲目 {i}"),"dt":180_000,
                "ar":[{"name":"测试歌手"}],
                "al":{"name":"测试专辑","picUrl":format!("https://example.test/{i}.jpg")}
            })).collect::<Vec<_>>()
        }}});
        let started = Instant::now();
        view.apply(
            &app,
            Event {
                name: "page".into(),
                data,
                token: 0,
                epoch: 0,
                error: None,
            },
        );
        let apply_ms = started.elapsed().as_secs_f64() * 1000.0;
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(600));
        app.window().take_snapshot().unwrap();
        let mut updates = Vec::new();
        let mut frames = Vec::new();
        for batch in 0..16 {
            let covers: Vec<_> = (batch * 4..batch * 4 + 4)
                .map(|i| DecodedImage {
                    palette: None,
                    target: ImageTarget::Cover(format!("https://example.test/{i}.jpg"), 128),
                    epoch: 0,
                    pixels: slint::SharedPixelBuffer::clone_from_slice(
                        &vec![128u8; 128 * 128 * 4],
                        128,
                        128,
                    ),
                })
                .collect();
            let started = Instant::now();
            view.apply_images(&app, covers.into_iter());
            updates.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(800.0, 600.0),
                    delta_x: 0.0,
                    delta_y: -62.0,
                });
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
            app.window().take_snapshot().unwrap();
            frames.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        updates.sort_by(f64::total_cmp);
        frames.sort_by(f64::total_cmp);
        println!("{count} songs: apply={apply_ms:.2}ms; cover batch median={:.2}ms max={:.2}ms; software scroll frame median={:.2}ms max={:.2}ms", updates[8], updates[15], frames[8], frames[15]);
        assert_eq!(playlist_state.get_playlist_tracks().row_count(), count);
    }
}

#[test]
#[ignore = "requires a real desktop; manual scroll profiling"]
fn profile_real_playlist_scrolling() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
    slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
        &mut builder,
        true,
    );
    crate::rendering::backend_selector()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    let navigation_state = app.global::<crate::NavigationState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let account_state = app.global::<crate::AccountState>();
    navigation_state.set_current_route("playlist".into());
    playlist_state.set_playlist_title("滚动性能测试（模拟数据）".into());
    account_state.set_is_logged_in(true);
    playlist_state.set_playlist_tracks(
        Rc::new(slint::VecModel::from(
            (0..10_000)
                .map(|id| crate::TrackItem {
                    id,
                    index_str: format!("{id:02}").into(),
                    name: format!("测试歌曲 {id}").into(),
                    artist: "测试歌手".into(),
                    album: "测试专辑".into(),
                    duration: "3:00".into(),
                    cover: if id < 500 {
                        slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
                            &vec![(id % 255) as u8; 128 * 128 * 4],
                            128,
                            128,
                        ))
                    } else {
                        slint::Image::default()
                    },
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    app.window().set_size(slint::PhysicalSize::new(1435, 1000));
    let start = Instant::now();
    let rendering = Rc::new(RefCell::new(Vec::<f64>::new()));
    let active = Rc::new(Cell::new(false));
    let renderer = crate::rendering::test_renderer();
    if renderer == crate::rendering::Renderer::OpenGl {
        let output = rendering.clone();
        let recording = active.clone();
        let mut before = Instant::now();
        app.window()
            .set_rendering_notifier(move |state, api| match state {
                slint::RenderingState::RenderingSetup => println!("renderer: {api:?}"),
                slint::RenderingState::BeforeRendering => before = Instant::now(),
                slint::RenderingState::AfterRendering if recording.get() => output
                    .borrow_mut()
                    .push(before.elapsed().as_secs_f64() * 1000.0),
                _ => {}
            })
            .unwrap();
    }
    app.show().unwrap();
    println!(
        "window: {:?}, scale: {}",
        app.window().size(),
        app.window().scale_factor()
    );
    let inputs = Rc::new(RefCell::new(Vec::new()));
    let input_times = inputs.clone();
    let gaps = Rc::new(RefCell::new(Vec::new()));
    let frame_gaps = gaps.clone();
    let weak = app.as_weak();
    let timer = slint::Timer::default();
    let mut last = Instant::now();
    let mut steps = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let now = Instant::now();
            if start.elapsed() < Duration::from_secs(2) {
                last = now;
                return;
            }
            active.set(true);
            frame_gaps
                .borrow_mut()
                .push(now.duration_since(last).as_secs_f64() * 1000.0);
            last = now;
            let app = weak.upgrade().unwrap();
            let event_start = Instant::now();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(800.0, 600.0),
                    delta_x: 0.0,
                    delta_y: -62.0,
                });
            input_times
                .borrow_mut()
                .push(event_start.elapsed().as_secs_f64() * 1000.0);
            steps += 1;
            if steps >= 120 || start.elapsed() > Duration::from_secs(25) {
                slint::quit_event_loop().unwrap();
            }
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    for (name, samples) in [
        ("wheel handler", inputs),
        ("render", rendering),
        ("event-loop gap", gaps),
    ] {
        let mut samples = samples.borrow_mut();
        samples.sort_by(f64::total_cmp);
        // Skia's Vulkan surface does not expose rendering notifications without
        // Slint's unstable wgpu integration. Keep input/event-loop measurements;
        // never report missing notifications as zero render time.
        if renderer != crate::rendering::Renderer::OpenGl && name == "render" && samples.is_empty()
        {
            println!(
                "render: unavailable ({} rendering notifications disabled)",
                renderer.name()
            );
            continue;
        }
        assert!(!samples.is_empty(), "no {name} samples");
        println!(
            "{name}: count={} median={:.2}ms p95={:.2}ms max={:.2}ms",
            samples.len(),
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100],
            samples[samples.len() - 1]
        );
    }
    app.hide().unwrap();
}

// Replay a private API response through the real View + ImageLoader. Both paths
// must be explicitly supplied; never check account responses into the repository.
#[test]
#[ignore = "requires a real desktop, API playlist response and isolated cover cache"]
fn profile_api_playlist_scrolling() {
    use std::{
        cell::RefCell,
        collections::HashMap,
        rc::Rc,
        sync::{atomic::AtomicU64, Arc},
    };
    let path = std::env::var("NCM_PROFILE_PLAYLIST_JSON").expect("set NCM_PROFILE_PLAYLIST_JSON");
    let cache_path =
        std::env::var("NCM_PROFILE_CACHE").expect("set NCM_PROFILE_CACHE to an isolated copy");
    let no_covers = std::env::var_os("NCM_PROFILE_NO_COVERS").is_some();
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
    slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
        &mut builder,
        true,
    );
    crate::rendering::backend_selector()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let app = AppWindow::new().unwrap();
    super::view::initialize(&app, &Preferences::default());
    app.global::<crate::NavigationState>()
        .set_current_route("playlist".into());
    app.global::<crate::AccountState>().set_is_logged_in(true);
    let (tx, mut commands) = tokio::sync::mpsc::unbounded_channel::<super::backend::Command>();
    let (images, mut image_receiver) = tokio::sync::mpsc::channel(super::images::QUEUE_CAPACITY);
    let background = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let cache = Arc::new(
                native::CacheService::new(cache_path, Some(2 * 1024 * 1024 * 1024)).unwrap(),
            );
            let loader = Arc::new(super::images::ImageLoader::new(cache, images));
            while let Some(command) = commands.recv().await {
                if command.name == "shutdown" {
                    break;
                }
                if command.name == "covers" && !no_covers {
                    let loader = loader.clone();
                    tokio::spawn(async move {
                        loader.load(&command.data, 0, &AtomicU64::new(0)).await;
                    });
                }
            }
        });
        rt.shutdown_timeout(Duration::from_secs(2));
    });
    let view = Rc::new(RefCell::new(View::new(tx.clone(), Preferences::default())));
    let pending = Rc::new(RefCell::new(HashMap::<i32, u32>::new()));
    let requests = pending.clone();
    app.global::<crate::RuntimeState>()
        .on_request_cover(move |id, size| {
            let mut requests = requests.borrow_mut();
            let value = requests.entry(id).or_default();
            *value = (*value).max(size.max(1) as u32);
        });
    let started = Instant::now();
    view.borrow_mut().apply(
        &app,
        Event {
            name: "page".into(),
            data,
            token: 0,
            epoch: 0,
            error: None,
        },
    );
    println!(
        "real API tracks: {}; model apply: {:.2}ms",
        app.global::<crate::PlaylistState>()
            .get_playlist_tracks()
            .row_count(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    app.window().set_size(slint::PhysicalSize::new(1435, 1000));
    app.show().unwrap();
    let updates = Rc::new(RefCell::new(Vec::new()));
    let deliveries = Rc::new(RefCell::new(HashMap::<(String, u32), usize>::new()));
    let pump = slint::Timer::default();
    let weak = app.as_weak();
    let output = updates.clone();
    let delivered = deliveries.clone();
    pump.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(50),
        move || {
            let app = weak.upgrade().unwrap();
            let images: Vec<_> = std::iter::from_fn(|| image_receiver.try_recv().ok())
                .take(super::images::UPDATES_PER_TICK)
                .collect();
            for image in &images {
                if let ImageTarget::Cover(url, size) = &image.target {
                    *delivered
                        .borrow_mut()
                        .entry((url.clone(), *size))
                        .or_default() += 1;
                }
            }
            let start = Instant::now();
            let mut view = view.borrow_mut();
            view.apply_images(&app, images.into_iter());
            view.request_covers(pending.borrow_mut().drain());
            output
                .borrow_mut()
                .push(start.elapsed().as_secs_f64() * 1000.0);
        },
    );
    let inputs = Rc::new(RefCell::new(vec![Vec::new(); 3]));
    let gaps = Rc::new(RefCell::new(vec![Vec::new(); 3]));
    let input_times = inputs.clone();
    let frame_gaps = gaps.clone();
    let weak = app.as_weak();
    let timer = slint::Timer::default();
    let start = Instant::now();
    let mut last = start;
    let mut steps = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if steps >= 690 {
                return;
            }
            let now = Instant::now();
            if start.elapsed() < Duration::from_secs(3) {
                last = now;
                return;
            }
            let phase = steps / 230;
            frame_gaps.borrow_mut()[phase].push(now.duration_since(last).as_secs_f64() * 1000.0);
            last = now;
            let app = weak.upgrade().unwrap();
            let event_start = Instant::now();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(800.0, 600.0),
                    delta_x: 0.0,
                    delta_y: if phase == 1 { 122.0 } else { -122.0 },
                });
            input_times.borrow_mut()[phase].push(event_start.elapsed().as_secs_f64() * 1000.0);
            steps += 1;
            if steps == 690 || start.elapsed() > Duration::from_secs(90) {
                slint::quit_event_loop().unwrap();
            }
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    pump.stop();
    for (label, groups) in [("wheel", inputs), ("event-loop gap", gaps)] {
        for (phase, samples) in groups.borrow_mut().iter_mut().enumerate() {
            samples.sort_by(f64::total_cmp);
            if samples.is_empty() {
                continue;
            }
            println!(
                "phase {phase} {label}: count={} median={:.2}ms p95={:.2}ms max={:.2}ms",
                samples.len(),
                samples[samples.len() / 2],
                samples[samples.len() * 95 / 100],
                samples[samples.len() - 1]
            );
        }
    }
    let mut updates = updates.borrow_mut();
    updates.sort_by(f64::total_cmp);
    println!(
        "cover pump: median={:.2}ms p95={:.2}ms max={:.2}ms",
        updates[updates.len() / 2],
        updates[updates.len() * 95 / 100],
        updates[updates.len() - 1]
    );
    let delivered = deliveries.borrow();
    println!(
        "cover deliveries: total={} unique={} repeated={}",
        delivered.values().sum::<usize>(),
        delivered.len(),
        delivered.values().filter(|&&n| n > 1).count()
    );
    app.hide().unwrap();
    tx.send(super::backend::Command {
        name: "shutdown".into(),
        data: serde_json::Value::Null,
        token: 0,
    })
    .unwrap();
    background.join().unwrap();
}

#[test]
#[ignore = "reads the saved account and named playlist over the live API; requires a real desktop"]
fn profile_logged_in_playlist_runtime() {
    use super::{data::Storage, ApplicationRuntime};
    use i_slint_backend_testing::ElementHandle;
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::{cell::RefCell, rc::Rc};
    let name = std::env::var("NCM_PROFILE_LIVE_PLAYLIST")
        .expect("explicitly set NCM_PROFILE_LIVE_PLAYLIST to the playlist name");
    let cache = std::path::PathBuf::from(
        std::env::var("NCM_PROFILE_CACHE").expect("set NCM_PROFILE_CACHE to an isolated copy"),
    );
    let actual = Storage::discover().unwrap();
    assert_ne!(
        cache.canonicalize().unwrap(),
        actual.cache.canonicalize().unwrap()
    );
    let cookie: String = actual.read("session.json").unwrap();
    assert!(!cookie.is_empty(), "no saved login session");
    // Remove the private session copy even if an assertion unwinds the test.
    struct PrivateConfig(std::path::PathBuf);
    impl Drop for PrivateConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let config = PrivateConfig(super::tests::temp("live-playlist"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let storage = Storage {
        config: config.0.clone(),
        cache,
    };
    storage.session(&cookie, true).unwrap();
    let preferences = actual.preferences().unwrap();
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
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
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let runtime =
        ApplicationRuntime::attach(&app, &windows._settings, storage, preferences).unwrap();
    app.window().set_size(slint::PhysicalSize::new(1435, 1000));
    app.show().unwrap();
    let timer = slint::Timer::default();
    let weak = app.as_weak();
    let started = Instant::now();
    let mut requested = None::<Instant>;
    let mut loaded = None::<Instant>;
    let mut previous = started;
    let mut steps = 0;
    let mut home_started = None::<Instant>;
    let mut home_step = 0;
    let home_samples = Rc::new(RefCell::new(Vec::<f64>::new()));
    let home_recorded = home_samples.clone();
    let samples = Rc::new(RefCell::new(vec![
        (Vec::<f64>::new(), Vec::<f64>::new());
        3
    ]));
    let recorded = samples.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if started.elapsed() > Duration::from_secs(100) {
                slint::quit_event_loop().unwrap();
                return;
            }
            let app = weak.upgrade().unwrap();
            let navigation_state = app.global::<crate::NavigationState>();
            let home_state = app.global::<crate::HomeState>();
            let player_state = app.global::<crate::PlayerState>();
            let playlist_state = app.global::<crate::PlaylistState>();
            if steps >= 690 {
                if home_started.is_none() {
                    navigation_state.set_current_route("home".into());
                    home_started = Some(Instant::now());
                }
                if home_started.unwrap().elapsed() < Duration::from_secs(2) {
                    return;
                }
                let position =
                    slint::LogicalPosition::new(1200.0 - 8.0 * home_step.min(100) as f32, 250.0);
                if home_step == 0 {
                    assert!(!home_state.get_home_playlists_loading());
                    assert!(home_state.get_recommend_playlists().row_count() > 0);
                    app.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                } else if home_step <= 100 {
                    let start = Instant::now();
                    app.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    home_recorded
                        .borrow_mut()
                        .push(start.elapsed().as_secs_f64() * 1000.0);
                } else if home_step == 110 {
                    app.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                } else if home_step == 125 {
                    assert_eq!(
                        navigation_state.get_current_route(),
                        "home",
                        "drag opened a playlist"
                    );
                    assert!(!player_state.get_is_playing(), "drag started playback");
                    let heading =
                        ElementHandle::find_by_element_id(&app, "HomeView::home-page-title")
                            .next()
                            .unwrap();
                    let before = heading.absolute_position().y;
                    app.window().dispatch_event(WindowEvent::PointerScrolled {
                        position,
                        delta_x: 0.0,
                        delta_y: -120.0,
                    });
                    assert!(
                        heading.absolute_position().y < before - 100.0,
                        "wheel over shelf did not scroll the home page"
                    );
                    println!("live home: drag did not activate cards; wheel scrolls the page");
                    slint::quit_event_loop().unwrap();
                }
                home_step += 1;
                return;
            }
            if requested.is_none() {
                if let Some(row) = playlist_state
                    .get_created_playlists()
                    .iter()
                    .find(|row| row.name.as_str() == name)
                {
                    println!(
                        "live account ready: {:.2}ms",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                    requested = Some(Instant::now());
                    playlist_state.invoke_open_playlist(row.id);
                }
                return;
            }
            if loaded.is_none() {
                if !playlist_state.get_playlist_loading()
                    && playlist_state.get_playlist_tracks().row_count() > 0
                {
                    println!(
                        "live playlist ready: {} tracks, {:.2}ms after click",
                        playlist_state.get_playlist_tracks().row_count(),
                        requested.unwrap().elapsed().as_secs_f64() * 1000.0
                    );
                    loaded = Some(Instant::now());
                }
                return;
            }
            let now = Instant::now();
            if loaded.unwrap().elapsed() < Duration::from_secs(3) {
                previous = now;
                return;
            }
            let phase = steps / 230;
            recorded.borrow_mut()[phase]
                .1
                .push(now.duration_since(previous).as_secs_f64() * 1000.0);
            previous = now;
            let input = Instant::now();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(800.0, 600.0),
                    delta_x: 0.0,
                    delta_y: if phase == 1 { 122.0 } else { -122.0 },
                });
            recorded.borrow_mut()[phase]
                .0
                .push(input.elapsed().as_secs_f64() * 1000.0);
            steps += 1;
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    app.hide().unwrap();
    drop(runtime);
    let mut home_samples = home_samples.borrow_mut();
    assert_eq!(home_samples.len(), 100, "live home drag did not finish");
    home_samples.sort_by(f64::total_cmp);
    println!(
        "live home drag handler: median={:.2}ms p95={:.2}ms max={:.2}ms",
        home_samples[50], home_samples[95], home_samples[99]
    );
    for (phase, (inputs, gaps)) in samples.borrow_mut().iter_mut().enumerate() {
        for (label, values) in [("wheel", inputs), ("event-loop gap", gaps)] {
            assert_eq!(values.len(), 230, "live API / scroll phase did not finish");
            values.sort_by(f64::total_cmp);
            println!(
                "live phase {phase} {label}: median={:.2}ms p95={:.2}ms max={:.2}ms",
                values[115], values[218], values[229]
            );
        }
    }
}

#[test]
#[ignore = "reads the saved account over the live API; requires a real desktop and isolated cache"]
fn profile_logged_in_home_cover_residency() {
    use super::{data::Storage, ApplicationRuntime};
    use i_slint_backend_testing::ElementHandle;
    use slint::platform::WindowEvent;
    use std::{cell::RefCell, rc::Rc};
    assert_eq!(std::env::var("NCM_PROFILE_LIVE_HOME").as_deref(), Ok("1"));
    let actual = Storage::discover().unwrap();
    let cache = std::path::PathBuf::from(std::env::var("NCM_PROFILE_CACHE").unwrap());
    assert_ne!(
        cache.canonicalize().unwrap(),
        actual.cache.canonicalize().unwrap()
    );
    let cookie: String = actual.read("session.json").unwrap();
    assert!(!cookie.is_empty());
    struct PrivateConfig(std::path::PathBuf);
    impl Drop for PrivateConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let config = PrivateConfig(super::tests::temp("live-home-covers"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let storage = Storage {
        config: config.0.clone(),
        cache,
    };
    storage.session(&cookie, true).unwrap();
    let preferences = actual.preferences().unwrap();
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
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
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let runtime =
        ApplicationRuntime::attach(&app, &windows._settings, storage, preferences).unwrap();
    app.show().unwrap();
    app.window()
        .set_size(slint::LogicalSize::new(1435.0, 1000.0));
    let mut actions = Vec::new();
    for _ in 0..2 {
        for (offset, next, prev) in [
            (
                0.0,
                "RecommendedPlaylistsSection::playlist-next",
                "RecommendedPlaylistsSection::playlist-prev",
            ),
            (
                400.0,
                "RecommendedSongsHeader::songs-next",
                "RecommendedSongsHeader::songs-prev",
            ),
            (
                740.0,
                "HomeDiscoverySection::albums-next",
                "HomeDiscoverySection::albums-prev",
            ),
        ] {
            actions.push((offset, ""));
            for _ in 0..5 {
                actions.push((offset, next));
            }
            for _ in 0..5 {
                actions.push((offset, prev));
            }
        }
        actions.push((1600.0, ""));
        actions.push((0.0, ""));
    }
    let step = Rc::new(std::cell::Cell::new(0usize));
    let completed = step.clone();
    let times = Rc::new(RefCell::new(Vec::new()));
    let samples = times.clone();
    let weak = app.as_weak();
    let controller = runtime.controller.clone();
    let started = Instant::now();
    let mut heading = None::<ElementHandle>;
    let mut heading_top = 0.0;
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(900), move || {
        if started.elapsed() > Duration::from_secs(125) { slint::quit_event_loop().unwrap(); return; }
        let app = weak.upgrade().unwrap();
        let account_state = app.global::<crate::AccountState>();
        let home_state = app.global::<crate::HomeState>();
        let navigation_state = app.global::<crate::NavigationState>();
        if !account_state.get_is_logged_in() || home_state.get_home_playlists_loading() || home_state.get_home_songs_loading() || started.elapsed() < Duration::from_secs(6) { return; }
        let i = step.get();
        if i == 0 {
            heading = ElementHandle::find_by_element_id(&app, "HomeView::home-page-title").next();
            heading_top = heading.as_ref().unwrap().absolute_position().y;
            println!("live home: playlists={} songs={} scale={}", home_state.get_recommend_playlists().row_count(), home_state.get_daily_songs().row_count(), app.window().scale_factor());
        }
        if i == actions.len() || i == actions.len()/2 {
            let view = controller.borrow();
            let p = &view.cover_probe;
            let (count, bytes) = view.cover_cache_usage();
            let mut sizes = std::collections::BTreeMap::<u32,usize>::new();
            for ((_, size), n) in &p.deliveries { *sizes.entry(*size).or_default() += n; }
            println!("home pass {}: cache={count} bytes={bytes} requests={} deliveries={} unique={} repeated={} evictions={} sizes={sizes:?}; albums={} charts={}", i/(actions.len()/2), p.requests.values().sum::<usize>(), p.deliveries.values().sum::<usize>(), p.deliveries.len(), p.deliveries.values().filter(|&&n| n > 1).count(), p.evictions, home_state.get_home_new_albums().row_count(), home_state.get_home_charts().row_count());
        }
        if i == actions.len() { slint::quit_event_loop().unwrap(); return; }
        let (offset, button) = actions[i];
        assert_eq!(navigation_state.get_current_route(), "home", "unexpected navigation at step {i}");
        let start = Instant::now();
        if button.is_empty() {
            let heading = heading.as_ref().unwrap();
            let window_size = app.window().size().to_logical(app.window().scale_factor());
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(window_size.width - 40.0, 400.0), delta_x: 0.0,
                delta_y: (heading_top - offset) - heading.absolute_position().y,
            });
        } else {
            let element = ElementHandle::find_by_element_id(&app, button).next().unwrap_or_else(|| panic!("missing {button}, step={i}, route={}", navigation_state.get_current_route()));
            element.invoke_accessible_default_action();
        }
        samples.borrow_mut().push(start.elapsed().as_secs_f64()*1000.0);
        step.set(i+1);
    });
    slint::run_event_loop().unwrap();
    timer.stop();
    assert_eq!(completed.get(), 70, "home traversal did not finish");
    assert!(
        app.global::<crate::HomeState>()
            .get_home_new_albums()
            .row_count()
            > 0
    );
    assert!(
        app.global::<crate::HomeState>()
            .get_home_charts()
            .row_count()
            > 0
    );
    assert_eq!(
        app.global::<crate::NavigationState>().get_current_route(),
        "home"
    );
    let mut times = times.borrow_mut();
    times.sort_by(f64::total_cmp);
    println!(
        "home input: median={:.2}ms p95={:.2}ms",
        times[times.len() / 2],
        times[times.len() * 95 / 100]
    );
    app.hide().unwrap();
    drop(runtime);
}
