use super::{
    backend::{Command, Event},
    data::Preferences,
    view::{self, View},
};
use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementRoot};
use serde_json::json;
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn settle(app: &AppWindow) {
    for _ in 0..15 {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(16));
    }
}
fn command(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>, name: &str) -> Command {
    loop {
        let c = rx.try_recv().unwrap_or_else(|_| panic!("missing {name}"));
        if c.name == name {
            return c;
        }
    }
}
fn event(c: &Command, data: serde_json::Value) -> Event {
    Event {
        name: c.name.clone(),
        token: c.token,
        epoch: 0,
        data,
        error: None,
    }
}

#[test]
fn discovery_loads_lazily_isolates_failures_and_opens_full_ids() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let runtime_state = app.global::<crate::RuntimeState>();
    let home_state = app.global::<crate::HomeState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let detail_state = app.global::<crate::DetailState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let controller = Rc::new(RefCell::new(View::new(tx, Preferences::default())));
    let weak = app.as_weak();
    let handle = controller.clone();
    runtime_state.on_runtime_action(move |name, id, arg| {
        handle.borrow_mut().action(&weak.unwrap(), &name, id, &arg)
    });
    app.show().unwrap();
    settle(&app);
    let albums = command(&mut rx, "home-albums");
    assert!(home_state.get_home_albums_loading());
    assert!(
        !home_state.get_home_charts_loading(),
        "offscreen charts requested eagerly"
    );
    controller.borrow_mut().apply(&app, event(&albums,json!({"albums":[{
        "id":"9007199254740991", "name":"New release", "picUrl":"https://example.test/new.jpg",
        "artists":[{"name":"Artist A"},{"name":"Artist B"}]
    }]})));
    assert!(home_state.get_home_albums_loaded());
    assert!(!home_state.get_home_albums_loading());
    let album = home_state.get_home_new_albums().row_data(0).unwrap();
    assert_eq!(album.artist, "Artist A / Artist B");
    controller.borrow_mut().action(&app, "home-charts", -1, "");
    let charts = command(&mut rx, "home-charts");
    controller.borrow_mut().apply(
        &app,
        Event {
            error: Some("network unavailable".into()),
            ..event(&charts, json!(null))
        },
    );
    assert!(!home_state.get_home_charts_error().is_empty());
    assert_eq!(home_state.get_home_new_albums().row_count(), 1);
    assert!(
        runtime_state.get_runtime_message().is_empty(),
        "discovery failure replaced the playback status"
    );
    assert!(
        home_state.get_home_playlists_loading(),
        "discovery modified the independent recommendations"
    );
    settle(&app);
    while rx.try_recv().is_ok() {}
    controller.borrow_mut().action(&app, "home-charts", -1, "");
    assert!(
        rx.try_recv().is_err(),
        "failed request retried without user action"
    );
    controller
        .borrow_mut()
        .action(&app, "home-charts", -1, "retry");
    let retried = command(&mut rx, "home-charts");
    assert!(retried.token > charts.token);
    controller.borrow_mut().apply(
        &app,
        event(&charts, json!({"list":[{"id":"wrong","name":"stale"}]})),
    );
    assert!(home_state.get_home_charts_loading());
    assert_eq!(home_state.get_home_charts().row_count(), 0);
    controller.borrow_mut().apply(&app,event(&retried,json!({"list":[{
        "id":"9007199254740990", "name":"Chart", "coverImgUrl":"https://example.test/chart.jpg",
        "updateFrequency":"每日更新", "tracks":[{"first":"Track", "second":"Artist"}]
    }]})));
    let chart = home_state.get_home_charts().row_data(0).unwrap();
    assert_eq!(chart.tracks.row_data(0).unwrap(), "Track · Artist");
    assert!(!home_state.get_home_charts_loading());
    controller
        .borrow_mut()
        .action(&app, "playlist", chart.id, "");
    let page = command(&mut rx, "page");
    assert_eq!(page.data["id"], "9007199254740990");
    assert_eq!(playlist_state.get_playlist_title(), "Chart");
    controller.borrow_mut().action(&app, "album", album.id, "");
    let page = command(&mut rx, "page");
    assert_eq!(page.data["id"], "9007199254740991");
    assert_eq!(detail_state.get_album_title(), "New release");
    assert_eq!(navigation_state.get_current_route(), "album");
    // A login transition invalidates an in-flight refresh without leaving a spinner.
    controller
        .borrow_mut()
        .action(&app, "home-albums", -1, "retry");
    let obsolete = command(&mut rx, "home-albums");
    controller.borrow_mut().action(&app, "qr", -1, "");
    assert!(!home_state.get_home_albums_loading());
    assert!(!home_state.get_home_albums_loaded());
    controller
        .borrow_mut()
        .apply(&app, event(&obsolete, json!({"albums":[]})));
    assert_eq!(home_state.get_home_new_albums().row_count(), 1);
}

