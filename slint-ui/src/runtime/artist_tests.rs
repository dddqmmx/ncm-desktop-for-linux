use super::{
    backend::{Command, Event},
    data::Preferences,
    view::{self, View},
};
use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::{json, Value};
use slint::{platform::WindowEvent, ComponentHandle, LogicalPosition, Model, PhysicalSize};
use std::time::Duration;

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
fn settle(app: &AppWindow) {
    for _ in 0..20 {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(16));
    }
}
fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}
fn event(token: u64, body: Value) -> Event {
    Event {
        name: "page".into(),
        token,
        epoch: 0,
        data: json!({"kind":"artist","id":"100","body":body}),
        error: None,
    }
}
fn next_page(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>) -> Command {
    loop {
        let c = rx.try_recv().expect("missing page request");
        if c.name == "page" {
            return c;
        }
    }
}
fn rect(label: &str, item: &ElementHandle, expected: &Value) {
    let p = item.absolute_position();
    let s = item.size();
    for (axis, actual) in [
        ("x", p.x),
        ("y", p.y),
        ("width", s.width),
        ("height", s.height),
    ] {
        let expected = expected[axis].as_f64().unwrap() as f32;
        assert!(
            (actual - expected).abs() < 1.0,
            "{label}.{axis}: Slint {actual} != Electron {expected}"
        );
    }
}
fn snapshot(app: &AppWindow, name: &str) {
    if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let pixels = app.window().take_snapshot().unwrap();
        image::save_buffer(
            std::path::PathBuf::from(dir).join(format!("{name}.png")),
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

// Geometry alone cannot catch an elided first line inside a reserved two-line box.
fn assert_two_line_release_title(app: &AppWindow) {
    let title = ElementHandle::find_by_element_id(app, "ArtistReleaseInfo::release-title")
        .find(|item| {
            item.size().height > 38.0 && (0.0..650.0).contains(&item.absolute_position().y)
        })
        .expect("missing visible wrapped release title");
    let p = title.absolute_position();
    let pixels = app.window().take_snapshot().unwrap();
    let scale_x = pixels.width() as f32 / app.root_element().size().width;
    let scale_y = pixels.height() as f32 / app.root_element().size().height;
    for (line, top, bottom) in [(1, 3.0, 18.0), (2, 23.0, 38.0)] {
        let mut ink = 0;
        for y in ((p.y + top) * scale_y) as u32..((p.y + bottom) * scale_y) as u32 {
            for x in (p.x * scale_x) as u32..((p.x + title.size().width) * scale_x) as u32 {
                let pixel = pixels.as_slice()[(y * pixels.width() + x) as usize];
                if pixel.r < 100 && pixel.g < 100 && pixel.b < 100 {
                    ink += 1;
                }
            }
        }
        assert!(
            ink > 20,
            "release title line {line} rendered no text ({ink} dark pixels)"
        );
    }
}

#[test]
fn artist_layout_matches_electron_with_long_descriptions_and_wrapped_grids() {
    init();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let navigation_state = app.global::<crate::NavigationState>();
    let artist_state = app.global::<crate::ArtistState>();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut controller = View::new(tx, Preferences::default());
    app.show().unwrap();
    let baseline: Value =
        serde_json::from_str(include_str!("../../tests/artist-ground-truth.json")).unwrap();
    for case in baseline["cases"].as_array().unwrap() {
        let width = case["width"].as_u64().unwrap() as u32;
        let name = case["name"].as_str().unwrap();
        // Expose below-the-fold elements to the testing backend, without changing page width.
        app.window().set_size(PhysicalSize::new(width, 10000));
        navigation_state.set_current_route("home".into());
        settle(&app);
        navigation_state.set_current_route("artist".into());
        controller.apply(&app, event(0, case["fixture"].clone()));
        seed_artwork(&app);
        settle(&app);
        let gt = &case["geometry"];
        for (key, id) in [
            ("main", "ArtistView::artist-main-content"),
            ("header", "ArtistView::artist-header"),
            ("avatar", "ArtistHeader::artist-avatar-col"),
            ("name", "ArtistHeader::artist-name-box"),
            ("summary", "ArtistHeader::artist-summary"),
            ("pills", "ArtistHeader::artist-pills"),
            ("actions", "ArtistHeader::artist-actions"),
        ] {
            rect(
                &format!("{name}/{width}/{key}"),
                &element(&app, id),
                &gt[key],
            );
        }
        for (index, id) in [
            "artist-latest-section",
            "artist-songs-section",
            "artist-albums-section",
            "artist-eps-section",
            "artist-mvs-section",
        ]
        .iter()
        .enumerate()
        {
            if name == "empty" {
                assert!(
                    ElementHandle::find_by_element_id(&app, &format!("ArtistView::{id}"))
                        .next()
                        .is_none()
                );
            } else {
                rect(
                    &format!("{name}/{width}/{id}"),
                    &element(&app, &format!("ArtistView::{id}")),
                    &gt["sections"][index],
                );
            }
        }
        if name != "empty" {
            rect(
                "songsCard",
                &element(&app, "ArtistSongsSection::artist-songs-card"),
                &gt["songsCard"],
            );
            let rows: Vec<_> =
                ElementHandle::find_by_element_type_name(&app, "ArtistSongRow").collect();
            assert_eq!(rows.len(), artist_state.get_artist_top_songs().row_count());
            for (i, row) in rows.iter().enumerate() {
                rect("song", row, &gt["sections"][1]["items"][i]);
            }
            for (i, cover) in
                ElementHandle::find_by_element_id(&app, "ArtistLatestSection::latest-cover")
                    .enumerate()
            {
                rect("latest cover", &cover, &gt["sections"][0]["covers"][i]);
            }
            let covers: Vec<_> =
                ElementHandle::find_by_element_id(&app, "ArtistMediaTile::tile-img-wrapper")
                    .collect();
            let expected: Vec<_> = gt["sections"]
                .as_array()
                .unwrap()
                .iter()
                .skip(2)
                .flat_map(|s| s["covers"].as_array().unwrap())
                .collect();
            assert_eq!(
                covers.len(),
                expected.len(),
                "hidden grid cells must not request or display covers"
            );
            for (i, (cover, expected)) in covers.iter().zip(expected).enumerate() {
                rect(&format!("{name}/{width}/cover{i}"), cover, expected);
            }
        }
        app.window().set_size(PhysicalSize::new(width, 1000));
        settle(&app);
        snapshot(&app, &format!("artist-{name}-{width}"));
        if name == "long" {
            let target = gt["sections"][2]["y"].as_f64().unwrap() as f32;
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(width as f32 - 120.0, 400.0),
                delta_x: 0.0,
                delta_y: -target,
            });
            settle(&app);
            snapshot(&app, &format!("artist-grid-{width}"));
            assert_two_line_release_title(&app);
            assert!(
                element(&app, "ArtistView::artist-albums-section")
                    .absolute_position()
                    .y
                    < 300.0
            );
        }
    }
}

