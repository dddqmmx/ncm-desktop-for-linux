use super::queue::{PlaybackQueue, Removal};

fn queue() -> PlaybackQueue {
    let mut queue = PlaybackQueue::default();
    queue.replace(["a", "b", "c", "d"].map(str::to_owned));
    queue.select("b".into());
    queue
}

#[test]
fn add_next_moves_songs_from_either_side_and_inserts_without_duplicates() {
    let mut queue = queue();
    assert!(queue.add_next("d".into()));
    assert_eq!(queue.ids(), ["a", "b", "d", "c"]);
    assert!(queue.add_next("a".into()));
    assert_eq!(queue.ids(), ["b", "a", "d", "c"]);
    assert!(queue.add_next("a".into()));
    assert_eq!(queue.ids(), ["b", "a", "d", "c"]);
    assert!(queue.add_next("local:/音乐/新歌.flac".into()));
    assert_eq!(queue.ids(), ["b", "local:/音乐/新歌.flac", "a", "d", "c"]);
    assert!(!queue.add_next("b".into()));
    assert!(!queue.add_next(String::new()));
    assert_eq!(queue.current().map(String::as_str), Some("b"));
    assert_eq!(queue.ids().len(), 5);

    queue.clear();
    queue.add_next("9007199254740991".into());
    queue.add_next("local:/音乐/新歌.flac".into());
    assert_eq!(queue.ids(), ["local:/音乐/新歌.flac", "9007199254740991"]);
    assert_eq!(queue.current(), None);
    assert_eq!(queue.step(1, false, "loop", 0), queue.ids().first());
}

#[test]
fn queue_mutations_preserve_valid_selection_and_report_current_removal() {
    let mut queue = queue();
    assert_eq!(queue.remove("missing"), Removal::Missing);
    assert_eq!(queue.remove("a"), Removal::Removed);
    assert_eq!(queue.current().map(String::as_str), Some("b"));
    assert_eq!(queue.remove("b"), Removal::Current(Some("c".into())));
    assert_eq!(queue.current(), None);
    queue.select("d".into());
    assert_eq!(queue.remove("d"), Removal::Current(Some("c".into())));
    queue.select("c".into());
    assert_eq!(queue.remove("c"), Removal::Current(None));
    assert_eq!(queue.current(), None);
    assert!(queue.ids().is_empty());

    queue.select("b".into());
    queue.replace(["a", "b", "a", ""].map(str::to_owned));
    assert_eq!(queue.ids(), ["a", "b"]);
    assert_eq!(queue.current().map(String::as_str), Some("b"));
    queue.replace(["c".to_owned()]);
    assert_eq!(queue.current(), None);
    queue.select("d".into());
    queue.add("d".into());
    assert_eq!(queue.ids(), ["c", "d"]);
    queue.clear();
    assert!(queue.ids().is_empty());
    assert_eq!(queue.current(), None);
    assert_eq!(queue.step(1, false, "random", 7), None);
}

#[test]
fn navigation_uses_the_reordered_queue_and_existing_play_modes() {
    let mut queue = queue();
    queue.add_next("d".into());
    for natural in [false, true] {
        assert_eq!(queue.step(1, natural, "loop", 0).unwrap(), "d");
    }
    assert_eq!(queue.step(-1, false, "loop", 0).unwrap(), "a");
    assert_eq!(queue.step(1, false, "single", 0).unwrap(), "d");
    assert_eq!(queue.step(1, true, "single", 0).unwrap(), "b");
    for sample in 0..30 {
        assert_ne!(queue.step(1, true, "random", sample).unwrap(), "b");
    }
    queue.select("c".into());
    assert_eq!(queue.step(1, true, "loop", 0).unwrap(), "a");
    queue.select("a".into());
    assert_eq!(queue.step(-1, false, "loop", 0).unwrap(), "c");
    queue.replace(["a".into()]);
    assert_eq!(queue.step(1, true, "random", 7).unwrap(), "a");
}
