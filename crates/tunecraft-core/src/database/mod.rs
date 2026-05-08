pub mod album_repo;
pub mod connection;
pub mod cover_art_repo;
pub mod migrations;
pub mod models;
pub mod mood_repo;
pub mod playlist_repo;
pub mod pref_repo;
pub mod scrobble_repo;
pub mod track_repo;

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

use crate::database::connection::{init_pool, DbConfig};

/// Explicit column list for the tracks table, excluding `cover_art`.
/// Using this instead of `SELECT *` avoids loading large BLOBs on every
/// query — cover art is now loaded lazily from the `cover_art_cache` table
/// via `Database::get_cover_art()`.
pub const TRACK_COLUMNS: &str = "
    id, file_path, file_hash, file_size, file_mtime, title, artist, album,
    genre, year, track_number, duration, sample_rate, bitrate, play_count,
    skip_count, rating, love, bpm, energy, bass_ratio, spectral_centroid,
    dynamic_range, mood, mood_override, date_added, last_played
";

/// Database wrapper providing async and sync execution.
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    /// Open a database at the given path, running migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let config = DbConfig::default();
        let pool = init_pool(path, &config)?;
        let db = Self { pool };

        let conn = db
            .pool
            .get()
            .context("failed to get connection for migrations")?;
        migrations::run_migrations(&conn)?;

        Ok(db)
    }

    /// Get the database file path (for display purposes).
    pub fn data_dir() -> Result<std::path::PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "tunecraft", "TuneCraft")
            .context("failed to determine project directories")?;
        let data_dir = dirs.data_dir().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;
        Ok(data_dir)
    }

    /// Get the default database path.
    pub fn default_path() -> Result<std::path::PathBuf> {
        Ok(Self::data_dir()?.join("tunecraft.db"))
    }

    /// Execute a statement asynchronously (runs on tokio blocking thread).
    #[allow(dead_code)]
    pub async fn execute_async(
        &self,
        sql: &str,
        param_values: Vec<Box<dyn rusqlite::types::ToSql + Send + Sync>>,
    ) -> Result<usize> {
        let pool = self.pool.clone();
        let sql = sql.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(&sql)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(|p| p.as_ref() as &dyn rusqlite::types::ToSql)
                .collect();
            let affected = stmt.execute(rusqlite::params_from_iter(param_refs))?;
            Ok(affected)
        })
        .await?
    }

    /// Execute a statement synchronously.
    pub fn execute_sync(&self, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Result<usize> {
        let conn = self.pool.get().context("failed to get connection")?;
        let affected = conn.execute(sql, params)?;
        Ok(affected)
    }

    /// Get a connection from the pool.
    pub fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool
            .get()
            .context("failed to get connection from pool")
    }

    /// Query and map rows.
    pub fn query_map<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<Vec<T>>
    where
        P: rusqlite::Params,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}
