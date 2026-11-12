pub use ncm_slint_ui_bindings::*;
use slint::ComponentHandle;
mod appearance;
#[cfg(test)]
mod appearance_tests;
mod detail_search;
mod fullscreen_palette;
#[cfg(test)]
mod fullscreen_tests;
#[cfg(test)]
mod home_tests;
#[cfg(feature = "hot-reload")]
mod hot_reload;
#[cfg(test)]
mod layout_tests;
mod login_animation;
#[cfg(test)]
mod login_animation_tests;
mod playback_ui;
#[cfg(test)]
mod playlist_tests;
mod playlists;
mod rendering;
#[cfg(test)]
mod responsive_tests;
mod runtime;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod virtual_list_tests;
#[cfg(test)]
mod window_size_tests;
#[cfg(test)]
mod window_tests;
mod windows;

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(feature = "hot-reload")]
    let _hot_reload = hot_reload::watch_atomic_saves()?;
    let demo = std::env::args().any(|arg| arg == "--demo");
    // Slint's platform is process-wide: read the persisted renderer before
    // creating any main, settings, or offscreen login window.
    let startup = if demo {
        None
    } else {
        let storage = runtime::Storage::discover()?;
        let log_path = storage.config.join("diagnostics.jsonl");
        if let Err(error) = native::diagnostics::init(&log_path) {
            eprintln!("无法打开诊断日志 {}: {error}", log_path.display());
        }
        native::diagnostics::record(
            "info",
            "app.start",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION")}),
        );
        let preferences = storage.preferences()?;
        Some((storage, preferences))
    };
    let renderer = startup
        .as_ref()
        .map(|(_, preferences)| preferences.renderer)
        .unwrap_or_default();
    renderer.backend_selector().select()?;
    let app = AppWindow::new()?;
    let windows = windows::ApplicationWindows::attach(&app)?;
    let _runtime = if let Some((storage, preferences)) = startup {
        match runtime::ApplicationRuntime::attach(&app, &windows._settings, storage, preferences) {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                eprintln!("启动失败: {error}");
                std::process::exit(1);
            }
        }
    } else {
        playlists::attach(&app);
        None
    };
    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
    use serde::Deserialize;
    use slint::{LogicalPosition, LogicalSize, PhysicalSize};
    use std::collections::HashMap;
    use std::time::Duration;

    #[derive(Debug, Clone, Deserialize)]
    struct GroundTruthRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GroundTruthNode {
        rect: GroundTruthRect,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GroundTruthAnimSample {
        #[serde(rename = "tMs")]
        t_ms: u64,
        value: f32,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GroundTruthAnimCurve {
        #[serde(rename = "durationMs")]
        duration_ms: u64,
        samples: Vec<GroundTruthAnimSample>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GroundTruthReport {
        views: HashMap<String, HashMap<String, Option<GroundTruthNode>>>,
        responsive: HashMap<String, HashMap<String, Option<GroundTruthNode>>>,
        animations: HashMap<String, GroundTruthAnimCurve>,
    }

    /// Standard CSS `cubic-bezier(x1, y1, x2, y2)` evaluator (Newton-Raphson + bisection, matching Blink/Chromium).
    fn eval_cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, progress_t: f64) -> f64 {
        if progress_t <= 0.0 {
            return 0.0;
        }
        if progress_t >= 1.0 {
            return 1.0;
        }
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;

        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;

        let sample_x = |t: f64| ((ax * t + bx) * t + cx) * t;
        let sample_dx = |t: f64| (3.0 * ax * t + 2.0 * bx) * t + cx;
        let sample_y = |t: f64| ((ay * t + by) * t + cy) * t;

        let mut u = progress_t;
        for _ in 0..8 {
            let x_err = sample_x(u) - progress_t;
            if x_err.abs() < 1e-7 {
                return sample_y(u);
            }
            let d = sample_dx(u);
            if d.abs() < 1e-6 {
                break;
            }
            u -= x_err / d;
        }

        let mut lo = 0.0;
        let mut hi = 1.0;
        u = progress_t;
        for _ in 0..20 {
            let x_val = sample_x(u);
            if (x_val - progress_t).abs() < 1e-7 {
                break;
            }
            if progress_t > x_val {
                lo = u;
            } else {
                hi = u;
            }
            u = 0.5 * (lo + hi);
        }
        sample_y(u)
    }

    /// Standard CSS `ease` = `cubic-bezier(0.25, 0.1, 0.25, 1.0)`.
    fn eval_css_ease(progress_t: f64) -> f64 {
        eval_cubic_bezier(0.25, 0.1, 0.25, 1.0, progress_t)
    }

    fn find_first_element(app: &impl slint::ComponentHandle, id: &str) -> ElementHandle {
        ElementHandle::find_by_element_id(app, id)
            .next()
            .unwrap_or_else(|| panic!("Element '{id}' not found in live Slint tree"))
    }

    fn find_nth_element(app: &impl slint::ComponentHandle, id: &str, idx: usize) -> ElementHandle {
        ElementHandle::find_by_element_id(app, id)
            .nth(idx)
            .unwrap_or_else(|| panic!("Element '{id}'[{idx}] not found in live Slint tree"))
    }

    fn assert_rect_matches_electron(
        label: &str,
        pos: LogicalPosition,
        size: LogicalSize,
        gt: &GroundTruthRect,
        tol: f32,
    ) {
        let dx = (pos.x - gt.x).abs();
        let dy = (pos.y - gt.y).abs();
        let dw = (size.width - gt.width).abs();
        let dh = (size.height - gt.height).abs();
        assert!(
            dx <= tol && dy <= tol && dw <= tol && dh <= tol,
            "[{label}] Slint rect (x={:.2}, y={:.2}, w={:.2}, h={:.2}) != Electron rect (x={:.2}, y={:.2}, w={:.2}, h={:.2}) [diff: dx={:.2}, dy={:.2}, dw={:.2}, dh={:.2}, tol={:.2}]",
            pos.x,
            pos.y,
            size.width,
            size.height,
            gt.x,
            gt.y,
            gt.width,
            gt.height,
            dx,
            dy,
            dw,
            dh,
            tol
        );
    }

    fn assert_pos_size_matches_electron(
        label: &str,
        pos: LogicalPosition,
        size: LogicalSize,
        gt: &GroundTruthRect,
        pos_tol: f32,
        w_tol: f32,
        h_tol: f32,
    ) {
        let dx = (pos.x - gt.x).abs();
        let dy = (pos.y - gt.y).abs();
        let dw = (size.width - gt.width).abs();
        let dh = (size.height - gt.height).abs();
        assert!(
            dx <= pos_tol && dy <= pos_tol && dw <= w_tol && dh <= h_tol,
            "[{label}] Slint rect (x={:.2}, y={:.2}, w={:.2}, h={:.2}) != Electron rect (x={:.2}, y={:.2}, w={:.2}, h={:.2}) [diff: dx={:.2}, dy={:.2}, dw={:.2}, dh={:.2}]",
            pos.x,
            pos.y,
            size.width,
            size.height,
            gt.x,
            gt.y,
            gt.width,
            gt.height,
            dx,
            dy,
            dw,
            dh
        );
    }

    fn get_gt<'a>(report: &'a GroundTruthReport, view: &str, key: &str) -> &'a GroundTruthRect {
        &report
            .views
            .get(view)
            .unwrap_or_else(|| panic!("Missing view '{view}' in ground truth"))
            .get(key)
            .unwrap_or_else(|| panic!("Missing key '{view}.{key}' in ground truth"))
            .as_ref()
            .unwrap_or_else(|| panic!("Null ground truth for '{view}.{key}'"))
            .rect
    }

    fn init_testing_platform() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
                i_slint_backend_testing::TestingBackendOptions {
                    mock_time: true,
                    threading: false,
                    renderer_name: Some("software".into()),
                },
            )))
            .expect("Failed to initialize Slint headless software testing backend");
        });
    }

    #[test]
    fn pixel_and_animation_parity_with_electron() {
        init_testing_platform();

        let gt_json = include_str!("../tests/electron-ground-truth.json");
        let mut gt: GroundTruthReport =
            serde_json::from_str(gt_json).expect("Failed to parse electron-ground-truth.json");
        // Playlist management changed after the original all-page capture. Use
        // current Electron measurements for those controls; retain other baselines.
        let current: HashMap<String, HashMap<String, Option<GroundTruthNode>>> =
            serde_json::from_str(include_str!(
                "../tests/playlist-management-ground-truth.json"
            ))
            .unwrap();
        gt.views
            .get_mut("home")
            .unwrap()
            .insert("navGroupTitle".into(), current["sidebar"]["title"].clone());
        for (old, new) in [
            ("searchBox", "search"),
            ("secondaryBtn", "subscribers"),
            ("moreBtn", "more"),
        ] {
            gt.views
                .get_mut("playlistDetail")
                .unwrap()
                .insert(old.into(), current["detail"][new].clone());
        }

        let app = AppWindow::new().expect("AppWindow should instantiate");
        crate::login_animation::attach(&app).unwrap();
        app.window().set_size(PhysicalSize::new(1435, 1000));
        let navigation_state = app.global::<crate::NavigationState>();
        let queue_state = app.global::<crate::QueueState>();
        let player_view_state = app.global::<crate::PlayerViewState>();
        let song_menu_state = app.global::<crate::SongMenuState>();
        let search_state = app.global::<crate::SearchState>();
        let dialog_state = app.global::<crate::DialogState>();
        let login_state = app.global::<crate::LoginState>();
        let home_state = app.global::<crate::HomeState>();
        let player_state = app.global::<crate::PlayerState>();
        let audio_state = app.global::<crate::AudioState>();
        let theme = app.global::<Theme>();
        navigation_state.set_window_width(1435.0);
        navigation_state.set_window_height(1000.0);

        // Force layout evaluation
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(1000));

        // =========================================================================
        // PART 1: PIXEL-LEVEL GEOMETRY PARITY AGAINST ELECTRON GROUND TRUTH (12 VIEWS)
        // =========================================================================

        // --- 1. HomeView + SideNavBar + PlayerBar ---
        navigation_state.set_current_route("home".into());
        mock_elapsed_time(Duration::from_millis(50));

        let sidebar = find_first_element(&app, "SideNavBar::sidebar-floating");
        assert_rect_matches_electron(
            "home.sidebar",
            sidebar.absolute_position(),
            sidebar.size(),
            get_gt(&gt, "home", "sidebar"),
            0.5,
        );

        let sidebar_content = find_first_element(&app, "SideNavBar::sidebar-content");
        assert_rect_matches_electron(
            "home.sidebarContent",
            sidebar_content.absolute_position(),
            sidebar_content.size(),
            get_gt(&gt, "home", "sidebarContent"),
            0.5,
        );

        let nav_search = find_first_element(&app, "SidebarNavigation::nav-item-search");
        assert_rect_matches_electron(
            "home.navItem(search)",
            nav_search.absolute_position(),
            nav_search.size(),
            get_gt(&gt, "home", "navItem"),
            0.5,
        );

        let nav_home = find_first_element(&app, "SidebarNavigation::nav-item-home");
        assert_rect_matches_electron(
            "home.navItemActive(home)",
            nav_home.absolute_position(),
            nav_home.size(),
            get_gt(&gt, "home", "navItemActive"),
            0.5,
        );

        let nav_icon = find_first_element(&app, "SidebarNavItem::nav-icon-box");
        assert_rect_matches_electron(
            "home.navIcon",
            nav_icon.absolute_position(),
            nav_icon.size(),
            get_gt(&gt, "home", "navIcon"),
            0.5,
        );

        let nav_group_title = find_first_element(&app, "SidebarNavigation::nav-group-title");
        let nav_group_header = find_first_element(&app, "SidebarNavigation::nav-group-header");
        let nav_local = find_first_element(&app, "SidebarNavigation::nav-item-local");
        // Hiding favorites moves the group up by one navigation row. Keep its
        // text dimensions from the capture and check its position in the header.
        crate::responsive_tests::assert_inside(
            &nav_group_header,
            &nav_group_title,
            "home.navGroupTitle",
        );
        let mut group_title_rect = get_gt(&gt, "home", "navGroupTitle").clone();
        group_title_rect.y = nav_local.absolute_position().y
            + nav_local.size().height
            + 24.0
            + (nav_group_header.size().height - group_title_rect.height) / 2.0;
        assert_rect_matches_electron(
            "home.navGroupTitle",
            nav_group_title.absolute_position(),
            nav_group_title.size(),
            &group_title_rect,
            0.5,
        );

        let profile_bubble = find_first_element(&app, "SideNavBar::profile-bubble");
        assert_rect_matches_electron(
            "home.profileBubble",
            profile_bubble.absolute_position(),
            profile_bubble.size(),
            get_gt(&gt, "home", "profileBubble"),
            0.5,
        );

        let bubble_trigger = find_first_element(&app, "SidebarProfileBubble::bubble-trigger");
        assert_rect_matches_electron(
            "home.bubbleTrigger",
            bubble_trigger.absolute_position(),
            bubble_trigger.size(),
            get_gt(&gt, "home", "bubbleTrigger"),
            0.5,
        );

        let avatar_wrapper = find_first_element(&app, "SidebarProfileBubble::avatar-wrapper");
        assert_rect_matches_electron(
            "home.avatarWrapper",
            avatar_wrapper.absolute_position(),
            avatar_wrapper.size(),
            get_gt(&gt, "home", "avatarWrapper"),
            0.5,
        );

        let player_bar = find_first_element(&app, "PlayerBar::player-bar-panel");
        assert_rect_matches_electron(
            "home.playerBar",
            player_bar.absolute_position(),
            player_bar.size(),
            get_gt(&gt, "home", "playerBar"),
            0.5,
        );

        let player_cover = find_first_element(&app, "PlayerTrackSummary::player-cover");
        assert_rect_matches_electron(
            "home.playerCover",
            player_cover.absolute_position(),
            player_cover.size(),
            get_gt(&gt, "home", "playerCover"),
            0.5,
        );

        let play_main_btn = find_first_element(&app, "PlayerTransportControls::play-main-btn");
        assert_rect_matches_electron(
            "home.playMainBtn",
            play_main_btn.absolute_position(),
            play_main_btn.size(),
            get_gt(&gt, "home", "playMainBtn"),
            1.0,
        );

        let progress_track = find_first_element(&app, "PlayerTransportControls::progress-track");
        assert_rect_matches_electron(
            "home.progressTrack",
            progress_track.absolute_position(),
            progress_track.size(),
            get_gt(&gt, "home", "progressTrack"),
            0.5,
        );

        let home_title = find_first_element(&app, "HomeView::home-page-title");
        assert_rect_matches_electron(
            "home.pageTitle",
            home_title.absolute_position(),
            home_title.size(),
            get_gt(&gt, "home", "pageTitle"),
            0.5,
        );

        let home_h2 =
            find_first_element(&app, "RecommendedPlaylistsSection::home-section-header-0");
        assert_pos_size_matches_electron(
            "home.sectionHeaderH2",
            home_h2.absolute_position(),
            home_h2.size(),
            get_gt(&gt, "home", "sectionHeaderH2"),
            0.5,
            1000.0,
            0.5,
        );

        let media_card = find_first_element(&app, "MediaCard::card-surface");
        assert_rect_matches_electron(
            "home.mediaCard",
            media_card.absolute_position(),
            media_card.size(),
            get_gt(&gt, "home", "mediaCard"),
            0.5,
        );

        let media_card_play_btn = find_first_element(&app, "MediaCard::play-btn-circle");
        assert_rect_matches_electron(
            "home.mediaCardPlayBtn",
            media_card_play_btn.absolute_position(),
            media_card_play_btn.size(),
            get_gt(&gt, "home", "mediaCardPlayBtn"),
            0.5,
        );

        let album_item = find_first_element(&app, "AlbumItem::root");
        assert_rect_matches_electron(
            "home.albumItem",
            album_item.absolute_position(),
            album_item.size(),
            get_gt(&gt, "home", "albumItem"),
            0.5,
        );

        let album_cover = find_first_element(&app, "AlbumItem::album-cover");
        assert_rect_matches_electron(
            "home.albumCover",
            album_cover.absolute_position(),
            album_cover.size(),
            get_gt(&gt, "home", "albumCover"),
            0.5,
        );

        // --- 2. PlaylistOverlay (plus frame-by-frame slide-fade animation verification) ---
        queue_state.set_is_playlist_open(true);
        // At t = 0ms (just opened), offset-y is +20px (y = 348 + 20 = 368), opacity = 0.0
        let playlist_popup = find_first_element(&app, "PlayerBar::playlist-popup");
        assert!((playlist_popup.absolute_position().y - 368.0).abs() <= 0.5);

        // Step 100ms into the 400ms cubic-bezier(0.16, 1, 0.3, 1) transition
        mock_elapsed_time(Duration::from_millis(100));
        let bez_100 = eval_cubic_bezier(0.16, 1.0, 0.3, 1.0, 0.25) as f32;
        let expected_y_100 = 368.0 - 20.0 * bez_100;
        assert!(
            (playlist_popup.absolute_position().y - expected_y_100).abs() <= 1.0,
            "PlaylistOverlay y at t=100ms ({}) != expected bezier ({})",
            playlist_popup.absolute_position().y,
            expected_y_100
        );
        assert!(
            (playlist_popup.computed_opacity() - bez_100).abs() <= 0.03,
            "PlaylistOverlay opacity at t=100ms ({}) != expected bezier ({})",
            playlist_popup.computed_opacity(),
            bez_100
        );

        // Complete the 400ms enter transition
        mock_elapsed_time(Duration::from_millis(350));
        assert_rect_matches_electron(
            "playlistOverlay.card",
            playlist_popup.absolute_position(),
            playlist_popup.size(),
            get_gt(&gt, "playlistOverlay", "card"),
            0.5,
        );

        let pl_header = find_first_element(&app, "PlaylistOverlay::playlist-header");
        assert_rect_matches_electron(
            "playlistOverlay.header",
            pl_header.absolute_position(),
            pl_header.size(),
            get_gt(&gt, "playlistOverlay", "header"),
            0.5,
        );

        let pl_title = find_first_element(&app, "PlaylistOverlay::playlist-header-title");
        assert_rect_matches_electron(
            "playlistOverlay.title",
            pl_title.absolute_position(),
            pl_title.size(),
            get_gt(&gt, "playlistOverlay", "title"),
            0.5,
        );

        let pl_clear = find_first_element(&app, "PlaylistOverlay::playlist-clear-btn");
        assert_rect_matches_electron(
            "playlistOverlay.clearBtn",
            pl_clear.absolute_position(),
            pl_clear.size(),
            get_gt(&gt, "playlistOverlay", "clearBtn"),
            0.5,
        );

        let pl_normal_item = find_nth_element(&app, "PlaylistQueueRow::root", 1);
        assert_rect_matches_electron(
            "playlistOverlay.normalItem",
            pl_normal_item.absolute_position(),
            pl_normal_item.size(),
            get_gt(&gt, "playlistOverlay", "normalItem"),
            2.5,
        );

        // Verify the 3 bouncing equalizer bars in PlaylistOverlay (bounce 600ms alternate, delays 0ms / 200ms / 400ms, height 4px <-> 12px)
        let bar0 = find_first_element(&app, "PlaylistQueueRow::playing-bar-0");
        let bar1 = find_first_element(&app, "PlaylistQueueRow::playing-bar-1");
        let bar2 = find_first_element(&app, "PlaylistQueueRow::playing-bar-2");
        let h0_t0 = bar0.size().height;
        let h1_t0 = bar1.size().height;
        let h2_t0 = bar2.size().height;
        assert!((4.0..=12.0).contains(&h0_t0));
        assert!((4.0..=12.0).contains(&h1_t0));
        assert!((4.0..=12.0).contains(&h2_t0));
        // Because of the 200ms and 400ms phase offsets, the 3 bars have distinct heights at any instant
        assert!((h0_t0 - h1_t0).abs() > 1.0 || (h1_t0 - h2_t0).abs() > 1.0);
        // After 300ms (half bounce), bar0 moves by 4px
        mock_elapsed_time(Duration::from_millis(300));
        assert!((bar0.size().height - h0_t0).abs() > 2.5);

        queue_state.set_is_playlist_open(false);
        mock_elapsed_time(Duration::from_millis(450));

        // --- 3. VolumePopup on PlayerBar (plus 200ms ease transition verification) ---
        player_view_state.set_is_volume_popup_open(true);
        let vol_popup = find_first_element(&app, "PlayerBar::volume-popup");
        // The popup is anchored 14px above the actual volume button; its
        // opening animation starts 6px below that measured position.
        let volume_button = find_first_element(&app, "PlayerUtilityControls::volume-btn");
        let popup_y = volume_button.absolute_position().y - 14.0 - vol_popup.size().height;
        assert!((vol_popup.absolute_position().y - popup_y - 6.0).abs() <= 0.5);
        mock_elapsed_time(Duration::from_millis(100));
        let ease_50 = eval_css_ease(0.5) as f32;
        assert!((vol_popup.absolute_position().y - (popup_y + 6.0 - 6.0 * ease_50)).abs() <= 0.8);
        mock_elapsed_time(Duration::from_millis(150));

        assert!((vol_popup.absolute_position().y - popup_y).abs() <= 0.5);
        assert!(
            (vol_popup.absolute_position().x + vol_popup.size().width / 2.0
                - volume_button.absolute_position().x
                - volume_button.size().width / 2.0)
                .abs()
                <= 0.5
        );

        let vol_slider = find_first_element(&app, "VolumeRow::slider");
        crate::responsive_tests::assert_inside(&vol_popup, &vol_slider, "volumePopup.slider");

        let vol_value = find_first_element(&app, "VolumeRow::value-label");
        crate::responsive_tests::assert_inside(&vol_popup, &vol_value, "volumePopup.value");

        let vol_track = find_first_element(&app, "RangeSlider::track-rect");
        crate::responsive_tests::assert_inside(&vol_popup, &vol_track, "volumePopup.track");

        player_view_state.set_is_volume_popup_open(false);
        mock_elapsed_time(Duration::from_millis(250));

        // --- 4. AddToPlaylistModal ---
        song_menu_state.set_is_add_to_playlist_open(true);
        mock_elapsed_time(Duration::from_millis(50));

        let add_pl_card = find_first_element(&app, "AddToPlaylistModal::add-playlist-card");
        crate::responsive_tests::assert_inside(
            &app.root_element(),
            &add_pl_card,
            "addToPlaylistModal.card",
        );

        let add_pl_header = find_first_element(&app, "AddToPlaylistModal::add-playlist-header");
        crate::responsive_tests::assert_inside(
            &add_pl_card,
            &add_pl_header,
            "addToPlaylistModal.header",
        );

        let add_pl_close = find_first_element(&app, "PlaylistPickerHeader::add-playlist-close-btn");
        crate::responsive_tests::assert_inside(
            &add_pl_header,
            &add_pl_close,
            "addToPlaylistModal.closeBtn",
        );

        song_menu_state.set_is_add_to_playlist_open(false);
        mock_elapsed_time(Duration::from_millis(50));

        // --- 5. SearchView + Floating Bubble Header Transition ---
        navigation_state.set_current_route("search".into());
        search_state.set_search_is_floating(false);
        search_state.set_search_show_results(false);
        mock_elapsed_time(Duration::from_millis(450));

        let search_title = find_first_element(&app, "SearchView::search-page-title");
        assert_pos_size_matches_electron(
            "search.pageTitle",
            search_title.absolute_position(),
            search_title.size(),
            get_gt(&gt, "search", "pageTitle"),
            0.5,
            20.0,
            0.5,
        );

        let search_bar = find_first_element(&app, "SearchQueryBar::search-bar-wrapper");
        assert_rect_matches_electron(
            "search.searchBarWrapper",
            search_bar.absolute_position(),
            search_bar.size(),
            get_gt(&gt, "search", "searchBarWrapper"),
            0.5,
        );

        let cat_card = find_first_element(&app, "CategoryCard::root");
        let browse_heading = find_first_element(&app, "SearchCategories::browse-heading");
        assert!(
            (cat_card.absolute_position().y
                - browse_heading.absolute_position().y
                - browse_heading.size().height
                - 16.0)
                .abs()
                <= 0.5
        );
        crate::responsive_tests::assert_disjoint(&search_bar, &cat_card, "search heading and grid");

        let cat_deco = find_first_element(&app, "CategoryCard::card-decoration");
        assert!((cat_deco.size().width - 60.0).abs() < 0.5);
        assert!((cat_deco.size().height - 60.0).abs() < 0.5);
        assert!(
            (cat_deco.absolute_position().x + cat_deco.size().width
                - cat_card.absolute_position().x
                - cat_card.size().width
                - 10.0)
                .abs()
                < 0.5
        );

        // Trigger SearchView floating glass bubble animation (300ms title collapse + 400ms bar narrow/center)
        search_state.set_search_is_floating(true);
        mock_elapsed_time(Duration::from_millis(150));
        let ease_150_300 = eval_css_ease(150.0 / 300.0) as f32;
        assert!(
            (search_title.size().height - 54.39 * (1.0 - ease_150_300)).abs() <= 1.0,
            "search-page-title height at 150ms should follow 300ms ease curve"
        );
        mock_elapsed_time(Duration::from_millis(300));
        assert!((search_title.size().height - 0.0).abs() <= 0.1);
        assert!((search_bar.size().width - 500.0).abs() <= 0.5);
        search_state.set_search_is_floating(false);
        mock_elapsed_time(Duration::from_millis(450));

        // --- 6. PlaylistDetail + SongList + SongContextMenu ---
        navigation_state.set_current_route("playlist".into());
        // This legacy snapshot opens the menu without clicking a row. Supply its
        // captured anchor explicitly; actual button placement is tested separately.
        let menu_rect = get_gt(&gt, "playlistDetail", "contextMenu");
        song_menu_state.set_open_menu_anchor_right(menu_rect.x + menu_rect.width);
        song_menu_state.set_open_menu_anchor_top(menu_rect.y - 40.0);
        song_menu_state.set_open_menu_anchor_bottom(menu_rect.y - 8.0);
        song_menu_state.set_open_menu_song_id(1);
        song_menu_state.set_open_menu_show_remove(true);
        song_menu_state.set_open_menu_submenu(false);
        mock_elapsed_time(Duration::from_millis(100));

        let detail_container = find_first_element(&app, "MediaDetailView::detail-container");
        assert_pos_size_matches_electron(
            "playlistDetail.container",
            detail_container.absolute_position(),
            detail_container.size(),
            get_gt(&gt, "playlistDetail", "container"),
            0.5,
            0.5,
            200.0,
        );

        let detail_header = find_first_element(&app, "MediaDetailView::detail-header");
        crate::responsive_tests::assert_inside(
            &detail_container,
            &detail_header,
            "playlistDetail.header",
        );

        let detail_cover = find_first_element(&app, "MediaDetailHeader::detail-cover-wrapper");
        crate::responsive_tests::assert_inside(
            &detail_header,
            &detail_cover,
            "playlistDetail.coverWrapper",
        );

        let detail_title = find_first_element(&app, "MediaDetailHeader::detail-title");
        crate::responsive_tests::assert_inside(
            &detail_header,
            &detail_title,
            "playlistDetail.title",
        );

        let detail_play_btn = find_first_element(&app, "MediaDetailActions::detail-play-main-btn");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &detail_play_btn,
            "playlistDetail.playMainBtn",
        );

        let detail_search_box = find_first_element(&app, "MediaDetailActions::detail-search-box");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &detail_search_box,
            "playlistDetail.searchBox",
        );

        let detail_sec_btn = find_first_element(&app, "MediaDetailActions::detail-secondary-btn");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &detail_sec_btn,
            "playlistDetail.secondaryBtn",
        );

        let detail_more_btn =
            find_first_element(&app, "MediaDetailActions::playlist-actions-trigger");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &detail_more_btn,
            "playlistDetail.moreBtn",
        );

        let list_header = find_first_element(&app, "SongList::list-header");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailView::song-list"),
            &list_header,
            "playlistDetail.listHeader",
        );

        let track_row = find_first_element(&app, "SongTrackRow::root");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailView::song-list"),
            &track_row,
            "playlistDetail.trackRow",
        );

        let mini_cover = find_first_element(&app, "SongTrackRow::mini-cover");
        crate::responsive_tests::assert_inside(&track_row, &mini_cover, "playlistDetail.miniCover");

        let ctx_menu = find_first_element(&app, "SongContextMenuOverlay::context-menu-card");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "SongContextMenuOverlay::root"),
            &ctx_menu,
            "playlistDetail.contextMenu",
        );

        let next_item = find_first_element(&app, "SongContextMenuOverlay::context-menu-item-next");
        let ctx_item = find_first_element(&app, "SongMenuAccountActions::context-menu-item-add");
        for item in [&next_item, &ctx_item] {
            crate::responsive_tests::assert_inside(
                &ctx_menu,
                item,
                "playlistDetail.contextMenuItem",
            );
            assert!(item.size().height >= 30.0);
        }
        assert!(
            (next_item.absolute_position().y - ctx_menu.absolute_position().y - 7.0).abs() < 0.1
        );
        assert!(
            next_item.absolute_position().y + next_item.size().height
                <= ctx_item.absolute_position().y + 0.1
        );

        song_menu_state.set_open_menu_song_id(-1);

        // --- 7. LocalMusicView + AppDialog (with 180ms ease fade verification) ---
        navigation_state.set_current_route("local-music".into());
        dialog_state.set_is_dialog_open(true);
        let dialog_container = find_first_element(&app, "AppWindow::dialog-modal-container");
        assert!((dialog_container.computed_opacity() - 0.0).abs() <= 0.01);
        mock_elapsed_time(Duration::from_millis(90));
        let ease_90_180 = eval_css_ease(0.5) as f32;
        assert!((dialog_container.computed_opacity() - ease_90_180).abs() <= 0.04);
        mock_elapsed_time(Duration::from_millis(120));
        assert!((dialog_container.computed_opacity() - 1.0).abs() <= 0.01);

        let local_import = find_first_element(&app, "LocalLibraryActions::local-import-btn");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &local_import,
            "localMusic.importBtn",
        );

        let local_clear = find_first_element(&app, "LocalLibraryActions::local-clear-btn");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "MediaDetailHeader::detail-action-bar"),
            &local_clear,
            "localMusic.clearIconBtn",
        );

        let app_dialog = find_first_element(&app, "AppDialogModal::app-dialog");
        assert_rect_matches_electron(
            "localMusic.appDialog",
            app_dialog.absolute_position(),
            app_dialog.size(),
            get_gt(&gt, "localMusic", "appDialog"),
            0.5,
        );

        let dlg_primary = find_first_element(&app, "DialogContent::dialog-primary-btn");
        assert_rect_matches_electron(
            "localMusic.dialogPrimaryBtn",
            dlg_primary.absolute_position(),
            dlg_primary.size(),
            get_gt(&gt, "localMusic", "dialogPrimaryBtn"),
            0.5,
        );

        let dlg_secondary = find_first_element(&app, "DialogContent::dialog-secondary-btn");
        assert_rect_matches_electron(
            "localMusic.dialogSecondaryBtn",
            dlg_secondary.absolute_position(),
            dlg_secondary.size(),
            get_gt(&gt, "localMusic", "dialogSecondaryBtn"),
            0.5,
        );

        dialog_state.set_is_dialog_open(false);
        mock_elapsed_time(Duration::from_millis(200));

        // Artist geometry is checked against current Electron fixtures in runtime::artist_tests.

        // --- 9. Fullscreen Player + LyricPanel + CommentPanel (plus 350ms player-fade & lyric transitions) ---
        player_view_state.set_is_comment_open(false);
        player_view_state.set_is_fullscreen(true);
        let fs_container = find_first_element(&app, "AppWindow::fullscreen-container");
        assert!((fs_container.absolute_position().y - 20.0).abs() <= 0.5);
        assert!((fs_container.computed_opacity() - 0.0).abs() <= 0.01);

        // Halfway through 350ms player-fade
        mock_elapsed_time(Duration::from_millis(175));
        let ease_175_350 = eval_css_ease(0.5) as f32;
        assert!((fs_container.absolute_position().y - 20.0 * (1.0 - ease_175_350)).abs() <= 0.8);
        assert!((fs_container.computed_opacity() - ease_175_350).abs() <= 0.03);

        // Complete 350ms player-fade
        mock_elapsed_time(Duration::from_millis(200));
        assert!((fs_container.absolute_position().y - 0.0).abs() <= 0.1);
        assert!((fs_container.computed_opacity() - 1.0).abs() <= 0.01);

        let fs_back = find_first_element(&app, "PlayerFullscreen::fullscreen-back-btn");
        assert_rect_matches_electron(
            "fullscreen.backBtn",
            fs_back.absolute_position(),
            fs_back.size(),
            get_gt(&gt, "fullscreen", "backBtn"),
            0.5,
        );

        let fs_layout = find_first_element(&app, "PlayerFullscreen::fullscreen-main-layout");
        assert_rect_matches_electron(
            "fullscreen.mainLayout",
            fs_layout.absolute_position(),
            fs_layout.size(),
            get_gt(&gt, "fullscreen", "mainLayout"),
            0.5,
        );

        let fs_cover = find_first_element(&app, "FullscreenVisualPanel::fullscreen-cover-wrapper");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "PlayerFullscreen::visual-panel"),
            &fs_cover,
            "fullscreen.coverWrapper",
        );

        let fs_title = find_first_element(&app, "FullscreenTrackMetadata::fullscreen-track-title");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "PlayerFullscreen::visual-panel"),
            &fs_title,
            "fullscreen.trackTitle",
        );

        let fs_prog = find_first_element(&app, "FullscreenProgress::fullscreen-progress-bar");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "FullscreenVisualPanel::playback-controls"),
            &fs_prog,
            "fullscreen.progressBarBg",
        );

        let fs_play = find_first_element(&app, "FullscreenTransport::fullscreen-play-btn");
        crate::responsive_tests::assert_inside(
            &find_first_element(&app, "FullscreenVisualPanel::playback-controls"),
            &fs_play,
            "fullscreen.playMainBtn",
        );

        let lrc_toggle = find_first_element(&app, "LyricToolbar::lyric-toggle-pronunciation");
        assert_rect_matches_electron(
            "fullscreen.lyricToggle",
            lrc_toggle.absolute_position(),
            lrc_toggle.size(),
            get_gt(&gt, "fullscreen", "lyricToggle"),
            0.5,
        );

        // Switch to CommentPanel
        player_view_state.set_is_comment_open(true);
        mock_elapsed_time(Duration::from_millis(50));
        let cmt_hot = find_first_element(&app, "CommentRow::comment-section-hot");
        assert_pos_size_matches_electron(
            "commentPanel.sectionLabel",
            cmt_hot.absolute_position(),
            cmt_hot.size(),
            get_gt(&gt, "commentPanel", "sectionLabel"),
            0.5,
            0.5,
            0.5,
        );

        player_view_state.set_is_fullscreen(false);
        mock_elapsed_time(Duration::from_millis(400));

        // --- 10. LoginModal (plus 300ms tab slide & 600ms 3D flip animation verification) ---
        login_state.set_is_login_open(true);
        login_state.set_login_mode("form".into());
        login_state.set_login_tab("password".into());
        mock_elapsed_time(Duration::from_millis(350));

        let login_card = find_first_element(&app, "LoginModal::login-card");
        assert_rect_matches_electron(
            "loginModal.card",
            login_card.absolute_position(),
            login_card.size(),
            get_gt(&gt, "loginModal", "card"),
            0.5,
        );

        let login_qr_btn = find_first_element(&app, "LoginForm::login-qr-btn");
        assert_rect_matches_electron(
            "loginModal.qrBtn",
            login_qr_btn.absolute_position(),
            login_qr_btn.size(),
            get_gt(&gt, "loginModal", "qrBtn"),
            0.5,
        );

        let login_close_btn = find_first_element(&app, "LoginForm::login-close-btn");
        assert_rect_matches_electron(
            "loginModal.closeBtn",
            login_close_btn.absolute_position(),
            login_close_btn.size(),
            get_gt(&gt, "loginModal", "closeBtn"),
            0.5,
        );

        let login_seg = find_first_element(&app, "LoginForm::login-segmented-control");
        assert_rect_matches_electron(
            "loginModal.segmentedControl",
            login_seg.absolute_position(),
            login_seg.size(),
            get_gt(&gt, "loginModal", "segmentedControl"),
            0.5,
        );

        let login_seg_bg = find_first_element(&app, "LoginMethodTabs::login-segment-bg");
        assert_rect_matches_electron(
            "loginModal.segmentBg",
            login_seg_bg.absolute_position(),
            login_seg_bg.size(),
            get_gt(&gt, "loginModal", "segmentBg"),
            0.5,
        );

        // Animate tab switch password -> phone (300ms cubic-bezier(0.4, 0, 0.2, 1))
        login_state.set_login_tab("phone".into());
        mock_elapsed_time(Duration::from_millis(150));
        let bez_tab_150 = eval_cubic_bezier(0.4, 0.0, 0.2, 1.0, 0.5) as f32;
        let expected_seg_x = 554.5 + 163.0 * bez_tab_150;
        assert!(
            (login_seg_bg.absolute_position().x - expected_seg_x).abs() <= 1.0,
            "login-segment-bg x at 150ms ({}) != expected bezier ({})",
            login_seg_bg.absolute_position().x,
            expected_seg_x
        );
        mock_elapsed_time(Duration::from_millis(200));
        assert!((login_seg_bg.absolute_position().x - 717.5).abs() <= 0.5);
        login_state.set_login_tab("password".into());
        mock_elapsed_time(Duration::from_millis(350));

        let login_phone = find_first_element(&app, "LoginForm::login-phone-container");
        assert_rect_matches_electron(
            "loginModal.phoneInputContainer",
            login_phone.absolute_position(),
            login_phone.size(),
            get_gt(&gt, "loginModal", "phoneInputContainer"),
            0.5,
        );

        let login_input = find_first_element(&app, "LoginForm::login-input-field");
        assert_rect_matches_electron(
            "loginModal.inputField",
            login_input.absolute_position(),
            login_input.size(),
            get_gt(&gt, "loginModal", "inputField"),
            0.5,
        );

        let login_check = find_first_element(&app, "LoginRememberChoice::login-checkmark");
        assert_rect_matches_electron(
            "loginModal.checkmark",
            login_check.absolute_position(),
            login_check.size(),
            get_gt(&gt, "loginModal", "checkmark"),
            0.5,
        );

        let login_submit = find_first_element(&app, "LoginForm::login-submit-btn");
        assert_rect_matches_electron(
            "loginModal.submitBtn",
            login_submit.absolute_position(),
            login_submit.size(),
            get_gt(&gt, "loginModal", "submitBtn"),
            0.5,
        );

        // Animate 600ms 3D flip to QR back face (cubic-bezier(0.4, 0, 0.2, 1))
        login_state.set_login_mode("qr".into());
        mock_elapsed_time(Duration::ZERO);
        mock_elapsed_time(Duration::ZERO);
        mock_elapsed_time(Duration::from_millis(240));
        // Layout stays fixed while the complete card is projected as one surface.
        assert_eq!(login_card.size().width, 400.0);
        assert_eq!(
            ElementHandle::find_by_element_id(&app, "LoginModal::flip-strip").count(),
            296
        );
        mock_elapsed_time(Duration::from_millis(400));
        assert!((login_card.size().width - 400.0).abs() <= 0.5);

        let login_qr_box = find_first_element(&app, "LoginQr::login-qr-box");
        crate::responsive_tests::assert_inside(&login_card, &login_qr_box, "loginModal.qrBox");

        login_state.set_is_login_open(false);
        mock_elapsed_time(Duration::from_millis(350));

        // --- 11. SettingsModal (plus 500ms appleScaleUp, 400ms navIndicator slide, 300ms ModernSwitch, 400ms SegmentedSlider) ---
        // Preserve the historical 1435×1000 settings geometry reference in its own window.
        let settings = SettingsWindow::new().unwrap();
        settings.window().set_size(PhysicalSize::new(1435, 1000));
        settings.show().unwrap();
        let settings_state_settings_state = settings.global::<crate::SettingsState>();
        let settings_theme = settings.global::<Theme>();
        let _ = settings.root_element().size();
        settings_state_settings_state.set_settings_tab("general".into());
        let settings_container =
            find_first_element(&settings, "SettingsWindow::settings-modal-container");
        assert!((settings_container.absolute_position().y - 16.0).abs() <= 0.5);
        settings.set_presented(true);
        mock_elapsed_time(Duration::from_millis(250));
        let bez_apple_250 = eval_cubic_bezier(0.2, 0.8, 0.2, 1.0, 0.5) as f32;
        assert!(
            (settings_container.absolute_position().y - 16.0 * (1.0 - bez_apple_250)).abs() <= 0.8
        );
        mock_elapsed_time(Duration::from_millis(300));

        let settings_win = find_first_element(&settings, "SettingsModal::settings-window");
        assert_rect_matches_electron(
            "settings.window",
            settings_win.absolute_position(),
            settings_win.size(),
            get_gt(&gt, "settings", "window"),
            0.5,
        );

        let settings_sidebar = find_first_element(&settings, "SettingsModal::settings-sidebar");
        assert_rect_matches_electron(
            "settings.sidebar",
            settings_sidebar.absolute_position(),
            settings_sidebar.size(),
            get_gt(&gt, "settings", "sidebar"),
            0.5,
        );

        let settings_ind = find_first_element(&settings, "SettingsSidebar::settings-nav-indicator");
        assert_rect_matches_electron(
            "settings.navIndicator",
            settings_ind.absolute_position(),
            settings_ind.size(),
            get_gt(&gt, "settings", "navIndicator"),
            0.5,
        );

        let settings_nav_0 = find_first_element(&settings, "SettingsSidebar::settings-nav-general");
        assert_rect_matches_electron(
            "settings.navItem",
            settings_nav_0.absolute_position(),
            settings_nav_0.size(),
            get_gt(&gt, "settings", "navItem"),
            0.5,
        );

        let settings_header = find_first_element(&settings, "SettingsMain::settings-main-header");
        assert_rect_matches_electron(
            "settings.mainHeader",
            settings_header.absolute_position(),
            settings_header.size(),
            get_gt(&gt, "settings", "mainHeader"),
            0.5,
        );

        let settings_h2 = find_first_element(&settings, "SettingsHeader::settings-header-h2");
        crate::responsive_tests::assert_inside(
            &settings_header,
            &settings_h2,
            "settings header title",
        );

        let settings_close = find_first_element(&settings, "SettingsHeader::settings-close-btn");
        assert_rect_matches_electron(
            "settings.closeBtn",
            settings_close.absolute_position(),
            settings_close.size(),
            get_gt(&gt, "settings", "closeBtn"),
            0.5,
        );

        let settings_scroll =
            find_first_element(&settings, "SettingsMain::settings-content-scroll");
        assert_rect_matches_electron(
            "settings.contentScroll",
            settings_scroll.absolute_position(),
            settings_scroll.size(),
            get_gt(&gt, "settings", "contentScroll"),
            0.5,
        );

        let control_card = find_first_element(&settings, "SettingGroupCard::control-card-box");
        crate::responsive_tests::assert_inside(
            &settings_scroll,
            &control_card,
            "settings.controlCard",
        );

        let control_row = find_first_element(&settings, "SettingRowItem::control-row-box");
        crate::responsive_tests::assert_inside(&control_card, &control_row, "settings.controlRow");

        let modern_sw =
            find_first_element(&settings, "GeneralSettingsTab::modern-switch-auto-launch");
        crate::responsive_tests::assert_inside(&control_row, &modern_sw, "settings.modernSwitch");

        // Verify ModernSwitch knob animation (300ms cubic-bezier(0.2, 1, 0.3, 1), x: 2px -> 22px)
        let sw_knob = find_first_element(&settings, "ModernSwitch::knob");
        let knob_x0 = sw_knob.absolute_position().x;
        settings_state_settings_state.set_setting_auto_launch(true);
        mock_elapsed_time(Duration::from_millis(75));
        let bez_sw_75 = eval_cubic_bezier(0.2, 1.0, 0.3, 1.0, 75.0 / 300.0) as f32;
        assert!(
            (sw_knob.absolute_position().x - (knob_x0 + 20.0 * bez_sw_75)).abs() <= 0.8,
            "ModernSwitch knob at 75ms ({}) != expected bezier ({})",
            sw_knob.absolute_position().x,
            knob_x0 + 20.0 * bez_sw_75
        );
        mock_elapsed_time(Duration::from_millis(250));
        assert!((sw_knob.absolute_position().x - (knob_x0 + 20.0)).abs() <= 0.5);

        // Verify SettingsSidebar nav-indicator animation (400ms cubic-bezier(0.2, 1, 0.3, 1), step 52px per tab)
        // Switch from general (idx 0, y=82.59) to appearance (idx 2, y=82.59 + 104 = 186.59)
        settings_state_settings_state.set_settings_tab("appearance".into());
        mock_elapsed_time(Duration::from_millis(100));
        let bez_ind_100 = eval_cubic_bezier(0.2, 1.0, 0.3, 1.0, 100.0 / 400.0) as f32;
        let expected_ind_y = 82.59 + 104.0 * bez_ind_100;
        assert!(
            (settings_ind.absolute_position().y - expected_ind_y).abs() <= 1.0,
            "Settings navIndicator y at 100ms ({}) != expected bezier ({})",
            settings_ind.absolute_position().y,
            expected_ind_y
        );
        mock_elapsed_time(Duration::from_millis(350));
        assert!((settings_ind.absolute_position().y - 186.59).abs() <= 0.5);

        // Verify SegmentedSlider3 spring animation (400ms cubic-bezier(0.18, 0.89, 0.32, 1.1))
        let seg_bg = find_first_element(&settings, "SegmentedSlider3::slider-bg");
        let seg_x0 = seg_bg.absolute_position().x;
        let seg_step = seg_bg.size().width + 8.0;
        settings_theme.set_theme_mode("dark".into());

        // At t = 200ms (50% duration):
        mock_elapsed_time(Duration::from_millis(200));
        let bez_spring_200 = eval_cubic_bezier(0.18, 0.89, 0.32, 1.1, 0.5) as f32;
        assert!(
            (seg_bg.absolute_position().x - (seg_x0 + seg_step * bez_spring_200)).abs() <= 1.5,
            "SegmentedSlider3 spring x at 200ms ({}) != expected spring bezier ({})",
            seg_bg.absolute_position().x,
            seg_x0 + seg_step * bez_spring_200
        );

        // At t = 300ms (75% duration), the spring curve overshoots 1.0 (y ≈ 1.021)!
        mock_elapsed_time(Duration::from_millis(100));
        let bez_spring_300 = eval_cubic_bezier(0.18, 0.89, 0.32, 1.1, 0.75) as f32;
        assert!(
            bez_spring_300 > 1.0,
            "Spring cubic-bezier should overshoot 1.0 at t=75%"
        );
        assert!(
            seg_bg.absolute_position().x > seg_x0 + seg_step,
            "SegmentedSlider3 slider-bg should physically overshoot target x at t=300ms"
        );
        assert!(
            (seg_bg.absolute_position().x - (seg_x0 + seg_step * bez_spring_300)).abs() <= 1.5,
            "SegmentedSlider3 spring x at 300ms ({}) != expected spring bezier ({})",
            seg_bg.absolute_position().x,
            seg_x0 + seg_step * bez_spring_300
        );

        // At t = 400ms (100% duration), settles back to exact target:
        mock_elapsed_time(Duration::from_millis(150));
        assert!((seg_bg.absolute_position().x - (seg_x0 + seg_step)).abs() <= 0.5);

        settings.set_presented(false);
        settings.hide().unwrap();
        mock_elapsed_time(Duration::from_millis(550));

        // --- 12. Additional Frame-by-Frame Hover & Transition Curve Verifications ---
        navigation_state.set_current_route("home".into());
        mock_elapsed_time(Duration::from_millis(50));

        // 12a. SideNavBar .profile-bubble hover (400ms cubic-bezier(0.165, 0.84, 0.44, 1): height 56 -> 200.4, lift -5px)
        let profile_bubble = find_first_element(&app, "SideNavBar::profile-bubble");
        assert!((profile_bubble.size().height - 56.0).abs() <= 0.5);
        assert!((profile_bubble.absolute_position().y - 915.0).abs() <= 0.5);
        // Move pointer over profile-bubble to trigger real hover
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(120.0, 940.0),
            });
        mock_elapsed_time(Duration::from_millis(100));
        let menu_ease_100 = eval_css_ease(100.0 / 400.0) as f32;
        assert!(
            (profile_bubble.size().height
                - (56.0 + (200.0 * menu_ease_100).min(136.4 + 8.0 * menu_ease_100)))
            .abs()
                <= 1.5,
            "profile-bubble height at 100ms ({}) != expected bezier ({})",
            profile_bubble.size().height,
            56.0 + (200.0 * menu_ease_100).min(136.4 + 8.0 * menu_ease_100)
        );
        mock_elapsed_time(Duration::from_millis(350));
        assert!((profile_bubble.size().height - 200.4).abs() <= 0.5);
        // Move pointer away to collapse
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(10.0, 10.0),
            });
        mock_elapsed_time(Duration::from_millis(450));
        assert!((profile_bubble.size().height - 56.0).abs() <= 0.5);

        // 12b. MediaCard hover (250ms ease: card translateY(0 -> -4px), play-btn translateY(6px -> 0) & opacity(0 -> 1))
        let media_card = find_first_element(&app, "MediaCard::card-surface");
        let media_play_btn = find_first_element(&app, "MediaCard::play-btn-circle");
        let mc_y0 = media_card.absolute_position().y;
        assert!((media_play_btn.computed_opacity() - 0.0).abs() <= 0.01);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(
                    media_card.absolute_position().x + 100.0,
                    mc_y0 + 100.0,
                ),
            });
        mock_elapsed_time(Duration::from_millis(125));
        let ease_50 = eval_css_ease(0.5) as f32;
        assert!(
            (media_card.absolute_position().y - (mc_y0 - 4.0 * ease_50)).abs() <= 0.6,
            "MediaCard y at 125ms should follow 250ms ease lift (-4px)"
        );
        assert!(
            (media_play_btn.computed_opacity() - ease_50).abs() <= 0.04,
            "MediaCard play-btn opacity at 125ms should follow 250ms ease"
        );
        mock_elapsed_time(Duration::from_millis(150));
        assert!((media_card.absolute_position().y - (mc_y0 - 4.0)).abs() <= 0.2);
        assert!((media_play_btn.computed_opacity() - 1.0).abs() <= 0.01);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(10.0, 10.0),
            });
        mock_elapsed_time(Duration::from_millis(300));

        // 12c. AlbumItem hover (300ms ease: translateY(0 -> -4px), 350ms ease: img scale(1.0 -> 1.06))
        let album_cov = find_first_element(&app, "AlbumItem::album-cover");
        let album_img = find_first_element(&app, "AlbumItem::album-cover-img");
        let ac_y0 = album_cov.absolute_position().y;
        assert!((album_img.size().width - 176.0).abs() <= 0.2);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(
                    album_cov.absolute_position().x + 80.0,
                    ac_y0 + 70.0,
                ),
            });
        mock_elapsed_time(Duration::from_millis(150));
        assert!(
            (album_cov.absolute_position().y - (ac_y0 - 4.0 * eval_css_ease(0.5) as f32)).abs()
                <= 0.6,
            "AlbumItem y at 150ms should follow 300ms ease lift (-4px)"
        );
        mock_elapsed_time(Duration::from_millis(220));
        assert!((album_cov.absolute_position().y - (ac_y0 - 4.0)).abs() <= 0.2);
        assert!((album_img.size().width - 176.0 * 1.06).abs() <= 0.5);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(10.0, 10.0),
            });
        mock_elapsed_time(Duration::from_millis(400));

        // 12d. HomeView skeleton sweep animation (@keyframes skeleton-sweep 1.4s infinite)
        home_state.set_home_playlists_loading(true);
        mock_elapsed_time(Duration::from_millis(50));
        let sweep_bar = find_first_element(&app, "SkeletonSweepOverlay::sweep-bar");
        let sweep_x0 = sweep_bar.absolute_position().x;
        mock_elapsed_time(Duration::from_millis(350)); // 25% of 1400ms period -> moves by 100px
        let sweep_x1 = sweep_bar.absolute_position().x;
        assert!(
            (sweep_x1 - sweep_x0).abs() > 20.0,
            "Skeleton sweep bar must continuously animate across the 200px card"
        );
        home_state.set_home_playlists_loading(false);
        mock_elapsed_time(Duration::from_millis(50));

        // 12e. PlayerBar buffered-bar 250ms ease width transition
        let buf_bar = find_first_element(&app, "PlayerTransportControls::buffered-bar");
        let track_w = find_first_element(&app, "PlayerTransportControls::progress-track")
            .size()
            .width;
        let w_start = buf_bar.size().width;
        player_state.set_buffered_percent(90.0);
        let w_end = track_w * 0.90;
        mock_elapsed_time(Duration::from_millis(125));
        assert!(
            (buf_bar.size().width - (w_start + (w_end - w_start) * ease_50)).abs() <= 1.0,
            "PlayerBar buffered-bar width at 125ms should follow 250ms ease curve"
        );
        mock_elapsed_time(Duration::from_millis(150));
        assert!((buf_bar.size().width - w_end).abs() <= 0.5);

        // 12f. Fullscreen 800ms cubic-bezier(0.2, 0, 0.2, 1) playing cover scale & LyricPanel 150ms/350ms cubic-bezier(0.23, 1, 0.32, 1) line activation
        player_view_state.set_is_comment_open(false);
        player_view_state.set_is_fullscreen(true);
        mock_elapsed_time(Duration::from_millis(400));

        let fs_scaled_cover =
            find_first_element(&app, "FullscreenVisualPanel::fullscreen-cover-scaled");
        assert!((fs_scaled_cover.size().width - 459.0).abs() <= 0.5); // 450 * 1.02 = 459px when playing
        player_state.set_is_playing(false);
        mock_elapsed_time(Duration::from_millis(200)); // 25% of 800ms cubic-bezier(0.2, 0, 0.2, 1)
        let bez_cov_200 = eval_cubic_bezier(0.2, 0.0, 0.2, 1.0, 200.0 / 800.0) as f32;
        assert!(
            (fs_scaled_cover.size().width - (459.0 - 9.0 * bez_cov_200)).abs() <= 0.8,
            "Fullscreen cover scale at 200ms should follow 800ms cubic-bezier(0.2, 0, 0.2, 1)"
        );
        mock_elapsed_time(Duration::from_millis(650));
        assert!((fs_scaled_cover.size().width - 450.0).abs() <= 0.5);
        player_state.set_is_playing(true);
        mock_elapsed_time(Duration::from_millis(850));

        // Electron scales the painted contents; activation must not grow the layout row.
        let lrc_item_2 = find_nth_element(&app, "LyricLineList::lyric-line-item", 2);
        assert!((lrc_item_2.computed_opacity() - 0.30).abs() <= 0.02);
        assert!((lrc_item_2.size().height - 106.23).abs() <= 0.5);
        player_view_state.set_active_lyric_index(2);
        mock_elapsed_time(Duration::from_millis(75)); // 50% of 150ms opacity
        let bez_lrc_op_75 = eval_cubic_bezier(0.23, 1.0, 0.32, 1.0, 75.0 / 150.0) as f32;
        assert!(
            (lrc_item_2.computed_opacity() - (0.30 + 0.70 * bez_lrc_op_75)).abs() <= 0.04,
            "Lyric line opacity at 75ms should follow 150ms cubic-bezier(0.23, 1, 0.32, 1)"
        );
        assert!(
            (lrc_item_2.size().height - 106.23).abs() <= 0.5,
            "Lyric activation must keep the layout height fixed"
        );
        mock_elapsed_time(Duration::from_millis(300));
        assert!((lrc_item_2.computed_opacity() - 1.0).abs() <= 0.01);
        assert!((lrc_item_2.size().height - 106.23).abs() <= 0.5);

        // LyricSeekIndicator 200ms ease fade, entered through a real wheel event.
        player_state.set_is_loading(false);
        for _ in 0..30 {
            let _ = app.root_element().size();
            mock_elapsed_time(Duration::from_millis(16));
        }
        let lyric_scroll = find_first_element(&app, "LyricPanelView::lyric-scroll");
        let lyric_pointer = slint::LogicalPosition::new(
            lyric_scroll.absolute_position().x + lyric_scroll.size().width / 2.0,
            lyric_scroll.absolute_position().y + lyric_scroll.size().height / 2.0,
        );
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: lyric_pointer,
                delta_x: 0.0,
                delta_y: -32.0,
            });
        let seek_ind = find_first_element(&app, "LyricPanelView::lyric-seek-indicator");
        assert!((seek_ind.computed_opacity() - 0.0).abs() <= 0.02);
        mock_elapsed_time(Duration::from_millis(100));
        assert!((seek_ind.computed_opacity() - eval_css_ease(0.5) as f32).abs() <= 0.04);
        mock_elapsed_time(Duration::from_millis(120));
        assert!((seek_ind.computed_opacity() - 1.0).abs() <= 0.02);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                position: lyric_pointer,
                button: slint::platform::PointerEventButton::Right,
            });
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                position: lyric_pointer,
                button: slint::platform::PointerEventButton::Right,
            });
        mock_elapsed_time(Duration::from_millis(220));

        // Regression check for commit 4f598dc: Fullscreen play button stays centered at x=337.75 regardless of side toggle visibility
        let fs_play = find_first_element(&app, "FullscreenTransport::fullscreen-play-btn");
        assert!((fs_play.absolute_position().x - 337.75).abs() <= 0.5);
        audio_state.set_setting_software_volume(false);
        mock_elapsed_time(Duration::from_millis(50));
        assert!((fs_play.absolute_position().x - 337.75).abs() <= 0.5);
        audio_state.set_setting_software_volume(true);
        mock_elapsed_time(Duration::from_millis(50));

        player_view_state.set_is_fullscreen(false);
        mock_elapsed_time(Duration::from_millis(400));

        // =========================================================================
        // PART 2: RESPONSIVE BREAKPOINT PARITY (@media max-width: 1000px)
        // =========================================================================
        navigation_state.set_current_route("home".into());
        app.window().set_size(PhysicalSize::new(950, 800));
        navigation_state.set_window_width(950.0);
        navigation_state.set_window_height(800.0);
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(50));

        let gt_w950 = gt.responsive.get("w950").unwrap();
        let sidebar_950 = find_first_element(&app, "SideNavBar::sidebar-floating");
        assert_rect_matches_electron(
            "responsive.w950.sidebar",
            sidebar_950.absolute_position(),
            sidebar_950.size(),
            &gt_w950.get("sidebar").unwrap().as_ref().unwrap().rect,
            0.5,
        );

        let player_bar_950 = find_first_element(&app, "PlayerBar::player-bar-panel");
        assert_rect_matches_electron(
            "responsive.w950.playerBar",
            player_bar_950.absolute_position(),
            player_bar_950.size(),
            &gt_w950.get("playerBar").unwrap().as_ref().unwrap().rect,
            0.5,
        );

        // Also verify @media (max-width: 650px) at 620x720 against Electron ground truth
        app.window().set_size(PhysicalSize::new(620, 720));
        navigation_state.set_window_width(620.0);
        navigation_state.set_window_height(720.0);
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(50));

        let gt_w620 = gt.responsive.get("w620").unwrap();
        let sidebar_620 = find_first_element(&app, "SideNavBar::sidebar-floating");
        assert_rect_matches_electron(
            "responsive.w620.sidebar",
            sidebar_620.absolute_position(),
            sidebar_620.size(),
            &gt_w620.get("sidebar").unwrap().as_ref().unwrap().rect,
            0.5,
        );

        let player_bar_620 = find_first_element(&app, "PlayerBar::player-bar-panel");
        assert_rect_matches_electron(
            "responsive.w620.playerBar",
            player_bar_620.absolute_position(),
            player_bar_620.size(),
            &gt_w620.get("playerBar").unwrap().as_ref().unwrap().rect,
            0.5,
        );

        // Restore 1435x1000 and verify multi-frame animation curves directly against Chromium's sampled DOMMatrix values
        app.window().set_size(PhysicalSize::new(1435, 1000));
        navigation_state.set_window_width(1435.0);
        navigation_state.set_window_height(1000.0);
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(500));

        // 1. Compare PlaylistOverlay 400ms cubic-bezier(0.16, 1, 0.3, 1) against Chromium's 5 sampled frames [0ms, 100ms, 200ms, 300ms, 400ms]
        let gt_pl_y = gt.animations.get("playlistOverlayEnterY").unwrap();
        let gt_pl_op = gt.animations.get("playlistOverlayEnterOpacity").unwrap();
        assert_eq!(gt_pl_y.duration_ms, 400);
        queue_state.set_is_playlist_open(true);
        let pl_popup = find_first_element(&app, "PlayerBar::playlist-popup");
        let mut prev_t = 0u64;
        for (sample_y, sample_op) in gt_pl_y.samples.iter().zip(gt_pl_op.samples.iter()) {
            let dt = sample_y.t_ms - prev_t;
            if dt > 0 {
                mock_elapsed_time(Duration::from_millis(dt));
            }
            prev_t = sample_y.t_ms;
            assert!(
                (pl_popup.absolute_position().y - sample_y.value).abs() <= 1.0,
                "PlaylistOverlay y at t={}ms: Slint {:.2} != Chromium {:.2}",
                sample_y.t_ms,
                pl_popup.absolute_position().y,
                sample_y.value
            );
            assert!(
                (pl_popup.computed_opacity() - sample_op.value).abs() <= 0.03,
                "PlaylistOverlay opacity at t={}ms: Slint {:.3} != Chromium {:.3}",
                sample_op.t_ms,
                pl_popup.computed_opacity(),
                sample_op.value
            );
        }
        queue_state.set_is_playlist_open(false);
        mock_elapsed_time(Duration::from_millis(450));

        // 2. Album search still expands 220px -> 260px. Playlist search now uses
        // a flex width capped at 240px, verified against the new capture separately.
        navigation_state.set_current_route("album".into());
        mock_elapsed_time(Duration::from_millis(50));
        let gt_detail_search = gt.animations.get("detailSearchFocusWidth").unwrap();
        assert_eq!(gt_detail_search.duration_ms, 300);
        let detail_search = find_first_element(&app, "MediaDetailActions::detail-search-box");
        // Focus the search input inside detail-search-box by clicking its center
        let search_input_el = find_first_element(&app, "DetailSearchField::detail-search-input");
        let center_pos = LogicalPosition::new(
            search_input_el.absolute_position().x + 20.0,
            search_input_el.absolute_position().y + 20.0,
        );
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                position: center_pos,
                button: slint::platform::PointerEventButton::Left,
            });
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                position: center_pos,
                button: slint::platform::PointerEventButton::Left,
            });
        prev_t = 0;
        for sample in &gt_detail_search.samples {
            let dt = sample.t_ms - prev_t;
            if dt > 0 {
                mock_elapsed_time(Duration::from_millis(dt));
            }
            prev_t = sample.t_ms;
            assert!(
                (detail_search.size().width - sample.value).abs() <= 1.2,
                "DetailSearchBox width at t={}ms: Slint {:.2} != Chromium {:.2}",
                sample.t_ms,
                detail_search.size().width,
                sample.value
            );
        }

        // 3. Compare ArtistMediaTile 300ms cubic-bezier(0.25, 1, 0.5, 1) hover lift (0 -> -4px) against Chromium's 5 sampled frames
        navigation_state.set_current_route("artist".into());
        app.window().set_size(PhysicalSize::new(1435, 2500));
        navigation_state.set_window_width(1435.0);
        navigation_state.set_window_height(1000.0);
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(50));
        let gt_tile_lift = gt.animations.get("artistTileHoverLift").unwrap();
        let tile_img = find_first_element(&app, "ArtistMediaTile::tile-img-wrapper");
        let tile_y0 = tile_img.absolute_position().y;
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: LogicalPosition::new(
                    tile_img.absolute_position().x + 50.0,
                    tile_y0 + 50.0,
                ),
            });
        prev_t = 0;
        for sample in &gt_tile_lift.samples {
            let dt = sample.t_ms - prev_t;
            if dt > 0 {
                mock_elapsed_time(Duration::from_millis(dt));
            }
            prev_t = sample.t_ms;
            let actual_lift = tile_img.absolute_position().y - tile_y0;
            assert!(
                (actual_lift - sample.value).abs() <= 0.6,
                "ArtistMediaTile hover lift at t={}ms: Slint {:.2} != Chromium {:.2}",
                sample.t_ms,
                actual_lift,
                sample.value
            );
        }
        app.window().set_size(PhysicalSize::new(1435, 1000));
        navigation_state.set_window_width(1435.0);
        navigation_state.set_window_height(1000.0);
        mock_elapsed_time(Duration::from_millis(350));

        // 4. Compare SettingsNavBtn active icon spring scale (300ms cubic-bezier(0.34, 1.56, 0.64, 1), 18px -> 20.7px) against Chromium's 5 sampled frames
        settings_state_settings_state.set_settings_tab("general".into());
        settings.show().unwrap();
        settings.set_presented(true);
        mock_elapsed_time(Duration::from_millis(550));
        let gt_icon_spring = gt.animations.get("settingsNavIconScale").unwrap();
        let audio_nav_icon = find_nth_element(&settings, "SettingsNavBtn::settings-nav-icon", 1);
        assert!((audio_nav_icon.size().width - 18.0).abs() <= 0.2);
        settings_state_settings_state.set_settings_tab("audio".into());
        prev_t = 0;
        for sample in &gt_icon_spring.samples {
            let dt = sample.t_ms - prev_t;
            if dt > 0 {
                mock_elapsed_time(Duration::from_millis(dt));
            }
            prev_t = sample.t_ms;
            assert!(
                (audio_nav_icon.size().width - sample.value).abs() <= 0.5,
                "SettingsNavBtn icon spring width at t={}ms: Slint {:.2} != Chromium {:.2}",
                sample.t_ms,
                audio_nav_icon.size().width,
                sample.value
            );
        }
        settings.set_presented(false);
        settings.hide().unwrap();
        mock_elapsed_time(Duration::from_millis(550));

        // =========================================================================
        // PART 3: BEHAVIORAL STATE MACHINE PARITY
        // =========================================================================
        // 1. Play mode cycle order: loop -> random -> single -> loop
        assert_eq!(player_state.get_play_mode().as_str(), "loop");
        player_state.invoke_cycle_play_mode();
        assert_eq!(player_state.get_play_mode().as_str(), "random");
        player_state.invoke_cycle_play_mode();
        assert_eq!(player_state.get_play_mode().as_str(), "single");
        player_state.invoke_cycle_play_mode();
        assert_eq!(player_state.get_play_mode().as_str(), "loop");

        // =========================================================================
        // PART 4: HEADLESS RASTERIZED PIXEL SNAPSHOT VERIFICATION (LIGHT & DARK)
        // =========================================================================
        navigation_state.set_current_route("home".into());
        login_state.set_is_login_open(false);
        dialog_state.set_is_dialog_open(false);
        player_view_state.set_is_fullscreen(false);
        theme.set_is_dark(false);
        theme.set_theme_mode("light".into());
        mock_elapsed_time(Duration::from_millis(600));

        let light_snap = app
            .window()
            .take_snapshot()
            .expect("Headless software renderer should capture 1435x1000 RGBA snapshot");
        assert_eq!(light_snap.width(), 1435);
        assert_eq!(light_snap.height(), 1000);

        // Sample edge of PlayerBar play-main-btn circle (x=838, y=918) while playing:
        // #6366f1 (99, 102, 241) + 15% white .inner-glow.active = rgb(122, 124, 243)
        let play_px = light_snap.as_slice()[918 * 1435 + 838];
        assert!(
            (play_px.r as i32 - 122).abs() <= 2
                && (play_px.g as i32 - 124).abs() <= 2
                && (play_px.b as i32 - 243).abs() <= 2,
            "PlayerBar play-main-btn playing pixel {:?} should match #6366f1 + 15% inner-glow (122, 124, 243)",
            play_px
        );

        // Pause playback and advance 300ms for .inner-glow opacity (0.15 -> 0.0) transition to complete -> pure #6366f1 (99, 102, 241)
        player_state.set_is_playing(false);
        mock_elapsed_time(Duration::from_millis(320));
        let paused_snap = app.window().take_snapshot().unwrap();
        let paused_px = paused_snap.as_slice()[918 * 1435 + 838];
        assert!(
            (paused_px.r as i32 - 99).abs() <= 2
                && (paused_px.g as i32 - 102).abs() <= 2
                && (paused_px.b as i32 - 241).abs() <= 2,
            "PlayerBar play-main-btn paused pixel {:?} should match pure #6366f1 (99, 102, 241)",
            paused_px
        );
        player_state.set_is_playing(true);

        // Sample app background in light mode (x=1350, y=50) -> near #eef1f6 (rgb(238, 241, 246))
        let bg_light_px = light_snap.as_slice()[50 * 1435 + 1350];
        assert!(
            bg_light_px.r > 220 && bg_light_px.g > 225 && bg_light_px.b > 230,
            "Light mode background pixel {:?} should be light (#eef1f6-#e7ebf2)",
            bg_light_px
        );

        // Switch to Dark mode and verify rasterized background changes to dark (#101217-#151821)
        theme.set_is_dark(true);
        theme.set_theme_mode("dark".into());
        mock_elapsed_time(Duration::from_millis(100));
        let dark_snap = app
            .window()
            .take_snapshot()
            .expect("Headless software renderer should capture dark mode snapshot");
        let bg_dark_px = dark_snap.as_slice()[50 * 1435 + 1350];
        assert!(
            bg_dark_px.r < 30 && bg_dark_px.g < 35 && bg_dark_px.b < 45,
            "Dark mode background pixel {:?} should be dark (#101217-#151821)",
            bg_dark_px
        );
    }
}
