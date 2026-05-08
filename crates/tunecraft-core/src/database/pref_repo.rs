use anyhow::Result;
use rusqlite::params;

use crate::database::Database;

impl Database {
    /// Get a user preference value.
    pub fn get_pref(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        match conn.query_row(
            "SELECT value FROM user_prefs WHERE key = ?1",
            params![key],
            |r| r.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a user preference value.
    pub fn set_pref(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO user_prefs (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get all EQ presets.
    pub fn get_eq_presets(&self) -> Result<Vec<(i64, String, String, f64)>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT id, name, bands, preamp FROM eq_presets ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        let mut presets = Vec::new();
        for row in rows {
            presets.push(row?);
        }
        Ok(presets)
    }

    /// Save an EQ preset.
    pub fn save_eq_preset(&self, name: &str, bands_json: &str, preamp: f64) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO eq_presets (name, bands, preamp) VALUES (?1, ?2, ?3)",
            params![name, bands_json, preamp],
        )?;
        conn.execute(
            "UPDATE eq_presets SET bands = ?2, preamp = ?3 WHERE name = ?1",
            params![name, bands_json, preamp],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM eq_presets WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Delete an EQ preset.
    pub fn delete_eq_preset(&self, id: i64) -> Result<usize> {
        let conn = self.conn()?;
        Ok(conn.execute("DELETE FROM eq_presets WHERE id = ?1", params![id])?)
    }
}
