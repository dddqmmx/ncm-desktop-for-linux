use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{platform::WindowEvent, ComponentHandle, LogicalPosition};
use std::time::Duration;

fn element(app: &impl ComponentHandle, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("Missing {id}"))
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

fn move_pointer(app: &AppWindow, x: f32, y: f32) {
    app.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(x, y),
    });
    let _ = app.root_element().size();
}

#[test]
fn local_cover_placeholders_keep_rounded_corners_in_all_player_views() {
    use slint::{Model, VecModel};
    use std::rc::Rc;
    init();
    let app = AppWindow::new().unwrap();
    let player = app.global::<crate::PlayerState>();
    let view = app.global::<crate::PlayerViewState>();
    let queue = app.global::<crate::QueueState>();
    let track = app
        .global::<crate::LibraryState>()
        .get_local_tracks()
        .row_data(0)
        .unwrap();
    player.set_current_song_id(track.id);
    player.set_current_song_title(track.name.clone());
    player.set_current_song_is_local(true);
    player.set_current_song_has_cover(false);
    player.set_current_song_cover(Default::default());
    player.set_is_playing(false);
    queue.set_queue_songs(
        Rc::new(VecModel::from(vec![crate::QueueSongItem {
            id: track.id,
            name: track.name.clone(),
            artist: track.artist.clone(),
            is_local: true,
            has_cover: false,
            ..Default::default()
        }]))
        .into(),
    );
    app.global::<crate::DetailState>()
        .set_detail_tracks(Rc::new(VecModel::from(vec![track])).into());
    app.global::<crate::NavigationState>()
        .set_current_route("local-music".into());
    app.show().unwrap();
    let settle = || {
        for _ in 0..40 {
            let _ = app.root_element().size();
            mock_elapsed_time(Duration::from_millis(25));
        }
    };
    let check = |ids: &[&str]| {
        let pixels = app.window().take_snapshot().unwrap();
        let scale = pixels.width() as f32 / app.root_element().size().width;
        for id in ids {
            let cover = element(&app, id);
            let p = cover.absolute_position();
            let size = cover.size();
            let pixel = |x: f32, y: f32| {
                let x = ((p.x + x) * scale).ceil() as usize;
                let y = ((p.y + y) * scale).ceil() as usize;
                let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
                [pixel.r, pixel.g, pixel.b]
            };
            // Sample the outermost pixels: at a 6px radius the next pixel
            // already belongs to the antialiased arc, especially on active rows.
            for y in [0.0, size.height - 1.0] {
                for (corner, edge) in [
                    (0.0, 24.0_f32.min(size.width / 2.0)),
                    (
                        size.width - 1.0,
                        size.width - 24.0_f32.min(size.width / 2.0),
                    ),
                ] {
                    let corner = pixel(corner, y);
                    let edge = pixel(edge, y);
                    let difference = corner
                        .into_iter()
                        .zip(edge)
                        .map(|(a, b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                    assert!(difference > 5, "{id}: square corner painted over the rounded background: {corner:?} / {edge:?}");
                }
            }
        }
    };
    for dark in [false, true] {
        app.global::<crate::Theme>().set_is_dark(dark);
        view.set_fullscreen_is_dark(dark);
        let background = if dark {
            slint::Color::from_rgb_u8(16, 18, 23)
        } else {
            slint::Color::from_rgb_u8(238, 241, 246)
        };
        view.set_fullscreen_bg_primary(background);
        view.set_fullscreen_bg_secondary(background);
        view.set_is_fullscreen(false);
        queue.set_is_playlist_open(false);
        settle();
        check(&[
            "MediaDetailHeader::detail-cover-wrapper",
            "SongTrackRow::mini-cover",
            "PlayerTrackSummary::player-cover",
        ]);
        crate::responsive_tests::snapshot(&app, &format!("local-covers-{dark}"));
        queue.set_is_playlist_open(true);
        settle();
        check(&["PlaylistQueueRow::item-cover"]);
        queue.set_is_playlist_open(false);
        app.global::<crate::NavigationState>()
            .set_current_route("favorites".into());
        settle();
        check(&["SongTrackRow::mini-cover"]);
        app.global::<crate::NavigationState>()
            .set_current_route("local-music".into());
        view.set_is_fullscreen(true);
        settle();
        crate::responsive_tests::snapshot(&app, &format!("local-cover-fullscreen-{dark}"));
        let fallback = app.window().take_snapshot().unwrap();
        let cover = element(&app, "FullscreenVisualPanel::fullscreen-cover-scaled");
        let p = cover.absolute_position();
        let size = cover.size();
        // The large fullscreen shadow can match the cover's fill. Compare to
        // the same rounded surface with an empty image instead of its brightness.
        player.set_current_song_has_cover(true);
        settle();
        let empty = app.window().take_snapshot().unwrap();
        let scale = fallback.width() as f32 / app.root_element().size().width;
        for corner_y in [0.0, size.height - 16.0] {
            for corner_x in [0.0, size.width - 16.0] {
                for dy in 0..16 {
                    for dx in 0..16 {
                        let x = ((p.x + corner_x + dx as f32) * scale).ceil() as usize;
                        let y = ((p.y + corner_y + dy as f32) * scale).ceil() as usize;
                        let index = y * fallback.width() as usize + x;
                        assert_eq!(
                            fallback.as_slice()[index],
                            empty.as_slice()[index],
                            "fullscreen placeholder repaints corner at ({x}, {y}), dark={dark}"
                        );
                    }
                }
            }
        }
        player.set_current_song_has_cover(false);
    }
}

#[test]
fn user_bubble_hover_keeps_its_anchor_and_does_not_oscillate() {
    init();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let account_state = app.global::<crate::AccountState>();
    let bubble = element(&app, "SideNavBar::profile-bubble");
    let trigger = element(&app, "SidebarProfileBubble::bubble-trigger");
    for logged_in in [true, false] {
        account_state.set_is_logged_in(logged_in);
        for pointer_y in [940.0, 969.0, 916.0] {
            move_pointer(&app, 400.0, 600.0);
            mock_elapsed_time(Duration::from_millis(600));
            assert!((bubble.size().height - 56.0).abs() < 0.5);
            let bottom = bubble.absolute_position().y + bubble.size().height;
            let trigger_y = trigger.absolute_position().y;
            let mut last_height = 56.0;
            let mut last_y = bubble.absolute_position().y;
            for frame in 0..90 {
                // Real backends hit-test on redraw. Re-send the stationary pointer
                // every frame so changing geometry cannot hide hover regressions.
                move_pointer(&app, 120.0, pointer_y);
                mock_elapsed_time(Duration::from_millis(16));
                let y = bubble.absolute_position().y;
                let height = bubble.size().height;
                let actual_bottom = y + height;
                assert!((bottom - 5.1..=bottom + 0.1).contains(&actual_bottom),
                    "frame {frame}: moving bottom {actual_bottom} escaped its 5px lift range, pointer={pointer_y}");
                assert!(
                    height + 0.2 >= last_height,
                    "hover collapsed at frame {frame}, pointer={pointer_y}"
                );
                assert!(
                    y <= last_y + 0.2,
                    "hover reversed at frame {frame}, pointer={pointer_y}"
                );
                assert!(
                    (trigger_y - 5.1..=trigger_y + 0.1).contains(&trigger.absolute_position().y)
                );
                last_height = height;
                last_y = y;
            }
            assert!((last_height - if logged_in { 200.4 } else { 109.8 }).abs() < 0.5);
            if logged_in {
                let gt: serde_json::Value =
                    serde_json::from_str(include_str!("../tests/sidebar-hover-ground-truth.json"))
                        .unwrap();
                for (key, id) in [
                    ("bubble", "SideNavBar::profile-bubble"),
                    ("trigger", "SidebarProfileBubble::bubble-trigger"),
                    ("menu", "SidebarProfileBubble::bubble-menu"),
                    ("content", "SideNavBar::sidebar-content"),
                ] {
                    let item = element(&app, id);
                    let p = item.absolute_position();
                    let size = item.size();
                    for (axis, actual) in [
                        ("x", p.x),
                        ("y", p.y),
                        ("width", size.width),
                        ("height", size.height),
                    ] {
                        let expected = gt["open"][key]["rect"][axis].as_f64().unwrap() as f32;
                        assert!(
                            (actual - expected).abs() < 0.5,
                            "hover {key}.{axis}: Slint {actual} != Electron {expected}"
                        );
                    }
                }
            }
            // Crossing menu children and their gaps must keep the ancestor hovered.
            let settings = element(&app, "SidebarProfileMenu::settings-menu-item");
            let p = settings.absolute_position();
            for _ in 0..30 {
                move_pointer(&app, p.x + 40.0, p.y + 15.0);
                mock_elapsed_time(Duration::from_millis(16));
                assert!((bubble.size().height - last_height).abs() < 0.5);
            }
            if logged_in {
                trigger.mock_single_click(slint::platform::PointerEventButton::Left);
            }
            move_pointer(&app, 400.0, 600.0);
            mock_elapsed_time(Duration::from_millis(500));
            assert!((bubble.size().height - 56.0).abs() < 0.5);
        }
    }
    if std::env::var_os("NCM_SLINT_SCREENSHOT_DIR").is_some() {
        capture_profile_surfaces(&app);
    }
}

fn capture_profile_surfaces(app: &AppWindow) {
    use crate::responsive_tests::snapshot;

    app.global::<crate::AccountState>().set_is_logged_in(true);
    for (dark, theme) in [(false, "light"), (true, "dark")] {
        app.global::<crate::Theme>().set_is_dark(dark);
        app.window().set_size(slint::PhysicalSize::new(1435, 1000));
        app.global::<crate::NavigationState>()
            .set_window_width(1435.0);
        move_pointer(app, 400.0, 600.0);
        settle_menu(app);
        snapshot(app, &format!("profile-{theme}-closed"));

        move_pointer(app, 120.0, 940.0);
        settle_menu(app);
        snapshot(app, &format!("profile-{theme}-open"));
        let menu = element(app, "SidebarProfileBubble::bubble-menu");
        let position = menu.absolute_position();
        move_pointer(app, position.x + menu.size().width / 2.0, position.y + 20.0);
        settle_menu(app);
        snapshot(app, &format!("profile-{theme}-item-hover"));

        app.window().set_size(slint::PhysicalSize::new(900, 1000));
        app.global::<crate::NavigationState>()
            .set_window_width(900.0);
        move_pointer(app, 400.0, 600.0);
        settle_menu(app);
        snapshot(app, &format!("profile-{theme}-compact"));
    }
}

fn settle_menu(app: &AppWindow) {
    let _ = app.root_element().size();
    mock_elapsed_time(Duration::from_millis(600));
    let _ = app.root_element().size();
}

fn assert_menu_contained(app: &AppWindow) {
    let overlay = element(app, "SongContextMenuOverlay::root");
    let card = element(app, "SongContextMenuOverlay::context-menu-card");
    let p = card.absolute_position();
    let s = card.size();
    let bounds = overlay.size();
    assert!(
        p.x >= 7.9
            && p.y >= 7.9
            && p.x + s.width <= bounds.width - 7.9
            && p.y + s.height <= bounds.height - 7.9,
        "menu at {p:?}, size {s:?} escaped window {bounds:?}"
    );
    for id in [
        "SongContextMenuOverlay::context-menu-item-next",
        "SongMenuAccountActions::context-menu-item-add",
        "SongMenuAccountActions::context-menu-item-download",
        "SongMenuRemoveAction::context-menu-item-remove",
        "SongMenuAccountActions::submenu-scroll",
    ] {
        for child in ElementHandle::find_by_element_id(app, id) {
            let cp = child.absolute_position();
            let cs = child.size();
            assert!(
                cp.x >= p.x + 6.9
                    && cp.y >= p.y + 6.9
                    && cp.x + cs.width <= p.x + s.width - 6.9
                    && cp.y + cs.height <= p.y + s.height - 6.9,
                "{id} at {cp:?}, size {cs:?} escaped menu at {p:?}, size {s:?}"
            );
        }
    }
}

#[test]
fn song_context_menu_aligns_icons_labels_and_playlist_rows() {
    use crate::responsive_tests::{assert_disjoint, assert_inside, snapshot};
    use slint::{Model, PhysicalSize, VecModel};
    use std::rc::Rc;

    init();
    let app = AppWindow::new().unwrap();
    app.global::<crate::NavigationState>()
        .set_current_route("playlist".into());
    app.global::<crate::AccountState>().set_is_logged_in(true);
    app.global::<crate::RuntimeState>()
        .set_runtime_enabled(true);
    let playlists = app.global::<crate::PlaylistState>();
    let mut items: Vec<_> = playlists.get_created_playlists().iter().collect();
    items[0].name = "很长的歌单名称 Long playlist title 应当省略并保留数量".into();
    items[0].track_count = 12345;
    playlists.set_created_playlists(Rc::new(VecModel::from(items)).into());
    app.show().unwrap();
    let menu = app.global::<crate::SongMenuState>();
    let center_y = |item: &ElementHandle| item.absolute_position().y + item.size().height / 2.0;

    for dark in [false, true] {
        app.global::<crate::Theme>().set_is_dark(dark);
        for width in [1435, 620] {
            app.window().set_size(PhysicalSize::new(width, 1000));
            settle_menu(&app);
            menu.set_open_menu_song_id(1);
            menu.set_open_menu_show_remove(true);
            menu.set_open_menu_anchor_right(width as f32 - 20.0);
            menu.set_open_menu_anchor_top(200.0);
            menu.set_open_menu_anchor_bottom(228.0);
            for expanded in [false, true] {
                menu.set_open_menu_submenu(expanded);
                settle_menu(&app);
                let mut label_x = None;
                let rows: Vec<_> =
                    ElementHandle::find_by_element_id(&app, "MenuItem::root").collect();
                assert_eq!(rows.len(), 4);
                for row in rows {
                    let label = row
                        .query_descendants()
                        .match_id("MenuItem::menu-label")
                        .find_first()
                        .unwrap();
                    let icon = row
                        .query_descendants()
                        .match_id("MenuItem::leading-icon")
                        .find_first()
                        .unwrap();
                    let x = label.absolute_position().x;
                    let reference = *label_x.get_or_insert(x);
                    assert!(
                        (x - reference).abs() < 0.5,
                        "menu labels do not share a column"
                    );
                    assert_disjoint(&icon, &label, "menu icon and label");
                    for child in [icon, label].into_iter().chain(
                        row.query_descendants()
                            .match_id("MenuItem::arrow")
                            .find_all(),
                    ) {
                        assert_inside(&row, &child, "menu content");
                        assert!(
                            (center_y(&child) - center_y(&row)).abs() < 0.5,
                            "menu content is not vertically centered: {:?}",
                            child.id()
                        );
                    }
                }
                if expanded {
                    let covers = ElementHandle::find_by_element_id(
                        &app,
                        "SongMenuPlaylistList::playlist-cover",
                    );
                    let names = ElementHandle::find_by_element_id(
                        &app,
                        "SongMenuPlaylistList::playlist-name",
                    );
                    let counts = ElementHandle::find_by_element_id(
                        &app,
                        "SongMenuPlaylistList::playlist-count",
                    );
                    let mut checked = 0;
                    for ((cover, name), count) in covers.zip(names).zip(counts) {
                        assert!((center_y(&cover) - center_y(&name)).abs() < 0.5);
                        assert!((center_y(&cover) - center_y(&count)).abs() < 0.5);
                        assert_disjoint(&cover, &name, "playlist cover and title");
                        assert_disjoint(&name, &count, "playlist title and count");
                        checked += 1;
                    }
                    assert!(checked > 0);
                }
                snapshot(&app, &format!("song-menu-{dark}-{width}-{expanded}"));
            }
        }
    }
}

#[test]
fn song_context_menu_stays_inside_window_as_content_and_size_change() {
    use crate::NavPlaylistItem;
    use slint::{PhysicalSize, VecModel};
    use std::rc::Rc;

    init();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let navigation_state = app.global::<crate::NavigationState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    let account_state = app.global::<crate::AccountState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    navigation_state.set_current_route("playlist".into());
    song_menu_state.set_open_menu_song_id(1);

    for (width, height) in [(1435, 1000), (1340, 960), (620, 720), (320, 260)] {
        app.window().set_size(PhysicalSize::new(width, height));
        for (logged_in, runtime, remove, count) in [
            (true, true, true, 30),
            (true, true, false, 2),
            (true, false, false, 30),
            (true, false, true, 0),
            (false, true, false, 30),
        ] {
            account_state.set_is_logged_in(logged_in);
            runtime_state.set_runtime_enabled(runtime);
            song_menu_state.set_open_menu_show_remove(remove);
            playlist_state.set_created_playlists(
                Rc::new(VecModel::from(
                    (0..count)
                        .map(|id| NavPlaylistItem {
                            id,
                            name: format!("playlist-{id}").into(),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                ))
                .into(),
            );
            // Include an old anchor beyond the window after shrinking it.
            for (right, top) in [
                (width as f32 - 8.0, height as f32 - 40.0),
                (12.0, 12.0),
                (width as f32 / 2.0, height as f32 / 2.0),
                (1900.0, 1000.0),
            ] {
                song_menu_state.set_open_menu_anchor_right(right);
                song_menu_state.set_open_menu_anchor_top(top);
                song_menu_state.set_open_menu_anchor_bottom(top + 28.0);
                for expanded in [false, true] {
                    song_menu_state.set_open_menu_submenu(expanded);
                    settle_menu(&app);
                    assert_menu_contained(&app);
                    if !logged_in {
                        assert_eq!(
                            element(&app, "SongContextMenuOverlay::context-menu-card")
                                .size()
                                .height,
                            element(&app, "SongContextMenuOverlay::context-menu-item-next")
                                .size()
                                .height
                                + 54.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn song_context_menu_closes_on_page_scroll_and_keeps_submenu_usable() {
    use crate::{NavPlaylistItem, TrackItem};
    use slint::{platform::PointerEventButton, PhysicalSize, VecModel};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    init();
    let app = AppWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1340, 960));
    app.show().unwrap();
    let account_state = app.global::<crate::AccountState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    let search_state = app.global::<crate::SearchState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let detail_state = app.global::<crate::DetailState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    let player_state = app.global::<crate::PlayerState>();
    account_state.set_is_logged_in(true);
    runtime_state.set_runtime_enabled(true);
    search_state.set_search_show_results(true);
    let tracks = Rc::new(VecModel::from(
        (0..200)
            .map(|id| TrackItem {
                id,
                name: format!("song-{id}").into(),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ));
    playlist_state.set_playlist_tracks(tracks.clone().into());
    detail_state.set_detail_tracks(tracks.into());
    playlist_state.set_created_playlists(
        Rc::new(VecModel::from(
            (0..30)
                .map(|id| NavPlaylistItem {
                    id: 1000 + id,
                    name: format!("playlist-{id}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    let added_to = Rc::new(Cell::new(-1));
    let sink = added_to.clone();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let action_sink = actions.clone();
    runtime_state.on_runtime_action(move |action, id, arg| {
        action_sink
            .borrow_mut()
            .push((action.to_string(), id, arg.to_string()));
        if action == "add-track" {
            sink.set(id);
        }
    });

    for (route, row) in [
        ("playlist", "SongRowActions"),
        ("search", "SearchSongActions"),
    ] {
        navigation_state.set_current_route(route.into());
        settle_menu(&app);
        for delta in [0.0, -1600.0] {
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(700.0, 600.0),
                delta_x: 0.0,
                delta_y: delta,
            });
            settle_menu(&app);
            let button = ElementHandle::find_by_element_id(&app, &format!("{row}::more-ta"))
                .filter(|b| (600.0..800.0).contains(&b.absolute_position().y))
                .last()
                .expect("visible song menu button near the bottom");
            let p = button.absolute_position();
            let s = button.size();
            button.mock_single_click(PointerEventButton::Left);
            settle_menu(&app);
            assert!(
                song_menu_state.get_open_menu_song_id() >= 0,
                "{route}: button did not open menu"
            );
            assert!((song_menu_state.get_open_menu_anchor_right() - p.x - s.width).abs() < 0.1);
            assert!((song_menu_state.get_open_menu_anchor_top() - p.y).abs() < 0.1);
            assert!((song_menu_state.get_open_menu_anchor_bottom() - p.y - s.height).abs() < 0.1);
            assert_menu_contained(&app);
            let card = element(&app, "SongContextMenuOverlay::context-menu-card");
            assert!((card.absolute_position().x + card.size().width - p.x - s.width).abs() < 0.1);

            // Both logged-in and guest menus use the currently visible row ID,
            // including virtual rows reused after scrolling.
            let target = song_menu_state.get_open_menu_song_id();
            for logged_in in [false, true] {
                account_state.set_is_logged_in(logged_in);
                player_state.set_current_song_id(target);
                settle_menu(&app);
                assert_menu_contained(&app);
                actions.borrow_mut().clear();
                let next = element(&app, "SongContextMenuOverlay::context-menu-item-next");
                next.mock_single_click(PointerEventButton::Left);
                settle_menu(&app);
                assert!(actions.borrow().is_empty(), "current song must be disabled");
                assert_eq!(song_menu_state.get_open_menu_song_id(), target);
                player_state.set_current_song_id(-1);
                settle_menu(&app);
                next.mock_single_click(PointerEventButton::Left);
                settle_menu(&app);
                assert_eq!(
                    *actions.borrow(),
                    vec![("queue-next".into(), target, String::new())]
                );
                assert_eq!(song_menu_state.get_open_menu_song_id(), -1);
                assert!(!song_menu_state.get_open_menu_submenu());
                // Reopen from the same song; the click must not reach its play action.
                button.mock_single_click(PointerEventButton::Left);
                settle_menu(&app);
                assert_eq!(song_menu_state.get_open_menu_song_id(), target);
            }

            element(&app, "SongMenuAccountActions::context-menu-item-add")
                .mock_single_click(PointerEventButton::Left);
            settle_menu(&app);
            assert!(song_menu_state.get_open_menu_submenu());
            assert_menu_contained(&app);
            assert!(card.absolute_position().y + card.size().height <= p.y - 7.9);

            let viewport = element(&app, "SongMenuAccountActions::submenu-scroll");
            let vp = viewport.absolute_position();
            let vs = viewport.size();
            // Scrolling again at the submenu's bottom must not reach the page.
            for _ in 0..2 {
                app.window().dispatch_event(WindowEvent::PointerScrolled {
                    position: LogicalPosition::new(vp.x + vs.width / 2.0, vp.y + vs.height / 2.0),
                    delta_x: 0.0,
                    delta_y: -10_000.0,
                });
                settle_menu(&app);
                assert_eq!(song_menu_state.get_open_menu_song_id(), target);
                assert!(song_menu_state.get_open_menu_submenu());
            }
            let last = ElementHandle::find_by_element_id(&app, "SongMenuPlaylistList::sub-ta")
                .filter(|b| {
                    b.absolute_position().y >= vp.y
                        && b.absolute_position().y + b.size().height <= vp.y + vs.height
                })
                .max_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y))
                .expect("last playlist remains reachable by scrolling");
            added_to.set(-1);
            last.mock_single_click(PointerEventButton::Left);
            settle_menu(&app);
            assert_eq!(added_to.get(), 1029);
            assert_eq!(
                actions.borrow().last(),
                Some(&("add-track".into(), 1029, target.to_string()))
            );
            assert_eq!(song_menu_state.get_open_menu_song_id(), -1);

            // Reopen, then scroll the page outside the menu. The same event
            // must dismiss both menu levels and still reach the page Flickable.
            button.mock_single_click(PointerEventButton::Left);
            song_menu_state.set_open_menu_submenu(true);
            settle_menu(&app);
            let before = button.absolute_position().y;
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(700.0, 500.0),
                delta_x: 0.0,
                delta_y: -160.0,
            });
            settle_menu(&app);
            assert_eq!(
                song_menu_state.get_open_menu_song_id(),
                -1,
                "{route}: page scroll left menu open"
            );
            assert!(!song_menu_state.get_open_menu_submenu());
            assert!(ElementHandle::find_by_element_id(
                &app,
                "SongContextMenuOverlay::context-menu-card"
            )
            .next()
            .is_none());
            assert!(
                button.absolute_position().y < before,
                "{route}: menu swallowed page scroll"
            );
        }
    }
}

fn assert_song_columns_aligned(
    app: &AppWindow,
    loading: bool,
    headings: &[(&str, ElementHandle)],
    clock: &ElementHandle,
) {
    for (column, heading) in headings {
        let id = if loading {
            format!("SongLoadingRow::loading-{column}-column")
        } else {
            format!("SongTrackRow::{column}-column")
        };
        let cells: Vec<_> = ElementHandle::find_by_element_id(app, &id).collect();
        assert!(!cells.is_empty(), "Missing {id}");
        for cell in cells {
            assert!(
                (cell.absolute_position().x - heading.absolute_position().x).abs() < 0.1
                    && (cell.size().width - heading.size().width).abs() < 0.1,
                "{id} at {:?}, size {:?} does not match heading at {:?}, size {:?}",
                cell.absolute_position(),
                cell.size(),
                heading.absolute_position(),
                heading.size()
            );
        }
    }
    if !loading {
        let index_heading = &headings
            .iter()
            .find(|(name, _)| *name == "index")
            .unwrap()
            .1;
        for index in ElementHandle::find_by_element_id(app, "SongTrackRow::song-index") {
            assert!(
                (index.absolute_position().x - index_heading.absolute_position().x).abs() < 0.1
            );
        }
        for (text, column) in [
            ("song-title", "title"),
            ("song-artist", "title"),
            ("album-name", "album"),
        ] {
            let Some((_, heading)) = headings.iter().find(|(name, _)| *name == column) else {
                continue;
            };
            for label in ElementHandle::find_by_element_id(app, &format!("SongTrackRow::{text}")) {
                if column == "album" {
                    assert!(
                        (label.absolute_position().x - heading.absolute_position().x).abs() < 0.1
                    );
                }
                assert!(
                    label.absolute_position().x >= heading.absolute_position().x - 0.1
                        && label.absolute_position().x + label.size().width
                            <= heading.absolute_position().x + heading.size().width - 15.9,
                    "{text} at {:?}, size {:?} spills outside heading at {:?}, size {:?}",
                    label.absolute_position(),
                    label.size(),
                    heading.absolute_position(),
                    heading.size()
                );
            }
        }
        for duration in ElementHandle::find_by_element_id(app, "SongRowActions::song-duration") {
            assert!(
                (duration.absolute_position().x + duration.size().width
                    - clock.absolute_position().x
                    - clock.size().width)
                    .abs()
                    < 0.1,
                "duration heading is not aligned with the duration text"
            );
        }
    }
}

#[test]
fn song_table_columns_align_with_variable_text_loading_and_recycled_rows() {
    use crate::TrackItem;
    use slint::{PhysicalSize, VecModel};
    use std::rc::Rc;

    init();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let playlist_state = app.global::<crate::PlaylistState>();
    let detail_state = app.global::<crate::DetailState>();
    let library_state = app.global::<crate::LibraryState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let tracks = Rc::new(VecModel::from(
        (0..200)
            .map(|id| TrackItem {
                id,
                index_str: (id + 1).to_string().into(),
                name: [
                    "短歌名".into(),
                    "很长的歌曲标题与不同文字长度的混排测试 ".repeat(12),
                    "".into(),
                ][id as usize % 3]
                    .clone()
                    .into(),
                artist: ["歌手", "多位歌手 / Very Long Artist Name / 合作音乐人", ""]
                    [id as usize % 3]
                    .into(),
                album: [
                    "专辑",
                    "非常长的专辑名与本地文件路径 / music / album / ",
                    "",
                ][id as usize % 3]
                    .repeat(10)
                    .into(),
                duration: ["3:05", "125:30", "0:09"][id as usize % 3].into(),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ));
    playlist_state.set_playlist_tracks(tracks.clone().into());
    detail_state.set_detail_tracks(tracks.clone().into());
    library_state.set_local_tracks(tracks.into());

    for (width, height) in [
        (1435, 1000),
        (1340, 960),
        (1800, 1000),
        (900, 1000),
        (620, 720),
    ] {
        app.window().set_size(PhysicalSize::new(width, height));
        navigation_state.set_window_width(width as f32);
        for route in ["playlist", "album", "favorites", "local-music"] {
            navigation_state.set_current_route(route.into());
            settle_menu(&app);
            if width <= 900 && route == "playlist" {
                app.window().dispatch_event(WindowEvent::PointerScrolled {
                    position: LogicalPosition::new(width as f32 - 200.0, 600.0),
                    delta_x: 0.0,
                    delta_y: -500.0,
                });
                settle_menu(&app);
            }
            // Retain handles before scrolling: element searches omit a header
            // once it is outside the Flickable's visible area.
            let headings: Vec<_> = ["index", "title", "album", "time"]
                .into_iter()
                .filter(|column| *column != "album" || width > 900)
                .map(|column| (column, element(&app, &format!("SongList::{column}-column"))))
                .collect();
            let clock = element(&app, "SongList::duration-heading");
            assert_song_columns_aligned(&app, false, &headings, &clock);
            if width == 1340 && (route == "playlist" || route == "local-music") {
                if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
                    let dir = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&dir).unwrap();
                    let pixels = app.window().take_snapshot().unwrap();
                    image::save_buffer(
                        dir.join(format!("song-table-{route}.png")),
                        pixels.as_bytes(),
                        pixels.width(),
                        pixels.height(),
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(width as f32 - 200.0, 600.0),
                delta_x: 0.0,
                delta_y: -1600.0,
            });
            settle_menu(&app);
            assert_song_columns_aligned(&app, false, &headings, &clock);
            if route == "playlist" {
                playlist_state.set_playlist_loading(true);
                settle_menu(&app);
                if width <= 900 {
                    app.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: LogicalPosition::new(width as f32 - 200.0, 600.0),
                        delta_x: 0.0,
                        delta_y: -500.0,
                    });
                    settle_menu(&app);
                }
                assert_song_columns_aligned(&app, true, &headings, &clock);
                playlist_state.set_playlist_loading(false);
            }
        }
    }
}

#[test]
fn add_to_playlist_modal_keeps_the_song_selected_when_it_was_opened() {
    use slint::{platform::PointerEventButton, VecModel};
    use std::{cell::RefCell, rc::Rc};
    init();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let runtime_state = app.global::<crate::RuntimeState>();
    let navigation_state = app.global::<crate::NavigationState>();
    let account_state = app.global::<crate::AccountState>();
    let player_state = app.global::<crate::PlayerState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    runtime_state.set_runtime_enabled(true);
    navigation_state.set_current_route("playlist".into());
    account_state.set_is_logged_in(true);
    player_state.set_current_song_is_local(false);
    player_state.set_current_song_id(42);
    player_state.set_current_song_title("打开弹窗时的歌曲".into());
    playlist_state.set_created_playlists(
        Rc::new(VecModel::from(vec![crate::NavPlaylistItem {
            id: 500,
            name: "目标歌单".into(),
            ..Default::default()
        }]))
        .into(),
    );
    let actions = Rc::new(RefCell::new(Vec::new()));
    let sink = actions.clone();
    runtime_state.on_runtime_action(move |action, id, arg| {
        sink.borrow_mut()
            .push((action.to_string(), id, arg.to_string()));
    });
    settle_menu(&app);
    actions.borrow_mut().clear();
    element(&app, "PlayerUtilityControls::more-btn").mock_single_click(PointerEventButton::Left);
    settle_menu(&app);
    assert!(song_menu_state.get_is_add_to_playlist_open());
    assert_eq!(song_menu_state.get_add_to_playlist_song_id(), 42);
    player_state.set_current_song_id(43);
    element(&app, "PlaylistPickerList::item-ta").mock_single_click(PointerEventButton::Left);
    settle_menu(&app);
    assert_eq!(*actions.borrow(), [("add-track".into(), 500, "42".into())]);
    assert!(!song_menu_state.get_is_add_to_playlist_open());
}
