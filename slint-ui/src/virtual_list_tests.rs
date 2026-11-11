use crate::{AppWindow, NavPlaylistItem, QueueSongItem, TrackItem};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{
    platform::{PointerEventButton, WindowEvent},
    ComponentHandle, LogicalPosition, ModelRc, VecModel,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

fn settle(app: &AppWindow) {
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(600));
    let _ = app.root_element().size();
}
fn rows(app: &AppWindow, name: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(app, name).collect()
}
fn scroll(app: &AppWindow, x: f32, y: f32, delta: f32) {
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(x, y),
        delta_x: 0.0,
        delta_y: delta,
    });
    settle(app);
}

#[test]
fn large_lists_only_construct_visible_rows_and_request_their_covers() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let account_state = app.global::<crate::AccountState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let detail_state = app.global::<crate::DetailState>();
    let library_state = app.global::<crate::LibraryState>();
    let queue_state = app.global::<crate::QueueState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let search_state = app.global::<crate::SearchState>();
    let player_state = app.global::<crate::PlayerState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    account_state.set_is_logged_in(false);
    let requested = Rc::new(RefCell::new(HashMap::<i32, i32>::new()));
    let sink = requested.clone();
    runtime_state.on_request_cover(move |id, size| {
        let mut requests = sink.borrow_mut();
        let previous = requests.entry(id).or_default();
        *previous = (*previous).max(size);
    });
    let tracks = Rc::new(VecModel::from(
        (0..10_000)
            .map(|id| TrackItem {
                id,
                name: format!("song-{id}").into(),
                index_str: id.to_string().into(),
                has_cover: true,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ));
    playlist_state.set_playlist_tracks(ModelRc::from(tracks.clone()));
    detail_state.set_detail_tracks(ModelRc::from(tracks.clone()));
    library_state.set_local_tracks(ModelRc::from(tracks.clone()));
    queue_state.set_queue_songs(
        Rc::new(VecModel::from(
            (0..10_000)
                .map(|id| QueueSongItem {
                    id,
                    name: format!("song-{id}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    for route in ["playlist", "album", "favorites", "local-music", "search"] {
        navigation_state.set_current_route(route.into());
        search_state.set_search_show_results(true);
        settle(&app);
        let id = if route == "search" {
            "SearchResultSongRow::root"
        } else {
            "SongTrackRow::root"
        };
        let visible = rows(&app, id);
        assert!(
            !visible.is_empty() && visible.len() <= 18,
            "{route}: {} constructed rows",
            visible.len()
        );
        assert!(
            rows(&app, "PlaylistQueueRow::root").is_empty(),
            "closed queue created rows"
        );
        assert!(
            rows(&app, "LyricPanelView::root").is_empty(),
            "hidden fullscreen player created lyrics"
        );
        assert!(
            requested.borrow().len() <= 25,
            "{route}: loaded offscreen covers: {}",
            requested.borrow().len()
        );
        assert!(
            requested.borrow().values().all(|size| *size == 128),
            "{route}: small rows requested full-size artwork"
        );
        requested.borrow_mut().clear();
        scroll(&app, 700.0, 500.0, -40_000.0);
        let visible = rows(&app, id);
        assert!(
            !visible.is_empty() && visible.len() <= 18,
            "{route}: scrolling instantiated {} rows",
            visible.len()
        );
        let target = visible
            .iter()
            .find(|r| r.absolute_position().y > 50.0 && r.absolute_position().y < 700.0)
            .unwrap();
        println!("{route}: {} visible rows for 10000 songs", visible.len());
        let position = LogicalPosition::new(
            target.absolute_position().x + 120.0,
            target.absolute_position().y + target.size().height / 2.0,
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
        let played = player_state.get_current_song_id();
        assert!(
            played > 100,
            "{route}: scrolled row played stale index {played}"
        );
        assert!(
            (1..=25).contains(&requested.borrow().len()),
            "{route}: scrolling did not request only visible covers"
        );
        requested.borrow_mut().clear();
        // Scroll to the end, including partial last rows, then return to the top.
        scroll(&app, 700.0, 500.0, -1_000_000.0);
        assert!(rows(&app, id).len() <= 18);
        scroll(&app, 700.0, 500.0, 1_000_000.0);
        requested.borrow_mut().clear();
    }
    // Removing almost the whole model while scrolled down clamps the page offset.
    scroll(&app, 700.0, 500.0, -40_000.0);
    tracks.set_vec(vec![TrackItem {
        id: 42,
        name: "replacement".into(),
        ..Default::default()
    }]);
    settle(&app);
    let visible = rows(&app, "SearchResultSongRow::root");
    assert_eq!(visible.len(), 1);
    visible[0].mock_single_click(PointerEventButton::Left);
    assert_eq!(player_state.get_current_song_id(), 42);

    queue_state.set_is_playlist_open(true);
    settle(&app);
    let visible = rows(&app, "PlaylistQueueRow::root");
    assert!(
        !visible.is_empty() && visible.len() < 20,
        "queue created {} rows",
        visible.len()
    );
    scroll(&app, 830.0, 600.0, -40_000.0);
    assert!(rows(&app, "PlaylistQueueRow::root").len() < 20);
    queue_state.set_is_playlist_open(false);
    settle(&app);
    assert!(rows(&app, "PlaylistQueueRow::root").is_empty());

    playlist_state.set_created_playlists(
        Rc::new(VecModel::from(
            (0..10_000)
                .map(|id| NavPlaylistItem {
                    id: id + 20_000,
                    name: format!("list-{id}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    account_state.set_is_logged_in(true);
    settle(&app);
    assert!(rows(&app, "SidebarPlaylistRow::root").len() < 20);
    scroll(&app, 150.0, 500.0, -40_000.0);
    assert!(rows(&app, "SidebarPlaylistRow::root").len() < 22);
    song_menu_state.set_is_add_to_playlist_open(true);
    settle(&app);
    let covers = rows(&app, "PlaylistPickerList::pl-col");
    assert_eq!(covers.len(), 1);
    assert!(
        covers[0]
            .query_descendants()
            .match_id("RoundedCover::root")
            .find_all()
            .len()
            < 10
    );
}

#[test]
fn scrolling_reuses_row_data_and_only_rebinds_newly_visible_songs() {
    use slint::{Model, ModelTracker};
    struct CountingTracks {
        rows: VecModel<TrackItem>,
        reads: RefCell<Vec<usize>>,
    }
    impl Model for CountingTracks {
        type Data = TrackItem;
        fn row_count(&self) -> usize {
            self.rows.row_count()
        }
        fn row_data(&self, row: usize) -> Option<TrackItem> {
            self.reads.borrow_mut().push(row);
            self.rows.row_data(row)
        }
        fn model_tracker(&self) -> &dyn ModelTracker {
            self.rows.model_tracker()
        }
    }
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let playlist_state = app.global::<crate::PlaylistState>();
    let detail_state = app.global::<crate::DetailState>();
    let library_state = app.global::<crate::LibraryState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let search_state = app.global::<crate::SearchState>();
    let player_state = app.global::<crate::PlayerState>();
    let tracks = Rc::new(CountingTracks {
        rows: VecModel::from(
            (0..10_000)
                .map(|id| TrackItem {
                    id,
                    name: format!("song-{id}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ),
        reads: RefCell::new(Vec::new()),
    });
    playlist_state.set_playlist_tracks(tracks.clone().into());
    detail_state.set_detail_tracks(tracks.clone().into());
    library_state.set_local_tracks(tracks.clone().into());
    for route in ["playlist", "album", "favorites", "local-music", "search"] {
        navigation_state.set_current_route(route.into());
        search_state.set_search_show_results(true);
        settle(&app);
        scroll(&app, 700.0, 500.0, -4_000.0);
        for delta in [-1.0, -62.0, 62.0, -62.0, 1.0] {
            tracks.reads.borrow_mut().clear();
            scroll(&app, 700.0, 500.0, delta);
            let reads = tracks.reads.borrow();
            assert!(
                reads.len() <= 3,
                "{route}: a {delta}px scroll rebound the visible list: {reads:?}"
            );
        }
        // Row data updates remain reactive even though unchanged indices are stable.
        let sample = rows(
            &app,
            if route == "search" {
                "SearchResultSongRow::root"
            } else {
                "SongTrackRow::root"
            },
        );
        let target = sample
            .iter()
            .find(|row| row.absolute_position().y > 150.0 && row.absolute_position().y < 600.0)
            .unwrap();
        let position = LogicalPosition::new(
            target.absolute_position().x + 120.0,
            target.absolute_position().y + target.size().height / 2.0,
        );
        for pressed in [true, false] {
            app.window().dispatch_event(if pressed {
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
        let id = player_state.get_current_song_id();
        assert!(id > 10);
        tracks.rows.set_row_data(
            id as usize,
            TrackItem {
                id,
                name: "updated title".into(),
                ..Default::default()
            },
        );
        settle(&app);
        for pressed in [true, false] {
            app.window().dispatch_event(if pressed {
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
        assert_eq!(player_state.get_current_song_title(), "updated title");
    }
}
