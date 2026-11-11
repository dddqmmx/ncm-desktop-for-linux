//! Run alone with a real compositor (and an external timeout to catch event-loop hangs):
//! cargo test --manifest-path slint-ui/Cargo.toml real_window_stays_responsive -- --ignored --nocapture
//! NCM_SLINT_TEST_LIVE_API=1 uses the configured real API instead of the delayed local fixture.
use super::{
    api::{ApiFuture, ApiResponse, MusicApi},
    data::{Preferences, Storage},
    tests::temp,
    ApplicationRuntime,
};
use crate::AppWindow;
use i_slint_backend_testing::ElementHandle;
use serde_json::{json, Value};
use slint::{
    platform::{PointerEventButton, WindowEvent},
    ComponentHandle, LogicalPosition, Model,
};
use std::{
    cell::{Cell, RefCell},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

struct SlowApi {
    ui_thread: ThreadId,
    base: String,
}

#[test]
#[ignore = "requires a real desktop; run separately for each NCM_SLINT_TEST_RENDERER"]
fn real_saved_renderer_opens_main_and_settings() {
    use crate::rendering::Renderer;
    let requested = crate::rendering::test_renderer();
    let path = temp("renderer-startup");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    storage
        .save(
            "preferences.json",
            &Preferences {
                renderer: requested,
                ..Preferences::default()
            },
        )
        .unwrap();
    // Re-read the file before creating any Slint window, just as main() does.
    let restored = storage.preferences().unwrap();
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
    slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
        &mut builder,
        true,
    );
    restored
        .renderer
        .backend_selector()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    super::view::initialize(&app, &restored);
    if requested == Renderer::Software {
        assert!(
            app.window().set_rendering_notifier(|_, _| {}).is_err(),
            "software mode exposed a GPU rendering notifier"
        );
    }
    app.show().unwrap();
    let weak = app.as_weak();
    let settings = windows._settings.as_weak();
    let artifacts = path.clone();
    let timer = slint::Timer::default();
    let completed = Rc::new(Cell::new(false));
    let finished = completed.clone();
    let mut stage = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            let app = weak.upgrade().unwrap();
            let settings = settings.upgrade().unwrap();
            if stage == 0 {
                app.global::<crate::SettingsState>()
                    .invoke_open_settings_window();
            } else if stage == 2 {
                let settings_state = settings.global::<crate::SettingsState>();
                assert_eq!(settings_state.get_setting_renderer(), requested.name());
                assert_eq!(settings_state.get_startup_renderer(), requested.name());
                assert!(!settings_state.get_renderer_restart_required());
                let selector = ElementHandle::find_by_element_id(
                    &settings,
                    "GeneralSettingsTab::renderer-select",
                )
                .next()
                .unwrap();
                assert!(selector.size().width >= 230.0);
                let pixels = settings.window().take_snapshot().unwrap();
                image::save_buffer(
                    artifacts.join("settings.png"),
                    pixels.as_bytes(),
                    pixels.width(),
                    pixels.height(),
                    image::ColorType::Rgba8,
                )
                .unwrap();
                click(&settings, "GeneralSettingsTab::renderer-select");
            } else if stage == 3 {
                assert_eq!(
                    ElementHandle::find_by_element_id(&settings, "DropdownMenu::option").count(),
                    3
                );
                let pixels = settings.window().take_snapshot().unwrap();
                image::save_buffer(
                    artifacts.join("renderer-dropdown.png"),
                    pixels.as_bytes(),
                    pixels.width(),
                    pixels.height(),
                    image::ColorType::Rgba8,
                )
                .unwrap();
                settings.invoke_close_settings();
                app.hide().unwrap();
                finished.set(true);
                slint::quit_event_loop().unwrap();
            }
            stage += 1;
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    assert!(completed.get());
    assert!(!app.window().is_visible());
    assert!(!windows._settings.window().is_visible());
    println!(
        "saved renderer={} opened main/settings; artifacts={}",
        requested.name(),
        path.display()
    );
}
impl MusicApi for SlowApi {
    fn call(&self, op: &str, _: Value, _: &str) -> ApiFuture<'_> {
        assert_ne!(std::thread::current().id(), self.ui_thread);
        let body = match op {
            "personalized" => json!({"code":200,"result":(0..12).map(|i| json!({
                "id":100+i,"name":format!("封面 {i}"),"picUrl":format!("{}/{i}.jpg",self.base)
            })).collect::<Vec<_>>()}),
            "personalized_newsong" => json!({"code":200,"result":[]}),
            "playlist_catlist" => json!({"code":200,"sub":[]}),
            "banner" => json!({"code":200,"banners":[]}),
            _ => json!({"code":200}),
        };
        Box::pin(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert_ne!(std::thread::current().id(), self.ui_thread);
            Ok(ApiResponse {
                status: 200,
                body,
                cookie: vec![],
            })
        })
    }
}

