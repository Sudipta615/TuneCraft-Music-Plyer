//! Playlist repository — playlist CRUD and track-association operations.

use rusqlite::params;

use super::{
    tracks::{log_and_filter, prefixed_track_columns, row_to_track},
    Database, DbError,
};
use crate::models::*;

impl Database {
    pub fn create_playlist(
        &self,
        name: &str,
        description: Option<&str>,
        is_smart: bool,
        smart_rules: Option<&str>,
    ) -> Result<i64, DbError> {
        if name.is_empty() {
            return Err(DbError::Validation(
                "Playlist name must not be empty".into(),
            ));
        }

        let conn = self.write_lock()?;
        conn.execute(
            "INSERT INTO playlists (name, description, is_smart, smart_rules) VALUES (?1, ?2, ?3, ?4)",
            params![name, description, is_smart as i32, smart_rules],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Update a playlist's name and/or description.
    ///
    ///
    /// their description without deleting and recreating them.
    pub fn update_playlist(
        &self,
        id: i64,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), DbError> {
        let conn = self.write_lock()?;

        if let Some(new_name) = name {
            if new_name.is_empty() {
                return Err(DbError::Validation(
                    "Playlist name must not be empty".into(),
                ));
            }
            conn.execute(
                "UPDATE playlists SET name = ?1, date_modified = CURRENT_TIMESTAMP WHERE id = ?2",
                params![new_name, id],
            )?;
        }
        if let Some(new_desc) = description {
            conn.execute(
                "UPDATE playlists SET description = ?1, date_modified = CURRENT_TIMESTAMP WHERE id = ?2",
                params![new_desc, id],
            )?;
        }
        Ok(())
    }

    /// Delete a playlist by ID.
    ///
    ///
    /// that all track associations are removed automatically.
    pub fn delete_playlist(&self, id: i64) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_all_playlists(&self) -> Result<Vec<Playlist>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT id, name, description, is_smart, smart_rules, track_count, duration_secs, date_created, date_modified FROM playlists ORDER BY name"
        )?;
        let playlists = stmt
            .query_map([], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_smart: row.get::<_, i32>(3)? != 0,
                    smart_rules: row.get(4)?,
                    track_count: row.get(5)?,
                    duration_secs: row.get(6)?,
                    date_created: row.get(7)?,
                    date_modified: row.get(8)?,
                })
            })?
            .filter_map(log_and_filter)
            .collect();
        Ok(playlists)
    }

    /// Add a track to a playlist at the specified position.
    ///
    ///
    /// query with proper error propagation. If the track does not
    /// exist, a `NotFound` error is returned instead of silently
    /// defaulting the duration to 0.0.
    pub fn add_track_to_playlist(
        &self,
        playlist_id: i64,
        track_id: i64,
        position: i32,
    ) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        // rows and counters out of sync.
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        // Verify the track exists and get its duration
        let _track_duration: f64 = tx
            .query_row(
                "SELECT duration_secs FROM tracks WHERE id = ?1",
                params![track_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DbError::NotFound(format!("Track with id {} does not exist", track_id))
                },
                other => DbError::Sqlite(other),
            })?;

        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, position],
        )?;
        tx.execute(
            "UPDATE playlists SET \
             track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1), \
             duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = ?1), 0.0), \
             date_modified = CURRENT_TIMESTAMP \
             WHERE id = ?1",
            params![playlist_id],
        )?;
        tx.commit().map_err(DbError::Sqlite)?;
        Ok(())
    }

    /// Remove a track from a playlist at the specified position.
    ///
    ///
    /// duration_secs within the same transaction to keep denormalized
    /// counters consistent.
    pub fn remove_track_from_playlist(
        &self,
        playlist_id: i64,
        position: i32,
    ) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        let removed = tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND position = ?2",
            params![playlist_id, position],
        )?;

        if removed > 0 {
            tx.execute(
                "UPDATE playlists SET \
                 track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1), \
                 duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = ?1), 0.0), \
                 date_modified = CURRENT_TIMESTAMP \
                 WHERE id = ?1",
                params![playlist_id],
            )?;
        }

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(())
    }

    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>, DbError> {
        let sql = format!(
            "SELECT {} FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = ?1 ORDER BY pt.position",
            prefixed_track_columns("t")
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map(params![playlist_id], |row| row_to_track(row))?
            .filter_map(log_and_filter)
            .collect();
        Ok(tracks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::tracks::tests::make_test_track;

    #[test]
    fn test_validation_empty_playlist_name() {
        let db = Database::open_in_memory().unwrap();
        let result = db.create_playlist("", None, false, None);
        assert!(
            result.is_err(),
            "Empty playlist name should fail validation"
        );
    }

    #[test]
    fn test_playlist_crud() {
        let db = Database::open_in_memory().unwrap();

        let playlist_id = db
            .create_playlist("My Playlist", Some("A test playlist"), false, None)
            .unwrap();
        assert!(playlist_id > 0);

        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "My Playlist");

        db.update_playlist(
            playlist_id,
            Some("Renamed Playlist"),
            Some("Updated description"),
        )
        .unwrap();
        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(playlists[0].name, "Renamed Playlist");

        db.delete_playlist(playlist_id).unwrap();
        let playlists = db.get_all_playlists().unwrap();
        assert!(playlists.is_empty());
    }

    #[test]
    fn test_add_and_remove_track_from_playlist() {
        let db = Database::open_in_memory().unwrap();

        let track = make_test_track("/music/test.flac", "Test Song");
        let track_id = db.insert_track(&track).unwrap();
        let playlist_id = db
            .create_playlist("My Playlist", None, false, None)
            .unwrap();

        db.add_track_to_playlist(playlist_id, track_id, 0).unwrap();

        // Verify playlist has the track
        let tracks = db.get_playlist_tracks(playlist_id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, track_id);

        // Verify playlist counters
        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(playlists[0].track_count, 1);
        assert!(playlists[0].duration_secs > 0.0);

        db.remove_track_from_playlist(playlist_id, 0).unwrap();
        let tracks = db.get_playlist_tracks(playlist_id).unwrap();
        assert!(tracks.is_empty());

        // Verify counters are decremented
        let playlists = db.get_all_playlists().unwrap();
        assert_eq!(playlists[0].track_count, 0);
    }

    #[test]
    fn test_add_nonexistent_track_to_playlist() {
        let db = Database::open_in_memory().unwrap();
        let playlist_id = db
            .create_playlist("My Playlist", None, false, None)
            .unwrap();

        // Try to add a track that doesn't exist
        let result = db.add_track_to_playlist(playlist_id, 99999, 0);
        assert!(
            result.is_err(),
            "Adding non-existent track should return an error"
        );
    }
}
