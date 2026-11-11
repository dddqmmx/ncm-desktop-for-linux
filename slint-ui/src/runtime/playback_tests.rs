use super::{
    api::{ApiFuture, ApiResponse, MusicApi},
    backend::{Command, Event},
    data::{Preferences, Song, Storage},
    playback::PlaybackState,
    tests::temp,
    view::{self, View},
    ApplicationRuntime,
};
use crate::AppWindow;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};
use std::{sync::Arc, time::Duration};

fn saved_queue() -> PlaybackState {
    PlaybackState {
        queue: vec![
            Song {
                id: "9007199254740991".into(),
                name: "网络歌曲".into(),
                artist: "歌手".into(),
                duration: 180_000,
                ..Song::default()
            },
            Song {
                id: "local:/音乐/歌曲.flac".into(),
                name: "本地歌曲".into(),
                duration: 240_000,
                path: Some("/音乐/歌曲.flac".into()),
                ..Song::default()
            },
        ],
        current_id: Some("local:/音乐/歌曲.flac".into()),
        position_ms: 65_432,
    }
}

fn event(name: &str, token: u64, data: Value) -> Event {
    Event {
        name: name.into(),
        token,
        epoch: 0,
        data,
        error: None,
    }
}

fn drain(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Command>) -> Vec<Command> {
    std::iter::from_fn(|| receiver.try_recv().ok()).collect()
}

#[test]
fn restoring_online_playback_loads_lyrics_and_seeks_them_without_starting_audio() {
    use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
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
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    let mut saved = saved_queue();
    saved.current_id = Some(saved.queue[0].id.clone());
    view.restore_playback(&app, saved.clone());
    let player_view = app.global::<crate::PlayerViewState>();
    player_view.set_is_fullscreen(true);
    app.show().unwrap();
    let settle = || {
        for _ in 0..60 {
            let _ = app.root_element().size();
            mock_elapsed_time(Duration::from_millis(16));
        }
    };
    let assert_centered = |index| {
        let viewport = ElementHandle::find_by_element_id(&app, "LyricPanelView::lyric-scroll")
            .next()
            .unwrap();
        let line = ElementHandle::find_by_element_id(&app, "LyricLineList::lyric-line-item")
            .find(|row| row.accessible_item_index() == Some(index))
            .unwrap();
        assert!(line.size().width > 0.0 && line.computed_opacity() > 0.9);
        let center = |item: &ElementHandle| item.absolute_position().y + item.size().height / 2.0;
        assert!(
            (center(&line) - center(&viewport)).abs() < 1.0,
            "restored lyric must be visible at the saved or sought position"
        );
    };
    settle();
    let commands = drain(&mut rx);
    assert!(commands
        .iter()
        .all(|cmd| cmd.name != "play" && cmd.name != "seek"));
    let lyrics = commands
        .iter()
        .find(|cmd| cmd.name == "lyrics")
        .expect("restoring an online track must request its lyrics before playback");
    assert_eq!(lyrics.data["id"], saved.current_id.unwrap());
    let lines = json!([
        {"ms":1000,"text":"开头","translation":"","pronunciation":""},
        {"ms":60000,"text":"恢复的歌词","translation":"","pronunciation":""},
        {"ms":120000,"text":"拖动后的歌词","translation":"","pronunciation":""},
        {"ms":150000,"text":"下一行","translation":"","pronunciation":""},
        {"ms":170000,"text":"结尾","translation":"","pronunciation":""}
    ]);
    view.apply(&app, event("lyrics", lyrics.token, lines.clone()));
    let player = app.global::<crate::PlayerState>();
    assert_eq!(player_view.get_lyrics().row_count(), 5);
    assert_eq!(player_view.get_active_lyric_index(), 1);
    assert_eq!(player.get_current_time_str(), "1:05");
    assert!(!player.get_is_playing());
    settle();
    assert_centered(1);
    crate::responsive_tests::snapshot(&app, "fullscreen-restored-lyrics");
    let reset = player_view.get_lyric_position_reset();
    view.seek(&app, 75.0);
    assert_eq!(player.get_current_time_str(), "2:15");
    assert_eq!(player_view.get_active_lyric_index(), 2);
    assert!(player_view.get_lyric_position_reset() > reset);
    settle();
    assert_centered(2);
    assert!(
        drain(&mut rx).is_empty(),
        "restored track must not seek an unloaded engine"
    );
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":0,"play_token":0,"active":false}),
        ),
    );
    assert_eq!(player_view.get_active_lyric_index(), 2);
    player_view.set_is_fullscreen(false);
    settle();
    player_view.set_is_fullscreen(true);
    settle();
    assert_centered(2);

    // Local restoration must also invalidate an outstanding online lyric request.
    view.restore_playback(&app, saved_queue());
    assert!(drain(&mut rx).iter().all(|cmd| cmd.name != "lyrics"));
    view.apply(&app, event("lyrics", lyrics.token, lines));
    assert_eq!(player_view.get_lyrics().row_count(), 0);
    assert_eq!(player_view.get_active_lyric_index(), -1);
}

