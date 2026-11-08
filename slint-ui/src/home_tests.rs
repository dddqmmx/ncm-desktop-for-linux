use crate::{AppState, AppWindow, DailySongItem, RecommendPlaylistItem, Theme};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{
    platform::{PointerEventButton, WindowEvent},
    ComponentHandle, LogicalPosition, Model, VecModel,
};
use std::{cell::RefCell, collections::HashSet, rc::Rc, time::Duration};

fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}
fn settle(app: &AppWindow) {
    for _ in 0..40 {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(16));
    }
    let _ = app.root_element().size();
}
fn wheel(app: &AppWindow, x: f32, y: f32, dx: f32, dy: f32) {
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(x, y),
        delta_x: dx,
        delta_y: dy,
    });
}
fn drag(app: &AppWindow, start: LogicalPosition, delta_x: f32) {
    drag_by(app, start, delta_x, 0.0);
}
fn drag_by(app: &AppWindow, start: LogicalPosition, delta_x: f32, delta_y: f32) {
    app.window()
        .dispatch_event(WindowEvent::PointerMoved { position: start });
    app.window().dispatch_event(WindowEvent::PointerPressed {
        position: start,
        button: PointerEventButton::Left,
    });
    for step in 1..=10 {
        mock_elapsed_time(Duration::from_millis(16));
        app.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(
                start.x + delta_x * step as f32 / 10.0,
                start.y + delta_y * step as f32 / 10.0,
            ),
        });
    }
    // Pause before releasing so assertions inspect the drag, not a fling.
    mock_elapsed_time(Duration::from_millis(120));
    app.window().dispatch_event(WindowEvent::PointerReleased {
        position: LogicalPosition::new(start.x + delta_x, start.y + delta_y),
        button: PointerEventButton::Left,
    });
    settle(app);
}
fn click(app: &AppWindow, id: &str) {
    let item = element(app, id);
    let position = LogicalPosition::new(
        item.absolute_position().x + item.size().width / 2.0,
        item.absolute_position().y + item.size().height / 2.0,
    );
    app.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    app.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    app.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
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
fn init() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
}

