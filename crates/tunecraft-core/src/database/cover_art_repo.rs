use anyhow::Result;
use rusqlite::params;

use crate::database::Database;

impl Database {
    /// Load cover art from the separate cover_art_cache table by file_hash.
    /// Returns None if no art is cached for this hash.
    pub fn get_cover_art(&self, file_hash: &str) -> Result<Option<Vec<u8>>> {
        if file_hash.is_empty() {
            return Ok(None);
        }
        let conn = self.conn()?;
        match conn.query_row(
            "SELECT data FROM cover_art_cache WHERE file_hash = ?1",
            [file_hash],
            |r| r.get(0),
        ) {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save cover art to the cover_art_cache table.
    /// Uses INSERT OR REPLACE so that updated art replaces old entries.
    pub fn save_cover_art(&self, file_hash: &str, data: &[u8], mime_type: &str) -> Result<()> {
        if file_hash.is_empty() {
            anyhow::bail!("save_cover_art: file_hash must not be empty");
        }
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO cover_art_cache (file_hash, data, mime_type) VALUES (?1, ?2, ?3)",
            params![file_hash, data, mime_type],
        )?;
        Ok(())
    }
}
