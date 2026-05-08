use anyhow::Result;

use crate::database::models::Track;
use crate::database::{Database, TRACK_COLUMNS};

impl Database {
    /// Get all distinct album names with track count and artist info.
    /// Returns Vec<(album_name, artist_names, track_count, total_duration_secs)>
    pub fn get_all_albums(&self) -> Result<Vec<(String, String, i64, i64)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT album, COALESCE(artist, 'Unknown Artist') AS primary_artist,
                    COUNT(*) AS track_count, COALESCE(SUM(duration), 0) AS total_duration
             FROM tracks
             WHERE album IS NOT NULL AND album != ''
             GROUP BY album, artist
             ORDER BY album, primary_artist",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get all tracks belonging to a specific album.
    pub fn get_tracks_by_album(&self, album: &str) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE album = ?1 ORDER BY track_number, title",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [album], |row| Track::from_row(row))
    }

    /// Get all distinct artist names with track count.
    /// Returns Vec<(artist_name, track_count, album_count)>
    pub fn get_all_artists(&self) -> Result<Vec<(String, i64, i64)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(artist, 'Unknown Artist') AS artist,
                    COUNT(*) AS track_count,
                    COUNT(DISTINCT album) AS album_count
             FROM tracks
             WHERE artist IS NOT NULL AND artist != ''
             GROUP BY artist
             ORDER BY artist",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get all tracks by a specific artist.
    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {} FROM tracks WHERE artist = ?1 ORDER BY album, track_number, title",
            TRACK_COLUMNS
        );
        self.query_map(&sql, [artist], |row| Track::from_row(row))
    }
}
