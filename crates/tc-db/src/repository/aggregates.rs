//! Aggregate reconciliation — album, artist, and playlist counter maintenance.

use crate::models::*;

use super::{Database, DbError};
use super::tracks::log_and_filter;

impl Database {

    pub fn get_all_albums(&self) -> Result<Vec<Album>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT id, title, artist, year, genre, track_count, duration_secs, has_cover, date_added FROM albums ORDER BY title"
        )?;
        let albums = stmt
            .query_map([], |row| {
                Ok(Album {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    year: row.get(3)?,
                    genre: row.get(4)?,
                    track_count: row.get(5)?,
                    duration_secs: row.get(6)?,
                    has_cover: row.get(7)?,
                    date_added: row.get(8)?,
                })
            })?
            .filter_map(log_and_filter)
            .collect();
        Ok(albums)
    }


    pub fn get_all_artists(&self) -> Result<Vec<Artist>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT id, name, album_count, track_count FROM artists ORDER BY name"
        )?;
        let artists = stmt
            .query_map([], |row| {
                Ok(Artist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    album_count: row.get(2)?,
                    track_count: row.get(3)?,
                })
            })?
            .filter_map(log_and_filter)
            .collect();
        Ok(artists)
    }


    /// Recompute derived album and artist tables from the current tracks.
    /// Call this after bulk changes (scan, cleanup) to keep aggregate
    /// counters in sync with reality.
    ///
    /// Uses a non-destructive approach: albums are refreshed via a temp table
    /// so that any user-editable columns (e.g. `has_cover`, `date_added`) on
    /// existing albums are preserved, and artists use UPSERT since they have
    /// a UNIQUE constraint on `name`.
    ///
    ///
    /// and updates the `has_cover` flag from the cover_art table.
    pub fn reconcile_aggregates(&self) -> Result<(), DbError> {
        let conn = self.write_lock()?;

        // transaction so a partial failure cannot leave albums/artists/playlists
        // in an inconsistent state.
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;

        // the albums that appear in the new data (preserving any albums that
        // might have user overrides without corresponding tracks), and insert
        // the refreshed rows.
        tx.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS _new_albums (\
               title TEXT NOT NULL, \
               artist TEXT, \
               year INTEGER, \
               genre TEXT, \
               track_count INTEGER NOT NULL, \
               duration_secs REAL NOT NULL);\
             DELETE FROM _new_albums;\
             INSERT INTO _new_albums (title, artist, year, genre, track_count, duration_secs) \
               SELECT album, album_artist, MAX(year), \
               MIN(genre), COUNT(*), SUM(duration_secs) \
               FROM tracks WHERE album IS NOT NULL AND album != '' \
               GROUP BY album, album_artist;\
             INSERT INTO albums (title, artist, year, genre, track_count, duration_secs) \
               SELECT title, artist, year, genre, track_count, duration_secs FROM _new_albums WHERE 1 \
               ON CONFLICT(title, artist) DO UPDATE SET \
                 year = excluded.year, \
                 genre = excluded.genre, \
                 track_count = excluded.track_count, \
                 duration_secs = excluded.duration_secs;\
             DROP TABLE IF EXISTS _new_albums;"
        )?;

        // Delete albums that no longer have any matching tracks.
        // Use NOT EXISTS to correctly handle cases where artist/album_artist is NULL (M13 fix).
        tx.execute(
            "DELETE FROM albums WHERE NOT EXISTS (\
              SELECT 1 FROM tracks \
              WHERE tracks.album = albums.title \
                AND COALESCE(tracks.album_artist, '') = COALESCE(albums.artist, '')\
            )",
            [],
        )?;

        tx.execute(
            "UPDATE albums SET has_cover = 1 WHERE id IN (\
             SELECT DISTINCT ca.album_id FROM cover_art ca \
             WHERE ca.album_id IS NOT NULL)",
            [],
        ).ok(); // Best-effort -- cover_art may not have album_id set

        // `artists` has a UNIQUE(name) constraint, so we can use UPSERT
        // to update existing rows in-place and insert new ones.
        tx.execute(
            "INSERT INTO artists (name, album_count, track_count) \
             SELECT artist, COUNT(DISTINCT album), COUNT(*) \
             FROM tracks WHERE artist IS NOT NULL AND artist != '' \
             GROUP BY artist \
             ON CONFLICT(name) DO UPDATE SET \
             album_count=excluded.album_count, track_count=excluded.track_count",
            [],
        )?;
        tx.execute(
            "DELETE FROM artists WHERE NOT EXISTS (\
              SELECT 1 FROM tracks WHERE tracks.artist = artists.name\
            )",
            [],
        )?;

        tx.execute(
            "UPDATE playlists SET \
             track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = playlists.id), \
             duration_secs = COALESCE((SELECT SUM(t.duration_secs) FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id WHERE pt.playlist_id = playlists.id), 0.0)",
            [],
        )?;

        tx.commit().map_err(DbError::Sqlite)?;
        Ok(())
    }

    /// Refresh album and artist tables from the current track data.
    /// Called after a library scan to populate the albums/artists tables.
    pub fn refresh_albums_and_artists(&self) -> Result<(), DbError> {
        self.reconcile_aggregates()
    }

    /// Rebuild the FTS5 full-text search index from scratch.
    ///
    /// This is a recovery tool for when the FTS index drifts out of sync
    /// with the tracks table (e.g., after a migration that temporarily
    /// disabled triggers, or a bug that caused triggers to miss updates).
    /// It drops and recreates the FTS virtual table, then repopulates it
    /// from the current tracks data.
    ///
    /// Should be called after migrations or as a manual maintenance operation.
    pub fn rebuild_fts_index(&self) -> Result<(), DbError> {
        let conn = self.write_lock()?;
        let tx = conn.unchecked_transaction().map_err(DbError::Sqlite)?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS tracks_fts;\
             CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(\
               title, artist, album, album_artist, genre,\
               content=tracks, content_rowid=id\
             );\
             INSERT INTO tracks_fts(rowid, title, artist, album, album_artist, genre)\
               SELECT id, title, artist, album, album_artist, genre FROM tracks;"
        )?;
        tx.commit().map_err(DbError::Sqlite)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebuild_fts_index() {
        let db = Database::open_in_memory().unwrap();
        let track = crate::repository::tracks::tests::make_test_track("/music/test.flac", "Unique Search Term");
        db.insert_track(&track).unwrap();

        // Rebuild FTS index
        db.rebuild_fts_index().unwrap();

        // Search should still work after rebuild
        let results = db.search_tracks("Unique", 10).unwrap();
        assert_eq!(results.len(), 1);
    }
}

