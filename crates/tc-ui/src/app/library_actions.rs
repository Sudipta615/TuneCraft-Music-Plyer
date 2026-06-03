//! Library and polling delegation methods for TuneCraftApp.

use super::TuneCraftApp;
use super::ToastLevel;

impl TuneCraftApp {
    pub fn refresh_tracks(&mut self) {
        self.ctx.library.refresh_tracks();
        let snapshot = self.ctx.library.snapshot();
        self.tracks = snapshot.tracks.clone();
        self.cached_favorite_ids = snapshot.favorite_ids.clone();
        self.total_track_count = snapshot.total_track_count;
        self.badge_cache = self.ctx.library.compute_badge_counts();
    }

    pub fn reload_playlists(&mut self) {
        self.ctx.library.refresh_playlists();
        let snapshot = self.ctx.library.snapshot();
        self.playlists = snapshot.playlists.clone();
    }

    pub fn create_playlist(&mut self, name: &str) {
        match self.ctx.library.create_playlist(name) {
            Ok(id) => {
                self.selected_playlist_id = Some(id);
                self.push_toast(format!("Playlist '{}' created", name), ToastLevel::Success);
                let snapshot = self.ctx.library.snapshot();
                self.playlists = snapshot.playlists.clone();
            }
            Err(e) => {
                self.push_toast(e, ToastLevel::Error);
            }
        }
    }

    pub fn add_current_track_to_playlist(&mut self, playlist_id: i64) {
        if let Some(track_id) = self.current_track_id {
            match self.ctx.library.add_track_to_playlist(playlist_id, track_id) {
                Ok(()) => self.push_toast("Track added to playlist", ToastLevel::Success),
                Err(e) => self.push_toast(e, ToastLevel::Error),
            }
        }
    }

    /// Poll media key actions from the platform service.
    pub fn poll_media_keys(&mut self) {
        while let Some(action) = self.ctx.platform.try_recv_action() {
            use tc_platform::MediaKeyAction;
            match action {
                MediaKeyAction::Play => {
                    if !self.is_playing {
                        self.toggle_playback();
                    }
                }
                MediaKeyAction::Pause => {
                    if self.is_playing {
                        self.toggle_playback();
                    }
                }
                MediaKeyAction::PlayPause => {
                    self.toggle_playback();
                }
                MediaKeyAction::Next => {
                    self.play_next();
                }
                MediaKeyAction::Previous => {
                    self.play_prev();
                }
                MediaKeyAction::Stop => {
                    self.stop_playback();
                }
                _ => {}
            }
        }
    }

    /// Poll lyrics results from the async fetch task.
    pub fn poll_lyrics(&mut self) {
        if self.ctx.lyrics.poll_results() {
            let state = self.ctx.lyrics.state();
            self.current_lyrics = state.current_lyrics.clone();
            self.lyrics_loading = state.loading;
        }
    }
}