fn seed_discovery(app: &AppWindow) {
    let state = app.global::<AppState>();
    let fixture = std::env::var("NCM_HOME_FIXTURE").ok().map(|path| {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(path).unwrap()).unwrap()
    });
    let covers: Vec<_> = state.get_daily_songs().iter().map(|s| s.cover).collect();
    let albums = (0..12)
        .map(|i| {
            let a = fixture.as_ref().map(|v| &v["albums"][i]);
            DailySongItem {
                id: 70000 + i as i32,
                title: a
                    .and_then(|v| v["name"].as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("新专辑 {}", i + 1))
                    .into(),
                artist: a
                    .and_then(|v| v["artist"]["name"].as_str())
                    .unwrap_or("测试歌手")
                    .into(),
                cover: a
                    .and_then(|v| v["localCover"].as_str())
                    .map(|p| slint::Image::load_from_path(std::path::Path::new(p)).unwrap())
                    .unwrap_or_else(|| covers[i % covers.len()].clone()),
            }
        })
        .collect::<Vec<_>>();
    let charts = (0..6)
        .map(|i| {
            let a = fixture.as_ref().map(|v| &v["charts"][i]);
            crate::HomeChartItem {
                id: 80000 + i as i32,
                title: a
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or(["飙升榜", "新歌榜", "原创榜", "热歌榜", "国风榜", "说唱榜"][i])
                    .into(),
                update: a
                    .and_then(|v| v["updateFrequency"].as_str())
                    .unwrap_or("每日更新")
                    .into(),
                cover: a
                    .and_then(|v| v["localCover"].as_str())
                    .map(|p| slint::Image::load_from_path(std::path::Path::new(p)).unwrap())
                    .unwrap_or_else(|| covers[i % covers.len()].clone()),
                tracks: Rc::new(VecModel::from(
                    (0..a
                        .map(|v| v["tracks"].as_array().map_or(0, |t| t.len().min(3)))
                        .unwrap_or(3))
                        .map(|j| {
                            a.map(|v| {
                                format!(
                                    "{} · {}",
                                    v["tracks"][j]["first"].as_str().unwrap_or(""),
                                    v["tracks"][j]["second"].as_str().unwrap_or("")
                                )
                            })
                            .unwrap_or_else(|| format!("榜单歌曲 {} · 测试歌手", j + 1))
                            .into()
                        })
                        .collect::<Vec<slint::SharedString>>(),
                ))
                .into(),
            }
        })
        .collect::<Vec<_>>();
    state.set_home_new_albums(Rc::new(VecModel::from(albums)).into());
    state.set_home_charts(
        Rc::new(VecModel::from(
            charts
                .into_iter()
                .filter(|c| c.tracks.row_count() > 0)
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    state.set_home_albums_loaded(true);
    state.set_home_charts_loaded(true);
}

#[test]
fn home_shelves_drag_without_clicking_and_wheel_scrolls_the_page() {
    init();
    let app = AppWindow::new().unwrap();
    seed_discovery(&app);
    app.show().unwrap();
    let state = app.global::<AppState>();
    settle(&app);
    snapshot(&app, "home-light");
    let title = element(&app, "HomeView::home-page-title");
    let title_y = title.absolute_position().y;
    // Vertical mouse drags must not pan the home page, including shelf gaps.
    // The remaining assertions check horizontal drags and vertical wheel input.
    for x in [288.0, 510.0] {
        drag_by(&app, LogicalPosition::new(x, 350.0), 0.0, -120.0);
        assert!(
            (title.absolute_position().y - title_y).abs() < 0.5,
            "vertical mouse drag moved the home page at x={x}"
        );
        assert_eq!(state.get_current_route(), "home");
    }
    let first = element(&app, "MediaCard::card-surface");
    let first_x = first.absolute_position().x;
    drag(&app, LogicalPosition::new(700.0, 250.0), -120.0);
    assert!(
        first.absolute_position().x < first_x - 60.0,
        "mouse drag did not browse the shelf"
    );
    assert!((title.absolute_position().y - title_y).abs() < 0.5);
    assert_eq!(state.get_current_route(), "home", "drag opened a card");
    let current_song = state.get_current_song_id();
    let song_shelf = element(&app, "HomeView::song-shelf");
    let song_y = song_shelf.absolute_position().y + 80.0;
    drag(&app, LogicalPosition::new(700.0, song_y), -120.0);
    assert_eq!(
        state.get_current_song_id(),
        current_song,
        "drag played a recommended song"
    );
    drag(&app, LogicalPosition::new(580.0, song_y), 120.0);
    let dragged_x = first.absolute_position().x;
    wheel(&app, 700.0, 250.0, -80.0, 0.0);
    settle(&app);
    assert!(
        (first.absolute_position().x - dragged_x).abs() < 0.5,
        "horizontal wheel moved the shelf"
    );
    wheel(&app, 700.0, 250.0, 0.0, -80.0);
    settle(&app);
    assert!(
        (first.absolute_position().x - dragged_x).abs() < 0.5,
        "vertical wheel moved the shelf"
    );
    assert!(
        title.absolute_position().y < title_y - 60.0,
        "wheel over a card did not scroll the page"
    );
    wheel(&app, 288.0, 500.0, 0.0, -220.0);
    settle(&app);
    assert!(
        title.absolute_position().y < title_y - 100.0,
        "page gutter did not scroll vertically"
    );
    wheel(&app, 288.0, 500.0, 0.0, -3000.0);
    settle(&app);
    let charts = element(&app, "HomeView::charts");
    assert!(
        charts.absolute_position().y + charts.size().height <= 881.0,
        "last section hidden by player"
    );
    assert_eq!(
        ElementHandle::find_by_element_id(&app, "MediaCard::root").count(),
        0
    );
    snapshot(&app, "home-scrolled-light");
    app.global::<Theme>().set_is_dark(true);
    settle(&app);
    snapshot(&app, "home-scrolled-dark");
    click(&app, "HomeChartCard::root");
    assert_eq!(state.get_current_route(), "playlist");
    assert_eq!(state.get_playlist_title(), "飙升榜");
    state.set_current_route("home".into());
    settle(&app);
    wheel(&app, 288.0, 500.0, 0.0, -600.0);
    settle(&app);
    let albums = element(&app, "HomeView::new-albums")
        .query_descendants()
        .match_id("AlbumItem::root")
        .find_all();
    assert!(!albums.is_empty());
    let position = LogicalPosition::new(
        albums[0].absolute_position().x + 80.0,
        albums[0].absolute_position().y + 80.0,
    );
    drag(&app, LogicalPosition::new(700.0, position.y), -80.0);
    assert_eq!(state.get_current_route(), "home", "drag opened a new album");
    drag(&app, LogicalPosition::new(620.0, position.y), 80.0);
    app.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    app.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    app.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    assert_eq!(state.get_current_route(), "album");
    assert_eq!(
        state.get_album_title(),
        state.get_home_new_albums().row_data(0).unwrap().title
    );
    state.set_current_route("home".into());
    settle(&app);
    snapshot(&app, "home-dark");
}

#[test]
fn home_shelves_virtualize_both_axes_and_clamp_after_data_changes() {
    init();
    let app = AppWindow::new().unwrap();
    let state = app.global::<AppState>();
    state.set_home_albums_loaded(true);
    state.set_home_charts_loaded(true);
    let lists = Rc::new(VecModel::from(
        (0..10000)
            .map(|i| RecommendPlaylistItem {
                id: 20000 + i,
                title: format!("playlist-{i}").into(),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ));
    state.set_recommend_playlists(lists.clone().into());
    state.set_daily_songs(
        Rc::new(VecModel::from(
            (0..10000)
                .map(|i| DailySongItem {
                    id: 40000 + i,
                    title: format!("song-{i}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    let requests = Rc::new(RefCell::new(HashSet::new()));
    let sink = requests.clone();
    state.on_request_cover(move |id, _| {
        if id >= 20000 {
            sink.borrow_mut().insert(id);
        }
    });
    app.show().unwrap();
    settle(&app);
    assert!((1..=7).contains(&ElementHandle::find_by_element_id(&app, "MediaCard::root").count()));
    assert!((1..=7).contains(&ElementHandle::find_by_element_id(&app, "AlbumItem::root").count()));
    assert!(requests.borrow().len() <= 12);
    let initially_visible = requests.borrow().clone();
    requests.borrow_mut().clear();
    click(&app, "HomeView::playlist-next");
    settle(&app);
    assert!(requests
        .borrow()
        .iter()
        .any(|id| *id >= 20005 && *id < 40000));
    assert!(requests.borrow().difference(&initially_visible).count() <= 6);
    let title_y = element(&app, "HomeView::home-page-title")
        .absolute_position()
        .y;
    drag(&app, LogicalPosition::new(1200.0, 250.0), -800.0);
    assert!(
        (element(&app, "HomeView::home-page-title")
            .absolute_position()
            .y
            - title_y)
            .abs()
            < 0.1
    );
    assert!(ElementHandle::find_by_element_id(&app, "MediaCard::root").count() <= 7);
    lists.set_vec(vec![RecommendPlaylistItem {
        id: 123,
        title: "replacement".into(),
        ..Default::default()
    }]);
    settle(&app);
    let cards: Vec<_> = ElementHandle::find_by_element_id(&app, "MediaCard::root").collect();
    assert_eq!(cards.len(), 1);
    assert!((cards[0].absolute_position().x - 300.0).abs() < 1.0);
    // The wheel scrolls the page even when a shelf has no horizontal overflow.
    let title = element(&app, "HomeView::home-page-title");
    wheel(&app, 700.0, 250.0, 0.0, -200.0);
    settle(&app);
    assert!(title.absolute_position().y < title_y - 100.0);
    state.set_recommend_playlists(
        Rc::new(VecModel::from(Vec::<RecommendPlaylistItem>::new())).into(),
    );
    state.set_daily_songs(Rc::new(VecModel::from(Vec::<DailySongItem>::new())).into());
    settle(&app);
    assert_eq!(
        ElementHandle::find_by_element_id(&app, "MediaCard::root").count(),
        0
    );
    assert_eq!(
        ElementHandle::find_by_element_id(&app, "AlbumItem::root").count(),
        0
    );
}

#[test]
#[ignore = "requires a real Wayland/X11 desktop; home scrolling and visual check"]
fn real_home_scroll_and_theme_layout() {
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
    seed_discovery(&app);
    app.show().unwrap();
    let weak = app.as_weak();
    let timer = slint::Timer::default();
    let mut stage = 0;
    let mut title = None::<ElementHandle>;
    let mut original_y = 0.0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let app = weak.upgrade().unwrap();
            match stage {
                0 => {
                    snapshot(&app, "home-real-light");
                    let heading = element(&app, "HomeView::home-page-title");
                    original_y = heading.absolute_position().y;
                    title = Some(heading);
                    wheel(&app, 700.0, 250.0, 0.0, -100.0);
                }
                1 => {
                    assert!(title.as_ref().unwrap().absolute_position().y < original_y - 80.0);
                    wheel(&app, 288.0, 500.0, 0.0, -700.0);
                }
                2 => {
                    wheel(&app, 288.0, 500.0, 0.0, -1200.0);
                }
                3 => {}
                4 => {
                    let charts = element(&app, "HomeView::charts");
                    let player = element(&app, "PlayerBar::player-bar-panel");
                    assert!(
                        charts.absolute_position().y + charts.size().height
                            <= player.absolute_position().y + 1.0
                    );
                    snapshot(&app, "home-real-scrolled-light");
                    app.global::<Theme>().set_is_dark(true);
                }
                5 => {
                    snapshot(&app, "home-real-scrolled-dark");
                    wheel(&app, 288.0, 500.0, 0.0, 1200.0);
                }
                6 => {
                    wheel(&app, 288.0, 500.0, 0.0, 700.0);
                }
                7 => {}
                8 => {
                    snapshot(&app, "home-real-dark");
                    app.window()
                        .set_size(slint::LogicalSize::new(1340.0, 960.0));
                }
                9 => {
                    snapshot(&app, "home-real-minimum");
                    app.hide().unwrap();
                    slint::quit_event_loop().unwrap();
                }
                _ => return,
            }
            stage += 1;
        },
    );
    slint::run_event_loop().unwrap();
}
