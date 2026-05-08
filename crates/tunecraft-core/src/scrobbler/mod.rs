#[cfg(feature = "lastfm")]
pub mod lastfm;

#[cfg(feature = "lastfm")]
use lastfm::{LastfmClient, ScrobbleEntry};

use crate::config::ScrobbleConfig;
use crate::database::Database;
use anyhow::Result;
use chrono::Utc;
use tracing::{info, warn};

/// Manages scrobble submission logic, enforcing duration and playback
/// percentage thresholds before queueing a track for scrobbling.
///
/// The manager is intentionally decoupled from the [`Database`] — the
/// `check_and_scrobble` and `flush_queue` methods accept a `&Database`
/// reference so that the caller controls the lifetime and the manager
/// stays free of interior-mutex overhead.
#[cfg(feature = "lastfm")]
pub struct ScrobbleManager {
    client: Option<LastfmClient>,
    min_duration_secs: u32,
    min_percent: u32,
}

#[cfg(feature = "lastfm")]
impl ScrobbleManager {
    /// Create a new `ScrobbleManager` with the given scrobble configuration.
    ///
    /// The `LastfmClient` is `None` initially; call [`set_client`] to
    /// provide one after the user has authenticated.
    pub fn new(config: &ScrobbleConfig) -> Self {
        Self {
            client: None,
            min_duration_secs: config.min_duration_secs,
            min_percent: config.min_percent,
        }
    }

    /// Set or replace the [`LastfmClient`] used for submissions.
    ///
    /// Call this after a successful Last.fm authentication flow so that
    /// subsequent `check_and_scrobble` / `flush_queue` calls can submit
    /// scrobbles.
    pub fn set_client(&mut self, client: LastfmClient) {
        self.client = Some(client);
    }

    /// Check whether a track qualifies for scrobbling and, if so, queue it.
    ///
    /// A track is scrobbled when **all** of the following are true:
    /// - The [`LastfmClient`] is authenticated ([`LastfmClient::is_ready`])
    /// - `duration_secs >= min_duration_secs`
    /// - `(accumulated_secs / duration_secs * 100.0) >= min_percent`
    ///
    /// When the criteria are met, a [`ScrobbleEntry`] is created with the
    /// current UTC timestamp and persisted via [`LastfmClient::queue_scrobble`].
    ///
    /// Returns `Ok(())` in all early-return cases (not an error to skip).
    pub fn check_and_scrobble(
        &self,
        track_title: &str,
        artist: &str,
        album: Option<&str>,
        duration_secs: f64,
        accumulated_secs: f64,
        track_id: i64,
        db: &Database,
    ) -> Result<()> {
        let client = match &self.client {
            Some(c) if c.is_ready() => c,
            _ => {
                info!("Scrobble skipped: client not authenticated");
                return Ok(());
            }
        };

        if !duration_secs.is_finite() {
            info!(
                "Scrobble skipped: duration is not finite ({:?})",
                duration_secs
            );
            return Ok(());
        }

        if duration_secs < self.min_duration_secs as f64 {
            info!(
                "Scrobble skipped: duration ({:.1}s) below minimum ({}s)",
                duration_secs, self.min_duration_secs
            );
            return Ok(());
        }

        let play_percent = (accumulated_secs / duration_secs) * 100.0;
        if play_percent < self.min_percent as f64 {
            info!(
                "Scrobble skipped: play percentage ({:.1}%) below minimum ({}%)",
                play_percent, self.min_percent
            );
            return Ok(());
        }

        let entry = ScrobbleEntry {
            track: track_title.to_string(),
            artist: artist.to_string(),
            album: album.map(|s| s.to_string()),
            timestamp: Utc::now().timestamp(),
            duration: Some(duration_secs as u64),
        };

        client.queue_scrobble(&entry, db, track_id)?;
        Ok(())
    }

    /// Flush the scrobble queue: read pending entries from the database,
    /// submit them to Last.fm, and mark successfully submitted entries as done.
    ///
    /// This method is async because it calls [`LastfmClient::process_queue`]
    /// which performs network I/O.
    ///
    /// # Retry semantics
    pub async fn flush_queue(&self, db: &Database) -> Result<()> {
        let client = match &self.client {
            Some(c) if c.is_ready() => c,
            _ => return Ok(()),
        };

        let pending = db.get_pending_scrobbles()?;
        if pending.is_empty() {
            return Ok(());
        }

        let ids: Vec<i64> = pending.iter().map(|(id, _, _, _, _)| *id).collect();
        let entries: Vec<ScrobbleEntry> = pending
            .into_iter()
            .map(|(_, artist, title, album, timestamp)| ScrobbleEntry {
                track: title,
                artist,
                album,
                timestamp,
                duration: None,
            })
            .collect();

        info!("Flushing {} pending scrobble(s)", entries.len());

        let results = client.process_queue(entries).await?;

        let successful_ids: Vec<i64> = ids
            .iter()
            .zip(results.iter())
            .filter(|(_, &success)| success)
            .map(|(id, _)| *id)
            .collect();

        if !successful_ids.is_empty() {
            db.mark_scrobbles_done(&successful_ids)?;
            info!("Marked {} scrobble(s) as done", successful_ids.len());
        }

        let failed_count = ids.len() - successful_ids.len();
        if failed_count > 0 {
            warn!(
                "{} scrobble(s) failed or were ignored; entries will be retried",
                failed_count
            );
        }

        Ok(())
    }
}
