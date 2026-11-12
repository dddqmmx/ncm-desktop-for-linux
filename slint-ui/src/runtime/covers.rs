use crate::*;
use slint::{Image, Model, ModelNotify, ModelPeer, ModelRc, ModelTracker, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub trait CoverRow: Clone + 'static {
    fn id(&self) -> i32;
    fn set_cover(&mut self, image: Image);
}
macro_rules! cover_rows {
    ($($ty:ty => $field:ident),+ $(,)?) => {$ (
        impl CoverRow for $ty {
            fn id(&self) -> i32 { self.id }
            fn set_cover(&mut self, image: Image) { self.$field = image; }
        }
    )+};
}
cover_rows! {
    TrackItem => cover, QueueSongItem => cover, DailySongItem => cover,
    NavPlaylistItem => cover, RecommendPlaylistItem => cover,
    HomeChartItem => cover,
    ArtistSongItem => cover, ArtistReleaseItem => cover, CommentItem => avatar,
}

// Build the index once with the model; cover delivery must not walk every song.
pub struct CoverModel<T> {
    rows: VecModel<T>,
    by_id: HashMap<i32, Vec<usize>>,
    row_trackers: RefCell<HashMap<usize, Rc<ModelNotify>>>,
}
pub fn cover_model<T: CoverRow>(rows: Vec<T>) -> ModelRc<T> {
    let mut by_id: HashMap<i32, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        by_id.entry(row.id()).or_default().push(index);
    }
    Rc::new(CoverModel {
        rows: VecModel::from(rows),
        by_id,
        row_trackers: RefCell::default(),
    })
    .into()
}
impl<T: CoverRow> Model for CoverModel<T> {
    type Data = T;
    fn row_count(&self) -> usize {
        self.rows.row_count()
    }
    fn row_data(&self, row: usize) -> Option<T> {
        self.rows.row_data(row)
    }
    fn set_row_data(&self, row: usize, data: T) {
        if row < self.rows.row_count() {
            assert!(
                self.by_id
                    .get(&data.id())
                    .is_some_and(|indices| indices.binary_search(&row).is_ok()),
                "replace the model when row identities change"
            );
            self.rows.set_row_data(row, data);
            let tracker = self.row_trackers.borrow().get(&row).cloned();
            if let Some(tracker) = tracker {
                tracker.row_changed(row);
            }
        }
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        self
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
impl<T: CoverRow> ModelTracker for CoverModel<T> {
    fn attach_peer(&self, peer: ModelPeer) {
        self.rows.model_tracker().attach_peer(peer);
    }
    fn track_row_count_changes(&self) {
        self.rows.model_tracker().track_row_count_changes();
    }
    fn track_row_data_changes(&self, row: usize) {
        if row >= self.rows.row_count() {
            return;
        }
        // Slint 1.18's VecModel shares one dirty property for all indexed row
        // bindings. One arriving cover therefore invalidates the entire visible
        // list, including text shaping and layout. These models have immutable
        // row identities/counts, so track indexed reads independently. Ordinary
        // repeaters still receive VecModel's row-specific peer notifications.
        self.row_trackers
            .borrow_mut()
            .entry(row)
            .or_default()
            .track_row_data_changes(row);
    }
}
pub fn update_covers<T: CoverRow>(rows: ModelRc<T>, changes: &HashMap<i32, Image>) {
    if let Some(indexed) = rows.as_any().downcast_ref::<CoverModel<T>>() {
        for (id, image) in changes {
            if let Some(indices) = indexed.by_id.get(id) {
                for &index in indices {
                    let mut row = indexed.rows.row_data(index).unwrap();
                    row.set_cover(image.clone());
                    indexed.set_row_data(index, row);
                }
            }
        }
    } else {
        // Empty/demo models can still use Slint's ordinary VecModel.
        for (index, mut row) in rows.iter().enumerate() {
            if let Some(image) = changes.get(&row.id()) {
                row.set_cover(image.clone());
                rows.set_row_data(index, row);
            }
        }
    }
}

// 256 thumbnails exhaust the count limit at only 16 MiB. Keep typical playlists
// resident while retaining the same 64 MiB pixel budget (including full artwork).
const MAX_IMAGES: usize = 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;
struct CachedCover {
    image: Image,
    palette: Option<crate::fullscreen_palette::CoverPalette>,
    size: u32,
    bytes: usize,
    used: Cell<u64>,
}
#[derive(Default)]
pub struct CoverCache {
    images: HashMap<String, CachedCover>,
    clock: Cell<u64>,
    bytes: usize,
}
impl CoverCache {
    #[cfg(test)]
    pub(super) fn usage(&self) -> (usize, usize) {
        (self.images.len(), self.bytes)
    }
    fn tick(&self) -> u64 {
        let next = self.clock.get().wrapping_add(1);
        self.clock.set(next);
        next
    }
    pub fn get(&self, url: &str) -> Option<&Image> {
        let image = self.images.get(url)?;
        image.used.set(self.tick());
        Some(&image.image)
    }
    pub fn contains(&self, url: &str, size: u32) -> bool {
        self.images.get(url).is_some_and(|entry| {
            entry.used.set(self.tick());
            entry.size >= size
        })
    }
    pub fn palette(&self, url: &str) -> Option<crate::fullscreen_palette::CoverPalette> {
        self.images.get(url).and_then(|entry| entry.palette)
    }
    pub fn insert(
        &mut self,
        url: String,
        size: u32,
        image: Image,
        palette: Option<crate::fullscreen_palette::CoverPalette>,
        pinned: &HashSet<String>,
    ) -> Vec<String> {
        if self.contains(&url, size) {
            return Vec::new();
        }
        let used = Cell::new(self.tick());
        let bytes = image.size().width as usize * image.size().height as usize * 4;
        if let Some(previous) = self.images.insert(
            url.clone(),
            CachedCover {
                image,
                palette,
                size,
                bytes,
                used,
            },
        ) {
            self.bytes -= previous.bytes;
        }
        self.bytes += bytes;
        let mut evicted = Vec::new();
        while self.images.len() > MAX_IMAGES || self.bytes > MAX_BYTES {
            let oldest = self
                .images
                .iter()
                .filter(|(key, _)| **key != url && !pinned.contains(*key))
                .min_by_key(|(_, entry)| entry.used.get())
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else { break };
            self.bytes -= self.images.remove(&oldest).unwrap().bytes;
            evicted.push(oldest);
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Reads<T> {
        model: ModelRc<T>,
        rows: RefCell<Vec<usize>>,
    }
    impl<T: Clone + 'static> Model for Reads<T> {
        type Data = T;
        fn row_count(&self) -> usize {
            self.model.row_count()
        }
        fn row_data(&self, row: usize) -> Option<T> {
            self.rows.borrow_mut().push(row);
            self.model.row_data(row)
        }
        fn model_tracker(&self) -> &dyn ModelTracker {
            self.model.model_tracker()
        }
    }

    struct CountedRow {
        id: i32,
        image: Image,
        clones: Rc<Cell<usize>>,
    }
    impl Clone for CountedRow {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                id: self.id,
                image: self.image.clone(),
                clones: self.clones.clone(),
            }
        }
    }
    impl CoverRow for CountedRow {
        fn id(&self) -> i32 {
            self.id
        }
        fn set_cover(&mut self, image: Image) {
            self.image = image;
        }
    }
    fn image(size: u32) -> Image {
        Image::from_rgba8(slint::SharedPixelBuffer::new(size, size))
    }
    #[test]
    fn cover_delivery_touches_matching_rows_including_duplicates_only() {
        let clones = Rc::new(Cell::new(0));
        let rows = cover_model(
            (0..10_000)
                .map(|id| CountedRow {
                    id: id % 5_000,
                    image: Image::default(),
                    clones: clones.clone(),
                })
                .collect(),
        );
        let cover = image(128);
        update_covers(
            rows.clone(),
            &HashMap::from([(42, cover.clone()), (20_000, cover.clone())]),
        );
        assert_eq!(clones.get(), 2, "cover update walked unrelated rows");
        assert_eq!(rows.row_data(42).unwrap().image, cover);
        assert_eq!(rows.row_data(5_042).unwrap().image, cover);
        assert_eq!(rows.row_data(43).unwrap().image.size().width, 0);
        update_covers(rows.clone(), &HashMap::from([(42, Image::default())]));
        assert_eq!(rows.row_data(42).unwrap().image.size().width, 0);
        assert_eq!(rows.row_data(5_042).unwrap().image.size().width, 0);
    }
    #[test]
    fn decoded_cache_is_bounded_preserves_pinned_artwork_and_never_downgrades_it() {
        let mut cache = CoverCache::default();
        let pinned = HashSet::from(["playing".into()]);
        let full = image(800);
        let small = image(64);
        cache.insert("playing".into(), 800, full.clone(), None, &pinned);
        cache.insert("playing".into(), 128, small.clone(), None, &pinned);
        assert_eq!(cache.get("playing").unwrap(), &full);
        let mut evicted = Vec::new();
        for i in 0..MAX_IMAGES + 40 {
            evicted.extend(cache.insert(i.to_string(), 128, small.clone(), None, &pinned));
        }
        assert_eq!(cache.images.len(), MAX_IMAGES);
        assert!(!evicted.is_empty());
        assert!(cache.contains("playing", 800));
        assert!(!cache.contains("0", 128));
        assert!(
            !cache.contains(&(MAX_IMAGES + 39).to_string(), 800),
            "thumbnail must allow a later artwork upgrade"
        );
        cache.insert("0".into(), 128, small, None, &pinned);
        assert!(cache.contains("0", 128));
        for i in 0..40 {
            cache.insert(format!("art-{i}"), 800, full.clone(), None, &pinned);
        }
        assert!(cache.bytes <= MAX_BYTES);
        assert!(cache.images.len() <= MAX_IMAGES);
        assert!(cache.contains("playing", 800));
    }

    #[test]
    fn a_cover_delivery_does_not_rebind_other_visible_tracks() {
        use i_slint_backend_testing::{mock_elapsed_time, ElementRoot};
        use slint::ComponentHandle;
        i_slint_backend_testing::init_no_event_loop();
        let app = AppWindow::new().unwrap();
        app.show().unwrap();
        let playlist_state = app.global::<crate::PlaylistState>();
        let navigation_state = app.global::<crate::NavigationState>();
        let model = cover_model(
            (0..1_000)
                .map(|id| TrackItem {
                    id,
                    name: format!("track {id}").into(),
                    artist: "artist".into(),
                    album: "album".into(),
                    has_cover: true,
                    ..Default::default()
                })
                .collect(),
        );
        let reads = Rc::new(Reads {
            model: model.clone(),
            rows: RefCell::default(),
        });
        playlist_state.set_playlist_tracks(reads.clone().into());
        navigation_state.set_current_route("playlist".into());
        let settle = || {
            let _ = app.root_element().size();
            mock_elapsed_time(std::time::Duration::from_millis(600));
            let _ = app.root_element().size();
        };
        settle();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700.0, 500.0),
                delta_x: 0.0,
                delta_y: -4_000.0,
            });
        settle();
        let target = *reads
            .rows
            .borrow()
            .iter()
            .filter(|&&row| row < 1_000)
            .max()
            .unwrap()
            - 2;
        reads.rows.borrow_mut().clear();
        update_covers(model.clone(), &HashMap::from([(target as i32, image(128))]));
        settle();
        assert_eq!(
            *reads.rows.borrow(),
            vec![target],
            "one thumbnail rebound unrelated text/layout"
        );
        reads.rows.borrow_mut().clear();
        // A previously visited, now offscreen row must not invalidate the viewport.
        update_covers(model.clone(), &HashMap::from([(0, image(128))]));
        settle();
        assert!(
            reads.rows.borrow().is_empty(),
            "offscreen cover rebound visible tracks"
        );
        let mut row = model.row_data(target).unwrap();
        row.name = "updated title".into();
        row.is_favorite = true;
        model.set_row_data(target, row);
        settle();
        assert_eq!(*reads.rows.borrow(), vec![target]);
    }

    #[test]
    fn a_playlist_of_small_covers_fits_without_decode_churn() {
        let mut cache = CoverCache::default();
        let thumbnail = image(128);
        let pinned = HashSet::from(["header".into()]);
        cache.insert("header".into(), 800, image(800), None, &pinned);
        for i in 0..500 {
            assert!(cache
                .insert(i.to_string(), 128, thumbnail.clone(), None, &pinned)
                .is_empty());
        }
        for i in 0..500 {
            assert!(cache.contains(&i.to_string(), 128));
        }
        assert!(cache.bytes <= MAX_BYTES);
    }

    #[test]
    fn home_covers_update_one_card_and_page_motion_keeps_card_bindings() {
        use i_slint_backend_testing::{mock_elapsed_time, ElementRoot};
        use slint::ComponentHandle;
        i_slint_backend_testing::init_no_event_loop();
        let app = AppWindow::new().unwrap();
        let account_state = app.global::<crate::AccountState>();
        let home_state = app.global::<crate::HomeState>();
        account_state.set_is_logged_in(true);
        let playlists = cover_model(
            (0..40)
                .map(|id| RecommendPlaylistItem {
                    id,
                    title: format!("playlist {id}").into(),
                    ..Default::default()
                })
                .collect(),
        );
        let songs = cover_model(
            (0..40)
                .map(|id| DailySongItem {
                    id: id + 100,
                    title: format!("song {id}").into(),
                    ..Default::default()
                })
                .collect(),
        );
        let pl_reads = Rc::new(Reads {
            model: playlists.clone(),
            rows: RefCell::default(),
        });
        let song_reads = Rc::new(Reads {
            model: songs.clone(),
            rows: RefCell::default(),
        });
        home_state.set_recommend_playlists(pl_reads.clone().into());
        home_state.set_daily_songs(song_reads.clone().into());
        app.show().unwrap();
        let settle = || {
            for _ in 0..40 {
                let _ = app.root_element().size();
                mock_elapsed_time(std::time::Duration::from_millis(16));
            }
            let _ = app.root_element().size();
        };
        settle();
        pl_reads.rows.borrow_mut().clear();
        song_reads.rows.borrow_mut().clear();
        update_covers(playlists.clone(), &HashMap::from([(0, image(800))]));
        update_covers(songs.clone(), &HashMap::from([(100, image(800))]));
        settle();
        assert_eq!(*pl_reads.rows.borrow(), vec![0]);
        assert_eq!(*song_reads.rows.borrow(), vec![0]);
        pl_reads.rows.borrow_mut().clear();
        song_reads.rows.borrow_mut().clear();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700.0, 250.0),
                delta_x: 0.0,
                delta_y: -60.0,
            });
        settle();
        assert!(
            pl_reads.rows.borrow().is_empty(),
            "vertical motion rebound playlist cards"
        );
        assert!(
            song_reads.rows.borrow().is_empty(),
            "vertical motion rebound song cards"
        );
        update_covers(playlists, &HashMap::from([(30, image(800))]));
        update_covers(songs, &HashMap::from([(130, image(800))]));
        settle();
        assert!(pl_reads.rows.borrow().is_empty());
        assert!(song_reads.rows.borrow().is_empty());
    }
}