#[test]
fn artist_loads_sorted_releases_and_handles_switch_retry_and_empty_data() {
    init();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let artist_state = app.global::<crate::ArtistState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut controller = View::new(tx, Preferences::default());
    app.show().unwrap();
    controller.action(&app, "route", -1, "album");
    controller.apply(
        &app,
        Event {
            data: json!({"kind":"album","body":{"album":{"artist":{"id":"9007199254740991"}}}}),
            ..event(0, json!({}))
        },
    );
    controller.action(&app, "artist", -1, "");
    let first = next_page(&mut rx);
    assert_eq!(first.data["id"], "9007199254740991");
    assert!(artist_state.get_artist_loading());
    settle(&app);
    let loading = element(&app, "ArtistView::artist-state");
    let spinner = element(&app, "ArtistLoadState::artist-loading-spinner");
    assert!(
        (spinner.absolute_position().x + spinner.size().width / 2.0
            - loading.absolute_position().x
            - loading.size().width / 2.0)
            .abs()
            < 1.0
    );
    assert!(
        ElementHandle::find_by_element_id(&app, "ArtistView::artist-header")
            .next()
            .is_none()
    );
    controller.apply(
        &app,
        Event {
            error: Some("歌手加载失败".into()),
            ..event(first.token, json!({}))
        },
    );
    assert!(!artist_state.get_artist_loading());
    assert_eq!(artist_state.get_artist_load_error(), "歌手加载失败");
    controller.action(&app, "artist-retry", -1, "");
    let retry = next_page(&mut rx);
    assert_eq!(retry.data["id"], first.data["id"]);
    controller.apply(
        &app,
        event(
            first.token,
            json!({"detail":{"data":{"artist":{"name":"stale"}}}}),
        ),
    );
    assert!(artist_state.get_artist_loading());
    let body = json!({
        "detail":{"data":{"artist":{"name":"歌手", "briefDesc":"  ", "avatar":"https://example.test/avatar.jpg", "cover":"https://example.test/cover.jpg"}}},
        "album":{"hotAlbums":[
            {"id":401,"name":"Old album","publishTime":946684800000_i64,"type":"录音室"},
            {"id":402,"name":"New single","publishTime":1704067200000_i64,"type":"专辑","subType":"Single"},
            {"id":403,"name":"EP","publishTime":1672531200000_i64,"type":"EP"}
        ]},
        "mv":{"mvs":[{"id":500,"name":"MV","imgurl16v9":"https://example.test/wide.jpg","publishTime":"2023-02-07"}]},
        "top":{"songs":[{"id":"9007199254740990","name":"Song","dt":180000},{"id":2,"name":"Song two"}]}
    });
    controller.apply(&app, event(retry.token, body));
    assert!(!artist_state.get_artist_loading());
    assert!(artist_state.get_artist_load_error().is_empty());
    assert_eq!(
        artist_state.get_artist_latest().row_data(0).unwrap().title,
        "New single"
    );
    assert_eq!(
        artist_state.get_artist_latest().row_data(0).unwrap().year,
        "2024"
    );
    assert_eq!(artist_state.get_artist_albums().row_count(), 1);
    assert_eq!(artist_state.get_artist_eps().row_count(), 2);
    assert_eq!(
        artist_state.get_artist_pills().row_data(1).unwrap(),
        "2 张 EP / 单曲"
    );
    assert_eq!(
        artist_state.get_artist_mvs().row_data(0).unwrap().year,
        "2023"
    );
    assert_eq!(
        artist_state
            .get_artist_top_songs()
            .row_data(0)
            .unwrap()
            .badge,
        "hot"
    );
    assert_eq!(
        artist_state
            .get_artist_top_songs()
            .row_data(1)
            .unwrap()
            .badge,
        "rec"
    );
    assert!(artist_state
        .get_artist_summary()
        .contains("最新发布《New single》"));
    let album = artist_state.get_artist_latest().row_data(0).unwrap();
    controller.action(&app, "album", album.id, "");
    let album_page = next_page(&mut rx);
    assert_eq!(album_page.data["id"], "402");
    controller.apply(
        &app,
        event(
            retry.token,
            json!({"detail":{"data":{"artist":{"name":"stale"}}}}),
        ),
    );
    assert_eq!(navigation_state.get_current_route(), "album");
    assert_eq!(artist_state.get_artist_name(), "歌手");
    controller.action(&app, "artist-retry", -1, "");
    let empty = next_page(&mut rx);
    assert_eq!(artist_state.get_artist_top_songs().row_count(), 0);
    controller.apply(
        &app,
        event(
            empty.token,
            json!({"detail":{"data":{"artist":{"name":"Empty"}}}}),
        ),
    );
    assert!(!artist_state.get_artist_loading());
    assert!(artist_state.get_artist_load_error().is_empty());
    assert_eq!(artist_state.get_artist_albums().row_count(), 0);
    assert_eq!(
        artist_state.get_artist_summary(),
        "当前已收录 0 首热门歌曲、0 张专辑。"
    );
    controller.action(&app, "artist-retry", -1, "");
    let missing = next_page(&mut rx);
    controller.apply(&app, event(missing.token, json!({})));
    assert!(!artist_state.get_artist_loading());
    assert_eq!(artist_state.get_artist_load_error(), "未获取到歌手详情");
}

