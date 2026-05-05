use anyhow::Result;
use rusqlite::params;

use crate::database::Database;

impl Database {
    /// Add a scrobble entry to the queue.
    pub fn queue_scrobble(
        &self,
        track_id: i64,
        artist: &str,
        title: &str,
        album: Option<&str>,
        timestamp: i64,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO scrobble_queue (track_id, artist, title, album, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![track_id, artist, title, album, timestamp],
        )?;
        Ok(())
    }

    /// Get pending scrobble entries.
    pub fn get_pending_scrobbles(&self) -> Result<Vec<(i64, String, String, Option<String>, i64)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, artist, title, album, timestamp FROM scrobble_queue WHERE status = 'pending' ORDER BY timestamp"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Mark scrobble entries as completed.
    /// Uses a single `WHERE id IN (...)` statement instead of N individual
    /// updates, reducing SQLite round-trips from N to 1.
    pub fn mark_scrobbles_done(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = self.conn()?;
        // Build a single UPDATE with parameterized IN clause
        let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "UPDATE scrobble_queue SET status = 'scrobbled' WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        conn.execute(&sql, params.as_slice())?;
        Ok(())
    }
}