#[test]
fn queue_next_preserves_current_audio_state_and_saves_only_the_reordered_queue() {
    i_slint_backend_testing::init_no_event_loop();
    for phase in ["restored", "loading", "playing", "paused"] {
        let app = AppWindow::new().unwrap();
        view::initialize(&app, &Preferences::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut view = View::new(tx, Preferences::default());
        view.restore_playback(&app, saved_queue());
        let queue_state = app.global::<crate::QueueState>();
        let player_state = app.global::<crate::PlayerState>();
        let target = queue_state.get_queue_songs().row_data(0).unwrap().id;
        let current = player_state.get_current_song_id();
        if phase != "restored" {
            view.action(&app, "toggle", -1, "");
            let play = drain(&mut rx)
                .into_iter()
                .find(|cmd| cmd.name == "play")
                .unwrap();
            if phase != "loading" {
                view.apply(
                    &app,
                    event("play", play.token, json!({"song": play.data["song"]})),
                );
                view.apply(
                    &app,
                    event(
                        "snapshot",
                        0,
                        json!({
                            "progress":65_432,"play_token":play.token,
                            "playing":phase == "playing", "paused":phase == "paused"
                        }),
                    ),
                );
            }
        }
        let before = (
            player_state.get_is_playing(),
            player_state.get_is_loading(),
            player_state.get_progress_percent(),
        );
        drain(&mut rx);
        for id in [target, target, current, i32::MAX, -1] {
            view.action(&app, "queue-next", id, "");
            assert_eq!(player_state.get_current_song_id(), current, "{phase}");
            assert_eq!(
                (
                    player_state.get_is_playing(),
                    player_state.get_is_loading(),
                    player_state.get_progress_percent()
                ),
                before,
                "{phase}"
            );
            assert_eq!(player_state.get_current_time_str(), "1:05");
            assert!(
                drain(&mut rx).is_empty(),
                "queue edits sent audio commands during {phase}"
            );
        }
        assert_eq!(
            queue_state
                .get_queue_songs()
                .iter()
                .map(|song| song.id)
                .collect::<Vec<_>>(),
            [current, target]
        );
        assert_eq!(
            queue_state
                .get_queue_songs()
                .iter()
                .map(|song| song.index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        view.save_playback_if_changed(&app, false);
        let saved = drain(&mut rx)
            .into_iter()
            .find(|cmd| cmd.name == "save-playback")
            .unwrap();
        assert!(saved.data.get("next_ids").is_none());
        let saved: PlaybackState = serde_json::from_value(saved.data).unwrap();
        assert_eq!(
            saved
                .queue
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["local:/音乐/歌曲.flac", "9007199254740991"]
        );
        assert_eq!(saved.current_id, saved_queue().current_id);
        assert_eq!(saved.position_ms, 65_432);
        assert_eq!(saved.queue[0].path, saved_queue().queue[1].path);
    }
}

#[test]
fn queue_next_reordering_is_used_by_manual_and_natural_playback() {
    i_slint_backend_testing::init_no_event_loop();
    for (mode, natural) in [
        ("loop", false),
        ("loop", true),
        ("single", false),
        ("single", true),
    ] {
        let app = AppWindow::new().unwrap();
        view::initialize(&app, &Preferences::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut view = View::new(tx, Preferences::default());
        let mut saved = saved_queue();
        saved.queue.push(Song {
            id: "third".into(),
            name: "原下一曲".into(),
            duration: 100_000,
            ..Song::default()
        });
        view.restore_playback(&app, saved);
        let player_state = app.global::<crate::PlayerState>();
        let queue_state = app.global::<crate::QueueState>();
        player_state.set_play_mode(mode.into());
        let target = queue_state.get_queue_songs().row_data(0).unwrap().id;
        view.action(&app, "toggle", -1, "");
        let play = drain(&mut rx)
            .into_iter()
            .find(|cmd| cmd.name == "play")
            .unwrap();
        view.apply(
            &app,
            event("play", play.token, json!({"song":play.data["song"]})),
        );
        view.action(&app, "queue-next", target, "");
        assert!(drain(&mut rx).is_empty());
        if natural {
            view.apply(
                &app,
                event("finished", play.token, json!({"reason":"ended"})),
            );
        } else {
            view.action(&app, "step", 1, "");
        }
        let next = drain(&mut rx)
            .into_iter()
            .find(|cmd| cmd.name == "play")
            .unwrap();
        assert_eq!(
            next.data["song"]["id"],
            if mode == "single" && natural {
                "local:/音乐/歌曲.flac"
            } else {
                "9007199254740991"
            }
        );
        assert_eq!(next.data["position_ms"], 0);
        assert_eq!(player_state.get_play_mode(), mode);
    }
}

#[test]
fn queue_next_inserts_search_results_and_empty_queues_without_starting_playback() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.restore_playback(&app, saved_queue());
    let detail_state = app.global::<crate::DetailState>();
    let queue_state = app.global::<crate::QueueState>();
    let player_state = app.global::<crate::PlayerState>();
    view.apply(&app, event("page", 0, json!({"kind":"search", "body":{"result":{
        "songCount":2,"songs":[{"id":"9007199254740993","name":"新歌曲","dt":180000}, {"id":"other","name":"另首歌曲","dt":180000}]
    }}})));
    let target = detail_state.get_detail_tracks().row_data(0).unwrap().id;
    drain(&mut rx);
    view.action(&app, "queue-next", target, "");
    assert_eq!(queue_state.get_queue_songs().row_count(), 3);
    assert_eq!(
        queue_state.get_queue_songs().row_data(2).unwrap().id,
        target
    );
    assert!(drain(&mut rx).is_empty());
    // Playing a page replaces the queue through the same module.
    view.action(&app, "play", target, "");
    assert_eq!(queue_state.get_queue_songs().row_count(), 2);
    assert_eq!(player_state.get_current_song_title(), "新歌曲");
    view.action(&app, "remove-queue", target, "");
    assert_eq!(queue_state.get_queue_songs().row_count(), 1);
    assert_eq!(player_state.get_current_song_title(), "另首歌曲");
    view.action(&app, "remove-queue", player_state.get_current_song_id(), "");
    assert_eq!(queue_state.get_queue_songs().row_count(), 0);
    assert_eq!(player_state.get_current_song_id(), -1);
    drain(&mut rx);
    view.action(&app, "queue-next", target, "");
    assert_eq!(queue_state.get_queue_songs().row_count(), 1);
    assert_eq!(
        queue_state.get_queue_songs().row_data(0).unwrap().id,
        target
    );
    assert_eq!(player_state.get_current_song_id(), -1);
    assert!(!player_state.get_is_playing());
    assert!(!player_state.get_is_loading());
    assert!(drain(&mut rx).is_empty());
    view.action(&app, "step", 1, "");
    let play = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "play")
        .unwrap();
    assert_eq!(play.data["song"]["id"], "9007199254740993");
}

#[test]
fn playback_restore_waits_for_play_and_keeps_progress_across_idle_and_stale_snapshots() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.restore_playback(&app, saved_queue());
    let queue_state = app.global::<crate::QueueState>();
    let player_state = app.global::<crate::PlayerState>();
    assert_eq!(queue_state.get_queue_songs().row_count(), 2);
    assert_eq!(
        queue_state.get_queue_songs().row_data(0).unwrap().name,
        "网络歌曲"
    );
    assert_eq!(player_state.get_current_song_title(), "本地歌曲");
    assert!(player_state.get_current_song_is_local());
    assert_eq!(player_state.get_current_time_str(), "1:05");
    assert!((player_state.get_progress_percent() - 65_432.0 / 2400.0).abs() < 0.001);
    assert!(!player_state.get_is_playing());
    assert!(!player_state.get_is_loading());
    assert!(drain(&mut rx).iter().all(|cmd| cmd.name != "play"));

    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":0,"play_token":0,"playing":false}),
        ),
    );
    assert_eq!(player_state.get_current_time_str(), "1:05");
    view.seek(&app, 50.0);
    assert_eq!(player_state.get_current_time_str(), "2:00");
    assert!(
        drain(&mut rx).is_empty(),
        "restored idle song must not seek an unloaded engine"
    );
    view.action(&app, "toggle", -1, "");
    let play = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "play")
        .unwrap();
    assert_eq!(play.data["song"]["id"], saved_queue().current_id.unwrap());
    assert_eq!(play.data["position_ms"], 120_000);
    view.apply(
        &app,
        event("play", play.token, json!({"song":play.data["song"]})),
    );
    view.apply(
        &app,
        event("snapshot", 0, json!({"progress":0,"play_token":0})),
    );
    assert_eq!(player_state.get_current_time_str(), "2:00");
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":121_250,"play_token":play.token,"playing":true}),
        ),
    );
    assert_eq!(player_state.get_current_time_str(), "2:01");
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":0,"play_token":0,"playing":false}),
        ),
    );
    assert!(
        player_state.get_is_playing(),
        "late idle snapshot paused the current song"
    );
    assert_eq!(player_state.get_current_time_str(), "2:01");
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":0,"play_token":play.token,"playing":false,"active":false}),
        ),
    );
    assert_eq!(
        player_state.get_current_time_str(),
        "2:01",
        "stopped engine erased saved progress"
    );
    view.save_playback_if_changed(&app, true);
    let saved = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    let saved: PlaybackState = serde_json::from_value(saved.data).unwrap();
    assert_eq!(saved.position_ms, 121_250);
    assert_eq!(saved.queue, saved_queue().queue);

    // A seek followed immediately by closing must retain the requested position.
    view.seek(&app, 75.0);
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":121_250,"play_token":play.token,"playing":true}),
        ),
    );
    view.save_playback_if_changed(&app, true);
    let saved = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    assert_eq!(saved.data["position_ms"], 180_000);

    view.action(&app, "step", 1, "");
    let next = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "play")
        .unwrap();
    assert_eq!(next.data["song"]["id"], "9007199254740991");
    assert_eq!(next.data["position_ms"], 0);
    view.apply(
        &app,
        event("play", next.token, json!({"song":next.data["song"]})),
    );
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress":185_000,"play_token":play.token,"playing":true}),
        ),
    );
    assert_eq!(player_state.get_current_time_str(), "0:00");
    view.save_playback_if_changed(&app, true);
    let saved = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    assert_eq!(saved.data["position_ms"], 0);
}

