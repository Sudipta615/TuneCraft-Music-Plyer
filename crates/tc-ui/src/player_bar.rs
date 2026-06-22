//! Player bar bridge — syncs playback state to the Slint PlayerBar component.

use slint::SharedString;

use crate::{
    app::TuneCraftApp,
    converters::{empty_player_state, repeat_mode_str},
    App,
};

/// Sync player bar state to Slint.
pub fn sync_player_bar(app: &TuneCraftApp, slint_app: &App) {
    let mut state = empty_player_state();

    if let Some(track) = app.current_track() {
        state.current_track_id = track.id as i32;
        state.title = track.title.clone().into();
        state.artist = track
            .artist
            .clone()
            .unwrap_or_else(|| "Unknown Artist".to_string())
            .into();
        state.album = track.album.clone().unwrap_or_default().into();
        state.has_track = true;
    }

    state.is_playing = app.is_playing;
    state.is_favorited = app.is_favorited;
    state.position_secs = app.position_secs;
    state.duration_secs = app.duration_secs;
    state.volume = app.volume;
    state.shuffle = app.shuffle;
    state.repeat = SharedString::from(repeat_mode_str(app.repeat));
    state.speed = app.speed;

    slint_app.set_player(state);
    slint_app.set_show_eq_panel(app.show_eq_panel);
}
