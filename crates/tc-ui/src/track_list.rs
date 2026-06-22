//! Track list bridge — syncs track data to the Slint TrackList component.

use slint::{ModelRc, SharedString};

use crate::{
    app::TuneCraftApp,
    converters::track_to_item,
    App,
};

/// Build the tracks model for the track list view.
pub fn build_tracks(app: &TuneCraftApp) -> ModelRc<crate::TrackItem> {
    let items: Vec<crate::TrackItem> = app
        .tracks
        .iter()
        .map(|track| {
            let is_playing = app.current_track_id == Some(track.id);
            let is_paused = is_playing && !app.is_playing;
            let is_favorite = app.cached_favorite_ids.contains(&track.id);
            track_to_item(track, is_playing, is_paused, is_favorite, slint::Image::default(), false)
        })
        .collect();
    ModelRc::new(slint::VecModel::from(items))
}

/// Sync track list state to Slint.
pub fn sync_track_list(app: &TuneCraftApp, slint_app: &App) {
    slint_app.set_tracks(build_tracks(app));
    slint_app.set_total_track_count(app.total_track_count as i32);
    slint_app.set_current_page(app.track_page as i32);
    slint_app.set_tracks_per_page(app.tracks_per_page as i32);
    slint_app.set_sort_active(app.sort_active);
    slint_app.set_sort_ascending(app.sort_ascending);
    slint_app.set_filter_favorites(app.filter_favorites);
    slint_app.set_list_view(app.list_view);
    slint_app.set_search_query(SharedString::from(app.search_query.clone()));
}
