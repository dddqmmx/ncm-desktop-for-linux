use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{platform::WindowEvent, ComponentHandle, LogicalPosition, PhysicalSize};
use std::time::Duration;

pub(crate) fn assert_inside(outer: &ElementHandle, inner: &ElementHandle, context: &str) {
    let a = outer.absolute_position();
    let b = inner.absolute_position();
    let os = outer.size();
    let size = inner.size();
    assert!(
        size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0,
        "{context}: {:?} has invalid size {size:?}",
        inner.id()
    );
    assert!(
        b.x >= a.x - 0.5
            && b.y >= a.y - 0.5
            && b.x + size.width <= a.x + os.width + 0.5
            && b.y + size.height <= a.y + os.height + 0.5,
        "{context}: {:?} at {b:?} {size:?} escapes {:?} at {a:?} {os:?}",
        inner.id(),
        outer.id()
    );
}

pub(crate) fn assert_disjoint(a: &ElementHandle, b: &ElementHandle, context: &str) {
    let ap = a.absolute_position();
    let bp = b.absolute_position();
    let az = a.size();
    let bz = b.size();
    assert!(
        ap.x + az.width <= bp.x + 0.5
            || bp.x + bz.width <= ap.x + 0.5
            || ap.y + az.height <= bp.y + 0.5
            || bp.y + bz.height <= ap.y + 0.5,
        "{context}: {:?} at {ap:?} {az:?} overlaps {:?} at {bp:?} {bz:?}",
        a.id(),
        b.id()
    );
}

fn element(app: &AppWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing {id}"))
}

fn app() -> AppWindow {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    app
}

fn resize(app: &AppWindow, width: u32, height: u32) {
    app.window().set_size(PhysicalSize::new(width, height));
    let navigation_state = app.global::<crate::NavigationState>();
    navigation_state.set_window_width(width as f32);
    navigation_state.set_window_height(height as f32);
    settle(app);
}

fn settle(app: &AppWindow) {
    for _ in 0..40 {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(25));
    }
}

