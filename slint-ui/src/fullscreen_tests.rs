use crate::{AppWindow, LyricLineItem};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{platform::WindowEvent, ComponentHandle, LogicalPosition, Model, VecModel};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}
fn advance(app: &AppWindow, ms: u64) {
    for _ in 0..ms.div_ceil(16) {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(16));
    }
    let _ = app.root_element().size();
}
fn rows(app: &AppWindow) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(app, "LyricLineList::lyric-line-item").collect()
}
fn row(app: &AppWindow, index: usize) -> ElementHandle {
    rows(app)
        .into_iter()
        .find(|row| row.accessible_item_index() == Some(index))
        .unwrap_or_else(|| panic!("lyric {index} is not visible"))
}
fn center(app: &AppWindow, row: &ElementHandle) -> f32 {
    row.absolute_position().y + row.size().height / 2.0
        - element(app, "LyricPanelView::lyric-scroll")
            .absolute_position()
            .y
}
fn midpoint(element: &ElementHandle) -> LogicalPosition {
    LogicalPosition::new(
        element.absolute_position().x + element.size().width / 2.0,
        element.absolute_position().y + element.size().height / 2.0,
    )
}
fn click_at(
    app: &AppWindow,
    position: LogicalPosition,
    button: slint::platform::PointerEventButton,
) {
    app.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    app.window()
        .dispatch_event(WindowEvent::PointerPressed { position, button });
    app.window()
        .dispatch_event(WindowEvent::PointerReleased { position, button });
}
fn snapshot(app: &AppWindow, name: &str) {
    if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = app.window().take_snapshot().unwrap();
        image::save_buffer(
            dir.join(format!("{name}.png")),
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn fullscreen_progress_fills_from_the_start_and_seeks_across_resizes() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    app.global::<crate::PlayerViewState>()
        .set_is_fullscreen(true);
    app.global::<crate::RuntimeState>()
        .set_runtime_enabled(true);
    let player = app.global::<crate::PlayerState>();
    let requests = Rc::new(RefCell::new(Vec::new()));
    let received = requests.clone();
    let weak = app.as_weak();
    player.on_seek_requested(move |value| {
        received.borrow_mut().push(value);
        weak.upgrade()
            .unwrap()
            .global::<crate::PlayerState>()
            .set_progress_percent(value);
    });
    app.show().unwrap();
    for (width, height) in [(1435, 1000), (1100, 800), (800, 1000)] {
        app.window()
            .set_size(slint::PhysicalSize::new(width, height));
        advance(&app, 900);
        let track = element(&app, "FullscreenProgress::fullscreen-progress-bar");
        assert!(track.size().width > 100.0);
        for progress in [0.0_f32, 38.0, 100.0] {
            player.set_progress_percent(progress);
            player.set_buffered_percent(progress + 20.0);
            advance(&app, 300);
            for (id, percent) in [
                ("played-fill", progress),
                ("buffered-fill", (progress + 20.0).min(100.0)),
            ] {
                let fill = element(&app, &format!("FullscreenProgress::{id}"));
                assert!(
                    (fill.absolute_position().x - track.absolute_position().x).abs() < 0.5,
                    "{id} must grow from the start of the track at {width}px"
                );
                assert!((fill.size().width - track.size().width * percent / 100.0).abs() < 0.5);
            }
        }
        let at = |fraction| {
            LogicalPosition::new(
                track.absolute_position().x + track.size().width * fraction,
                midpoint(&track).y,
            )
        };
        let left = slint::platform::PointerEventButton::Left;
        click_at(&app, at(0.25), left);
        assert!((player.get_progress_percent() - 25.0).abs() < 0.1);
        app.window().dispatch_event(WindowEvent::PointerPressed {
            position: at(0.5),
            button: left,
        });
        for fraction in [0.75, -0.1, 1.1] {
            app.window().dispatch_event(WindowEvent::PointerMoved {
                position: at(fraction),
            });
            assert!((player.get_progress_percent() - fraction.clamp(0.0, 1.0) * 100.0).abs() < 0.1);
        }
        app.window().dispatch_event(WindowEvent::PointerReleased {
            position: at(1.1),
            button: left,
        });
        let count = requests.borrow().len();
        click_at(&app, at(0.5), slint::platform::PointerEventButton::Right);
        assert_eq!(requests.borrow().len(), count, "right click must not seek");
        player.set_progress_percent(38.0);
        player.set_buffered_percent(68.0);
        advance(&app, 300);
        snapshot(&app, &format!("fullscreen-progress-{width}"));
    }
}

#[test]
fn fullscreen_lyrics_match_electron_layout_follow_and_manual_scroll() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let player_state = app.global::<crate::PlayerState>();
    player_view_state.set_is_fullscreen(true);
    player_view_state.set_is_comment_open(false);
    player_state.set_is_loading(false);
    player_state.set_current_song_is_local(false);
    player_view_state.set_lyrics(
        Rc::new(VecModel::from(
            player_view_state
                .get_lyrics()
                .iter()
                .take(5)
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    player_view_state.set_active_lyric_index(3);
    app.show().unwrap();
    advance(&app, 900);
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fullscreen-ground-truth.json")).unwrap();
    let initial = rows(&app);
    for (index, row) in initial.iter().enumerate() {
        let expected = &baseline["initial"]["rows"][index];
        println!(
            "row {index}: center={} height={}",
            center(&app, row),
            row.size().height
        );
        assert!(
            (row.size().height - expected["layoutHeight"].as_f64().unwrap() as f32).abs() < 1.5
        );
        assert!((center(&app, row) - expected["center"].as_f64().unwrap() as f32).abs() < 2.0);
    }
    snapshot(&app, "fullscreen-slint-initial");
    let previous_center = center(&app, &initial[4]);
    player_view_state.set_active_lyric_index(4);
    advance(&app, 80);
    let during = center(&app, &rows(&app)[4]);
    assert!(during < previous_center - 1.0);
    assert!(
        during > 455.0,
        "automatic follow snapped instead of animating: {during}"
    );
    advance(&app, 600);
    let end = baseline["transition"].as_array().unwrap().last().unwrap();
    assert!(
        (center(&app, &rows(&app)[4]) - end["rows"][4]["center"].as_f64().unwrap() as f32).abs()
            < 2.0
    );
    assert!(
        (rows(&app)[3].size().height - rows(&app)[4].size().height).abs() < 0.5,
        "activation changed layout height"
    );
    player_view_state.set_show_translation(false);
    advance(&app, 80);
    assert!(
        (center(&app, &rows(&app)[4])
            - baseline["withoutTranslation"]["rows"][4]["center"]
                .as_f64()
                .unwrap() as f32)
            .abs()
            < 2.0
    );
    player_view_state.set_show_pronunciation(false);
    advance(&app, 80);
    for (index, line) in rows(&app).iter().enumerate() {
        let expected = &baseline["withoutExtras"]["rows"][index];
        assert!(
            (line.size().height - expected["layoutHeight"].as_f64().unwrap() as f32).abs() < 1.0
        );
        assert!((center(&app, line) - expected["center"].as_f64().unwrap() as f32).abs() < 2.0);
    }

    // Long, wrapped lyrics and optional translations use their actual measured height.
    player_view_state.set_lyrics(Rc::new(VecModel::from((0..80).map(|index| LyricLineItem {
        time_str: format!("{}:{:02}", index / 12, index % 12 * 5).into(),
        text: if index == 22 { "这是一行很长的歌词，需要在窗口内自动换行，保持所有文字完整显示，不得与下一行重叠".into() } else { format!("Song line {index}").into() },
        translation: if index % 2 == 0 { "翻译文字".into() } else { "".into() },
        pronunciation: "".into(),
        ..Default::default()
    }).collect::<Vec<_>>())).into());
    player_view_state.set_show_translation(true);
    player_view_state.set_active_lyric_index(20);
    player_view_state.set_lyric_position_reset(player_view_state.get_lyric_position_reset() + 1);
    advance(&app, 100);
    let viewport_height = element(&app, "LyricPanelView::lyric-scroll").size().height;
    assert!((center(&app, &row(&app, 20)) - viewport_height / 2.0).abs() < 1.0);
    assert!((row(&app, 20).size().height - 82.0).abs() < 0.5);
    assert!(
        (row(&app, 21).size().height - 56.4).abs() < 0.5,
        "missing translation/pronunciation left an empty slot"
    );
    for index in 19..23 {
        let current = row(&app, index);
        let next = row(&app, index + 1);
        let gap =
            next.absolute_position().y - current.absolute_position().y - current.size().height;
        assert!(
            (gap - 12.0).abs() < 0.5,
            "unexpected gap after lyric {index}: {gap}"
        );
    }
    snapshot(&app, "fullscreen-slint-mixed-lines");
    assert!(row(&app, 22).size().height > row(&app, 20).size().height + 30.0);
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(1000.0, 540.0),
        delta_x: 0.0,
        delta_y: -180.0,
    });
    advance(&app, 600);
    assert!(player_view_state.get_show_lyric_seek_indicator());
    let centered = player_view_state.get_centered_lyric_index() as usize;
    assert!(centered > 20);
    assert_eq!(
        player_view_state.get_centered_lyric_time(),
        player_view_state
            .get_lyrics()
            .row_data(centered)
            .unwrap()
            .time_str
    );
    assert!(
        (center(&app, &row(&app, centered)) - viewport_height / 2.0).abs() < 2.0,
        "manual scroll did not snap to a line"
    );
    let before = center(&app, &row(&app, 20));
    player_view_state.set_active_lyric_index(21);
    advance(&app, 600);
    assert!(
        (center(&app, &row(&app, 20)) - before).abs() < 1.0,
        "playback stole manual scroll"
    );
    advance(&app, 3000);
    assert!(!player_view_state.get_show_lyric_seek_indicator());
    assert!((center(&app, &row(&app, 21)) - viewport_height / 2.0).abs() < 1.0);
    player_view_state.set_active_lyric_index(60);
    player_view_state.set_lyric_position_reset(player_view_state.get_lyric_position_reset() + 1);
    advance(&app, 32);
    assert!(
        (center(&app, &row(&app, 60)) - viewport_height / 2.0).abs() < 1.0,
        "seek should position immediately"
    );
}

#[test]
fn opening_fullscreen_requests_artwork_even_when_a_thumbnail_is_already_loaded() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let player_state = app.global::<crate::PlayerState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let player_view_state = app.global::<crate::PlayerViewState>();
    player_state.set_current_song_id(42);
    player_state.set_current_song_cover(slint::Image::from_rgba8(slint::SharedPixelBuffer::new(
        128, 128,
    )));
    let requests = Rc::new(RefCell::new(Vec::new()));
    let sink = requests.clone();
    runtime_state.on_request_cover(move |id, size| sink.borrow_mut().push((id, size)));
    app.show().unwrap();
    advance(&app, 400);
    requests.borrow_mut().clear();
    player_view_state.set_is_fullscreen(true);
    advance(&app, 500);
    assert!(
        requests.borrow().contains(&(42, 1600)),
        "fullscreen reused a small cover without requesting artwork"
    );
    requests.borrow_mut().clear();
    player_state.set_current_song_id(43);
    advance(&app, 32);
    assert!(
        requests.borrow().contains(&(43, 1600)),
        "changing songs in fullscreen did not request artwork"
    );
}

