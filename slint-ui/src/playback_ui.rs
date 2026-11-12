use crate::{PlayerState, PlayerViewState};

/// Reset the player display after clearing a queue, for runtime and demo adapters.
pub(crate) fn clear(player: &PlayerState<'_>, view: &PlayerViewState<'_>) {
    crate::fullscreen_palette::apply(view, None);
    view.set_lyrics(Default::default());
    view.set_comments(Default::default());
    player.set_current_song_title("未在播放".into());
    player.set_current_song_artist("".into());
    player.set_current_song_id(-1);
    player.set_current_song_cover(Default::default());
    player.set_current_song_is_local(false);
    player.set_current_song_has_cover(false);
    player.set_current_song_favorite(false);
    player.set_current_time_str("0:00".into());
    player.set_duration_time_str("0:00".into());
    player.set_progress_percent(0.0);
    player.set_buffered_percent(0.0);
    player.set_is_playing(false);
    player.set_is_loading(false);
}