pub(crate) fn snapshot(app: &AppWindow, name: &str) {
    if let Some(path) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let pixels = app.window().take_snapshot().unwrap();
        image::save_buffer(
            path.join(format!("{name}.png")),
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn detail_headers_and_actions_reflow_without_overlap() {
    let app = app();
    let navigation_state = app.global::<crate::NavigationState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    navigation_state.set_current_route("playlist".into());
    playlist_state.set_playlist_title(
        "一份很长的歌单标题，用来验证窗口变窄后标题、简介与操作栏仍然各自拥有空间".into(),
    );
    playlist_state.set_playlist_description("歌单简介会随可用宽度换行。Long descriptions must not cover the search field or the song list. ".repeat(5).into());
    let mut wide_title_height = 0.0;
    for (width, height) in [
        (1435, 1000),
        (1340, 960),
        (1100, 800),
        (950, 800),
        (620, 720),
    ] {
        resize(&app, width, height);
        let header = element(&app, "MediaDetailView::detail-header");
        let cover = element(&app, "MediaDetailHeader::detail-cover-wrapper");
        let title = element(&app, "MediaDetailHeader::detail-title");
        let details = element(&app, "MediaDetailHeader::details");
        let actions = element(&app, "MediaDetailHeader::detail-action-bar");
        let songs = element(&app, "MediaDetailView::song-list");
        let context = format!("detail {width}x{height}");
        for child in [&cover, &details, &title, &actions] {
            assert_inside(&header, child, &context);
        }
        assert_disjoint(&cover, &details, &context);
        assert_disjoint(&title, &actions, &context);
        assert_disjoint(&header, &songs, &context);
        assert!((cover.size().width - cover.size().height).abs() < 0.5);
        let mut buttons = vec![];
        for id in [
            "detail-play-main-btn",
            "detail-search-box",
            "detail-secondary-btn",
            "playlist-actions-trigger",
        ] {
            for item in
                ElementHandle::find_by_element_id(&app, &format!("MediaDetailActions::{id}"))
            {
                assert_inside(&actions, &item, &context);
                for previous in &buttons {
                    assert_disjoint(previous, &item, &context);
                }
                buttons.push(item);
            }
        }
        if width == 1435 {
            wide_title_height = title.size().height;
        }
        if width == 620 {
            // Check pixels as well as geometry: tight font leading must not
            // silently elide the first or second line of a wrapped title.
            let pixels = app.window().take_snapshot().unwrap();
            let pos = title.absolute_position();
            let size = title.size();
            let scale = pixels.width() as f32 / app.root_element().size().width;
            for line in 0..2 {
                let top = pos.y + size.height * line as f32 / 2.0;
                let bottom = top + size.height / 2.0;
                let mut ink = 0;
                for y in (top * scale) as u32..(bottom * scale) as u32 {
                    for x in (pos.x * scale) as u32..((pos.x + size.width) * scale) as u32 {
                        let p = pixels.as_slice()[(y * pixels.width() + x) as usize];
                        if p.r < 100 && p.g < 100 && p.b < 100 {
                            ink += 1;
                        }
                    }
                }
                assert!(ink > 50, "title line {line} has no rendered text");
            }
            assert!(title.size().height >= wide_title_height);
            assert!(
                details.absolute_position().y
                    >= cover.absolute_position().y + cover.size().height + 39.5
            );
            assert!(
                actions.size().height > buttons[0].size().height,
                "narrow actions did not wrap"
            );
        }
        snapshot(&app, &format!("responsive-detail-{width}"));
    }
}

#[test]
fn fullscreen_panels_share_space_and_keep_controls_inside_at_each_breakpoint() {
    let app = app();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let player_state = app.global::<crate::PlayerState>();
    player_view_state.set_is_fullscreen(true);
    player_state.set_current_song_title(
        "长标题 Long track title that should elide within the visual panel".into(),
    );
    player_state.set_current_song_artist("歌手一 / Artist Two / 歌手三 / Artist Four".into());
    for (width, height) in [
        (1435, 1000),
        (1340, 960),
        (1100, 800),
        (1000, 900),
        (800, 1000),
    ] {
        resize(&app, width, height);
        let visual = element(&app, "PlayerFullscreen::visual-panel");
        let right = element(&app, "PlayerFullscreen::right-panel");
        let context = format!("fullscreen {width}x{height}");
        for panel in [&visual, &right] {
            assert_inside(&app.root_element(), panel, &context);
        }
        assert_disjoint(&visual, &right, &context);
        for id in ["fullscreen-cover-wrapper", "metadata", "playback-controls"] {
            assert_inside(
                &visual,
                &element(&app, &format!("FullscreenVisualPanel::{id}")),
                &context,
            );
        }
        assert_disjoint(
            &element(&app, "FullscreenVisualPanel::metadata"),
            &element(&app, "FullscreenVisualPanel::playback-controls"),
            &context,
        );
        let cover = element(&app, "FullscreenVisualPanel::fullscreen-cover-scaled");
        assert!((cover.size().width - cover.size().height).abs() < 0.5);
        assert!(cover.size().width > 0.0 && cover.size().width <= 460.0);
        assert_inside(
            &visual,
            &element(&app, "FullscreenTransport::fullscreen-play-btn"),
            &context,
        );
        if width > 1000 {
            assert!(
                (visual.size().width - right.size().width).abs() <= 1.0,
                "{context}: visual {:?}, right {:?}",
                visual.size(),
                right.size()
            );
            assert!((visual.absolute_position().y - right.absolute_position().y).abs() < 0.5);
        } else {
            assert!((visual.size().height - right.size().height).abs() <= 1.0);
            assert!(
                right.absolute_position().y
                    >= visual.absolute_position().y + visual.size().height + 39.5
            );
        }
        snapshot(&app, &format!("responsive-fullscreen-{width}"));
    }
}

#[test]
fn dialogs_measure_content_and_scroll_long_forms_without_covering_the_footer() {
    let app = app();
    crate::playlists::attach(&app);
    let playlist_state = app.global::<crate::PlaylistState>();
    let login_state = app.global::<crate::LoginState>();
    playlist_state.invoke_open_playlist(101);
    for (width, height) in [(1435, 1000), (620, 720), (620, 420)] {
        resize(&app, width, height);
        playlist_state.invoke_open_edit_playlist();
        playlist_state.set_playlist_editor_error(
            "这是一条很长的错误提示，必须换行显示并且不能盖住保存按钮。"
                .repeat(5)
                .into(),
        );
        settle(&app);
        let card = element(&app, "PlaylistEditor::editor-card");
        let footer = element(&app, "PlaylistEditorContent::editor-footer");
        let scroll = element(&app, "PlaylistEditorContent::editor-scroll");
        for (field, component) in [
            ("name", "PlaylistEditorForm"),
            ("description", "PlaylistDescriptionField"),
        ] {
            let heading = element(&app, &format!("{component}::{field}-heading"));
            let input = element(&app, &format!("{component}::{field}-box"));
            assert_disjoint(&heading, &input, "field labels stay above the input");
            assert!(
                heading.absolute_position().y + heading.size().height
                    <= input.absolute_position().y - 7.5,
                "{field}: heading overlaps the input or loses its spacing"
            );
        }
        for child in [&card, &footer, &scroll] {
            assert_inside(&app.root_element(), child, "editor");
        }
        assert_disjoint(&scroll, &footer, "editor scroll and footer");
        assert_inside(
            &footer,
            &element(&app, "PlaylistEditorFooter::submit-button"),
            "editor save",
        );
        assert_inside(
            &footer,
            &element(&app, "PlaylistEditorFooter::cancel-button"),
            "editor cancel",
        );
        let contents = element(&app, "PlaylistEditorContent::form-content");
        let before = contents.absolute_position().y;
        app.window().dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(
                scroll.absolute_position().x + 50.0,
                scroll.absolute_position().y + 20.0,
            ),
            delta_x: 0.0,
            delta_y: -1000.0,
        });
        settle(&app);
        if height == 420 {
            assert!(
                contents.absolute_position().y < before,
                "short windows must scroll the form"
            );
        }
        snapshot(&app, &format!("responsive-editor-{height}"));
        playlist_state.invoke_close_playlist_editor();
    }
    resize(&app, 620, 720);
    login_state.set_is_login_open(true);
    settle(&app);
    let scroll = element(&app, "LoginForm::login-form-scroll");
    let submit = element(&app, "LoginForm::login-submit-btn");
    login_state.set_login_error("登录失败：请检查网络连接和登录信息。".repeat(12).into());
    settle(&app);
    assert!(submit.absolute_position().y > scroll.absolute_position().y + scroll.size().height);
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(
            scroll.absolute_position().x + 40.0,
            scroll.absolute_position().y + 40.0,
        ),
        delta_x: 0.0,
        delta_y: -2000.0,
    });
    settle(&app);
    assert_inside(&scroll, &submit, "login submit is reachable by scrolling");
    snapshot(&app, "responsive-login-error");
}

