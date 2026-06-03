//! State synchronization methods for TuneCraftApp.

use super::TuneCraftApp;
use super::toasts::ToastLevel;

impl TuneCraftApp {
    /// Sync UI playback mirrors from PlaybackService.
    ///
    ///
    /// every frame. Only clones play_queue and shuffle_order when the version
    /// has changed since the last sync. Position/progress updates are read
    /// directly from the engine's PlaybackInfo arc.
    pub fn sync_from_playback_service(&mut self) {
        self.ctx.playback.sync_from_engine();
        let state = self.ctx.playback.state();
        self.current_track_id = state.current_track_id;
        self.is_playing = state.is_playing;
        self.is_favorited = state.is_favorited;
        self.position_secs = state.position_secs;
        self.duration_secs = state.duration_secs;
        self.volume = state.volume;
        self.speed = state.speed;
        self.shuffle = state.shuffle;
        self.repeat = state.repeat;
        self.play_started_at = state.play_started_at;

        self.accumulated_play_secs = state.accumulated_play_secs;

        self.resampler_disabled = state.resampler_disabled;

        if state.version != self.last_synced_playback_version {
            self.play_queue = state.play_queue.clone();
            self.play_queue_index = state.play_queue_index;
            self.shuffle_order = state.shuffle_order.clone();
            self.shuffle_position = state.shuffle_position;
            self.last_synced_playback_version = state.version;
        }
    }

    /// Sync UI EQ state mirrors from EqService.
    pub fn sync_from_eq_service(&mut self) {
        let eq_state = self.ctx.eq.state_snapshot();
        // show_eq_panel is managed by the UI (EQ button + back arrow) and written
        // back to EqService.show_panel on change. We do NOT sync it here to avoid
        // overwriting the UI state every frame.
        self.eq_enabled = eq_state.enabled;
        self.eq_bands = eq_state.bands;
        self.eq_preset = eq_state.preset;
        self.eq_preamp = eq_state.preamp;
        self.eq_bass_shelf = eq_state.bass_shelf;
        self.eq_treble_shelf = eq_state.treble_shelf;

        self.eq_stereo_width = eq_state.stereo_width;
        self.eq_balance = eq_state.balance;
        self.eq_dither = eq_state.dither;
        self.eq_midside = eq_state.midside;
        self.cached_dither_enabled = eq_state.cached_dither_enabled;
        self.cached_midside_enabled = eq_state.cached_midside_enabled;
    }

    pub fn save_config_if_dirty(&mut self) {
        self.ctx.config.save_if_dirty();
    }

    pub fn mark_config_dirty(&mut self) {
        self.ctx.config.mark_dirty();
    }

    pub fn compute_badge_counts(&mut self) {
        self.badge_cache = self.ctx.library.compute_badge_counts();
    }

    pub fn refresh_favorite_ids(&mut self) {
        self.ctx.library.refresh_favorite_ids();
        let snapshot = self.ctx.library.snapshot();
        self.cached_favorite_ids = snapshot.favorite_ids.clone();
    }

    pub fn status_message(&self) -> String {
        let snapshot = self.ctx.library.snapshot();
        snapshot.status_message.clone()
    }

    pub fn is_scanning(&self) -> bool {
        let snapshot = self.ctx.library.snapshot();
        snapshot.is_scanning
    }

    ///
    pub fn update_scan_state(&mut self) {
        self.ctx.library.check_scan_state();
        let snapshot = self.ctx.library.snapshot();
        let was_scanning = self.is_scanning;
        self.is_scanning = snapshot.is_scanning;
        self.status_message = snapshot.status_message.clone();

        // When a scan transitions from in-progress to complete, refresh the
        // play queue so newly discovered tracks are immediately playable.
        // Without this, the queue is built once at startup from the initial
        // (possibly empty) library snapshot and never updated.
        if was_scanning && !self.is_scanning {
            self.ctx.library.refresh_tracks();
            let fresh = self.ctx.library.snapshot();
            let new_queue: Vec<i64> = self.ctx.library.get_all_track_ids();
            // Only update the UI track list and queue if the library actually grew.
            if new_queue.len() > self.play_queue.len() {
                self.tracks = fresh.tracks.clone();
                self.total_track_count = fresh.total_track_count;
                // Only replace the queue when nothing is playing so we don't
                // interrupt an active session.
                if self.current_track_id.is_none() {
                    self.ctx.playback.set_play_queue(new_queue.clone());
                    self.play_queue = new_queue;
                }
            }
        }
    }

    ///
    /// and show a toast notification to the user when issues are detected.
    /// Only shows the toast once per warning to avoid spamming.
    pub fn check_dsp_warnings(&mut self) {
        #[cfg(feature = "audio-output")]
        {
            let info = self.ctx.playback.sync_from_engine();
            let _ = info; // sync_from_engine doesn't return a value, we read from state

            let state = self.ctx.playback.state();
            // The resampler_disabled flag is updated every 2s in the engine tick
        }

        // This is done via sync_from_playback_service which reads engine state
        if self.resampler_disabled && !self.dsp_warning_shown {
            self.push_toast(
                "Audio resampler disabled — playback may be at wrong speed/pitch. \
                 Try restarting playback or changing the resampler quality setting.",
                ToastLevel::Error,
            );
            self.dsp_warning_shown = true;
        } else if !self.resampler_disabled {
            self.dsp_warning_shown = false;
        }
    }
}

