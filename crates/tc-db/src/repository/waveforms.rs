//! Waveform cache repository — waveform data persistence.

use rusqlite::params;

use super::{Database, DbError};
use crate::models::*;

impl Database {
    pub fn save_waveform(
        &self,
        track_id: i64,
        samples_per_pixel: i32,
        data: &[u8],
    ) -> Result<(), DbError> {
        self.write_lock()?.execute(
            "INSERT OR REPLACE INTO waveform_cache (track_id, samples_per_pixel, data, date_generated) VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)",
            params![track_id, samples_per_pixel, data],
        )?;
        Ok(())
    }

    pub fn get_waveform(
        &self,
        track_id: i64,
        samples_per_pixel: i32,
    ) -> Result<Option<WaveformCache>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT track_id, samples_per_pixel, data, date_generated FROM waveform_cache WHERE track_id = ?1 AND samples_per_pixel = ?2"
        )?;
        let result = stmt.query_row(params![track_id, samples_per_pixel], |row| {
            Ok(WaveformCache {
                track_id: row.get(0)?,
                samples_per_pixel: row.get(1)?,
                data: row.get(2)?,
                date_generated: row.get(3)?,
            })
        });
        match result {
            Ok(w) => Ok(Some(w)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }
}
