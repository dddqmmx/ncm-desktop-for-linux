use crate::{AppWindow, DetailState, TrackItem};
use slint::{ComponentHandle, FilterModel, ModelRc};
use std::rc::Rc;

fn filter_tracks(tracks: ModelRc<TrackItem>, query: &str) -> ModelRc<TrackItem> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return tracks;
    }
    // Keep the source model intact so cover/favorite updates and removals also
    // reach the filtered rows without copying the playlist or changing song IDs.
    Rc::new(FilterModel::new(tracks, move |track| {
        [&track.name, &track.artist, &track.album]
            .iter()
            .any(|text| text.to_lowercase().contains(&query))
    }))
    .into()
}

pub fn attach(app: &AppWindow) {
    app.global::<DetailState>()
        .on_filter_tracks(|tracks, query| filter_tracks(tracks, &query));
}

#[cfg(test)]
mod tests {
    use super::*;
    use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
    use slint::{platform::WindowEvent, Model, VecModel};
    use std::time::Duration;

    fn tracks() -> Rc<VecModel<TrackItem>> {
        Rc::new(VecModel::from(vec![
            TrackItem {
                id: 11,
                name: "晴天".into(),
                artist: "周杰伦".into(),
                album: "叶惠美".into(),
                ..Default::default()
            },
            TrackItem {
                id: 22,
                name: "海阔天空".into(),
                artist: "BEYOND".into(),
                album: "乐与怒".into(),
                ..Default::default()
            },
        ]))
    }

    #[test]
    fn playlist_search_matches_metadata_and_tracks_source_updates() {
        let source = tracks();
        let all: ModelRc<TrackItem> = source.clone().into();
        for query in ["海阔", " bEyOnD ", "乐与怒"] {
            let filtered = filter_tracks(all.clone(), query);
            assert_eq!(filtered.row_count(), 1, "{query}");
            assert_eq!(filtered.row_data(0).unwrap().id, 22);
        }
        for query in ["", " \t\n "] {
            assert_eq!(filter_tracks(all.clone(), query), all);
        }
        assert_eq!(filter_tracks(all.clone(), "不存在").row_count(), 0);
        assert_eq!(all.row_count(), 2);

        let filtered = filter_tracks(all, "beyond");
        let mut song = source.row_data(1).unwrap();
        song.is_favorite = true;
        source.set_row_data(1, song.clone());
        assert!(filtered.row_data(0).unwrap().is_favorite);
        song.id = 33;
        source.push(song);
        assert_eq!(filtered.row_count(), 2);
        source.remove(1);
        assert_eq!(filtered.row_count(), 1);
        assert_eq!(filtered.row_data(0).unwrap().id, 33);
    }

    fn settle(app: &AppWindow) {
        let _ = app.root_element().size();
        mock_elapsed_time(Duration::from_millis(600));
        let _ = app.root_element().size();
    }

    fn element(app: &AppWindow, id: &str) -> ElementHandle {
        ElementHandle::find_by_element_id(app, id).next().unwrap()
    }

    #[test]
    fn playlist_search_input_filters_rows_and_clear_and_navigation_restore_them() {
        i_slint_backend_testing::init_no_event_loop();
        let app = AppWindow::new().unwrap();
        crate::playlists::attach(&app);
        let playlist = app.global::<crate::PlaylistState>();
        let detail = app.global::<DetailState>();
        let navigation = app.global::<crate::NavigationState>();
        let source = tracks();
        playlist.set_playlist_tracks(source.clone().into());
        navigation.set_current_route("playlist".into());
        app.show().unwrap();
        settle(&app);
        let rows = || ElementHandle::find_by_element_id(&app, "SongTrackRow::row-ta").count();
        assert_eq!(rows(), 2);

        element(&app, "DetailSearchField::detail-search-input")
            .mock_single_click(slint::platform::PointerEventButton::Left);
        for ch in "bEyOnD".chars() {
            let text = ch.to_string().into();
            app.window()
                .dispatch_event(WindowEvent::KeyPressed { text });
        }
        settle(&app);
        assert_eq!(detail.get_detail_search_query(), "bEyOnD");
        assert_eq!(rows(), 1);
        element(&app, "SongTrackRow::row-ta")
            .mock_single_click(slint::platform::PointerEventButton::Left);
        assert_eq!(app.global::<crate::PlayerState>().get_current_song_id(), 22);
        assert_eq!(playlist.get_playlist_tracks().row_count(), 2);

        detail.set_detail_search_query("不存在".into());
        settle(&app);
        assert_eq!(rows(), 0);
        element(&app, "SongList::empty-state");
        element(&app, "DetailSearchField::clear-search-button")
            .mock_single_click(slint::platform::PointerEventButton::Left);
        settle(&app);
        assert!(detail.get_detail_search_query().is_empty());
        assert_eq!(rows(), 2);

        detail.set_detail_search_query("晴天".into());
        settle(&app);
        playlist.set_active_playlist_id(102);
        settle(&app);
        assert!(detail.get_detail_search_query().is_empty());
        assert_eq!(rows(), 2);
        detail.set_detail_search_query("海阔".into());
        navigation.set_current_route("home".into());
        settle(&app);
        assert!(detail.get_detail_search_query().is_empty());
        app.hide().unwrap();
    }
}
