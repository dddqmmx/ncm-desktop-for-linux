use super::{
    backend::{Command, Event},
    data::Preferences,
    images::{DecodedImage, ImageTarget},
    view::{self, View},
};
use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::{json, Value};
use slint::{
    platform::{PointerEventButton, WindowEvent},
    ComponentHandle, LogicalPosition, Model,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn response(name: &str, token: u64, data: Value) -> Event {
    Event {
        name: name.into(),
        token,
        epoch: 0,
        data,
        error: None,
    }
}
fn playlist(id: &str, count: usize) -> Value {
    json!({"kind":"playlist", "id":id, "body":{"playlist":{
        "name":format!("Playlist {id}"), "description":format!("Description {id}"),
        "coverImgUrl":format!("https://example.test/{id}.jpg"), "trackCount":count,
        "creator":{"nickname":format!("Creator {id}")},
        "tracks":(0..count).map(|n| json!({"id":1000+n,"name":format!("Song {n}"),"dt":180000})).collect::<Vec<_>>()
    }}})
}
fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}
fn advance(app: &AppWindow) {
    for _ in 0..30 {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(16));
    }
}
fn next_page(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>) -> Command {
    loop {
        let command = rx.try_recv().expect("click did not request a page");
        if command.name == "page" {
            return command;
        }
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

#[test]
fn playlist_search_preserves_full_song_ids_and_playback_queue() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    let mut page = playlist("9000000001", 3);
    page["body"]["playlist"]["tracks"] = json!([
        {"id":4500000001_u64,"name":"First","dt":180000},
        {"id":4500000002_u64,"name":"Target","dt":180000,"ar":[{"name":"BEYOND"}]},
        {"id":4500000003_u64,"name":"Last","dt":180000}
    ]);
    view.apply(&app, response("page", 0, page));
    let playlist = app.global::<crate::PlaylistState>();
    let detail = app.global::<crate::DetailState>();
    let all = playlist.get_playlist_tracks();
    detail.set_detail_search_query("beyond".into());
    let matches = detail.invoke_filter_tracks(all.clone(), detail.get_detail_search_query());
    assert_eq!(matches.row_count(), 1);
    view.action(&app, "play", matches.row_data(0).unwrap().id, "");
    let play = std::iter::from_fn(|| rx.try_recv().ok())
        .find(|command| command.name == "play")
        .unwrap();
    assert_eq!(play.data["song"]["id"], "4500000002");
    assert_eq!(
        app.global::<crate::QueueState>()
            .get_queue_songs()
            .row_count(),
        3
    );
    assert_eq!(playlist.get_playlist_tracks(), all);
}

#[test]
fn playlist_switches_immediately_and_isolates_delayed_metadata_tracks_and_errors() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let playlist_state = app.global::<crate::PlaylistState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let view = Rc::new(RefCell::new(View::new(tx, Preferences::default())));
    let weak = app.as_weak();
    let controller = view.clone();
    playlist_state.on_open_playlist(move |id| {
        controller
            .borrow_mut()
            .action(&weak.unwrap(), "playlist", id, "");
        true
    });
    let weak = app.as_weak();
    let controller = view.clone();
    runtime_state.on_runtime_action(move |name, id, arg| {
        controller
            .borrow_mut()
            .action(&weak.unwrap(), &name, id, &arg)
    });
    view.borrow_mut().apply(&app, response("account", 0, json!({"body":{
        "profile":{"userId":42}, "likes":[], "playlists":(1..=3).map(|id|json!({
            "id":id,"name":format!("Preview {id}"),"coverImgUrl":format!("https://example.test/{id}.jpg"),"creator":{"userId":42}
        })).collect::<Vec<_>>()
    }})));
    let ids: Vec<_> = playlist_state
        .get_created_playlists()
        .iter()
        .map(|p| p.id)
        .collect();
    app.show().unwrap();
    advance(&app);
    let click = |index| {
        ElementHandle::find_by_element_id(&app, "SidebarPlaylistRow::ta")
            .nth(index)
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
    };
    click(0);
    let first = next_page(&mut rx);
    assert_eq!(navigation_state.get_current_route(), "playlist");
    assert_eq!(playlist_state.get_active_playlist_id(), ids[0]);
    assert_eq!(playlist_state.get_playlist_title(), "Preview 1");
    assert!(playlist_state.get_playlist_loading());
    assert!(playlist_state.get_playlist_metadata_loading());
    advance(&app);
    element(&app, "SongList::loading-rows");
    assert!(
        ElementHandle::find_by_element_id(&app, "SongList::empty-state")
            .next()
            .is_none()
    );
    snapshot(&app, "playlist-loading-header");

    let first_page = playlist("1", 80);
    view.borrow_mut().apply(
        &app,
        response(
            "playlist-metadata",
            first.token,
            json!({"id":"1","playlist":first_page["body"]["playlist"]}),
        ),
    );
    assert_eq!(playlist_state.get_playlist_title(), "Playlist 1");
    assert!(!playlist_state.get_playlist_metadata_loading());
    assert!(playlist_state.get_playlist_loading());
    assert!(!playlist_state.get_playlist_can_manage());
    assert_eq!(playlist_state.get_playlist_tracks().row_count(), 0);
    advance(&app);
    snapshot(&app, "playlist-loading-tracks");
    view.borrow_mut()
        .apply(&app, response("page", first.token, first_page));
    assert!(!playlist_state.get_playlist_loading());
    assert!(playlist_state.get_playlist_can_manage());
    assert_eq!(playlist_state.get_playlist_tracks().row_count(), 80);
    advance(&app);
    let header_cover = element(&app, "MediaDetailHeader::detail-cover-wrapper");
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(1000.0, 600.0),
        delta_x: 0.0,
        delta_y: -400.0,
    });
    advance(&app);
    assert!(header_cover.absolute_position().y < 0.0);

    click(1);
    let second = next_page(&mut rx);
    assert_eq!(playlist_state.get_active_playlist_id(), ids[1]);
    assert_eq!(playlist_state.get_playlist_title(), "Preview 2");
    assert!(playlist_state.get_playlist_creator().is_empty());
    assert!(playlist_state.get_playlist_description().is_empty());
    assert_eq!(playlist_state.get_playlist_tracks().row_count(), 0);
    advance(&app);
    assert!(
        (element(&app, "MediaDetailHeader::detail-cover-wrapper")
            .absolute_position()
            .y
            - 40.0)
            .abs()
            < 0.5
    );
    click(2);
    let third = next_page(&mut rx);
    view.borrow_mut().apply_images(
        &app,
        [DecodedImage {
            palette: None,
            target: ImageTarget::Cover("https://example.test/1.jpg".into(), 800),
            epoch: 0,
            pixels: slint::SharedPixelBuffer::clone_from_slice(&[255u8, 0, 0, 255], 1, 1),
        }]
        .into_iter(),
    );
    assert_eq!(
        playlist_state.get_playlist_cover().size().width,
        0,
        "old cover reached new header"
    );
    view.borrow_mut().apply(
        &app,
        response(
            "playlist-metadata",
            second.token,
            json!({"id":"2","playlist":playlist("2", 1)["body"]["playlist"]}),
        ),
    );
    view.borrow_mut()
        .apply(&app, response("page", second.token, playlist("2", 1)));
    view.borrow_mut().apply(
        &app,
        Event {
            error: Some("stale error".into()),
            ..response("page", second.token, Value::Null)
        },
    );
    assert_eq!(playlist_state.get_active_playlist_id(), ids[2]);
    assert_eq!(playlist_state.get_playlist_title(), "Preview 3");
    assert!(playlist_state.get_playlist_loading());
    assert!(playlist_state.get_playlist_load_error().is_empty());

    view.borrow_mut().apply(
        &app,
        Event {
            error: Some("network unavailable".into()),
            ..response("page", third.token, Value::Null)
        },
    );
    assert!(!playlist_state.get_playlist_loading());
    assert!(!playlist_state.get_playlist_metadata_loading());
    advance(&app);
    element(&app, "SongList::load-error-panel");
    snapshot(&app, "playlist-loading-error");
    element(&app, "SongList::retry-button").mock_single_click(PointerEventButton::Left);
    let retry = next_page(&mut rx);
    assert_eq!(retry.data["id"], "3");
    assert!(playlist_state.get_playlist_loading());
    assert!(playlist_state.get_playlist_load_error().is_empty());
    view.borrow_mut()
        .apply(&app, response("page", retry.token, playlist("3", 0)));
    advance(&app);
    assert!(!playlist_state.get_playlist_loading());
    element(&app, "SongList::empty-state");
    assert!(
        ElementHandle::find_by_element_id(&app, "SongList::loading-rows")
            .next()
            .is_none()
    );

    click(0);
    let leaving = next_page(&mut rx);
    navigation_state.set_current_route("home".into());
    advance(&app);
    view.borrow_mut().apply(
        &app,
        response(
            "playlist-metadata",
            leaving.token,
            json!({"id":"1","playlist":playlist("1", 2)["body"]["playlist"]}),
        ),
    );
    view.borrow_mut()
        .apply(&app, response("page", leaving.token, playlist("1", 2)));
    assert_eq!(navigation_state.get_current_route(), "home");
    assert!(!playlist_state.get_playlist_loading());
    assert_eq!(playlist_state.get_playlist_tracks().row_count(), 0);
    assert_eq!(playlist_state.get_playlist_title(), "Preview 1");
}
