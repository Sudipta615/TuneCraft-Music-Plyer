//! Local offline scrobble service.
//!
//! A fully offline play journal stored in SQLite.  No network calls, no credentials,
//! no API keys — every listen is recorded locally and permanently.
//!
//! ## Architecture
//!
//! ```text
//! Engine (playback thread)
//!   └─ crosses 50%/4-min threshold
//!        └─ ScrobbleService::record()   ← called from UI tick
//!             ├─ INSERT INTO scrobbles
//!             └─ UPSERT INTO listening_stats
//! ```
//!
//! `ScrobbleService` is synchronous — it owns a direct `rusqlite::Connection`
//! clone via the existing `tc_db::Database` handle.  All DB writes happen on
//! the UI thread during the per-frame tick, which is exactly what the rest of
//! the app does for library mutations.  Scrobble writes are tiny (one INSERT +
//! one UPSERT) and complete in < 1 ms, so they cannot cause frame drops.
//!
//! ## Events
//!
//! The service emits [`ScrobbleEvent`] values through a `std::sync::mpsc`
//! channel that the UI polls each frame to show toast notifications.

use std::{
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{info, warn};

/// Events emitted by the scrobble service for UI feedback (toasts).
#[derive(Debug, Clone)]
pub enum ScrobbleEvent {
    /// A listen was successfully recorded to the local journal.
    Recorded { artist: String, track: String },
    /// Recording failed (DB error — should be rare).
    Failed {
        artist: String,
        track: String,
        error: String,
    },
}

/// A single completed-listen record to be journalled.
#[derive(Debug, Clone)]
pub struct LocalScrobbleEntry {
    pub track_id: i64,
    pub artist: String,
    pub track: String,
    pub duration_played_secs: f64,
}

/// Offline local scrobble service.

/// Drop-in replacement for the old `ScrobbleService` from the user's
/// perspective: call [`record`] when the play threshold is crossed;
/// poll [`try_recv_event`] each frame for toast feedback.
pub struct ScrobbleService {
    enabled: bool,
    db: std::sync::Arc<tc_db::Database>,
    event_tx: mpsc::Sender<ScrobbleEvent>,
    event_rx: std::sync::Mutex<mpsc::Receiver<ScrobbleEvent>>,
}

impl ScrobbleService {
    /// Create a new local scrobble service.
    pub fn new(db: std::sync::Arc<tc_db::Database>, enabled: bool) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            enabled,
            db,
            event_tx,
            event_rx: std::sync::Mutex::new(event_rx),
        }
    }

    /// Whether local scrobbling is active.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable local scrobbling at runtime.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Record a completed listen to the local journal.
    ///
    /// Inserts one row into `scrobbles` and upserts `listening_stats`.
    /// Also updates the legacy `play_count` and `last_played` columns on
    /// the `tracks` table so existing UI queries continue to work unchanged.
    ///
    /// Called from the UI tick after the 50%-or-4-minute threshold is
    /// crossed.  All operations are synchronous and complete in < 1 ms.
    pub fn record(&self, entry: LocalScrobbleEntry) {
        if !self.enabled {
            return;
        }

        let played_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let result = self.db.with_connection(|conn| {
            // 1. Append to the permanent play journal.
            conn.execute(
                "INSERT INTO scrobbles (track_id, played_at, duration_played_secs, completed)
                 VALUES (?1, ?2, ?3, 1)",
                rusqlite::params![entry.track_id, played_at, entry.duration_played_secs],
            )?;

            // 2. Upsert materialised aggregate stats.
            conn.execute(
                "INSERT INTO listening_stats
                     (track_id, play_count, total_seconds_listened, first_played_at, last_played_at)
                 VALUES (?1, 1, ?2, ?3, ?3)
                 ON CONFLICT(track_id) DO UPDATE SET
                     play_count             = play_count + 1,
                     total_seconds_listened = total_seconds_listened + excluded.total_seconds_listened,
                     first_played_at        = COALESCE(first_played_at, excluded.first_played_at),
                     last_played_at         = excluded.last_played_at",
                rusqlite::params![entry.track_id, entry.duration_played_secs, played_at],
            )?;

            // 3. Keep legacy tracks columns in sync (UI already reads these).
            conn.execute(
                "UPDATE tracks SET
                     play_count  = play_count + 1,
                     last_played = datetime(?1, 'unixepoch')
                 WHERE id = ?2",
                rusqlite::params![played_at, entry.track_id],
            )?;

            Ok(())
        });

        match result {
            Ok(()) => {
                info!(
                    "Local scrobble recorded: {} – {}",
                    entry.artist, entry.track
                );
                let _ = self.event_tx.send(ScrobbleEvent::Recorded {
                    artist: entry.artist,
                    track: entry.track,
                });
            },
            Err(e) => {
                warn!("Failed to record local scrobble: {}", e);
                let _ = self.event_tx.send(ScrobbleEvent::Failed {
                    artist: entry.artist,
                    track: entry.track,
                    error: e.to_string(),
                });
            },
        }
    }

    /// Poll for the next pending scrobble event (non-blocking).
    ///
    /// Call once per UI frame.  Returns `None` when the queue is empty.
    pub fn try_recv_event(&self) -> Option<ScrobbleEvent> {
        self.event_rx.lock().ok().and_then(|rx| rx.try_recv().ok())
    }

    // -----------------------------------------------------------------------
    // Statistics queries — all read from listening_stats for O(1) lookups.
    // -----------------------------------------------------------------------

    /// Top N most-played tracks (by completed play count).
    pub fn top_tracks(&self, limit: usize) -> Vec<TopTrack> {
        self.db
            .with_connection(|conn| {
                let mut stmt = conn.prepare(
                "SELECT t.id, t.title, t.artist, t.album, ls.play_count, ls.total_seconds_listened
                 FROM listening_stats ls
                 JOIN tracks t ON t.id = ls.track_id
                 ORDER BY ls.play_count DESC
                 LIMIT ?1",
            )?;
                let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok(TopTrack {
                        track_id: row.get(0)?,
                        title: row.get(1)?,
                        artist: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        album: row.get(3)?,
                        play_count: row.get(4)?,
                        total_seconds_listened: row.get(5)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
    }

    /// Top N most-listened artists (by total seconds listened).
    pub fn top_artists(&self, limit: usize) -> Vec<TopArtist> {
        self.db
            .with_connection(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT t.artist,
                        COUNT(DISTINCT t.id)    AS track_count,
                        SUM(ls.play_count)       AS total_plays,
                        SUM(ls.total_seconds_listened) AS total_secs
                 FROM listening_stats ls
                 JOIN tracks t ON t.id = ls.track_id
                 WHERE t.artist IS NOT NULL AND t.artist != ''
                 GROUP BY t.artist
                 ORDER BY total_secs DESC
                 LIMIT ?1",
                )?;
                let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok(TopArtist {
                        artist: row.get(0)?,
                        track_count: row.get(1)?,
                        total_plays: row.get(2)?,
                        total_seconds_listened: row.get(3)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
    }

    /// Listening history for a date range (Unix timestamps, inclusive).
    ///
    /// Returns one row per scrobble, newest first.
    pub fn history_in_range(&self, from: i64, to: i64) -> Vec<HistoryEntry> {
        self.db
            .with_connection(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT s.id, s.track_id, t.title, t.artist, t.album,
                        s.played_at, s.duration_played_secs
                 FROM scrobbles s
                 JOIN tracks t ON t.id = s.track_id
                 WHERE s.played_at BETWEEN ?1 AND ?2
                   AND s.completed = 1
                 ORDER BY s.played_at DESC",
                )?;
                let rows = stmt.query_map(rusqlite::params![from, to], |row| {
                    Ok(HistoryEntry {
                        scrobble_id: row.get(0)?,
                        track_id: row.get(1)?,
                        title: row.get(2)?,
                        artist: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        album: row.get(4)?,
                        played_at: row.get(5)?,
                        duration_played_secs: row.get(6)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
    }

    /// Total listening time in seconds across all recorded plays.
    pub fn total_listening_secs(&self) -> f64 {
        self.db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT COALESCE(SUM(total_seconds_listened), 0.0) FROM listening_stats",
                    [],
                    |row| row.get::<_, f64>(0),
                )
            })
            .unwrap_or(0.0)
    }

    /// Number of unique days with at least one completed play (listening streak base).
    pub fn active_day_count(&self) -> u32 {
        self.db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT COUNT(DISTINCT date(played_at, 'unixepoch'))
                 FROM scrobbles WHERE completed = 1",
                    [],
                    |row| row.get::<_, u32>(0),
                )
            })
            .unwrap_or(0)
    }

    /// Current listening streak: consecutive days ending today with ≥ 1 play.
    ///
    /// Returns 0 if nothing was played today.
    pub fn current_streak_days(&self) -> u32 {
        // Compute the streak entirely inside a single SQL query.
        //
        // Previous implementation fetched up to 366 day strings and then
        // fired one additional DB round-trip per consecutive pair to check
        // adjacency — O(N) connections per call, where N ≤ 366.
        //
        // The replacement uses SQLite's `julianday()` to convert each
        // distinct listen-day to a Julian day number, then checks for
        // gaps by comparing each day to its predecessor via a window
        // function (LAG).  All arithmetic stays inside a single
        // `with_connection` closure → one DB round-trip.
        self.db
            .with_connection(|conn| {
                conn.query_row(
                    "WITH listen_days AS (
                     SELECT DISTINCT date(played_at, 'unixepoch') AS d
                     FROM scrobbles
                     WHERE completed = 1
                 ),
                 today_check AS (
                     SELECT MAX(d) AS latest FROM listen_days
                 ),
                 -- Bail early when the most-recent play was not today.
                 numbered AS (
                     SELECT d,
                            julianday(d) AS jd,
                            ROW_NUMBER() OVER (ORDER BY d DESC) AS rn
                     FROM listen_days
                 ),
                 with_gap AS (
                     SELECT d, jd, rn,
                            LAG(jd) OVER (ORDER BY d DESC) AS prev_jd
                     FROM numbered
                 ),
                 -- A gap exists when the day difference is > 1.
                 -- Row 1 always has prev_jd = NULL, so COALESCE forces 1 (no gap).
                 streak_rows AS (
                     SELECT d, jd, rn,
                            CASE WHEN COALESCE(prev_jd - jd, 1) > 1 THEN 1 ELSE 0 END AS gap
                     FROM with_gap
                 )
                 SELECT
                     CASE WHEN (SELECT date('now') != latest FROM today_check)
                          THEN 0
                          ELSE (SELECT COUNT(*) FROM streak_rows WHERE rn <= (
                               SELECT COALESCE(MIN(rn), 366)
                               FROM streak_rows
                               WHERE gap = 1
                          ) - 1 OR (SELECT MIN(rn) FROM streak_rows WHERE gap = 1) IS NULL)
                     END AS streak",
                    [],
                    |row| row.get::<_, u32>(0),
                )
            })
            .unwrap_or(0)
    }

    /// "On this day" — tracks played on this calendar day in previous years.
    pub fn on_this_day(&self) -> Vec<HistoryEntry> {
        self.db
            .with_connection(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT s.id, s.track_id, t.title, t.artist, t.album,
                        s.played_at, s.duration_played_secs
                 FROM scrobbles s
                 JOIN tracks t ON t.id = s.track_id
                 WHERE strftime('%m-%d', s.played_at, 'unixepoch')
                       = strftime('%m-%d', 'now')
                   AND strftime('%Y',    s.played_at, 'unixepoch')
                       < strftime('%Y',  'now')
                   AND s.completed = 1
                 ORDER BY s.played_at DESC
                 LIMIT 50",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(HistoryEntry {
                        scrobble_id: row.get(0)?,
                        track_id: row.get(1)?,
                        title: row.get(2)?,
                        artist: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        album: row.get(4)?,
                        played_at: row.get(5)?,
                        duration_played_secs: row.get(6)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
    }
}

/// No-op stubs for now-playing / clear-now-playing called by PlaybackService,
/// and availability query used by AppContext.
///
/// These are needed because the offline scrobble service doesn't have a
/// remote server to notify, but PlaybackService always calls them.
impl ScrobbleService {
    /// Returns true — the offline journal is always available (no network
    /// dependency). Mirrors the naming used in AppContext::init() checks.
    pub fn is_available(&self) -> bool {
        true
    }

    /// Update the "now playing" status (no-op for offline scrobble).
    pub fn update_now_playing(&self, _artist: &str, _title: &str, _album: Option<&str>) {
        // No-op: offline scrobble service has no remote server to notify.
    }

    /// Clear the "now playing" status (no-op for offline scrobble).
    pub fn clear_now_playing(&self) {
        // No-op: offline scrobble service has no remote server to notify.
    }
}

// ---------------------------------------------------------------------------
// Data transfer types
// ---------------------------------------------------------------------------

/// A track entry in the most-played list.
#[derive(Debug, Clone)]
pub struct TopTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub play_count: u32,
    pub total_seconds_listened: f64,
}

/// An artist entry in the most-listened list.
#[derive(Debug, Clone)]
pub struct TopArtist {
    pub artist: String,
    pub track_count: u32,
    pub total_plays: u32,
    pub total_seconds_listened: f64,
}

/// One row from the listening history.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub scrobble_id: i64,
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub played_at: i64, // Unix timestamp
    pub duration_played_secs: f64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unix_now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    // Helper: create an in-memory DB with the full schema applied.
    fn test_db() -> std::sync::Arc<tc_db::Database> {
        let db = tc_db::Database::open_in_memory().expect("in-memory DB");
        std::sync::Arc::new(db)
    }

    fn insert_test_track(db: &tc_db::Database, title: &str, artist: &str) -> i64 {
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, duration_secs, format, file_size, file_modified)
                 VALUES (?1, ?2, ?3, 180.0, 'flac', 0, 0)",
                rusqlite::params![format!("/music/{}.flac", title), title, artist],
            )?;
            Ok(conn.last_insert_rowid())
        }).expect("insert track")
    }

    #[test]
    fn test_record_increments_play_count() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Northern Lights", "Aurora Glass");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Aurora Glass".into(),
            track: "Northern Lights".into(),
            duration_played_secs: 200.0,
        });

        let play_count: u32 = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT play_count FROM tracks WHERE id = ?1",
                    rusqlite::params![track_id],
                    |r| r.get(0),
                )
            })
            .unwrap();

        assert_eq!(play_count, 1, "play_count should be 1 after one record()");
    }

    #[test]
    fn test_record_multiple_increments() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Electric Pulse", "Neon Drive");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        for _ in 0..5 {
            svc.record(LocalScrobbleEntry {
                track_id,
                artist: "Neon Drive".into(),
                track: "Electric Pulse".into(),
                duration_played_secs: 200.0,
            });
        }

        let play_count: u32 = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT play_count FROM listening_stats WHERE track_id = ?1",
                    rusqlite::params![track_id],
                    |r| r.get(0),
                )
            })
            .unwrap();

        assert_eq!(play_count, 5);
    }

    #[test]
    fn test_disabled_service_does_not_record() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Rainy Afternoon", "Velvet Echo");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), false); // disabled

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Velvet Echo".into(),
            track: "Rainy Afternoon".into(),
            duration_played_secs: 200.0,
        });

        let count: i64 = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM scrobbles WHERE track_id = ?1",
                    rusqlite::params![track_id],
                    |r| r.get(0),
                )
            })
            .unwrap();

        assert_eq!(count, 0, "disabled service should not write scrobbles");
    }

    #[test]
    fn test_record_emits_recorded_event() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Golden Hour", "Sunfield");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Sunfield".into(),
            track: "Golden Hour".into(),
            duration_played_secs: 180.0,
        });

        let event = svc.try_recv_event().expect("should have one event");
        match event {
            ScrobbleEvent::Recorded { artist, track } => {
                assert_eq!(artist, "Sunfield");
                assert_eq!(track, "Golden Hour");
            },
            other => panic!("expected Recorded, got {:?}", other),
        }
    }

    #[test]
    fn test_total_listening_secs() {
        let db = test_db();
        let t1 = insert_test_track(&db, "Track A", "Artist A");
        let t2 = insert_test_track(&db, "Track B", "Artist B");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id: t1,
            artist: "Artist A".into(),
            track: "Track A".into(),
            duration_played_secs: 180.0,
        });
        svc.record(LocalScrobbleEntry {
            track_id: t2,
            artist: "Artist B".into(),
            track: "Track B".into(),
            duration_played_secs: 240.0,
        });

        let total = svc.total_listening_secs();
        assert!(
            (total - 420.0).abs() < 0.01,
            "total should be 420s, got {}",
            total
        );
    }

    #[test]
    fn test_top_tracks_ordering() {
        let db = test_db();
        let t1 = insert_test_track(&db, "Popular Song", "Big Artist");
        let t2 = insert_test_track(&db, "Obscure Track", "Small Artist");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        // Play t1 three times, t2 once.
        for _ in 0..3 {
            svc.record(LocalScrobbleEntry {
                track_id: t1,
                artist: "Big Artist".into(),
                track: "Popular Song".into(),
                duration_played_secs: 200.0,
            });
        }
        svc.record(LocalScrobbleEntry {
            track_id: t2,
            artist: "Small Artist".into(),
            track: "Obscure Track".into(),
            duration_played_secs: 200.0,
        });

        let top = svc.top_tracks(10);
        assert!(!top.is_empty());
        assert_eq!(top[0].track_id, t1, "most played should be first");
        assert_eq!(top[0].play_count, 3);
    }

    #[test]
    fn test_top_artists() {
        let db = test_db();
        let t1 = insert_test_track(&db, "Song 1", "Arijit Singh");
        let t2 = insert_test_track(&db, "Song 2", "Arijit Singh");
        let t3 = insert_test_track(&db, "Song 3", "Other Artist");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        for _ in 0..4 {
            svc.record(LocalScrobbleEntry {
                track_id: t1,
                artist: "Arijit Singh".into(),
                track: "Song 1".into(),
                duration_played_secs: 250.0,
            });
        }
        svc.record(LocalScrobbleEntry {
            track_id: t2,
            artist: "Arijit Singh".into(),
            track: "Song 2".into(),
            duration_played_secs: 200.0,
        });
        svc.record(LocalScrobbleEntry {
            track_id: t3,
            artist: "Other Artist".into(),
            track: "Song 3".into(),
            duration_played_secs: 180.0,
        });

        let artists = svc.top_artists(10);
        assert!(!artists.is_empty());
        assert_eq!(artists[0].artist, "Arijit Singh");
    }

    #[test]
    fn test_history_in_range() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Drift", "Pale Motion");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        let before = unix_now() - 1;
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Pale Motion".into(),
            track: "Drift".into(),
            duration_played_secs: 360.0,
        });
        let after = unix_now() + 1;

        let history = svc.history_in_range(before, after);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "Drift");
    }

    #[test]
    fn test_active_day_count() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Canopy", "Moss & Wire");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        // No plays yet.
        assert_eq!(svc.active_day_count(), 0);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Moss & Wire".into(),
            track: "Canopy".into(),
            duration_played_secs: 260.0,
        });

        assert_eq!(svc.active_day_count(), 1);
    }

    #[test]
    fn test_set_enabled_toggles_recording() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Override", "Test");
        let mut svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Test".into(),
            track: "Override".into(),
            duration_played_secs: 200.0,
        });
        svc.set_enabled(false);
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Test".into(),
            track: "Override".into(),
            duration_played_secs: 200.0,
        });

        let count: i64 = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM scrobbles WHERE track_id = ?1",
                    rusqlite::params![track_id],
                    |r| r.get(0),
                )
            })
            .unwrap();

        assert_eq!(
            count, 1,
            "only the first (enabled) record should have written"
        );
    }

    #[test]
    fn test_first_and_last_played_timestamps() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Timestamp Test", "Artist");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        let t_start = unix_now();
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Artist".into(),
            track: "Timestamp Test".into(),
            duration_played_secs: 200.0,
        });
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Artist".into(),
            track: "Timestamp Test".into(),
            duration_played_secs: 200.0,
        });
        let t_end = unix_now();

        let (first, last): (i64, i64) = db
            .with_connection(|conn| {
                conn.query_row(
                "SELECT first_played_at, last_played_at FROM listening_stats WHERE track_id = ?1",
                rusqlite::params![track_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            })
            .unwrap();

        assert!(first >= t_start, "first_played_at should be >= t_start");
        assert!(last <= t_end, "last_played_at should be <= t_end");
        assert!(last >= first, "last_played_at should be >= first_played_at");
    }

    // ── v0.29.0: Scrobble streak tests ────────────────────────────────────
    //
    // These tests exercise the `current_streak_days()` SQL query, which
    // computes a consecutive-day streak ending today using window functions.
    // This is one of the most complex queries in the scrobble path and
    // previously had zero test coverage.

    #[test]
    fn test_streak_zero_when_no_plays() {
        let db = test_db();
        let svc = ScrobbleService::new(db, true);
        // No scrobbles recorded at all.
        assert_eq!(
            svc.current_streak_days(),
            0,
            "streak should be 0 with no plays"
        );
    }

    #[test]
    fn test_streak_one_after_single_play_today() {
        let db = test_db();
        let track_id = insert_test_track(&db, "Streak Today", "Streaker");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Streaker".into(),
            track: "Streak Today".into(),
            duration_played_secs: 200.0,
        });

        // After recording one play today (system time), streak should be 1.
        let streak = svc.current_streak_days();
        assert_eq!(
            streak, 1,
            "streak should be 1 after playing today, got {}",
            streak
        );
    }

    #[test]
    fn test_streak_zero_when_last_play_was_yesterday() {
        // Insert a scrobble with a played_at timestamp from yesterday.
        // The streak query only counts consecutive days ending today,
        // so a single play yesterday → streak = 0.
        let db = test_db();
        let track_id = insert_test_track(&db, "Yesterday Play", "Late Listener");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        // Insert a scrobble with yesterday's timestamp directly.
        let yesterday = unix_now() - 86400; // 86400 = seconds in a day
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO scrobbles (track_id, played_at, duration_played_secs, completed)
                 VALUES (?1, ?2, 200.0, 1)",
                rusqlite::params![track_id, yesterday],
            )?;
            // Also update listening_stats manually.
            conn.execute(
                "INSERT INTO listening_stats (track_id, play_count, total_seconds_listened, first_played_at, last_played_at)
                 VALUES (?1, 1, 200.0, ?2, ?2)",
                rusqlite::params![track_id, yesterday],
            )?;
            Ok::<(), rusqlite::Error>(())
        }).unwrap();

        let streak = svc.current_streak_days();
        assert_eq!(
            streak, 0,
            "streak should be 0 when last play was yesterday, got {}",
            streak
        );
    }

    #[test]
    fn test_streak_multi_day_consecutive() {
        // Insert scrobbles for today and yesterday → streak should be 2.
        let db = test_db();
        let track_id = insert_test_track(&db, "Consecutive", "Dedicated");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        let now = unix_now();
        let yesterday = now - 86400;

        // Insert yesterday's scrobble.
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO scrobbles (track_id, played_at, duration_played_secs, completed)
                 VALUES (?1, ?2, 200.0, 1)",
                rusqlite::params![track_id, yesterday],
            )
        })
        .unwrap();

        // Insert today's scrobble via the normal record() path.
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Dedicated".into(),
            track: "Consecutive".into(),
            duration_played_secs: 200.0,
        });

        let streak = svc.current_streak_days();
        assert!(
            streak >= 2,
            "streak should be >= 2 with consecutive day plays, got {}",
            streak
        );
    }

    #[test]
    fn test_streak_breaks_on_gap() {
        // Insert scrobbles for today and 3 days ago (gap of 2 days) →
        // streak should only be 1 (today), since the gap breaks it.
        let db = test_db();
        let track_id = insert_test_track(&db, "Gap Play", "Breaker");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        let now = unix_now();
        let three_days_ago = now - (86400 * 3);

        // Insert a scrobble 3 days ago.
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO scrobbles (track_id, played_at, duration_played_secs, completed)
                 VALUES (?1, ?2, 200.0, 1)",
                rusqlite::params![track_id, three_days_ago],
            )
        })
        .unwrap();

        // Record today.
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Breaker".into(),
            track: "Gap Play".into(),
            duration_played_secs: 200.0,
        });

        let streak = svc.current_streak_days();
        assert_eq!(streak, 1, "streak should be 1 with a gap, got {}", streak);
    }

    #[test]
    fn test_on_this_day_returns_empty_for_first_year() {
        // In the first year of using the app, there are no previous-year
        // plays, so on_this_day() should return empty.
        let db = test_db();
        let track_id = insert_test_track(&db, "New User", "Fresh");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Fresh".into(),
            track: "New User".into(),
            duration_played_secs: 200.0,
        });

        let results = svc.on_this_day();
        assert!(
            results.is_empty(),
            "on_this_day should be empty in first year of use"
        );
    }

    #[test]
    fn test_active_day_count_multiple_days() {
        // Insert scrobbles on two different days.
        let db = test_db();
        let track_id = insert_test_track(&db, "Multi Day", "Regular");
        let svc = ScrobbleService::new(std::sync::Arc::clone(&db), true);

        let now = unix_now();
        let yesterday = now - 86400;

        // Yesterday's scrobble.
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO scrobbles (track_id, played_at, duration_played_secs, completed)
                 VALUES (?1, ?2, 200.0, 1)",
                rusqlite::params![track_id, yesterday],
            )
        })
        .unwrap();

        // Today's scrobble.
        svc.record(LocalScrobbleEntry {
            track_id,
            artist: "Regular".into(),
            track: "Multi Day".into(),
            duration_played_secs: 200.0,
        });

        let days = svc.active_day_count();
        assert!(
            days >= 2,
            "active_day_count should be >= 2 with plays on two days, got {}",
            days
        );
    }
}
