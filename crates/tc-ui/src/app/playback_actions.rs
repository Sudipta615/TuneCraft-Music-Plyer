//! Playback delegation methods for TuneCraftApp.

use super::{ToastLevel, TuneCraftApp};

impl TuneCraftApp {
    pub fn play_track(&mut self, track_id: i64) {
        let track = self.ctx.library.get_track(track_id);

        if let Some(track) = track {
            let is_fav = self.ctx.library.is_favorite(track_id);
            // v3.1.1: Only bump play_count if the track actually loaded.
            // Previously record_play was called unconditionally after
            // play_track, which silently returned early on engine.load_track
            // failure — the user got no playback but the play counter went
            // up, and `current_track_id` ended up pointing at the previous
            // track (or None).
            match self.ctx.playback.play_track(&track, is_fav) {
                Ok(()) => {
                    self.ctx.library.record_play(track_id);
                    self.sync_from_playback_service();
                },
                Err(e) => {
                    self.push_toast(format!("Failed to play track: {}", e), ToastLevel::Error);
                },
            }
        } else {
            self.push_toast("Track not found in library", ToastLevel::Error);
        }
    }

    pub fn toggle_playback(&mut self) {
        // Bug #4 fix: both the "is_playing" and "not playing but has track"
        // branches called the same service method (toggle_playback), making
        // the conditional logic redundant. Simplified: if there is a current
        // track (or one is playing), delegate to the service's toggle_playback
        // which handles both pause and resume internally. Only when there is
        // no current track do we attempt to navigate and start from the queue.
        if self.current_track_id.is_some() || self.is_playing {
            self.ctx.playback.toggle_playback();
        } else if !self.play_queue.is_empty() {
            if let Some(track_id) = self.ctx.playback.navigate_next() {
                self.play_track(track_id);
                return;
            }
        }
        self.sync_from_playback_service();
    }

    pub fn play_next(&mut self) {
        // Bug #8 fix: distinguish between empty queue (nothing to play)
        // and repeat-off-end (reached end of queue with RepeatMode::Off).
        // Previously both cases called stop_playback(), which fully stops
        // and resets. When the queue is non-empty but we hit the end with
        // repeat off, we should just pause playback so the user can resume
        // or navigate back, instead of losing their position entirely.
        match self.ctx.playback.navigate_next() {
            Some(track_id) => {
                self.play_track(track_id);
            },
            None => {
                if self.play_queue.is_empty() {
                    // Truly empty queue — full stop
                    self.ctx.playback.stop_playback();
                } else {
                    // Reached end of queue with repeat off — just pause
                    // so the user can go back or change repeat mode
                    if self.is_playing {
                        self.ctx.playback.toggle_playback();
                    }
                }
                self.sync_from_playback_service();
            },
        }
    }

    pub fn play_prev(&mut self) {
        if let Some(track_id) = self.ctx.playback.navigate_prev() {
            if Some(track_id) == self.current_track_id {
                self.seek(0.0);
                self.position_secs = 0.0;
                self.ctx.playback.reset_play_started_at();
                self.sync_from_playback_service();
            } else {
                self.play_track(track_id);
            }
        }
    }

    pub fn stop_playback(&mut self) {
        self.ctx.playback.stop_playback();
        self.sync_from_playback_service();
    }

    pub fn seek(&self, pos_secs: f32) {
        self.ctx.playback.seek(pos_secs);
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(track_id) = self.current_track_id {
            let new_state = self
                .ctx
                .library
                .toggle_favorite(track_id, self.is_favorited);
            self.is_favorited = new_state;
            self.ctx.playback.set_favorited(new_state);
        }
    }

    /// Set volume.
    ///
    pub fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        // If the volume is being set to something audible other than via
        // `toggle_mute`'s restore (which clears `volume_before_mute` via
        // `.take()` before calling this), drop any stale "muted" memory so
        // a later Mute press doesn't ignore a manual slider change made
        // while supposedly muted.
        if clamped > 0.0 {
            self.volume_before_mute = None;
        }
        self.volume = clamped;
        self.ctx.playback.set_volume(clamped);
        self.ctx.config.write(|c| c.playback.volume = clamped);
    }

    /// Toggle mute, remembering the actual volume that was set before
    /// muting so un-mute restores it exactly.
    ///
    /// v3.1.2: Previously this was inlined at each call site as
    /// `if volume > 0.0 { 0.0 } else { 0.7 }` — un-mute always landed on a
    /// hardcoded 0.7 instead of whatever the user's volume actually was
    /// (e.g. muting at 0.3 and un-muting jumped to 0.7, not back to 0.3).
    pub fn toggle_mute(&mut self) {
        match self.volume_before_mute.take() {
            Some(previous) => {
                // Currently muted — restore.
                self.set_volume(previous);
            },
            None => {
                // Currently audible — remember it and mute. If volume is
                // already 0 (e.g. user dragged the slider down manually),
                // there's nothing meaningful to remember or restore, so
                // just leave it at 0 and don't store a "previous" value.
                if self.volume > 0.0 {
                    self.volume_before_mute = Some(self.volume);
                    self.set_volume(0.0);
                }
            },
        }
    }

    pub fn set_speed(&mut self, speed: f32) {
        let clamped = speed.clamp(0.25, 4.0);
        self.speed = clamped;
        self.ctx.playback.set_speed(clamped);
        self.ctx.config.write(|c| c.playback.speed = clamped);
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.ctx.playback.set_shuffle(self.shuffle);
        let shuffle = self.shuffle;
        self.ctx.config.write(|c| {
            c.playback.shuffle = shuffle;
        });
        self.sync_from_playback_service();
    }

    pub fn set_repeat(&mut self, repeat: super::RepeatMode) {
        self.repeat = repeat;
        self.ctx.playback.set_repeat(repeat);
        self.ctx.config.write(|c| {
            c.playback.repeat = repeat;
        });
    }

    pub fn toggle_repeat(&mut self) {
        let next = match self.repeat {
            super::RepeatMode::Off => super::RepeatMode::All,
            super::RepeatMode::All => super::RepeatMode::One,
            super::RepeatMode::One => super::RepeatMode::Off,
        };
        self.set_repeat(next);
    }
}
