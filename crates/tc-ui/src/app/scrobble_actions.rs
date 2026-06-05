//! Scrobble-related actions for TuneCraftApp.
//!
//! Wires the playback threshold check to the local offline scrobble service.
//! No network calls, no credentials required.

use log::info;

use crate::app::ToastLevel;
use crate::services::{
    playback::ScrobbleCheck,
    scrobble::{LocalScrobbleEntry, ScrobbleEvent},
};

impl super::TuneCraftApp {
    /// Called every UI frame: checks whether the play threshold has been
    /// crossed and, if so, records the listen to the local journal.
    pub fn check_and_scrobble(&mut self) {
        match self
            .ctx
            .playback
            .check_scrobble_threshold(&mut self.last_scrobbled_track_id)
        {
            ScrobbleCheck::Ready(track_id) => {
                if self.scrobble_enabled {
                    // Resolve full track metadata for the journal entry.
                    if let Some(track) = self.ctx.library.get_track(track_id) {
                        let artist = track.artist.clone().unwrap_or_default();
                        let title = track.title.clone();
                        let duration_secs = self.ctx.playback.state().position_secs;

                        self.ctx.scrobble.record(LocalScrobbleEntry {
                            track_id,
                            artist: artist.clone(),
                            track: title.clone(),
                            duration_played_secs: duration_secs,
                        });

                        info!("Local scrobble queued: {} by {}", title, artist);
                    }
                }
            },
            ScrobbleCheck::NotYet => {},
            ScrobbleCheck::Error(msg) => {
                log::debug!("Scrobble check skipped: {}", msg);
            },
        }
    }

    /// Poll the local scrobble service for events and show toast notifications.
    ///
    /// Called once per frame from the main update loop.
    pub fn poll_scrobble_events(&mut self) {
        while let Some(event) = self.ctx.scrobble.try_recv_event() {
            self.handle_scrobble_event(event);
        }
    }

    /// Convert a scrobble event into a toast notification.
    pub(crate) fn handle_scrobble_event(&mut self, event: ScrobbleEvent) {
        match event {
            ScrobbleEvent::Recorded { artist, track } => {
                self.push_toast(
                    format!("Saved: {} by {}", track, artist),
                    ToastLevel::Success,
                );
            },
            ScrobbleEvent::Failed {
                artist,
                track,
                error,
            } => {
                self.push_toast(
                    format!("Could not save listen: {} by {} — {}", track, artist, error),
                    ToastLevel::Error,
                );
            },
        }
    }
}