#[test]
fn playback_state_normalizes_invalid_selection_duplicates_and_progress() {
    let mut saved = saved_queue();
    saved.queue.push(saved.queue[0].clone());
    saved.queue.push(Song::default());
    saved.position_ms = u64::MAX;
    saved.normalize();
    assert_eq!(saved.queue.len(), 2);
    assert_eq!(saved.position_ms, 240_000);
    saved.current_id = Some("missing".into());
    saved.normalize();
    assert_eq!(saved.current_id.as_deref(), Some("9007199254740991"));
    assert_eq!(saved.position_ms, 0);
    saved.queue.clear();
    saved.position_ms = 100;
    saved.normalize();
    assert_eq!(saved.current_id, None);
    assert_eq!(saved.position_ms, 0);
}

#[test]
fn playback_progress_is_saved_periodically_and_failed_writes_can_retry() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.restore_playback(&app, saved_queue());
    view.action(&app, "toggle", -1, "");
    let play = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "play")
        .unwrap();
    view.apply(
        &app,
        event("play", play.token, json!({"song": play.data["song"]})),
    );
    view.save_playback_if_changed(&app, false);
    assert!(
        drain(&mut rx).is_empty(),
        "unchanged saved state was rewritten"
    );
    view.apply(
        &app,
        event(
            "snapshot",
            0,
            json!({"progress": 70_000, "playing": true, "play_token": play.token}),
        ),
    );
    view.save_playback_if_changed(&app, false);
    assert!(
        drain(&mut rx).is_empty(),
        "progress writes were not throttled"
    );
    std::thread::sleep(Duration::from_millis(2050));
    view.save_playback_if_changed(&app, false);
    let write = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    assert_eq!(write.data["position_ms"], 70_000);
    view.apply(
        &app,
        Event {
            error: Some("disk full".into()),
            ..event("save-playback", write.token, Value::Null)
        },
    );
    view.save_playback_if_changed(&app, true);
    let retry = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    assert_eq!(retry.data, write.data);

    view.action(&app, "logout", -1, "");
    view.save_playback_if_changed(&app, true);
    let cleared = drain(&mut rx)
        .into_iter()
        .find(|cmd| cmd.name == "save-playback")
        .unwrap();
    assert_eq!(
        serde_json::from_value::<PlaybackState>(cleared.data).unwrap(),
        PlaybackState::default()
    );
}