#[test]
fn add_to_playlist_labels_are_centered_with_the_cover_and_status() {
    use crate::{NavPlaylistItem, Theme};
    use slint::VecModel;
    use std::rc::Rc;
    let app = app();
    let navigation_state = app.global::<crate::NavigationState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    navigation_state.set_current_route("playlist".into());
    playlist_state.set_created_playlists(
        Rc::new(VecModel::from(vec![
            NavPlaylistItem {
                id: 1,
                name: "深夜循环".into(),
                track_count: 42,
                play_count: "123".into(),
                ..Default::default()
            },
            NavPlaylistItem {
                id: 2,
                name: "这是一份名称很长的歌单，用来检查文字与已添加状态互不挤压".into(),
                track_count: 1234,
                play_count: "987654321".into(),
                already_added: true,
                ..Default::default()
            },
            NavPlaylistItem {
                id: 3,
                name: "Long playlist name with different text metrics".into(),
                track_count: 7,
                play_count: "12.3万".into(),
                ..Default::default()
            },
        ]))
        .into(),
    );
    song_menu_state.set_is_add_to_playlist_open(true);
    for dark in [false, true] {
        app.global::<Theme>().set_is_dark(dark);
        for (width, height) in [(1435, 1000), (620, 720), (320, 400)] {
            resize(&app, width, height);
            let header = element(&app, "AddToPlaylistModal::add-playlist-header");
            assert_inside(
                &header,
                &element(&app, "PlaylistPickerHeader::header-labels"),
                "picker header labels",
            );
            let items = |id: &str| {
                ElementHandle::find_by_element_id(&app, &format!("PlaylistCapsuleContent::{id}"))
                    .collect::<Vec<_>>()
            };
            let rows = ElementHandle::find_by_element_id(&app, "PlaylistPickerList::item-ta")
                .collect::<Vec<_>>();
            let covers = items("item-cover");
            let names = items("item-name");
            let metadata = items("item-meta");
            let mut statuses = items("item-status");
            statuses.extend(items("item-add-icon"));
            statuses.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
            assert_eq!(rows.len(), 3);
            assert_eq!(names.len(), rows.len());
            assert_eq!(metadata.len(), rows.len());
            assert_eq!(covers.len(), rows.len());
            assert_eq!(statuses.len(), rows.len());
            for index in 0..rows.len() {
                let row = &rows[index];
                let name = &names[index];
                let meta = &metadata[index];
                let cover = &covers[index];
                let status = &statuses[index];
                for child in [name, meta, cover] {
                    assert_inside(row, child, "playlist capsule");
                }
                assert_disjoint(name, meta, "playlist name and metadata");
                assert_disjoint(cover, name, "playlist cover and text");
                let row_center = row.absolute_position().y + row.size().height / 2.0;
                let text_center =
                    (name.absolute_position().y + meta.absolute_position().y + meta.size().height)
                        / 2.0;
                assert!(
                    (text_center - row_center).abs() < 0.5,
                    "text group is off-center at {width}x{height}: {text_center} vs {row_center}"
                );
                assert!(
                    (cover.absolute_position().y + cover.size().height / 2.0 - row_center).abs()
                        < 0.5
                );
                assert!(
                    (status.absolute_position().y + status.size().height / 2.0 - row_center).abs()
                        < 0.5,
                    "status is off-center at {width}x{height} in row {index}"
                );
                assert_inside(row, status, "playlist status");
                assert_disjoint(name, status, "playlist title and status");
                assert_disjoint(meta, status, "playlist metadata and status");
            }
            snapshot(
                &app,
                &format!(
                    "playlist-labels-{width}-{}",
                    if dark { "dark" } else { "light" }
                ),
            );
        }
    }
}

