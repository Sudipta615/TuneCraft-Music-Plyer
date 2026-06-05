//! Favorites repository — favorite track operations.

use rusqlite::params;

use super::{
    tracks::{log_and_filter, prefixed_track_columns, row_to_track},
    Database, DbError,
};
use crate::models::*;

impl Database {
    /// Add a track to the user's favorites
    pub fn add_favorite(&self, track_id: i64) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "INSERT OR IGNORE INTO favorites (track_id) VALUES (?1)",
            params![track_id],
        )?;
        Ok(())
    }

    /// Remove a track from the user's favorites
    pub fn remove_favorite(&self, track_id: i64) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "DELETE FROM favorites WHERE track_id = ?1",
            params![track_id],
        )?;
        Ok(())
    }

    /// Check whether a track is in the user's favorites
    pub fn is_favorite(&self, track_id: i64) -> Result<bool, DbError> {
        let count: i64 = self.read_lock()?.query_row(
            "SELECT COUNT(*) FROM favorites WHERE track_id = ?1",
            params![track_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get all favorited track IDs
    pub fn get_favorite_track_ids(&self) -> Result<Vec<i64>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt =
            lock.prepare_cached("SELECT track_id FROM favorites ORDER BY date_favorited DESC")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(log_and_filter)
            .collect();
        Ok(ids)
    }

    /// Get all favorited tracks (with full track data)
    pub fn get_favorite_tracks(&self) -> Result<Vec<Track>, DbError> {
        let sql = format!(
            "SELECT {} FROM favorites f JOIN tracks t ON t.id = f.track_id ORDER BY f.date_favorited DESC",
            prefixed_track_columns("t")
        );
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(&sql)?;
        let tracks = stmt
            .query_map([], row_to_track)?
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
    fn test_favorites() {
        let db = Database::open_in_memory().unwrap();
        let track = make_test_track("/music/test.flac", "Test Song");
        let track_id = db.insert_track(&track).unwrap();

        db.add_favorite(track_id).unwrap();
        assert!(db.is_favorite(track_id).unwrap());

        let ids = db.get_favorite_track_ids().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], track_id);

        let tracks = db.get_favorite_tracks().unwrap();
        assert_eq!(tracks.len(), 1);

        db.remove_favorite(track_id).unwrap();
        assert!(!db.is_favorite(track_id).unwrap());
    }
}