fn seed_artwork(app: &AppWindow) {
    let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(32, 32);
    pixels
        .make_mut_slice()
        .fill(slint::Rgba8Pixel::new(99, 102, 241, 255));
    let cover = slint::Image::from_rgba8(pixels);
    let artist_state = app.global::<crate::ArtistState>();
    artist_state.set_artist_avatar(cover.clone());
    let releases = |items: slint::ModelRc<crate::ArtistReleaseItem>| {
        view::model(
            items
                .iter()
                .map(|mut item| {
                    item.cover = cover.clone();
                    item
                })
                .collect(),
        )
    };
    artist_state.set_artist_latest(releases(artist_state.get_artist_latest()));
    artist_state.set_artist_albums(releases(artist_state.get_artist_albums()));
    artist_state.set_artist_eps(releases(artist_state.get_artist_eps()));
    artist_state.set_artist_mvs(releases(artist_state.get_artist_mvs()));
    artist_state.set_artist_top_songs(view::model(
        artist_state
            .get_artist_top_songs()
            .iter()
            .map(|mut song| {
                song.cover = cover.clone();
                song
            })
            .collect(),
    ));
}

#[test]
#[ignore = "requires a real Wayland/X11 desktop; checks artist page scrolling with the application renderer"]
fn real_artist_page_scrolls_through_songs_albums_and_mvs() {
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
    view::initialize(&app, &Preferences::default());
    app.global::<crate::NavigationState>()
        .set_current_route("artist".into());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut controller = View::new(tx, Preferences::default());
    let baseline: Value =
        serde_json::from_str(include_str!("../../tests/artist-ground-truth.json")).unwrap();
    let case = baseline["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "long" && c["width"] == 1435)
        .unwrap();
    controller.apply(&app, event(0, case["fixture"].clone()));
    seed_artwork(&app);
    app.show().unwrap();
    let weak = app.as_weak();
    let timer = slint::Timer::default();
    let stage = std::cell::Cell::new(0);
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let app = weak.unwrap();
            let scroll = |dy| {
                app.window().dispatch_event(WindowEvent::PointerScrolled {
                    position: LogicalPosition::new(1250.0, 400.0),
                    delta_x: 0.0,
                    delta_y: dy,
                })
            };
            match stage.get() {
                1 => {
                    let summary = element(&app, "ArtistHeader::artist-summary");
                    let actions = element(&app, "ArtistHeader::artist-actions");
                    assert!(
                        actions.absolute_position().y
                            >= summary.absolute_position().y + summary.size().height + 19.0
                    );
                    snapshot(&app, "artist-real-header");
                    scroll(-4751.0);
                }
                3 => {
                    let albums = element(&app, "ArtistView::artist-albums-section");
                    assert!(albums.absolute_position().y.abs() < 100.0);
                    snapshot(&app, "artist-real-albums");
                    assert_two_line_release_title(&app);
                    scroll(-10000.0);
                }
                5 => {
                    let mvs = element(&app, "ArtistView::artist-mvs-section");
                    assert!(mvs.absolute_position().y < 1000.0);
                    assert!(mvs.absolute_position().y + mvs.size().height <= 1000.0);
                    snapshot(&app, "artist-real-mvs");
                    app.global::<crate::ArtistState>().set_artist_loading(true);
                }
                7 => {
                    assert!(
                        element(&app, "ArtistView::artist-state")
                            .absolute_position()
                            .y
                            >= 55.0
                    );
                    snapshot(&app, "artist-real-loading");
                    slint::quit_event_loop().unwrap();
                }
                _ => {}
            }
            stage.set(stage.get() + 1);
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    app.hide().unwrap();
}