#[test]
fn comments_keep_metadata_replies_and_scroll_footer_in_their_layout() {
    use crate::CommentItem;
    use slint::VecModel;
    use std::rc::Rc;

    let app = app();
    let player_view_state = app.global::<crate::PlayerViewState>();
    let player_state = app.global::<crate::PlayerState>();
    player_view_state.set_is_fullscreen(true);
    player_view_state.set_is_comment_open(true);
    player_state.set_current_song_is_local(false);
    player_view_state.set_comments(Rc::new(VecModel::<CommentItem>::default()).into());
    resize(&app, 1435, 1000);
    assert_inside(
        &element(&app, "CommentPanelView::comment-scroll"),
        &element(&app, "CommentPanelView::footer"),
        "empty comment list",
    );

    player_view_state.set_comments(
        Rc::new(VecModel::from(
            (0..2)
                .map(|index| CommentItem {
                    id: index + 1,
                    nickname: format!("听众 {}", index + 1).into(),
                    is_vip: index == 0,
                    time: "2026-11-10".into(),
                    content: "这段旋律让人想起旅途中的风景，长评论需要自然换行。"
                        .repeat(8)
                        .into(),
                    has_reply: index == 1,
                    reply_nickname: "同行的人".into(),
                    reply_content: "引用的内容也要换行，保持在评论正文的宽度内。"
                        .repeat(4)
                        .into(),
                    liked: "128".into(),
                    is_hot_header: index == 0,
                    is_recent_header: index == 1,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    for dark in [false, true] {
        player_view_state.set_fullscreen_is_dark(dark);
        let (primary, secondary) = if dark {
            ((40, 42, 85), (18, 20, 38))
        } else {
            ((244, 242, 250), (222, 229, 242))
        };
        player_view_state
            .set_fullscreen_bg_primary(slint::Color::from_rgb_u8(primary.0, primary.1, primary.2));
        player_view_state.set_fullscreen_bg_secondary(slint::Color::from_rgb_u8(
            secondary.0,
            secondary.1,
            secondary.2,
        ));
        for width in [1435, 800] {
            resize(&app, width, 1800);
            let scroll = element(&app, "CommentPanelView::comment-scroll");
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(
                    scroll.absolute_position().x + 40.0,
                    scroll.absolute_position().y + 40.0,
                ),
                delta_x: 0.0,
                delta_y: 10000.0,
            });
            settle(&app);
            let items = |id: &str| {
                ElementHandle::find_by_element_id(&app, &format!("CommentRow::{id}"))
                    .collect::<Vec<_>>()
            };
            let avatars = items("avatar");
            let bodies = items("body");
            let metadata = items("metadata");
            let contents = items("content");
            assert_eq!(avatars.len(), 2);
            assert_eq!(bodies.len(), 2);
            assert_eq!(metadata.len(), 2);
            assert_eq!(contents.len(), 2);
            for index in 0..2 {
                assert_disjoint(&avatars[index], &bodies[index], "comment avatar and body");
                assert_inside(&bodies[index], &metadata[index], "comment author metadata");
                assert_inside(&bodies[index], &contents[index], "wrapped comment content");
                assert_disjoint(
                    &metadata[index],
                    &contents[index],
                    "comment metadata and text",
                );
                if index > 0 {
                    assert_disjoint(&bodies[index - 1], &bodies[index], "successive comments");
                }
            }
            let replies = items("reply");
            assert_eq!(replies.len(), 1);
            assert_inside(&bodies[1], &replies[0], "quoted reply");
            assert_disjoint(&contents[1], &replies[0], "comment and quoted reply");
            resize(&app, width, 1000);
            snapshot(
                &app,
                &format!(
                    "comments-top-{width}-{}",
                    if dark { "dark" } else { "light" }
                ),
            );
            let scroll = element(&app, "CommentPanelView::comment-scroll");
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(
                    scroll.absolute_position().x + scroll.size().width / 2.0,
                    scroll.absolute_position().y + scroll.size().height / 2.0,
                ),
                delta_x: 0.0,
                delta_y: -10000.0,
            });
            settle(&app);
            assert_inside(
                &scroll,
                &element(&app, "CommentPanelView::footer"),
                "comment footer after scrolling",
            );
            snapshot(
                &app,
                &format!("comments-{width}-{}", if dark { "dark" } else { "light" }),
            );
        }
    }
}

#[test]
fn search_categories_use_available_columns_and_keep_their_labels_inside() {
    let app = app();
    let navigation_state = app.global::<crate::NavigationState>();
    let search_state = app.global::<crate::SearchState>();
    navigation_state.set_current_route("search".into());
    search_state.set_search_show_results(false);
    resize(&app, 1435, 1000);
    let cards: Vec<_> = ElementHandle::find_by_element_id(&app, "CategoryCard::root").collect();
    assert_eq!(cards.len(), 12);
    for (width, height) in [(1435, 1000), (1340, 960), (1000, 800), (620, 720)] {
        resize(&app, width, height);
        let grid = element(&app, "SearchCategories::browse-grid");
        let heading = element(&app, "SearchCategories::browse-heading");
        assert_disjoint(&heading, &grid, "search heading and categories");
        let columns = ((grid.size().width + 16.0) / 196.0).floor().max(1.0) as usize;
        let first_row_y = cards[0].absolute_position().y;
        assert_eq!(
            cards
                .iter()
                .filter(|c| (c.absolute_position().y - first_row_y).abs() < 0.5)
                .count(),
            columns
        );
        for (index, card) in cards.iter().enumerate() {
            assert_inside(&grid, card, "category grid");
            assert!(card.size().width >= 179.5);
            for other in &cards[..index] {
                assert_disjoint(card, other, "category grid");
            }
        }
        snapshot(&app, &format!("responsive-search-{width}"));
    }
}