#[test]
fn fullscreen_hover_and_center_indicator_match_electron() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let player_state = app.global::<crate::PlayerState>();
    player_view_state.set_is_fullscreen(true);
    player_state.set_is_loading(false);
    player_view_state.set_show_pronunciation(false);
    player_view_state.set_show_translation(false);
    player_view_state.set_lyrics(
        Rc::new(VecModel::from(
            (0..80)
                .map(|index| LyricLineItem {
                    text: format!("Lyric {index}").into(),
                    time_str: format!("{}:{:02}", index / 12, index % 12 * 5).into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    player_view_state.set_active_lyric_index(20);
    app.show().unwrap();
    advance(&app, 900);
    let hovered = row(&app, 21);
    let pointer = LogicalPosition::new(
        hovered.absolute_position().x + hovered.size().width / 2.0,
        hovered.absolute_position().y + hovered.size().height / 2.0,
    );
    app.window()
        .dispatch_event(WindowEvent::PointerMoved { position: pointer });
    advance(&app, 200);
    assert!((row(&app, 21).computed_opacity() - 0.65).abs() < 0.02);
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: pointer,
        delta_x: 0.0,
        delta_y: -68.4,
    });
    advance(&app, 750);
    assert_eq!(player_view_state.get_centered_lyric_index(), 21);
    let viewport = element(&app, "LyricPanelView::lyric-scroll");
    let guide = element(&app, "LyricSeekIndicator::seek-guide");
    let play = element(&app, "LyricSeekIndicator::seek-play-button");
    let time = element(&app, "LyricSeekIndicator::seek-time");
    assert!(
        guide.size().width > viewport.size().width * 0.7,
        "guide collapsed between time and play button"
    );
    assert!((time.absolute_position().x - viewport.absolute_position().x - 12.0).abs() < 0.5);
    assert!(
        (play.absolute_position().x + play.size().width
            - viewport.absolute_position().x
            - viewport.size().width
            + 12.0)
            .abs()
            < 0.5
    );
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fullscreen-ground-truth.json")).unwrap();
    let hovered_center = baseline["interaction"]["scrolled"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["hovered"] == true && row["centered"] == true)
        .unwrap()["opacity"]
        .as_f64()
        .unwrap() as f32;
    assert!(
        (row(&app, 22).computed_opacity() - 0.3).abs() < 0.02,
        "layout movement changed hover without a pointer move: {}",
        row(&app, 22).computed_opacity()
    );
    assert!(
        (row(&app, 21).computed_opacity() - hovered_center).abs() < 0.02,
        "scrolling did not preserve Electron's hovered + centered emphasis: {}",
        row(&app, 21).computed_opacity()
    );
    app.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(pointer.x + 1.0, pointer.y),
    });
    advance(&app, 220);
    assert!((row(&app, 22).computed_opacity() - 0.65).abs() < 0.02);
    assert!((row(&app, 21).computed_opacity() - 0.75).abs() < 0.02);
    advance(&app, 700);
    assert_eq!(
        row(&app, 22).accessible_description().as_deref(),
        Some("跳转到 1:50 播放")
    );
    assert!(
        ElementHandle::find_by_element_id(&app, "LyricPanelView::lyric-hover-hint")
            .next()
            .is_some(),
        "hovering a lyric did not show its seek tooltip"
    );

    player_state.set_is_loading(true);
    advance(&app, 32);
    assert!(!player_view_state.get_show_lyric_seek_indicator());
    assert_eq!(player_view_state.get_centered_lyric_index(), -1);
    advance(&app, 220);
    assert!((row(&app, 22).computed_opacity() - 0.3).abs() < 0.02);
    assert!(
        ElementHandle::find_by_element_id(&app, "LyricPanelView::lyric-hover-hint")
            .next()
            .is_none()
    );
    player_state.set_is_loading(false);
    advance(&app, 32);
    assert!(player_view_state.get_show_lyric_seek_indicator());

    // The guide/time overlay passes clicks through to the centered lyric.
    let center_point = midpoint(&row(&app, 21));
    app.window().dispatch_event(WindowEvent::PointerMoved {
        position: center_point,
    });
    advance(&app, 220);
    assert!((row(&app, 21).computed_opacity() - hovered_center).abs() < 0.02);
    click_at(
        &app,
        center_point,
        slint::platform::PointerEventButton::Left,
    );
    advance(&app, 32);
    assert_eq!(player_view_state.get_active_lyric_index(), 21);
    assert!(!player_view_state.get_show_lyric_seek_indicator());

    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: pointer,
        delta_x: 0.0,
        delta_y: -68.4,
    });
    advance(&app, 750);
    assert_eq!(player_view_state.get_centered_lyric_index(), 22);
    let play_point = midpoint(&element(&app, "LyricSeekIndicator::seek-play-button"));
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: play_point,
        delta_x: 0.0,
        delta_y: -68.4,
    });
    advance(&app, 750);
    assert_eq!(
        player_view_state.get_centered_lyric_index(),
        23,
        "the play button swallowed the wheel"
    );
    click_at(&app, play_point, slint::platform::PointerEventButton::Left);
    advance(&app, 32);
    assert_eq!(player_view_state.get_active_lyric_index(), 23);
    assert_eq!(player_state.get_current_time_str(), "1:55");
    assert!(!player_view_state.get_show_lyric_seek_indicator());

    // A tall next line can contain the viewport center while its center is
    // farther away than the preceding short line (Electron chooses nearest).
    let mut mixed = player_view_state.get_lyrics().iter().collect::<Vec<_>>();
    mixed[21].text = "Long line\nSecond line\nThird line\nFourth line".into();
    player_view_state.set_lyrics(Rc::new(VecModel::from(mixed)).into());
    player_view_state.set_active_lyric_index(20);
    player_view_state.set_lyric_position_reset(player_view_state.get_lyric_position_reset() + 1);
    advance(&app, 200);
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: pointer,
        delta_x: 0.0,
        delta_y: -50.0,
    });
    advance(&app, 48);
    assert_eq!(
        player_view_state.get_centered_lyric_index(),
        20,
        "indicator selected the containing row instead of the nearest line center"
    );
    let viewport = element(&app, "LyricPanelView::lyric-scroll");
    click_at(
        &app,
        LogicalPosition::new(
            viewport.absolute_position().x + 8.0,
            viewport.absolute_position().y + viewport.size().height / 2.0,
        ),
        slint::platform::PointerEventButton::Right,
    );
    advance(&app, 600);
    assert!(
        !player_view_state.get_show_lyric_seek_indicator(),
        "right click in padding did not cancel manual scrolling"
    );
    assert_eq!(player_view_state.get_centered_lyric_index(), -1);
    assert!((center(&app, &row(&app, 20)) - viewport.size().height / 2.0).abs() < 1.0);
}