struct EmptyApi;
impl MusicApi for EmptyApi {
    fn call(&self, operation: &str, _params: Value, _cookie: &str) -> ApiFuture<'_> {
        assert_ne!(
            operation, "song_url",
            "startup must not automatically start playback"
        );
        Box::pin(async {
            Ok(ApiResponse {
                status: 200,
                body: json!({"code":200}),
                cookie: vec![],
            })
        })
    }
}

#[test]
fn playback_queue_edits_and_last_seek_survive_runtime_shutdown_and_restart() {
    i_slint_backend_testing::init_no_event_loop();
    let path = temp("playback-restore");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    storage.save("playback.json", &saved_queue()).unwrap();
    let preferences = Preferences {
        visual: json!({"play_mode":"random"}),
        ..Preferences::default()
    };
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let runtime = ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        storage.clone(),
        preferences,
        Some(Arc::new(EmptyApi)),
    )
    .unwrap();
    let player_state = app.global::<crate::PlayerState>();
    let queue_state = app.global::<crate::QueueState>();
    let runtime_state = app.global::<crate::RuntimeState>();
    assert_eq!(player_state.get_current_time_str(), "1:05");
    assert_eq!(player_state.get_play_mode(), "random");
    let first = queue_state.get_queue_songs().row_data(0).unwrap().id;
    runtime_state.invoke_runtime_action("queue-next".into(), first, "".into());
    player_state.invoke_seek_requested(50.0);
    // No event-loop tick: Drop must send the final state before shutdown.
    drop(runtime);
    let saved: PlaybackState = storage.read("playback.json").unwrap();
    assert_eq!(saved.queue.len(), 2);
    assert_eq!(saved.queue[0].id, "local:/音乐/歌曲.flac");
    assert_eq!(saved.queue[1].id, "9007199254740991");
    assert_eq!(saved.position_ms, 120_000);

    let restored = AppWindow::new().unwrap();
    let restored_windows = crate::windows::ApplicationWindows::attach(&restored).unwrap();
    let runtime = ApplicationRuntime::attach_with(
        &restored,
        &restored_windows._settings,
        storage.clone(),
        storage.preferences().unwrap(),
        Some(Arc::new(EmptyApi)),
    )
    .unwrap();
    let queue_state = restored.global::<crate::QueueState>();
    let player_state = restored.global::<crate::PlayerState>();
    let runtime_state = restored.global::<crate::RuntimeState>();
    assert_eq!(queue_state.get_queue_songs().row_count(), 2);
    assert_eq!(
        queue_state.get_queue_songs().row_data(0).unwrap().name,
        "本地歌曲"
    );
    assert_eq!(
        queue_state.get_queue_songs().row_data(1).unwrap().name,
        "网络歌曲"
    );
    assert_eq!(player_state.get_current_song_title(), "本地歌曲");
    assert_eq!(player_state.get_current_time_str(), "2:00");
    assert!(!player_state.get_is_playing());
    // Idle backend snapshots arrive while the saved song is still unloaded.
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(10));
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(60));
    }
    assert_eq!(player_state.get_current_time_str(), "2:00");
    let network = queue_state.get_queue_songs().row_data(1).unwrap().id;
    runtime_state.invoke_runtime_action("remove-queue".into(), network, "".into());
    assert_eq!(queue_state.get_queue_songs().row_count(), 1);
    assert_eq!(player_state.get_current_time_str(), "2:00");
    runtime_state.invoke_runtime_action("clear-queue".into(), -1, "".into());
    drop(runtime);
    assert_eq!(
        storage.read::<PlaybackState>("playback.json").unwrap(),
        PlaybackState::default()
    );

    let empty = AppWindow::new().unwrap();
    let empty_windows = crate::windows::ApplicationWindows::attach(&empty).unwrap();
    let runtime = ApplicationRuntime::attach_with(
        &empty,
        &empty_windows._settings,
        storage.clone(),
        Preferences::default(),
        Some(Arc::new(EmptyApi)),
    )
    .unwrap();
    assert_eq!(
        empty
            .global::<crate::QueueState>()
            .get_queue_songs()
            .row_count(),
        0
    );
    assert_eq!(
        empty.global::<crate::PlayerState>().get_current_song_id(),
        -1
    );
    drop(runtime);
    std::fs::write(path.join("playback.json"), "{broken").unwrap();
    let corrupt = AppWindow::new().unwrap();
    let corrupt_windows = crate::windows::ApplicationWindows::attach(&corrupt).unwrap();
    let runtime = ApplicationRuntime::attach_with(
        &corrupt,
        &corrupt_windows._settings,
        storage.clone(),
        Preferences::default(),
        Some(Arc::new(EmptyApi)),
    )
    .unwrap();
    assert!(corrupt
        .global::<crate::RuntimeState>()
        .get_runtime_message()
        .contains("playback.json"));
    assert_eq!(
        corrupt
            .global::<crate::QueueState>()
            .get_queue_songs()
            .row_count(),
        0
    );
    drop(runtime);
    assert_eq!(
        std::fs::read_to_string(path.join("playback.json")).unwrap(),
        "{broken"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn song_actions_use_explicit_targets_and_preserve_favorite_guards() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    view::initialize(&app, &Preferences::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(tx, Preferences::default());
    view.restore_playback(&app, saved_queue());
    let queue_state = app.global::<crate::QueueState>();
    let player_state = app.global::<crate::PlayerState>();
    let playlist_state = app.global::<crate::PlaylistState>();
    let song_menu_state = app.global::<crate::SongMenuState>();
    let login_state = app.global::<crate::LoginState>();
    let account_state = app.global::<crate::AccountState>();
    let song = queue_state.get_queue_songs().row_data(0).unwrap().id;
    let local = player_state.get_current_song_id();
    view.apply(
        &app,
        event(
            "page",
            0,
            json!({"kind":"playlist", "id":"9007199254740993",
        "body":{"playlist":{"name":"目标歌单", "tracks":[]}}}),
        ),
    );
    let playlist = playlist_state.get_active_playlist_id();
    drain(&mut rx);
    // The menu and current song deliberately point at another song.
    song_menu_state.set_open_menu_song_id(local);
    view.action(&app, "add-track", playlist, &song.to_string());
    let add = drain(&mut rx);
    assert_eq!(add.len(), 1);
    assert_eq!(
        add[0].data,
        json!({"op":"playlist_track_add", "params":{
            "pid":"9007199254740993", "tracks":["9007199254740991"]
        }})
    );
    view.action(&app, "add-track", playlist, "");
    view.action(&app, "add-track", playlist, "invalid");
    assert!(
        drain(&mut rx).is_empty(),
        "missing target must not silently select current song"
    );

    view.action(&app, "favorite", song, "");
    assert!(login_state.get_is_login_open());
    assert!(drain(&mut rx).is_empty());
    account_state.set_is_logged_in(true);
    view.action(&app, "favorite", local, "");
    assert!(drain(&mut rx).is_empty());
    view.action(&app, "favorite", song, "");
    view.action(&app, "favorite", song, "");
    let liked = drain(&mut rx);
    assert_eq!(
        liked.len(),
        1,
        "an in-flight favorite mutation must not be duplicated"
    );
    assert_eq!(
        liked[0].data,
        json!({"op":"like", "params":{"id":"9007199254740991", "like":true}})
    );
}
