use anyhow::Result;
use rusqlite::params;

use crate::database::Database;

impl Database {
    /// Load cover art from the separate cover_art_cache table by file_hash.
    /// Returns None if no art is cached for this hash.
    // Fix Bug #17: Distinguish "not found" from real DB errors instead of
    // using .ok() which swallows corruption/locked DB/IO failures as None.
    // Fix Bug #22: Reject empty file_hash. An empty hash would match a single
    // row that all empty-hash tracks share, causing the wrong cover art to be
    // returned for every track missing a hash.
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
    // Fix Bug #22: Validate file_hash is non-empty. An empty hash would insert
    // a single row keyed on '' that get_cover_art would then return for every
    // track with an empty file_hash, causing all such tracks to share one cover.
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