#[test]
fn visited_home_covers_survive_card_recycling_and_fullscreen_upgrades() {
    use super::images::{DecodedImage, ImageTarget};
    use i_slint_backend_testing::ElementHandle;
    use slint::platform::WindowEvent;
    use std::collections::HashMap;

    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    app.show().unwrap();
    app.window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    app.window()
        .set_size(slint::LogicalSize::new(1435.0, 1000.0));
    let account_state = app.global::<crate::AccountState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let home_state = app.global::<crate::HomeState>();
    account_state.set_is_logged_in(true);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut controller = View::new(tx, Preferences::default());
    let pending = Rc::new(RefCell::new(HashMap::<i32, u32>::new()));
    let sink = pending.clone();
    runtime_state.on_request_cover(move |id, size| {
        let mut pending = sink.borrow_mut();
        let entry = pending.entry(id).or_default();
        *entry = (*entry).max(size as u32);
    });
    for (name, data) in [
        (
            "home",
            json!({"body": {
                "playlists": {"recommend": (0..8).map(|i| json!({"id":i+1,"name":format!("Playlist {i}"),"picUrl":format!("https://example.test/playlist-{i}")})).collect::<Vec<_>>()},
                "songs": {"data": {"dailySongs": (0..33).map(|i| json!({"id":i+100,"name":format!("Song {i}"),"al":{"picUrl":format!("https://example.test/song-{i}")}})).collect::<Vec<_>>()}}
            }}),
        ),
        (
            "home-albums",
            json!({"albums": (0..24).map(|i| json!({"id":i+200,"name":format!("Album {i}"),"picUrl":format!("https://example.test/album-{i}")})).collect::<Vec<_>>()}),
        ),
        (
            "home-charts",
            json!({"list": (0..6).map(|i| json!({"id":i+300,"name":format!("Chart {i}"),"coverImgUrl":format!("https://example.test/chart-{i}"),"tracks":[{"first":"Track"}]})).collect::<Vec<_>>()}),
        ),
    ] {
        controller.apply(
            &app,
            Event {
                name: name.into(),
                data,
                token: 0,
                epoch: 0,
                error: None,
            },
        );
    }
    // Feed actual viewport requests through View and the bounded decoded cache.
    // Image allocation uses the real requested dimensions, not a 1px test stub.
    let pump = |view: &mut View, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>| {
        for _ in 0..3 {
            settle(&app);
            view.request_covers(pending.borrow_mut().drain());
            while let Ok(command) = rx.try_recv() {
                if command.name != "covers" {
                    continue;
                }
                let images = command
                    .data
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|request| {
                        let size = request["size"].as_u64().unwrap() as u32;
                        DecodedImage {
                            palette: None,
                            target: ImageTarget::Cover(
                                request["cover"].as_str().unwrap().into(),
                                size,
                            ),
                            epoch: 0,
                            pixels: slint::SharedPixelBuffer::new(size, size),
                        }
                    })
                    .collect::<Vec<_>>();
                view.apply_images(&app, images.into_iter());
            }
        }
    };
    pump(&mut controller, &mut rx);
    assert_eq!(
        home_state
            .get_recommend_playlists()
            .row_data(0)
            .unwrap()
            .cover
            .size()
            .width,
        512
    );
    assert_eq!(
        home_state
            .get_daily_songs()
            .row_data(0)
            .unwrap()
            .cover
            .size()
            .width,
        384
    );
    assert_eq!(
        home_state
            .get_home_new_albums()
            .row_data(23)
            .unwrap()
            .cover
            .size()
            .width,
        0,
        "offscreen covers must remain lazy"
    );
    let heading = ElementHandle::find_by_element_id(&app, "HomeView::home-page-title")
        .next()
        .unwrap();
    let top = heading.absolute_position().y;
    let mut first_requests = 0;
    for pass in 0..2 {
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
            (1800.0, "", ""),
            (0.0, "", ""),
        ] {
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(1300.0, 400.0),
                delta_x: 0.0,
                delta_y: top - offset - heading.absolute_position().y,
            });
            pump(&mut controller, &mut rx);
            for button in [next, prev].into_iter().filter(|id| !id.is_empty()) {
                for _ in 0..9 {
                    ElementHandle::find_by_element_id(&app, button)
                        .next()
                        .unwrap()
                        .invoke_accessible_default_action();
                    pump(&mut controller, &mut rx);
                }
            }
        }
        assert_eq!(
            controller.cover_probe.evictions, 0,
            "home covers evicted while browsing"
        );
        for row in home_state.get_recommend_playlists().iter() {
            assert_eq!(row.cover.size().width, 512);
        }
        for row in home_state
            .get_daily_songs()
            .iter()
            .chain(home_state.get_home_new_albums().iter())
        {
            assert_eq!(row.cover.size().width, 384);
        }
        for row in home_state.get_home_charts().iter() {
            assert_eq!(row.cover.size().width, 256);
        }
        let requests = controller.cover_probe.requests.values().sum::<usize>();
        if pass == 0 {
            first_requests = requests;
        } else {
            assert_eq!(
                requests, first_requests,
                "returning to visited cards reloaded covers"
            );
        }
        assert!(controller.cover_cache_usage().1 <= 64 * 1024 * 1024);
    }
    // A list thumbnail must permit the existing fullscreen request to upgrade;
    // recycled home cards and a late smaller delivery must never downgrade it.
    let song = home_state.get_daily_songs().row_data(0).unwrap();
    controller.request_covers(std::iter::once((song.id, 1600)));
    pump(&mut controller, &mut rx);
    let full = home_state.get_daily_songs().row_data(0).unwrap().cover;
    assert_eq!(full.size().width, 1600);
    controller.apply_images(
        &app,
        std::iter::once(DecodedImage {
            palette: None,
            target: ImageTarget::Cover("https://example.test/song-0".into(), 384),
            epoch: 0,
            pixels: slint::SharedPixelBuffer::new(384, 384),
        }),
    );
    controller.request_covers(std::iter::once((song.id, 384)));
    assert!(rx.try_recv().is_err());
    assert_eq!(
        home_state.get_daily_songs().row_data(0).unwrap().cover,
        full
    );
    assert_eq!(controller.cover_probe.evictions, 0);
}
