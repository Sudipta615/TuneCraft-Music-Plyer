use anyhow::Result;
use rusqlite::params;

use crate::database::models::Track;
use crate::database::{Database, TRACK_COLUMNS};

impl Database {
    /// Update mood analysis results for a track by file path.
    pub fn update_track_mood(
        &self,
        file_path: &str,
        bpm: f32,
        energy: f32,
        bass_ratio: f32,
        spectral_centroid: f32,
        dynamic_range: f32,
        mood: &str,
    ) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute(
            "UPDATE tracks
             SET bpm = ?1, energy = ?2, bass_ratio = ?3,
                 spectral_centroid = ?4, dynamic_range = ?5, mood = ?6
             WHERE file_path = ?7",
            params![
                bpm,
                energy,
                bass_ratio,
                spectral_centroid,
                dynamic_range,
                mood,
                file_path
            ],
        )?)
    }

    /// Check if a track needs mood analysis (mood column is NULL).
    pub fn track_needs_mood_analysis(&self, file_path: &str) -> Result<bool> {
        let conn = self.conn()?;
        let needs: bool = conn.query_row(
            "SELECT mood IS NULL FROM tracks WHERE file_path = ?1",
            params![file_path],
            |row| row.get(0),
        )?;
        Ok(needs)
    }

    /// Set a manual mood override for a track by file path.
    /// Pass an empty string or None to clear the override.
    pub fn set_mood_override(&self, file_path: &str, mood: Option<&str>) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute(
            "UPDATE tracks SET mood_override = ?1 WHERE file_path = ?2",
            params![mood, file_path],
        )?)
    }

    /// Get tracks filtered by mood. Uses COALESCE so mood_override takes priority.
    ///
    /// Fix M8: Replaced ORDER BY RANDOM() (which is O(n log n)) with a
    /// random offset approach. First counts matching tracks, then selects
    /// a random offset and uses LIMIT to retrieve the desired number.
    /// This is O(1) for small limits regardless of table size.
    /// Fix Bug #19: Use a single connection for both COUNT and SELECT
    /// queries to prevent a TOCTOU race. Previously, the COUNT used one
    /// pooled connection and query_map() obtained a second, allowing tracks
    /// to be added/removed between the two queries.
    pub fn get_tracks_by_mood(&self, mood: &str, limit: usize) -> Result<Vec<Track>> {
        let conn = self.conn()?;

        if limit == 0 {
            let sql = format!(
                "SELECT {} FROM tracks
                 WHERE COALESCE(mood_override, mood) = ?1
                 ORDER BY id",
                TRACK_COLUMNS
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params![mood], |row| Track::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            return Ok(results);
        }

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tracks WHERE COALESCE(mood_override, mood) = ?1",
            rusqlite::params![mood],
            |r| r.get(0),
        )?;

        if count == 0 {
            return Ok(Vec::new());
        }

        let max_offset = (count as usize).saturating_sub(limit);
        let offset = if max_offset == 0 {
            0
        } else {
            use rand::Rng;
            let mut rng = rand::rng();
            rng.random_range(0..max_offset)
        };

        let sql = format!(
            "SELECT {} FROM tracks
             WHERE COALESCE(mood_override, mood) = ?1
             ORDER BY id
             LIMIT ?2 OFFSET ?3",
            TRACK_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![mood, limit as i64, offset as i64],
            |row| Track::from_row(row),
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Count tracks for a given mood without loading full Track objects.
    /// Uses COALESCE so mood_override takes priority.
    pub fn count_tracks_for_mood(&self, mood: &str) -> Result<i64> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tracks WHERE COALESCE(mood_override, mood) = ?1",
            params![mood],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Set the love/heart flag for a track.
    pub fn set_track_love(&self, track_id: i64, loved: bool) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE tracks SET love = ?1 WHERE id = ?2",
            params![loved as i32, track_id],
        )?;
        Ok(())
    }

    /// Set the rating for a track on a 0–5 star scale (0.0 to 5.0).
    /// Each star is worth 1.0, half-stars are 0.5. Values are clamped
    /// to the 0.0–5.0 range to enforce consistency between UI and DB.
    pub fn set_track_rating(&self, track_id: i64, rating: f64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE tracks SET rating = ?1 WHERE id = ?2",
            params![rating.clamp(0.0, 5.0), track_id],
        )?;
        Ok(())
    }

    /// Get mood distribution statistics for diagnostic purposes.
    /// Returns (mood_label, count, avg_bpm, avg_energy, avg_bass_ratio).
    pub fn get_mood_distribution(&self) -> Result<Vec<(String, i64, f64, f64, f64)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(mood_override, mood) AS effective_mood,
                    COUNT(*) AS count,
                    ROUND(AVG(bpm), 1) AS avg_bpm,
                    ROUND(AVG(energy), 4) AS avg_energy,
                    ROUND(AVG(bass_ratio), 3) AS avg_bass_ratio
             FROM tracks
             WHERE COALESCE(mood_override, mood) IS NOT NULL
             GROUP BY effective_mood
             ORDER BY count DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Count tracks that still need mood analysis.
    pub fn unanalyzed_track_count(&self) -> Result<i64> {
        let conn = self.conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM tracks WHERE mood IS NULL", [], |r| {
                r.get(0)
            })?;
        Ok(count)
    }
}
