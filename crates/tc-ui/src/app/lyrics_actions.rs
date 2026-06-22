//! Lyrics integration helpers for TuneCraftApp.
//!
//! Wires `LyricsService` (LRCLIB HTTP client) into the per-frame update loop:
//! - `maybe_fetch_lyrics` fires a fetch request when the current track changes
//!   and the track has no cached lyrics in the DB.
//! - `poll_lyrics_events` drains the event channel and shows toast feedback.

use crate::app::{toasts::ToastLevel, TuneCraftApp};
use crate::converters::parse_lrc;
use crate::services::lyrics::{LyricsEvent, LyricsRequest};
use crate::App;

impl TuneCraftApp {
    /// Trigger a LRCLIB lyrics fetch when the current track changes and the
    /// user has `lyrics.fetch_on_play` enabled. We skip the fetch if:
    /// - The service is disabled (`lyrics.enabled = false`).
    /// - `fetch_on_play` is false.
    /// - The track has already been fetched in this play session.
    /// - The track has no title (LRCLIB queries by title).
    /// - The track already has synced lyrics in the DB (no network needed).
    pub fn maybe_fetch_lyrics(&mut self) {
        let track_id = match self.current_track_id {
            Some(id) => id,
            None => {
                self.lyrics_fetched_for = None;
                return;
            },
        };

        // Already fetched for this track? Skip.
        if self.lyrics_fetched_for == Some(track_id) {
            return;
        }

        // Mark as fetched early to prevent duplicate requests if the user
        // rapidly changes tracks. The LyricsService also deduplicates on
        // its end, but this is a cheap fast-path.
        self.lyrics_fetched_for = Some(track_id);

        // Find the track in the in-memory cache to get its metadata.
        // We don't go through LibraryService here because the cache is
        // already populated for the current view.
        let track = match self.tracks.iter().find(|t| t.id == track_id) {
            Some(t) => t.clone(),
            None => return,
        };

        // If the track already has synced lyrics in the DB, no need to fetch.
        if track
            .lyrics_synced
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        {
            return;
        }

        // Check config — both `enabled` and `fetch_on_play` must be true.
        let (enabled, fetch_on_play) = self
            .ctx
            .config
            .read(|c| (c.lyrics.enabled, c.lyrics.fetch_on_play))
            .unwrap_or((false, false));
        if !enabled || !fetch_on_play {
            return;
        }

        // Skip tracks with no title — LRCLIB queries by title.
        if track.title.is_empty() {
            return;
        }

        self.ctx.lyrics.fetch_for_track(LyricsRequest {
            track_id,
            artist: track.artist.clone(),
            album: track.album.clone(),
            title: track.title.clone(),
            duration_secs: track.duration_secs,
        });
    }

    /// Poll the lyrics service for completed fetches and show toast feedback.
    /// The actual lyrics text is persisted to the DB by the service itself;
    /// the UI reads it back from the DB when the lyrics panel is opened.
    pub fn poll_lyrics_events(&mut self) {
        while let Some(event) = self.ctx.lyrics.try_recv_event() {
            match event {
                LyricsEvent::Fetched { track_id, synced } => {
                    // Refresh the in-memory track cache so the lyrics panel
                    // sees the new lyrics without a full library refresh.
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.lyrics_synced = Some(synced);
                    }
                    log::debug!("Lyrics fetched for track {}", track_id);
                },
                LyricsEvent::NotFound { track_id } => {
                    log::debug!("No lyrics found on LRCLIB for track {}", track_id);
                },
                LyricsEvent::Failed { track_id, error } => {
                    log::warn!("Lyrics fetch failed for track {}: {}", track_id, error);
                    // Only show a toast if the user is actively viewing the
                    // lyrics panel — otherwise silent failure is fine.
                    // For now we don't track lyrics-panel visibility, so
                    // we silently log and let the UI fall back to "no lyrics".
                    let _ = ToastLevel::Info; // suppress unused-import warning if any
                },
            }
        }
    }
}

/// Sync lyrics panel state to Slint: panel visibility, button availability,
/// the current track title, and parsed/highlighted synced-lyrics lines.
///
/// Bug fix: this was missing entirely after the Slint migration — the
/// `show-lyrics-panel`, `lyrics-available`, `lyrics-lines`, and
/// `lyrics-track-title` properties declared in `app.slint` were never set
/// from Rust, so the lyrics button stayed permanently disabled and the
/// panel never showed any content even after `LyricsService` successfully
/// fetched and cached lyrics.
pub fn sync_lyrics_panel(app: &TuneCraftApp, slint_app: &App) {
    slint_app.set_show_lyrics_panel(app.show_lyrics_panel);
    slint_app.set_lyrics_available(app.current_track_id.is_some());

    let Some(track) = app.current_track() else {
        slint_app.set_lyrics_track_title(slint::SharedString::from(""));
        slint_app.set_lyrics_lines(slint::ModelRc::new(slint::VecModel::from(Vec::<
            crate::LyricsLine,
        >::new())));
        return;
    };

    slint_app.set_lyrics_track_title(slint::SharedString::from(track.title.clone()));

    let lines = match track.lyrics_synced.as_deref() {
        Some(lrc) if !lrc.is_empty() => parse_lrc(lrc),
        _ => Vec::new(),
    };

    // Highlight the last line whose timestamp has already passed.
    let position_ms = (app.position_secs * 1000.0).round() as i64;
    let current_idx = lines.iter().rposition(|(ts, _)| *ts <= position_ms);

    let items: Vec<crate::LyricsLine> = lines
        .iter()
        .enumerate()
        .map(|(i, (ts, text))| {
            crate::converters::lyrics_line_to_item(*ts, text, Some(i) == current_idx)
        })
        .collect();

    slint_app.set_lyrics_lines(slint::ModelRc::new(slint::VecModel::from(items)));
}
