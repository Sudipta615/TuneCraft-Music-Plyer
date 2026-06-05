//! EQ preset repository — equalizer preset CRUD operations.

use crate::models::*;
use rusqlite::params;

use super::tracks::log_and_filter;
use super::{Database, DbError};

impl Database {
    pub fn save_eq_preset(
        &self,
        name: &str,
        config_json: &str,
        is_builtin: bool,
    ) -> Result<i64, DbError> {
        if name.is_empty() {
            return Err(DbError::Validation(
                "EQ preset name must not be empty".into(),
            ));
        }

        let conn = self.write_lock()?;
        // Use UPSERT instead of INSERT OR REPLACE to preserve row ID and
        // date_created for existing presets.
        conn.execute(
            "INSERT INTO eq_presets (name, config_json, is_builtin) VALUES (?1, ?2, ?3)\
             ON CONFLICT(name) DO UPDATE SET config_json=excluded.config_json, is_builtin=excluded.is_builtin",
            params![name, config_json, is_builtin as i32],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM eq_presets WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn get_eq_presets(&self) -> Result<Vec<EqPreset>, DbError> {
        let lock = self.read_lock()?;
        let mut stmt = lock.prepare_cached(
            "SELECT id, name, config_json, is_builtin, date_created FROM eq_presets ORDER BY name",
        )?;
        let presets = stmt
            .query_map([], |row| {
                Ok(EqPreset {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    config_json: row.get(2)?,
                    is_builtin: row.get::<_, i32>(3)? != 0,
                    date_created: row.get(4)?,
                })
            })?
            .filter_map(log_and_filter)
            .collect();
        Ok(presets)
    }

    /// Delete an EQ preset by ID.
    ///
    ///
    /// Returns `NotFound` if no preset with the given ID exists.
    /// Built-in presets can be deleted (callers should check `is_builtin`
    /// first if they want to protect built-in presets).
    pub fn delete_eq_preset(&self, id: i64) -> Result<(), DbError> {
        let removed = self
            .write_lock()?
            .execute("DELETE FROM eq_presets WHERE id = ?1", params![id])?;
        if removed == 0 {
            return Err(DbError::NotFound(format!(
                "EQ preset with id {} does not exist",
                id
            )));
        }
        Ok(())
    }

    /// Delete an EQ preset by name.
    ///
    ///
    pub fn delete_eq_preset_by_name(&self, name: &str) -> Result<(), DbError> {
        let removed = self
            .write_lock()?
            .execute("DELETE FROM eq_presets WHERE name = ?1", params![name])?;
        if removed == 0 {
            return Err(DbError::NotFound(format!(
                "EQ preset '{}' does not exist",
                name
            )));
        }
        Ok(())
    }
}