struct CoverServer {
    address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl CoverServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        // This is intentionally an original, larger than the UI texture limit.
        let pixels = image::RgbImage::from_fn(1800, 1200, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&pixels)
            .unwrap();
        let thread = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                let Ok(mut connection) = connection else {
                    break;
                };
                let _ = connection.set_read_timeout(Some(Duration::from_secs(1)));
                let _ = connection.set_write_timeout(Some(Duration::from_secs(1)));
                let mut request = [0; 4096];
                let _ = connection.read(&mut request);
                std::thread::sleep(Duration::from_millis(60));
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", jpeg.len());
                let _ = connection.write_all(header.as_bytes());
                let _ = connection.write_all(&jpeg);
            }
        });
        Self {
            address,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for CoverServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        let _ = self.thread.take().unwrap().join();
    }
}

fn click(window: &impl ComponentHandle, id: &str) {
    let item = ElementHandle::find_by_element_id(window, id)
        .next()
        .unwrap_or_else(|| panic!("Missing element {id}"));
    let origin = item.absolute_position();
    let size = item.size();
    let position = LogicalPosition::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0);
    let button = PointerEventButton::Left;
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    window
        .window()
        .dispatch_event(WindowEvent::PointerPressed { position, button });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased { position, button });
}

#[test]
#[ignore = "requires a real Wayland/X11 compositor; run alone"]
fn real_window_stays_responsive() {
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
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let live = std::env::var_os("NCM_SLINT_TEST_LIVE_API").is_some();
    let server = (!live).then(CoverServer::start);
    let path = temp("real-window");
    let provider: Option<Arc<dyn MusicApi>> = server.as_ref().map(|server| {
        Arc::new(SlowApi {
            ui_thread: std::thread::current().id(),
            base: format!("http://{}", server.address),
        }) as Arc<dyn MusicApi>
    });
    let runtime = ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        Storage {
            config: path.clone(),
            cache: path.join("cache"),
        },
        Preferences::default(),
        provider,
    )
    .unwrap();
    app.show().unwrap();
    use slint::winit_030::WinitWindowAccessor;
    app.window().with_winit_window(|window| {
        assert!(
            !window.is_decorated(),
            "main window still has a system title bar"
        );
    });
    let timer = slint::Timer::default();
    let start = Instant::now();
    let mut previous = start;
    let maximum_gap = Rc::new(Cell::new(Duration::ZERO));
    let recorded_gap = maximum_gap.clone();
    let failures = Rc::new(RefCell::new(Vec::new()));
    let recorded_failures = failures.clone();
    let mut step = 0;
    let weak = app.as_weak();
    let settings = windows._settings.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(20),
        move || {
            let now = Instant::now();
            if previous.duration_since(start) > Duration::from_secs(2) {
                recorded_gap.set(recorded_gap.get().max(now.duration_since(previous)));
            }
            previous = now;
            let app = weak.upgrade().unwrap();
            let settings = settings.upgrade().unwrap();
            let settings_state = app.global::<crate::SettingsState>();
            let navigation_state = app.global::<crate::NavigationState>();
            let audio_state = app.global::<crate::AudioState>();
            let elapsed = now.duration_since(start);
            if elapsed >= Duration::from_secs(3 + step) {
                match step {
                    0 | 6 => {
                        settings_state.invoke_open_settings_window();
                        if !settings.window().is_visible() {
                            recorded_failures.borrow_mut().push("settings did not open");
                        }
                    }
                    1 => {
                        click(&settings, "SettingsSidebar::settings-nav-audio");
                        if settings.global::<crate::SettingsState>().get_settings_tab() != "audio" {
                            recorded_failures
                                .borrow_mut()
                                .push("settings tab did not react");
                        }
                        settings
                            .global::<crate::AudioState>()
                            .set_volume_percent(42.0);
                    }
                    2 => {
                        settings.invoke_close_settings();
                        click(&app, "SidebarNavigation::nav-item-search");
                        if navigation_state.get_current_route() != "search" {
                            recorded_failures
                                .borrow_mut()
                                .push("search click did not react");
                        }
                        if (audio_state.get_volume_percent() - 42.0).abs() > 0.01 {
                            recorded_failures.borrow_mut().push("settings did not sync");
                        }
                    }
                    3 => {
                        click(&app, "SidebarNavigation::nav-item-home");
                        if navigation_state.get_current_route() != "home" {
                            recorded_failures
                                .borrow_mut()
                                .push("home click did not react");
                        }
                    }
                    7 => settings.invoke_close_settings(),
                    _ => {}
                }
                step += 1;
            }
            if elapsed >= Duration::from_secs(if live { 20 } else { 15 }) {
                slint::quit_event_loop().unwrap();
            }
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    let home_state = app.global::<crate::HomeState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let loaded = home_state
        .get_recommend_playlists()
        .iter()
        .filter(|row| row.cover.size().width > 0)
        .count();
    if let Ok(pixels) = app.window().take_snapshot() {
        image::save_buffer(
            path.join("window.png"),
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    app.hide().unwrap();
    drop(runtime);
    println!("real winit test: live={live}, loaded_covers={loaded}, maximum_event_gap={:?}, artifacts={}", maximum_gap.get(), path.display());
    assert!(failures.borrow().is_empty(), "{:?}", failures.borrow());
    assert!(
        !home_state.get_home_playlists_loading(),
        "home still loading"
    );
    assert!(
        loaded >= if live { 1 } else { 5 },
        "covers did not load: {}",
        runtime_state.get_runtime_message()
    );
    if !live {
        assert!(loaded < 12, "offscreen home covers were loaded eagerly");
    }
    assert!(
        maximum_gap.get() < Duration::from_millis(500),
        "UI stalled: {:?}",
        maximum_gap.get()
    );
}

#[test]
#[ignore = "requires a real Wayland/X11 compositor; run alone"]
fn real_login_window_matches_electron_behavior() {
    use super::login_tests::LoginApi;
    use slint::platform::Key;
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
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let path = temp("real-login");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    let api = LoginApi::new();
    let runtime = ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        storage.clone(),
        Preferences::default(),
        Some(api.clone()),
    )
    .unwrap();
    app.show().unwrap();
    let timer = slint::Timer::default();
    let start = Instant::now();
    let mut previous = start;
    let maximum_gap = Rc::new(Cell::new(Duration::ZERO));
    let recorded_gap = maximum_gap.clone();
    let failures = Rc::new(RefCell::new(Vec::new()));
    let recorded_failures = failures.clone();
    let mut step = 0;
    let weak = app.as_weak();
    let artifacts = path.clone();
    let snapshots = Rc::new(RefCell::new(Vec::new()));
    let recorded_snapshots = snapshots.clone();
    let mut next_step = Instant::now() + Duration::from_secs(1);
    let screenshot_time = Rc::new(Cell::new(Duration::ZERO));
    let recorded_screenshot_time = screenshot_time.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(20),
        move || {
            let now = Instant::now();
            if previous.duration_since(start) > Duration::from_secs(2) {
                // Test-only GPU readback is not part of the application's event loop work.
                let gap = now
                    .duration_since(previous)
                    .saturating_sub(recorded_screenshot_time.replace(Duration::ZERO));
                recorded_gap.set(recorded_gap.get().max(gap));
            }
            previous = now;
            if now < next_step {
                return;
            }
            let app = weak.upgrade().unwrap();
            let login_state = app.global::<crate::LoginState>();
            let account_state = app.global::<crate::AccountState>();
            let check = |condition: bool, message: &'static str| {
                if !condition {
                    eprintln!("step {step}: {message}");
                    recorded_failures.borrow_mut().push(message);
                }
            };
            let press = |text: slint::SharedString| {
                app.window()
                    .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                app.window()
                    .dispatch_event(WindowEvent::KeyReleased { text });
            };
            let snapshot = |name: &str| {
                let started = Instant::now();
                if let Ok(pixels) = app.window().take_snapshot() {
                    recorded_snapshots
                        .borrow_mut()
                        .push((artifacts.join(name), pixels));
                }
                recorded_screenshot_time.set(recorded_screenshot_time.get() + started.elapsed());
            };
            match step {
                0 => login_state.set_is_login_open(true),
                1 => {
                    check(
                        !login_state.get_login_remember(),
                        "remember must default to unchecked",
                    );
                    snapshot("initial.png");
                    click(&app, "LoginModal::login-card");
                    check(
                        login_state.get_is_login_open(),
                        "card background must not close dialog",
                    );
                    login_state.set_login_phone("13000000000".into());
                    login_state.set_login_password("test-password".into());
                    click(&app, "LoginSecretField::sec-input");
                    press(Key::Return.into());
                    check(
                        login_state.get_login_busy(),
                        "Enter must submit and show loading",
                    );
                    click(&app, "LoginForm::login-submit-btn");
                }
                2 => {
                    check(
                        login_state.get_login_error() == "账号或密码错误",
                        "password error missing",
                    );
                    check(
                        login_state.get_login_password() == "test-password",
                        "failure cleared password",
                    );
                    check(api.count("login") == 1, "loading button submitted twice");
                    snapshot("password-error.png");
                    click(&app, "LoginMethodTabs::login-sms-tab");
                }
                3 => {
                    check(
                        login_state.get_login_error().is_empty(),
                        "tab change must clear errors",
                    );
                    check(
                        login_state.get_login_sms_phone().is_empty(),
                        "forms must keep separate phones",
                    );
                    login_state.set_login_sms_country_code("81".into());
                    login_state.set_login_sms_phone("09000000000".into());
                    click(&app, "LoginSecretField::login-code-button");
                    check(
                        login_state.get_login_captcha_busy(),
                        "captcha request did not start",
                    );
                }
                4 => {
                    check(
                        login_state.get_login_captcha_seconds() > 0,
                        "SMS countdown missing",
                    );
                    snapshot("sms-countdown.png");
                    click(&app, "LoginSecretField::login-code-button");
                    check(
                        api.count("captcha_sent") == 1,
                        "countdown allowed another SMS",
                    );
                    click(&app, "LoginForm::login-qr-btn");
                    let weak = app.as_weak();
                    let snapshots = recorded_snapshots.clone();
                    let path = artifacts.join("flip-middle.png");
                    let screenshot_time = recorded_screenshot_time.clone();
                    slint::Timer::single_shot(Duration::from_millis(160), move || {
                        let app = weak.upgrade().unwrap();
                        assert!(app.global::<crate::LoginState>().get_login_flip_ready());
                        assert_eq!(
                            ElementHandle::find_by_element_id(&app, "LoginModal::flip-strip")
                                .count(),
                            296
                        );
                        let started = Instant::now();
                        snapshots
                            .borrow_mut()
                            .push((path, app.window().take_snapshot().unwrap()));
                        screenshot_time.set(screenshot_time.get() + started.elapsed());
                    });
                }
                5 => {
                    check(login_state.get_login_qr_ready(), "QR pixels did not arrive");
                    check(
                        !login_state.get_login_busy(),
                        "QR polling disabled form login",
                    );
                    snapshot("qr.png");
                    click(&app, "LoginQr::login-qr-box");
                    click(&app, "LoginQr::login-form-btn");
                }
                6 => {
                    check(
                        api.count("login_qr_key") == 1,
                        "QR box click unexpectedly refreshed",
                    );
                    check(
                        login_state.get_login_sms_phone() == "09000000000",
                        "QR transition cleared form",
                    );
                    click(&app, "LoginRememberChoice::remember-label");
                    check(
                        login_state.get_login_remember(),
                        "remember label is not clickable",
                    );
                    press(Key::Escape.into());
                    check(
                        login_state.get_is_login_open(),
                        "Escape unexpectedly closed login",
                    );
                    click(&app, "LoginForm::login-close-btn");
                }
                7 => {
                    check(!login_state.get_is_login_open(), "form close failed");
                    login_state.set_is_login_open(true);
                }
                8 => {
                    check(
                        login_state.get_login_tab() == "password"
                            && !login_state.get_login_remember(),
                        "reopen defaults differ",
                    );
                    check(
                        login_state.get_login_phone().is_empty()
                            && login_state.get_login_sms_phone().is_empty(),
                        "reopen retained form input",
                    );
                    check(
                        login_state.get_login_captcha_seconds() == 0,
                        "reopen retained countdown",
                    );
                    click(&app, "LoginForm::login-qr-btn");
                }
                9 => {
                    check(login_state.get_login_qr_ready(), "second QR did not load");
                    click(&app, "LoginQr::login-qr-close-btn");
                }
                10 => {
                    check(!login_state.get_is_login_open(), "QR close failed");
                    login_state.set_is_login_open(true);
                }
                11 => {
                    check(
                        login_state.get_login_mode() == "form",
                        "reopen from QR must show form",
                    );
                    let position = LogicalPosition::new(25.0, 200.0);
                    let button = PointerEventButton::Left;
                    app.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    app.window()
                        .dispatch_event(WindowEvent::PointerPressed { position, button });
                    app.window()
                        .dispatch_event(WindowEvent::PointerReleased { position, button });
                    check(!login_state.get_is_login_open(), "backdrop click failed");
                }
                12 => login_state.set_is_login_open(true),
                13 => {
                    login_state.set_login_phone("13000000000".into());
                    login_state.set_login_password("accepted-test-password".into());
                    click(&app, "LoginRememberChoice::remember-label");
                    check(
                        login_state.get_login_remember(),
                        "remembered login was not selected",
                    );
                    click(&app, "LoginForm::login-submit-btn");
                }
                14 => {
                    check(
                        account_state.get_is_logged_in() && !login_state.get_is_login_open(),
                        "successful login did not close dialog",
                    );
                    check(
                        account_state.get_user_nickname() == "测试账号",
                        "account did not load after login",
                    );
                    check(
                        api.count("configure_xeapi") == 1,
                        "login did not configure API session",
                    );
                    slint::quit_event_loop().unwrap();
                }
                _ => unreachable!(),
            }
            step += 1;
            next_step = Instant::now() + Duration::from_secs(1);
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    app.hide().unwrap();
    drop(runtime);
    for (path, pixels) in snapshots.borrow().iter() {
        image::save_buffer(
            path,
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    println!(
        "real login winit test: maximum_event_gap={:?}, artifacts={}",
        maximum_gap.get(),
        path.display()
    );
    assert!(failures.borrow().is_empty(), "{:?}", failures.borrow());
    assert_eq!(
        storage.read::<String>("session.json").unwrap(),
        "mock-session"
    );
    assert!(
        maximum_gap.get() < Duration::from_millis(500),
        "UI stalled: {:?}",
        maximum_gap.get()
    );
}