#[test]
#[ignore = "requires a real Wayland/X11 desktop; fullscreen rendering check"]
fn real_fullscreen_artwork_and_lyric_follow() {
    use slint::winit_030::{
        winit::event::WindowEvent as WinitEvent, EventResult, WinitWindowAccessor,
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
    // This test injects pointer events below. Desktop cursor movement must not
    // replace that pointer while an animation or GPU snapshot is in progress.
    app.window().on_winit_window_event(|_, event| match event {
        WinitEvent::CursorMoved { .. }
        | WinitEvent::CursorEntered { .. }
        | WinitEvent::CursorLeft { .. }
        | WinitEvent::MouseWheel { .. }
        | WinitEvent::MouseInput { .. } => EventResult::PreventDefault,
        _ => EventResult::Propagate,
    });
    let player_state = app.global::<crate::PlayerState>();
    let player_view_state = app.global::<crate::PlayerViewState>();
    player_state.set_current_song_title("全屏显示验证 · 模拟数据".into());
    player_state.set_is_loading(false);
    player_view_state.set_is_fullscreen(true);
    let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(1600, 1600);
    for (i, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
        let x = i % 1600;
        let y = i / 1600;
        let stripe = if (x / 8 + y / 8) % 2 == 0 { 35 } else { 0 };
        *pixel = slint::Rgba8Pixel::new(
            (x * 200 / 1600 + stripe) as u8,
            (y * 200 / 1600 + stripe) as u8,
            160,
            255,
        );
    }
    player_state.set_current_song_cover(slint::Image::from_rgba8(pixels));
    player_view_state.set_lyrics(
        Rc::new(VecModel::from(
            (0..80)
                .map(|index| LyricLineItem {
                    time_str: format!("{}:{:02}", index / 12, index % 12 * 5).into(),
                    text: if index == 22 {
                        "这是一行很长的歌词，需要在窗口内自动换行，保持所有文字完整显示，不得与下一行重叠".into()
                    } else {
                        format!("Lyric line {index} · 原文跟随播放居中").into()
                    },
                    translation: if index % 2 == 0 {
                        "译文与原文一起缩放，布局高度保持稳定".into()
                    } else {
                        "".into()
                    },
                    pronunciation: "".into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    player_view_state.set_active_lyric_index(20);
    app.show().unwrap();
    let weak = app.as_weak();
    let samples = Rc::new(RefCell::new(Vec::new()));
    let sink = samples.clone();
    let mut stage_started = std::time::Instant::now();
    let timer = slint::Timer::default();
    let mut stage = 0;
    let mut captured_initial = false;
    let mut hover_index = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let Some(app) = weak.upgrade() else { return };
            let elapsed = stage_started.elapsed().as_millis();
            let player_view_state = app.global::<crate::PlayerViewState>();
            if elapsed >= 800 && stage == 0 {
                if !captured_initial {
                    snapshot(&app, "fullscreen-real-initial");
                    captured_initial = true;
                    // Refresh Slint's animation clock on the next event-loop
                    // turn after the potentially slow GPU readback.
                    return;
                }
                player_view_state.set_active_lyric_index(21);
                stage_started = std::time::Instant::now();
                stage = 1;
            } else if elapsed >= 100 && stage == 1 {
                sink.borrow_mut().push(center(&app, &row(&app, 21)));
                stage = 2;
            } else if elapsed >= 700 && stage == 2 {
                sink.borrow_mut().push(center(&app, &row(&app, 21)));
                snapshot(&app, "fullscreen-real-follow");
                app.window().dispatch_event(WindowEvent::PointerScrolled {
                    position: LogicalPosition::new(1000.0, 540.0),
                    delta_x: 0.0,
                    delta_y: -180.0,
                });
                stage_started = std::time::Instant::now();
                stage = 3;
            } else if elapsed >= 1200 && stage == 3 {
                assert!(player_view_state.get_show_lyric_seek_indicator());
                hover_index = player_view_state.get_centered_lyric_index() as usize;
                app.window().dispatch_event(WindowEvent::PointerMoved {
                    position: midpoint(&row(&app, hover_index)),
                });
                stage_started = std::time::Instant::now();
                stage = 4;
            } else if elapsed >= 700 && stage == 4 {
                assert!(
                    (row(&app, hover_index).computed_opacity() - 0.9).abs() < 0.02,
                    "real hover {hover_index}: opacity={}, centered={}, active={}, indicator={}",
                    row(&app, hover_index).computed_opacity(),
                    player_view_state.get_centered_lyric_index(),
                    player_view_state.get_active_lyric_index(),
                    player_view_state.get_show_lyric_seek_indicator()
                );
                assert!(ElementHandle::find_by_element_id(&app, "LyricPanelView::lyric-hover-hint").next().is_some(),
                    "real hover did not show the lyric hint");
                snapshot(&app, "fullscreen-real-hover-centered");
                let from = row(&app, hover_index);
                let next = row(&app, hover_index + 1);
                let delta = center(&app, &next) - center(&app, &from);
                println!("real scroll: hover={hover_index}, center={}, next={}, delta={delta}, model={}/{}", center(&app, &from), center(&app, &next),
                    player_view_state.get_lyrics().row_data(hover_index).unwrap().layout_center,
                    player_view_state.get_lyrics().row_data(hover_index + 1).unwrap().layout_center);
                app.window().dispatch_event(WindowEvent::PointerScrolled {
                    position: midpoint(&from),
                    delta_x: 0.0,
                    delta_y: -delta,
                });
                stage_started = std::time::Instant::now();
                stage = 5;
            } else if elapsed >= 700 && stage == 5 {
                assert_eq!(player_view_state.get_centered_lyric_index() as usize, hover_index + 1,
                    "after wheel: previous center={}, next center={}", center(&app, &row(&app, hover_index)), center(&app, &row(&app, hover_index + 1)));
                assert!(
                    (row(&app, hover_index).computed_opacity() - 0.65).abs() < 0.02,
                    "real window changed hover while only scrolling"
                );
                assert!((row(&app, hover_index + 1).computed_opacity() - 0.75).abs() < 0.02);
                snapshot(&app, "fullscreen-real-hover-scroll");
                stage = 6;
            } else if elapsed >= 4200 && stage == 6 {
                slint::quit_event_loop().unwrap();
            }
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    let samples = samples.borrow();
    println!("real fullscreen follow samples: {samples:?}");
    assert_eq!(samples.len(), 2);
    assert!(
        samples[0] > samples[1] + 1.0,
        "follow animation did not move: {samples:?}"
    );
    let viewport_height = element(&app, "LyricPanelView::lyric-scroll").size().height;
    assert!(
        (samples[1] - viewport_height / 2.0).abs() < 1.0,
        "lyric center {} != viewport center {}",
        samples[1],
        viewport_height / 2.0
    );
    assert!(!player_view_state.get_show_lyric_seek_indicator());
    assert!(
        (center(&app, &row(&app, 21)) - viewport_height / 2.0).abs() < 1.0,
        "manual scrolling did not return to the active line"
    );
    for index in 19..23 {
        let current = row(&app, index);
        let next = row(&app, index + 1);
        let gap =
            next.absolute_position().y - current.absolute_position().y - current.size().height;
        assert!(
            (gap - 12.0).abs() < 0.5,
            "real window lyric gap {index}: {gap}"
        );
    }
    assert_eq!(player_state.get_current_song_cover().size().width, 1600);
    println!("real fullscreen: lyric center during/after follow={samples:?}; cover={}px; manual follow resumed", player_state.get_current_song_cover().size().width);
    app.hide().unwrap();
}
