//! Track repository — all track-related database operations.
//!
//! This module contains every method on [`Database`] that reads from or
//! writes to the `tracks` table (including FTS queries). The constant
//! `TRACK_COLUMNS` and the `row_to_track()` helper also live here
//! because they are track-specific.

use rusqlite::params;

use super::{Database, DbError};
use crate::models::*;

/// Centralized column list for the `tracks` table.
/// Use this constant instead of repeating the 32-column list in every SELECT.
/// Any schema change to the tracks table only requires updating this single
/// constant and the `row_to_track()` mapping function.
pub const TRACK_COLUMNS: &str = "id, path, title, artist, album, album_artist, genre, year, \
    track_number, disc_number, duration_secs, sample_rate, channels, bitrate_kbps, format, \
    file_size, file_modified, crc32, replaygain_track_db, replaygain_album_db, \
    replaygain_track_peak, replaygain_album_peak, ebu_r128_loudness, ebu_r128_peak, \
    bpm, lyrics_synced, lyrics_unsynced, last_played, play_count, date_added, date_scanned";

pub(crate) fn prefixed_track_columns(prefix: &str) -> String {
    TRACK_COLUMNS
        .split(", ")
        .map(|c| format!("{}.{}", prefix, c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Map a database row to a Track struct using named column access.
/// Uses column names instead of indices for robustness against
/// schema changes and column reordering.
pub(crate) fn row_to_track(row: &rusqlite::Row<'_>) -> Result<Track, rusqlite::Error> {
    Ok(Track {
        id: row.get("id")?,
        path: row.get("path")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        album_artist: row.get("album_artist")?,
        genre: row.get("genre")?,
        year: row.get("year")?,
        track_number: row.get("track_number")?,
        disc_number: row.get("disc_number")?,
        duration_secs: row.get("duration_secs")?,
        sample_rate: row.get("sample_rate")?,
        channels: row.get("channels")?,
        bitrate_kbps: row.get("bitrate_kbps")?,
        format: row.get("format")?,
        file_size: row.get("file_size")?,
        file_modified: row.get("file_modified")?,
        crc32: row.get("crc32")?,
        replaygain_track_db: row.get("replaygain_track_db")?,
        replaygain_album_db: row.get("replaygain_album_db")?,
        replaygain_track_peak: row.get("replaygain_track_peak")?,
        replaygain_album_peak: row.get("replaygain_album_peak")?,
        ebu_r128_loudness: row.get("ebu_r128_loudness")?,
        ebu_r128_peak: row.get("ebu_r128_peak")?,
        bpm: row.get("bpm")?,
        lyrics_synced: row.get("lyrics_synced")?,
        lyrics_unsynced: row.get("lyrics_unsynced")?,
        last_played: row.get("last_played")?,
        play_count: row.get("play_count")?,
        date_added: row.get("date_added")?,
        date_scanned: row.get("date_scanned")?,
    })
}

/// Helper: filter and log database row mapping errors consistently.
/// Standardizes error handling across all query_map chains so that
/// errors are always logged and incomplete results are visible.
pub(crate) fn log_and_filter<T>(result: Result<T, rusqlite::Error>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            log::warn!("Failed to map database row: {}", e);
            None
        },
    }
}

impl Database {
    /// Insert a track, using UPSERT to handle conflicts on the `path` column.
    ///
    ///
    /// columns (bpm, replaygain_*, ebu_r128_*). These fields are
    /// computed by tc-analysis and tc-engine after the initial scan, and
    /// overwriting them during re-insertion would destroy expensive analysis
    /// results. Only file-derived metadata (title, artist, album, duration,
    /// format, file_size, etc.) is updated on conflict.
    pub fn insert_track(&self, track: &Track) -> Result<i64, DbError> {
        if track.path.is_empty() {
            return Err(DbError::Validation("Track path must not be empty".into()));
        }

        let conn = self.write_lock()?;
        // Use INSERT ... ON CONFLICT UPDATE (UPSERT) instead of INSERT OR REPLACE.
        // relationships, invalidates row IDs, and can cause hard-to-debug side effects.
        // UPSERT keeps the same rowid and preserves user data columns.
        //
        // ON CONFLICT UPDATE only sets file-derived metadata columns.
        // Analysis columns (bpm, replaygain_*, ebu_r128_*) are NOT
        // overwritten on conflict — they are preserved from any prior
        // analysis run. Only the INSERT path sets these to the values
        // provided in the Track struct (typically NULL for new tracks).
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, album_artist, genre, year, track_number, disc_number, duration_secs, sample_rate, channels, bitrate_kbps, format, file_size, file_modified, crc32, replaygain_track_db, replaygain_album_db, replaygain_track_peak, replaygain_album_peak, ebu_r128_loudness, ebu_r128_peak, bpm, play_count, date_scanned)\
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)\
             ON CONFLICT(path) DO UPDATE SET \
             title=excluded.title, artist=excluded.artist, album=excluded.album,\
             album_artist=excluded.album_artist, genre=excluded.genre, year=excluded.year,\
             track_number=excluded.track_number, disc_number=excluded.disc_number,\
             duration_secs=excluded.duration_secs, sample_rate=excluded.sample_rate,\
             channels=excluded.channels, bitrate_kbps=excluded.bitrate_kbps,\
             format=excluded.format, file_size=excluded.file_size,\
             file_modified=excluded.file_modified, crc32=excluded.crc32,\
             date_scanned=excluded.date_scanned",
            params![
                track.path, track.title, track.artist, track.album, track.album_artist,
                track.genre, track.year, track.track_number, track.disc_number,
                track.duration_secs, track.sample_rate, track.channels, track.bitrate_kbps,
                track.format, track.file_size, track.file_modified, track.crc32,
                track.replaygain_track_db, track.replaygain_album_db,
                track.replaygain_track_peak, track.replaygain_album_peak,
                track.ebu_r128_loudness, track.ebu_r128_peak,
                track.bpm, track.play_count,
                track.date_scanned,
            ],
        )?;
        // FTS is kept in sync by SQL triggers (tracks_ai, tracks_au), so no
        // manual FTS insert/delete is needed here.  The triggers fire
        // automatically for both INSERT and UPDATE paths.
        let id: i64 = conn.query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            params![track.path],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn get_track(&self, id: i64) -> Result<Option<Track>, DbError> {
        let sql = format!("SELECT {} FROM tracks WHERE id = ?1", TRACK_COLUMNS);
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let result = stmt.query_row(params![id], row_to_track);
        match result {
            Ok(track) => Ok(Some(track)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    pub fn get_track_by_path(&self, path: &str) -> Result<Option<Track>, DbError> {
        let sql = format!("SELECT {} FROM tracks WHERE path = ?1", TRACK_COLUMNS);
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let result = stmt.query_row(params![path], row_to_track);
        match result {
            Ok(track) => Ok(Some(track)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    pub fn search_tracks(&self, query: &str, limit: i64) -> Result<Vec<Track>, DbError> {
        if limit < 0 {
            return Err(DbError::Validation(
                "Search limit must not be negative".into(),
            ));
        }
        // special to FTS5 (quotes, colons, asterisks) and split on
        // whitespace to AND individual tokens.  Each token gets a
        // trailing * for prefix matching.
        let tokens: Vec<String> = query
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| {
                // Strip FTS5-special characters (quotes, colons, asterisks) but
                // preserve hyphens and apostrophes within words, as they are
                // common in track/artist names (e.g., "Rock-n-Roll", "O'Brien").
                let cleaned: String = t
                    .chars()
                    .filter(|c| {
                        c.is_alphanumeric() || *c == '-' || *c == '\'' || *c == '.' || *c == '&'
                    })
                    .collect();
                if cleaned.is_empty() {
                    return String::new();
                }
                format!("{}*", cleaned)
            })
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let fts_query = tokens.join(" ");
        let sql = format!(
            "SELECT {} FROM tracks_fts f JOIN tracks t ON t.id = f.rowid WHERE tracks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            prefixed_track_columns("t")
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![fts_query, limit], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    pub fn get_all_tracks(&self, limit: i64, offset: i64) -> Result<Vec<Track>, DbError> {
        if limit < 0 {
            return Err(DbError::Validation("Limit must not be negative".into()));
        }
        if offset < 0 {
            return Err(DbError::Validation("Offset must not be negative".into()));
        }
        let sql = format!(
            "SELECT {} FROM tracks ORDER BY title LIMIT ?1 OFFSET ?2",
            TRACK_COLUMNS
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![limit, offset], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    pub fn get_tracks_by_album(&self, album: &str) -> Result<Vec<Track>, DbError> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE album = ?1 ORDER BY disc_number, track_number",
            TRACK_COLUMNS
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![album], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    pub fn update_play_count(&self, id: i64) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn update_loudness_meta(&self, id: i64, meta: &LoudnessMeta) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "UPDATE tracks SET replaygain_track_db = ?1, replaygain_album_db = ?2, replaygain_track_peak = ?3, replaygain_album_peak = ?4, ebu_r128_loudness = ?5, ebu_r128_peak = ?6 WHERE id = ?7",
            params![
                meta.replaygain_track_db,
                meta.replaygain_album_db,
                meta.replaygain_track_peak,
                meta.replaygain_album_peak,
                meta.ebu_r128_loudness,
                meta.ebu_r128_peak,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_bpm(&self, id: i64, bpm: f32) -> Result<(), DbError> {
        self.write_lock()?
            .execute("UPDATE tracks SET bpm = ?1 WHERE id = ?2", params![bpm, id])?;
        Ok(())
    }

    /// Delete a track by ID.
    ///
    ///
    /// denormalized counters (track_count, duration_secs) on all playlists
    /// that contained this track, and cleans up albums that have no
    /// remaining tracks. Previously, callers had to manually call
    /// `reconcile_aggregates()` to fix these counters.
    pub fn delete_track(&self, id: i64) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        // FTS is kept in sync by the tracks_ad trigger, so we only delete
        // from the main tracks table.  CASCADE handles playlist_tracks,
        // waveform_cache, and cover_art.
        tx.execute("DELETE FROM tracks WHERE id = ?1", params![id])?;

        tx.execute(
            "UPDATE playlists SET \
             track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = playlists.id), \
             duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = playlists.id), 0.0), \
             date_modified = CURRENT_TIMESTAMP",
            [],
        )?;

        // `album_id` column in the tracks table. The correct columns are
        // `tracks.album` (TEXT) and `tracks.album_artist` (TEXT), which must
        // be matched against `albums.title` and `albums.artist`.
        tx.execute(
            "DELETE FROM albums WHERE id NOT IN (\
             SELECT DISTINCT a.id FROM albums a \
             INNER JOIN tracks t ON t.album = a.title AND COALESCE(t.album_artist, '') = COALESCE(a.artist, '')\
             )",
            [],
        ).ok(); // Best-effort — don't fail the delete if album cleanup has issues

        // Best-effort: remove orphaned artists (those with no remaining tracks).
        tx.execute(
            "DELETE FROM artists WHERE name NOT IN (\
             SELECT DISTINCT artist FROM tracks WHERE artist IS NOT NULL AND artist != '')",
            [],
        )
        .ok();

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(())
    }

    pub fn track_count(&self) -> Result<i64, DbError> {
        let count: i64 = self
            .read_lock()?
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Get all tracks (no pagination)
    pub fn get_all_tracks_no_limit(&self) -> Result<Vec<Track>, DbError> {
        let sql = format!("SELECT {} FROM tracks ORDER BY title", TRACK_COLUMNS);
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map([], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    /// Get all track IDs ordered by title
    pub fn get_all_track_ids(&self) -> Result<Vec<i64>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached("SELECT id FROM tracks ORDER BY title")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(log_and_filter)
            .collect();
        Ok(ids)
    }

    /// Increment play count and update last_played for a track.
    ///
    /// This is a convenience alias for [`update_play_count`].
    #[deprecated(since = "0.8.9", note = "Use `update_play_count` directly instead")]
    pub fn increment_play_count(&self, id: i64) -> Result<(), DbError> {
        self.update_play_count(id)
    }

    /// Update last_played timestamp for a track
    pub fn update_last_played(&self, id: i64) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "UPDATE tracks SET last_played = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Update lyrics for a track.
    ///
    /// Each field is updated independently: passing `Some(value)` updates the
    /// column, while `None` leaves the existing value unchanged. This prevents
    /// synced lyrics from accidentally overwriting unsynced text or vice versa.
    pub fn update_lyrics(
        &self,
        id: i64,
        synced: Option<&str>,
        unsynced: Option<&str>,
    ) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        if let Some(synced_text) = synced {
            conn.execute(
                "UPDATE tracks SET lyrics_synced = ?1 WHERE id = ?2",
                params![synced_text, id],
            )?;
        }
        if let Some(unsynced_text) = unsynced {
            conn.execute(
                "UPDATE tracks SET lyrics_unsynced = ?1 WHERE id = ?2",
                params![unsynced_text, id],
            )?;
        }
        Ok(())
    }

    /// Update an existing track's metadata while preserving user data columns.
    ///
    /// The following columns are preserved (not overwritten):
    /// - `play_count`, `last_played`, `date_added` — user interaction data
    /// - `lyrics_synced`, `lyrics_unsynced` — user/lyrics-provider data
    /// - `bpm` — analysis results (set via `update_bpm`)
    /// - `replaygain_*`, `ebu_r128_*` — loudness analysis (set via `update_loudness_meta`)
    ///
    /// Only file-derived metadata (title, artist, album, duration, format,
    /// file_size, etc.) and `date_scanned` are updated.
    pub fn update_track_preserving_userdata(&self, track: &Track) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        conn.execute(
            "UPDATE tracks SET title=?1, artist=?2, album=?3, album_artist=?4, genre=?5, year=?6, \
             track_number=?7, disc_number=?8, duration_secs=?9, sample_rate=?10, channels=?11, \
             bitrate_kbps=?12, format=?13, file_size=?14, file_modified=?15, crc32=?16, \
             bpm=COALESCE(NULLIF(?17, NULL), bpm), \
             replaygain_track_db=COALESCE(NULLIF(?18, NULL), replaygain_track_db), \
             replaygain_album_db=COALESCE(NULLIF(?19, NULL), replaygain_album_db), \
             replaygain_track_peak=COALESCE(NULLIF(?20, NULL), replaygain_track_peak), \
             replaygain_album_peak=COALESCE(NULLIF(?21, NULL), replaygain_album_peak), \
             ebu_r128_loudness=COALESCE(NULLIF(?22, NULL), ebu_r128_loudness), \
             ebu_r128_peak=COALESCE(NULLIF(?23, NULL), ebu_r128_peak), \
             lyrics_synced=COALESCE(NULLIF(?24, ''), lyrics_synced), \
             lyrics_unsynced=COALESCE(NULLIF(?25, ''), lyrics_unsynced), \
             date_scanned=?26 WHERE path=?27",
            params![
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.genre,
                track.year,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.sample_rate,
                track.channels,
                track.bitrate_kbps,
                track.format,
                track.file_size,
                track.file_modified,
                track.crc32,
                track.bpm,
                track.replaygain_track_db,
                track.replaygain_album_db,
                track.replaygain_track_peak,
                track.replaygain_album_peak,
                track.ebu_r128_loudness,
                track.ebu_r128_peak,
                track.lyrics_synced,
                track.lyrics_unsynced,
                track.date_scanned,
                track.path,
            ],
        )?;
        // FTS is kept in sync by the tracks_au trigger, so no manual FTS
        // tracks_au AFTER UPDATE trigger which handles FTS deletion and
        // re-insertion automatically.
        Ok(())
    }

    /// Get tracks with their stored file modification timestamps.
    ///
    /// Returns `(id, path, file_modified)` for every track that has a
    /// non-null `file_modified` value.  The caller should compare each
    /// timestamp against the file's current mtime on disk to decide
    /// which tracks need rescanning.  This method does **not** return
    /// only tracks that have been modified — it returns all tracks
    /// that *have* a stored modification time.
    pub fn get_tracks_with_mtime(&self) -> Result<Vec<(i64, String, i64)>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT id, path, file_modified FROM tracks WHERE file_modified IS NOT NULL",
        )?;
        let results = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .filter_map(log_and_filter)
            .collect();
        Ok(results)
    }

    /// Delete tracks whose files no longer exist on disk.
    /// Uses batched deletion to avoid exceeding SQLite's variable limit.
    ///
    ///
    /// counters and cleans up zombie albums (albums with no remaining tracks).
    pub fn cleanup_missing_tracks(&self, existing_paths: &[&str]) -> Result<usize, DbError> {
        if existing_paths.is_empty() {
            log::warn!(
                "cleanup_missing_tracks called with empty paths — refusing to delete all tracks"
            );
            return Ok(0);
        }

        // SQLite's default SQLITE_MAX_VARIABLE_NUMBER is 999 for older versions
        // and 32766 for newer ones. We batch at 500 to be safe across all versions.
        const BATCH_SIZE: usize = 500;

        // then delete tracks not in that table. This avoids the variable limit.
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        tx.execute(
            "CREATE TEMP TABLE IF NOT EXISTS _existing_paths (path TEXT NOT NULL)",
            [],
        )?;
        tx.execute("DELETE FROM _existing_paths", [])?;

        for chunk in existing_paths.chunks(BATCH_SIZE) {
            let placeholders: Vec<String> = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let sql = format!(
                "INSERT INTO _existing_paths (path) VALUES ({})",
                placeholders.join("), (")
            );
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk
                .iter()
                .map(|p| p as &dyn rusqlite::types::ToSql)
                .collect();
            tx.execute(&sql, params.as_slice())?;
        }

        let deleted = tx.execute(
            "DELETE FROM tracks WHERE path NOT IN (SELECT path FROM _existing_paths)",
            [],
        )?;

        // Clean up temp table
        tx.execute("DROP TABLE IF EXISTS _existing_paths", [])?;

        tx.execute(
            "UPDATE playlists SET \
             track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = playlists.id), \
             duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = playlists.id), 0.0), \
             date_modified = CURRENT_TIMESTAMP",
            [],
        )?;

        tx.execute(
            "DELETE FROM albums WHERE (title, artist) NOT IN (\
             SELECT album, album_artist FROM tracks \
             WHERE album IS NOT NULL AND album != '' \
             GROUP BY album, album_artist)",
            [],
        )
        .ok(); // Best-effort cleanup

        // Best-effort: remove orphaned artists (those with no remaining tracks).
        tx.execute(
            "DELETE FROM artists WHERE name NOT IN (\
             SELECT DISTINCT artist FROM tracks WHERE artist IS NOT NULL AND artist != '')",
            [],
        )
        .ok();

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(deleted)
    }

    /// Get tracks that haven't been analyzed yet (no BPM)
    pub fn get_unanalyzed_tracks(&self) -> Result<Vec<Track>, DbError> {
        let sql = format!("SELECT {} FROM tracks WHERE bpm IS NULL", TRACK_COLUMNS);
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map([], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    /// Get tracks that are missing any kind of analysis (BPM, EBU R128
    /// loudness, or ReplayGain). This is the superset of `get_unanalyzed_tracks`
    /// — it also returns tracks that have BPM but lack loudness metadata,
    /// which is the common case for libraries created before v3.0.0
    /// (when the loudness columns existed but were never populated).
    ///
    /// Use this for the "force analysis" / background-analysis code paths
    /// to ensure loudness backfill for pre-existing libraries.
    pub fn get_tracks_missing_analysis(&self) -> Result<Vec<Track>, DbError> {
        let sql = format!(
            "SELECT {} FROM tracks \
             WHERE bpm IS NULL \
                OR replaygain_track_db IS NULL \
                OR ebu_r128_loudness IS NULL",
            TRACK_COLUMNS
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map([], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    /// Get all tracks by a specific artist
    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>, DbError> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE artist = ?1 ORDER BY album, disc_number, track_number",
            TRACK_COLUMNS
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![artist], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    /// Insert multiple tracks in a single transaction (batch write).
    ///
    /// This is ~25x faster than individual `insert_track()` calls during scan
    /// because it acquires the write lock once, opens a single transaction,
    /// and reuses the prepared statement for all inserts.
    ///
    ///
    /// columns (bpm, replaygain_*, ebu_r128_*), consistent with
    /// `insert_track()`. Only file-derived metadata is updated on conflict.
    ///
    /// Returns a list of `(track_index, track_id)` for successfully inserted tracks.
    /// Tracks that fail to insert are logged and skipped.
    pub fn insert_tracks_batch(&self, tracks: &[Track]) -> Result<Vec<(usize, i64)>, DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        let mut stmt = tx.prepare_cached(
            "INSERT INTO tracks (path, title, artist, album, album_artist, genre, year, track_number, disc_number, duration_secs, sample_rate, channels, bitrate_kbps, format, file_size, file_modified, crc32, replaygain_track_db, replaygain_album_db, replaygain_track_peak, replaygain_album_peak, ebu_r128_loudness, ebu_r128_peak, bpm, play_count, date_scanned)\
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)\
             ON CONFLICT(path) DO UPDATE SET \
             title=excluded.title, artist=excluded.artist, album=excluded.album,\
             album_artist=excluded.album_artist, genre=excluded.genre, year=excluded.year,\
             track_number=excluded.track_number, disc_number=excluded.disc_number,\
             duration_secs=excluded.duration_secs, sample_rate=excluded.sample_rate,\
             channels=excluded.channels, bitrate_kbps=excluded.bitrate_kbps,\
             format=excluded.format, file_size=excluded.file_size,\
             file_modified=excluded.file_modified, crc32=excluded.crc32,\
             date_scanned=excluded.date_scanned"
        ).map_err(DbError::Sqlite)?;

        let mut id_stmt = tx
            .prepare_cached("SELECT id FROM tracks WHERE path = ?1")
            .map_err(DbError::Sqlite)?;

        let mut results = Vec::with_capacity(tracks.len());
        for (i, track) in tracks.iter().enumerate() {
            if track.path.is_empty() {
                log::warn!("Batch insert skipped track at index {}: empty path", i);
                continue;
            }
            match stmt.execute(params![
                track.path,
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.genre,
                track.year,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.sample_rate,
                track.channels,
                track.bitrate_kbps,
                track.format,
                track.file_size,
                track.file_modified,
                track.crc32,
                track.replaygain_track_db,
                track.replaygain_album_db,
                track.replaygain_track_peak,
                track.replaygain_album_peak,
                track.ebu_r128_loudness,
                track.ebu_r128_peak,
                track.bpm,
                track.play_count,
                track.date_scanned,
            ]) {
                Ok(_) => {
                    if let Ok(id) = id_stmt.query_row(params![track.path], |r| r.get::<_, i64>(0)) {
                        results.push((i, id));
                    }
                },
                Err(e) => {
                    log::warn!("Batch insert failed for track {}: {}", track.path, e);
                },
            }
        }

        drop(stmt);
        drop(id_stmt);
        tx.commit().map_err(DbError::Sqlite)?;
        Ok(results)
    }

    /// Update multiple tracks preserving user data in a single transaction (batch write).
    ///
    /// Similar performance benefit as `insert_tracks_batch()` — acquires the
    /// write lock once and reuses the prepared statement.
    ///
    ///
    /// preserving existing non-NULL values when the new value is NULL.
    /// This matches the behavior of `update_track_preserving_userdata()`.
    pub fn update_tracks_batch_preserving_userdata(
        &self,
        tracks: &[Track],
    ) -> Result<usize, DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        let mut stmt = tx.prepare_cached(
            "UPDATE tracks SET title=?1, artist=?2, album=?3, album_artist=?4, genre=?5, year=?6, \
             track_number=?7, disc_number=?8, duration_secs=?9, sample_rate=?10, channels=?11, \
             bitrate_kbps=?12, format=?13, file_size=?14, file_modified=?15, crc32=?16, \
             bpm=COALESCE(NULLIF(?17, NULL), bpm), \
             replaygain_track_db=COALESCE(NULLIF(?18, NULL), replaygain_track_db), \
             replaygain_album_db=COALESCE(NULLIF(?19, NULL), replaygain_album_db), \
             replaygain_track_peak=COALESCE(NULLIF(?20, NULL), replaygain_track_peak), \
             replaygain_album_peak=COALESCE(NULLIF(?21, NULL), replaygain_album_peak), \
             ebu_r128_loudness=COALESCE(NULLIF(?22, NULL), ebu_r128_loudness), \
             ebu_r128_peak=COALESCE(NULLIF(?23, NULL), ebu_r128_peak), \
             lyrics_synced=COALESCE(NULLIF(?24, ''), lyrics_synced), \
             lyrics_unsynced=COALESCE(NULLIF(?25, ''), lyrics_unsynced), \
             date_scanned=?26 WHERE path=?27"
        ).map_err(DbError::Sqlite)?;

        let mut updated = 0;
        for track in tracks {
            match stmt.execute(params![
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.genre,
                track.year,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.sample_rate,
                track.channels,
                track.bitrate_kbps,
                track.format,
                track.file_size,
                track.file_modified,
                track.crc32,
                track.bpm,
                track.replaygain_track_db,
                track.replaygain_album_db,
                track.replaygain_track_peak,
                track.replaygain_album_peak,
                track.ebu_r128_loudness,
                track.ebu_r128_peak,
                track.lyrics_synced,
                track.lyrics_unsynced,
                track.date_scanned,
                track.path,
            ]) {
                Ok(rows) => updated += rows,
                Err(e) => {
                    log::warn!("Batch update failed for track {}: {}", track.path, e);
                },
            }
        }

        drop(stmt);
        tx.commit().map_err(DbError::Sqlite)?;
        Ok(updated)
    }

    /// Get all tracks whose file path is inside the given folder (recursive).
    ///
    /// Matches tracks where `path` starts with `folder_path` followed by a
    /// path separator. This finds all tracks at any depth within the folder.
    pub fn get_tracks_by_folder(&self, folder_path: &str) -> Result<Vec<Track>, DbError> {
        // Escape LIKE wildcards (%, _, \) in the user-supplied path and use
        // an explicit ESCAPE clause. Without this, a folder named e.g.
        // `/music/100%_Hits` would match unrelated paths.
        let escaped = escape_like_pattern(folder_path);
        let prefix = if escaped.ends_with('/') {
            escaped
        } else {
            format!("{}/", escaped)
        };
        let pattern = format!("{}%", prefix);
        let sql = format!(
            "SELECT {} FROM tracks WHERE path LIKE ?1 ESCAPE '\\' ORDER BY path, title",
            TRACK_COLUMNS
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![pattern], row_to_track)?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }

    /// Count tracks whose file path is inside the given folder (recursive).
    ///
    /// Lightweight query for sidebar badge display — avoids materializing
    /// full Track structs.
    pub fn count_tracks_in_folder(&self, folder_path: &str) -> Result<i64, DbError> {
        let escaped = escape_like_pattern(folder_path);
        let prefix = if escaped.ends_with('/') {
            escaped
        } else {
            format!("{}/", escaped)
        };
        let pattern = format!("{}%", prefix);
        let count: i64 = self.read_lock()?.query_row(
            "SELECT COUNT(*) FROM tracks WHERE path LIKE ?1 ESCAPE '\\'",
            params![pattern],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Delete all tracks whose file path is inside the given folder (recursive).
    pub fn delete_tracks_by_folder(&self, folder_path: &str) -> Result<usize, DbError> {
        let escaped = escape_like_pattern(folder_path);
        let prefix = if escaped.ends_with('/') {
            escaped
        } else {
            format!("{}/", escaped)
        };
        let pattern = format!("{}%", prefix);

        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        let deleted = tx.execute(
            "DELETE FROM tracks WHERE path LIKE ?1 ESCAPE '\\'",
            params![pattern],
        )?;

        tx.execute(
            "UPDATE playlists SET \
             track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = playlists.id), \
             duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = playlists.id), 0.0), \
             date_modified = CURRENT_TIMESTAMP",
            [],
        )?;

        tx.execute(
            "DELETE FROM albums WHERE (title, artist) NOT IN (\
             SELECT album, album_artist FROM tracks \
             WHERE album IS NOT NULL AND album != '' \
             GROUP BY album, album_artist)",
            [],
        )
        .ok(); // Best-effort cleanup

        // Best-effort: remove orphaned artists (those with no remaining tracks).
        tx.execute(
            "DELETE FROM artists WHERE name NOT IN (\
             SELECT DISTINCT artist FROM tracks WHERE artist IS NOT NULL AND artist != '')",
            [],
        )
        .ok();

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(deleted)
    }
}

/// Escape LIKE wildcard characters (`%`, `_`, `\`) in a user-supplied
/// string so it matches literally when used with `LIKE ... ESCAPE '\\'`.
fn escape_like_pattern(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(c);
            },
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {

    use super::*;
    use crate::models::Track;

    /// Helper to create a test track with default values
    pub(crate) fn make_test_track(path: &str, title: &str) -> Track {
        Track {
            id: 0,
            path: path.to_string(),
            title: title.to_string(),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            album_artist: Some("Test Artist".to_string()),
            genre: Some("Rock".to_string()),
            year: Some(2024),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 180.0,
            sample_rate: 44100,
            channels: 2,
            bitrate_kbps: Some(320),
            format: "FLAC".to_string(),
            file_size: 1000000,
            file_modified: 1700000000,
            crc32: Some(12345),
            replaygain_track_db: None,
            replaygain_album_db: None,
            replaygain_track_peak: None,
            replaygain_album_peak: None,
            ebu_r128_loudness: None,
            ebu_r128_peak: None,
            bpm: None,
            lyrics_synced: None,
            lyrics_unsynced: None,
            last_played: None,
            play_count: 0,
            date_added: chrono::DateTime::from_timestamp(1700000000, 0)
                .unwrap()
                .naive_utc(),
            date_scanned: chrono::DateTime::from_timestamp(1700000000, 0)
                .unwrap()
                .naive_utc(),
        }
    }

    pub(crate) fn make_test_track_with_analysis(path: &str, title: &str) -> Track {
        let mut track = make_test_track(path, title);
        track.bpm = Some(120.0);
        track.replaygain_track_db = Some(-5.0);
        track.replaygain_album_db = Some(-6.0);
        track.replaygain_track_peak = Some(0.95);
        track.replaygain_album_peak = Some(0.90);
        track.ebu_r128_loudness = Some(-10.0);
        track.ebu_r128_peak = Some(1.0);
        track
    }

    #[test]
    fn test_insert_and_get_track() {
        let db = Database::open_in_memory().unwrap();
        let track = make_test_track("/music/test.flac", "Test Song");

        let id = db.insert_track(&track).unwrap();
        assert!(id > 0, "Insert should return a positive ID");

        let fetched = db.get_track(id).unwrap().unwrap();
        assert_eq!(fetched.path, "/music/test.flac");
        assert_eq!(fetched.title, "Test Song");
        assert_eq!(fetched.artist.as_deref(), Some("Test Artist"));
    }

    #[test]
    fn test_upsert_preserves_analysis_data() {
        let db = Database::open_in_memory().unwrap();

        let track_with_analysis = make_test_track_with_analysis("/music/test.flac", "Test Song");
        let id = db.insert_track(&track_with_analysis).unwrap();

        // Verify analysis data is stored
        let fetched = db.get_track(id).unwrap().unwrap();
        assert_eq!(fetched.bpm, Some(120.0));
        assert_eq!(fetched.replaygain_track_db, Some(-5.0));

        // Re-insert the same track without analysis data (simulating a re-scan)
        let track_without_analysis = make_test_track("/music/test.flac", "Test Song Updated");
        let id2 = db.insert_track(&track_without_analysis).unwrap();

        assert_eq!(id, id2, "UPSERT should preserve the same ID");

        // Analysis data should be preserved, only file metadata updated
        let fetched = db.get_track(id).unwrap().unwrap();
        assert_eq!(
            fetched.title, "Test Song Updated",
            "File metadata should be updated"
        );
        assert_eq!(
            fetched.bpm,
            Some(120.0),
            "BPM should be preserved on re-insert"
        );
        assert_eq!(
            fetched.replaygain_track_db,
            Some(-5.0),
            "ReplayGain should be preserved on re-insert"
        );
        assert_eq!(
            fetched.ebu_r128_loudness,
            Some(-10.0),
            "EBU R128 should be preserved on re-insert"
        );
    }

    #[test]
    fn test_batch_upsert_preserves_analysis_data() {
        let db = Database::open_in_memory().unwrap();

        let track_with_analysis = make_test_track_with_analysis("/music/test.flac", "Test Song");
        let id = db.insert_track(&track_with_analysis).unwrap();

        // Re-insert via batch without analysis data
        let track_no_analysis = make_test_track("/music/test.flac", "Test Song Updated");
        let results = db.insert_tracks_batch(&[track_no_analysis]).unwrap();

        assert_eq!(results.len(), 1);

        // Analysis data should be preserved
        let fetched = db.get_track(id).unwrap().unwrap();
        assert_eq!(fetched.title, "Test Song Updated");
        assert_eq!(
            fetched.bpm,
            Some(120.0),
            "BPM should be preserved in batch upsert"
        );
    }

    #[test]
    fn test_get_track_by_path() {
        let db = Database::open_in_memory().unwrap();
        let track = make_test_track("/music/test.flac", "Test Song");
        let id = db.insert_track(&track).unwrap();

        let fetched = db.get_track_by_path("/music/test.flac").unwrap().unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.title, "Test Song");

        let missing = db.get_track_by_path("/nonexistent.flac").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_search_tracks() {
        let db = Database::open_in_memory().unwrap();
        let track = make_test_track("/music/test.flac", "Bohemian Rhapsody");
        db.insert_track(&track).unwrap();

        let results = db.search_tracks("Bohemian", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Bohemian Rhapsody");

        // Test with negative limit (should return error)
        let result = db.search_tracks("test", -1);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_empty_path() {
        let db = Database::open_in_memory().unwrap();
        let mut track = make_test_track("", "No Path");
        track.path = String::new();

        let result = db.insert_track(&track);
        assert!(result.is_err(), "Empty path should fail validation");
    }

    #[test]
    fn test_delete_track_updates_playlist_counters() {
        let db = Database::open_in_memory().unwrap();

        let track1 = make_test_track("/music/test1.flac", "Song 1");
        let track2 = make_test_track("/music/test2.flac", "Song 2");
        let id1 = db.insert_track(&track1).unwrap();
        let id2 = db.insert_track(&track2).unwrap();

        let playlist_id = db
            .create_playlist("My Playlist", None, false, None)
            .unwrap();
        db.add_track_to_playlist(playlist_id, id1, 0).unwrap();
        db.add_track_to_playlist(playlist_id, id2, 1).unwrap();

        // Verify initial state
        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(playlists[0].track_count, 2);

        db.delete_track(id1).unwrap();

        // Playlist counters should be updated
        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(
            playlists[0].track_count, 1,
            "Playlist counter should be decremented after track deletion"
        );
    }

    #[test]
    fn test_update_track_preserving_userdata() {
        let db = Database::open_in_memory().unwrap();

        let track_with_analysis = make_test_track_with_analysis("/music/test.flac", "Test Song");
        let id = db.insert_track(&track_with_analysis).unwrap();

        let mut updated_track = make_test_track("/music/test.flac", "Updated Title");
        updated_track.bpm = None;
        db.update_track_preserving_userdata(&updated_track).unwrap();

        // Analysis data should be preserved
        let fetched = db.get_track(id).unwrap().unwrap();
        assert_eq!(fetched.title, "Updated Title");
        assert_eq!(fetched.bpm, Some(120.0), "BPM should be preserved");
    }

    #[test]
    fn test_get_unanalyzed_tracks() {
        let db = Database::open_in_memory().unwrap();

        let track = make_test_track("/music/test.flac", "Test Song");
        db.insert_track(&track).unwrap();

        let unanalyzed = db.get_unanalyzed_tracks().unwrap();
        assert_eq!(
            unanalyzed.len(),
            1,
            "Track without analysis should be returned"
        );

        let id = unanalyzed[0].id;
        db.update_bpm(id, 120.0).unwrap();

        let unanalyzed = db.get_unanalyzed_tracks().unwrap();
        assert!(
            unanalyzed.is_empty(),
            "Track with analysis should not be returned"
        );
    }
}
