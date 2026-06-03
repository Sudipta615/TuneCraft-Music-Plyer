//! Cover art repository — cover art data persistence.

use rusqlite::params;

use super::{Database, DbError};

impl Database {
    /// Insert a cover art entry into the cover_art table.
    ///
    ///
    /// album record when an `album_id` is provided, fixing the issue where
    /// the flag was never set to true through any code path.
    ///
    ///
    /// cover_art table gained a BLOB column in V005. Callers may pass `None`
    /// for `data` if the image is stored on disk and `path` is provided instead.
    pub fn insert_cover_art(
        &self,
        album_id: Option<i64>,
        track_id: Option<i64>,
        path: Option<&str>,
        data: Option<&[u8]>,
        data_hash: Option<&str>,
        width: i32,
        height: i32,
        mime_type: &str,
    ) -> Result<i64, DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        tx.execute(
            "INSERT INTO cover_art (album_id, track_id, path, data, data_hash, width, height, mime_type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![album_id, track_id, path, data, data_hash, width, height, mime_type],
        )?;
        let id = tx.last_insert_rowid();

        if let Some(aid) = album_id {
            tx.execute(
                "UPDATE albums SET has_cover = 1 WHERE id = ?1",
                params![aid],
            )?;
        }

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(id)
    }

    /// Retrieve the cover art binary data for an album, if stored inline.
    ///
    /// Returns `(data, mime_type)` or `None` if no inline data is available
    /// (the row may still exist with a `path` pointing to an on-disk cache).
    pub fn get_cover_art_data(
        &self,
        album_id: i64,
    ) -> Result<Option<(Vec<u8>, String)>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT data, mime_type FROM cover_art \
             WHERE album_id = ?1 AND data IS NOT NULL \
             ORDER BY id DESC LIMIT 1",
        )?;
        let result = stmt.query_row(params![album_id], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        });
        match result {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Look up the album_id for a given album title and album_artist pair.
    ///
    /// Used by tc-library after a scan to link cover_art rows to their albums.
    pub fn get_album_id(
        &self,
        title: &str,
        artist: Option<&str>,
    ) -> Result<Option<i64>, DbError> {
        let result = match artist {
            Some(a) => self.read_lock()?.query_row(
                "SELECT id FROM albums WHERE title = ?1 AND artist = ?2",
                params![title, a],
                |row| row.get(0),
            ),
            None => self.read_lock()?.query_row(
                "SELECT id FROM albums WHERE title = ?1 AND artist IS NULL",
                params![title],
                |row| row.get(0),
            ),
        };
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::tracks::tests::make_test_track;

    #[test]
    fn test_cover_art_updates_has_cover() {
        let db = Database::open_in_memory().unwrap();

        let track = make_test_track("/music/test.flac", "Test Song");
        db.insert_track(&track).unwrap();
        db.reconcile_aggregates().unwrap();

        let albums = db.get_all_albums().unwrap();
        let album = albums.iter().find(|a| a.title == "Test Album").unwrap();
        assert!(!album.has_cover, "Album should not have cover initially");

        db.insert_cover_art(Some(album.id), None, Some("/covers/test.jpg"), None, None, 500, 500, "image/jpeg").unwrap();

        let albums = db.get_all_albums().unwrap();
        let album = albums.iter().find(|a| a.title == "Test Album").unwrap();
        assert!(album.has_cover, "Album should have cover after insert_cover_art");
    }
}

