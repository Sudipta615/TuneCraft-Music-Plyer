use anyhow::Result;
use rusqlite::params;

use crate::database::models::Track;
use crate::database::{Database, TRACK_COLUMNS};

impl Database {
    /// Insert a track and return its row id.
    /// If a track with the same file_path already exists, performs an UPSERT
    /// (INSERT ... ON CONFLICT DO UPDATE) to update all metadata columns, so
    /// that re-tagged files correctly reflect their new metadata in the database.
    pub fn insert_track(&self, track: &Track) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO tracks (file_path, file_hash, file_size, file_mtime, title, artist, album,
             genre, year, track_number, duration, sample_rate, bitrate, play_count, skip_count, rating,
             date_added, last_played)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(file_path) DO UPDATE SET
                 file_hash = excluded.file_hash,
                 file_size = excluded.file_size,
                 file_mtime = excluded.file_mtime,
                 title = excluded.title,
                 artist = excluded.artist,
                 album = excluded.album,
                 genre = excluded.genre,
                 year = excluded.year,
                 track_number = excluded.track_number,
                 duration = excluded.duration,
                 sample_rate = excluded.sample_rate,
                 bitrate = excluded.bitrate",
            params![
                track.file_path,
                track.file_hash.as_deref().unwrap_or(""),
                track.file_size,
                track.file_mtime,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.year,
                track.track_number,
                track.duration.map(|d| d as i64),
                track.sample_rate,
                track.bitrate,
                track.play_count.unwrap_or(0),
                track.skip_count.unwrap_or(0),
                track.rating.unwrap_or(0.0).clamp(0.0, 5.0),
                track.date_added.to_string(),
                track.last_played.map(|d| d.to_string()),
            ],
        )?;
        let rows_changed = conn.changes();
        if rows_changed > 0 {
            Ok(conn.last_insert_rowid())
        } else {
            let existing_id: i64 = conn.query_row(
                "SELECT id FROM tracks WHERE file_path = ?1",
                params![track.file_path],
                |r| r.get(0),
            )?;
            Ok(existing_id)
        }
    }

    /// Update an existing track by id.
    pub fn update_track(&self, track: &Track) -> Result<usize> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE tracks SET title=?1, artist=?2, album=?3, genre=?4, year=?5, track_number=?6,
             duration=?7, sample_rate=?8, bitrate=?9, rating=?10, last_played=?11,
             play_count=?12, skip_count=?13, love=?14,
             bpm=?15, energy=?16, bass_ratio=?17, spectral_centroid=?18, dynamic_range=?19,
             mood=?20, mood_override=?21, updated_at=datetime('now')
             WHERE id=?22",
            params![
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.year,
                track.track_number,
                track.duration.map(|d| d as i64),
                track.sample_rate,
                track.bitrate,
                track.rating.unwrap_or(0.0).clamp(0.0, 5.0),
                track.last_played.map(|d| d.to_string()),
                track.play_count.unwrap_or(0),
                track.skip_count.unwrap_or(0),
                track.love.unwrap_or(0),
                track.bpm,
                track.energy,
                track.bass_ratio,
                track.spectral_centroid,
                track.dynamic_range,
                track.mood.as_deref(),
                track.mood_override.as_deref(),
                track.id,
            ],
        )?;
        Ok(affected)
    }

    /// Increment the play count for a track.
    pub fn increment_play_count(&self, track_id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = datetime('now') WHERE id = ?1",
            params![track_id],
        )?;
        Ok(())
    }

    /// Get a track by its id.
    pub fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let sql = format!("SELECT {} FROM tracks WHERE id = ?1", TRACK_COLUMNS);
        let results: Vec<Track> = self.query_map(&sql, [id], |row| Track::from_row(row))?;
        Ok(results.into_iter().next())
    }

    /// Get a track by file path.
    pub fn get_track_by_path(&self, path: &str) -> Result<Option<Track>> {
        let sql = format!("SELECT {} FROM tracks WHERE file_path = ?1", TRACK_COLUMNS);
        let results: Vec<Track> = self.query_map(&sql, [path], |row| Track::from_row(row))?;
        Ok(results.into_iter().next())
    }

    /// Get all tracks.
    pub fn get_all_tracks(&self) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {} FROM tracks ORDER BY artist, album, track_number, title",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [], |row| Track::from_row(row))
    }

    /// Search tracks by title, artist, or album.
    /// Uses parameterized queries to prevent SQL injection.
    pub fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{}%", escaped);
        let sql = format!(
            "SELECT {} FROM tracks WHERE title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\' OR album LIKE ?1 ESCAPE '\\' ORDER BY artist, album, title",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [&pattern], |row| Track::from_row(row))
    }

    /// Search tracks by genre and/or year range, with optional text query.
    /// Unlike `search_tracks()` which only matches title/artist/album, this
    /// method applies genre and year as proper SQL WHERE clauses on their
    /// respective columns.
    ///
    /// Parameters:
    /// - `query`: Optional text search for title/artist/album (empty = no filter)
    /// - `genre`: Optional genre filter (exact match, empty = no filter)
    /// - `year_from`: Optional start year (inclusive, None = no lower bound)
    /// - `year_to`: Optional end year (inclusive, None = no upper bound)
    pub fn search_tracks_advanced(
        &self,
        query: &str,
        genre: &str,
        year_from: Option<i32>,
        year_to: Option<i32>,
    ) -> Result<Vec<Track>> {
        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx = 1u32;
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !query.is_empty() {
            let escaped = query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = format!("%{}%", escaped);
            conditions.push(format!(
                "(title LIKE ?{p} ESCAPE '\\' OR artist LIKE ?{p} ESCAPE '\\' OR album LIKE ?{p} ESCAPE '\\')",
                p = param_idx
            ));
            param_values.push(Box::new(pattern));
            param_idx += 1;
        }

        if !genre.is_empty() {
            conditions.push(format!("genre = ?{}", param_idx));
            param_values.push(Box::new(genre.to_string()));
            param_idx += 1;
        }

        if let Some(yf) = year_from {
            conditions.push(format!("year >= ?{}", param_idx));
            param_values.push(Box::new(yf));
            param_idx += 1;
        }
        if let Some(yt) = year_to {
            conditions.push(format!("year <= ?{}", param_idx));
            param_values.push(Box::new(yt));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT {} FROM tracks {} ORDER BY artist, album, title",
            TRACK_COLUMNS, where_clause
        );

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        self.query_map(&sql, rusqlite::params_from_iter(param_refs), |row| {
            Track::from_row(row)
        })
    }

    /// Get track count.
    pub fn track_count(&self) -> Result<i64> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))?;
        Ok(count)
    }

    /// Delete a track by id.
    pub fn delete_track(&self, id: i64) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute("DELETE FROM tracks WHERE id = ?1", params![id])?)
    }

    /// Delete a track by file path.
    pub fn delete_track_by_path(&self, path: &str) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute("DELETE FROM tracks WHERE file_path = ?1", params![path])?)
    }

    /// Get tracks that exist in the database but are missing from the given file set.
    /// Returns paths of tracks to remove.
    pub fn get_stale_tracks(&self, existing_paths: &[String]) -> Result<Vec<String>> {
        let conn = self.conn()?;

        conn.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS _existing_paths (file_path TEXT NOT NULL);
             DELETE FROM _existing_paths;",
        )?;

        let mut stmt = conn.prepare("INSERT INTO _existing_paths (file_path) VALUES (?1)")?;
        for path in existing_paths {
            stmt.execute(rusqlite::params![path])?;
        }
        drop(stmt);

        let mut stale_stmt = conn.prepare(
            "SELECT t.file_path FROM tracks t 
             LEFT JOIN _existing_paths e ON t.file_path = e.file_path 
             WHERE e.file_path IS NULL",
        )?;
        let rows = stale_stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut stale = Vec::new();
        for row in rows {
            if let Ok(path) = row {
                stale.push(path);
            }
        }

        conn.execute_batch("DROP TABLE IF EXISTS _existing_paths")?;

        Ok(stale)
    }

    /// Remove all tracks and reset the library.
    pub fn clear_library(&self) -> Result<usize> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM cover_art_cache", [])?;
        conn.execute("DELETE FROM waveforms", [])?;
        let affected = conn.execute("DELETE FROM tracks", [])?;
        Ok(affected)
    }

    /// Set the love status of a track identified by file_hash or file_path.
    pub fn set_track_loved(&self, track_key: &str, loved: bool) -> Result<()> {
        if track_key.is_empty() {
            anyhow::bail!("set_track_loved: track_key must not be empty");
        }
        let conn = self.conn()?;
        let love_val = if loved { 1 } else { 0 };
        conn.execute(
            "UPDATE tracks SET love = ?1 WHERE file_hash = ?2 OR file_path = ?2",
            params![love_val, track_key],
        )?;
        Ok(())
    }

    /// Get all loved track keys (file_hash if available, otherwise file_path).
    pub fn get_loved_tracks(&self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(NULLIF(file_hash, ''), file_path) AS key FROM tracks WHERE love = 1",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut keys = Vec::new();
        for row in rows {
            if let Ok(key) = row {
                keys.push(key);
            }
        }
        Ok(keys)
    }

    /// Get tracks ordered by last_played date (most recent first).
    pub fn get_recent_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE last_played IS NOT NULL ORDER BY last_played DESC LIMIT ?",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [limit], |row| Track::from_row(row))
    }

    /// Get tracks ordered by play_count (most played first).
    pub fn get_most_played_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE play_count > 0 ORDER BY play_count DESC LIMIT ?",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [limit], |row| Track::from_row(row))
    }

    /// Get full Track records for loved tracks.
    pub fn get_loved_track_records(&self) -> Result<Vec<Track>> {
        let sql = format!("SELECT {} FROM tracks WHERE love = 1", TRACK_COLUMNS);
        self.query_map(&sql, [], |row| Track::from_row(row))
    }
}
