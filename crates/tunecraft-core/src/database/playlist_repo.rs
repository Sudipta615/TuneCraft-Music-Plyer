use anyhow::Result;
use rusqlite::{params, TransactionBehavior};

use crate::database::models::{Playlist, Track};
use crate::database::{Database, TRACK_COLUMNS};

impl Database {
    /// Create a new playlist and return its id.
    pub fn create_playlist(
        &self,
        name: &str,
        description: Option<&str>,
        is_smart: bool,
    ) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO playlists (name, description, is_smart) VALUES (?1, ?2, ?3)",
            params![name, description, is_smart as i32],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get all playlists.
    pub fn get_all_playlists(&self) -> Result<Vec<Playlist>> {
        self.query_map("SELECT * FROM playlists ORDER BY name", [], |row| {
            Playlist::from_row(row)
        })
    }

    /// Get a playlist by id.
    pub fn get_playlist(&self, id: i64) -> Result<Option<Playlist>> {
        let results: Vec<Playlist> =
            self.query_map("SELECT * FROM playlists WHERE id = ?1", [id], |row| {
                Playlist::from_row(row)
            })?;
        Ok(results.into_iter().next())
    }

    /// Update a playlist name/description.
    pub fn update_playlist(&self, id: i64, name: &str, description: Option<&str>) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute(
            "UPDATE playlists SET name=?1, description=?2, updated_at=datetime('now') WHERE id=?3",
            params![name, description, id],
        )?)
    }

    /// Delete a playlist.
    pub fn delete_playlist(&self, id: i64) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?)
    }

    /// Add a track to a playlist.
    pub fn add_track_to_playlist(&self, playlist_id: i64, track_id: i64) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position) \
             SELECT ?1, ?2, COALESCE(MAX(position), 0) + 1 \
             FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id, track_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Remove a track from a playlist.
    pub fn remove_track_from_playlist(&self, playlist_id: i64, track_id: i64) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
            params![playlist_id, track_id],
        )?)
    }

    /// Get all tracks in a playlist, ordered by position.
    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        let prefixed_columns: String = TRACK_COLUMNS
            .split(',')
            .map(|c| format!("t.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {} FROM tracks t
             INNER JOIN playlist_tracks pt ON t.id = pt.track_id
             WHERE pt.playlist_id = ?1
             ORDER BY pt.position",
            prefixed_columns
        );
        self.query_map(&sql, [playlist_id], |row| Track::from_row(row))
    }
}
