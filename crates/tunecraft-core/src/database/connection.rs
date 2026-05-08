use anyhow::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;
use std::path::Path;

/// Database configuration.
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub max_connections: u32,
    pub busy_timeout_ms: u32,
    pub journal_mode: String,
    pub synchronous: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            max_connections: 3,
            busy_timeout_ms: 5000,
            journal_mode: "WAL".to_string(),
            synchronous: "NORMAL".to_string(),
        }
    }
}

/// Initialise a connection pool with pragmas applied.
pub fn init_pool(path: &Path, config: &DbConfig) -> Result<Pool<SqliteConnectionManager>> {
    let manager = SqliteConnectionManager::file(path)
        .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE);

    let pool = Pool::builder()
        .max_size(config.max_connections)
        .connection_customizer(Box::new(PragmaCustomizer {
            busy_timeout_ms: config.busy_timeout_ms,
            journal_mode: config.journal_mode.clone(),
            synchronous: config.synchronous.clone(),
        }))
        .build(manager)?;

    Ok(pool)
}

/// r2d2 customizer to apply SQLite pragmas on each new connection.
#[derive(Debug, Clone)]
struct PragmaCustomizer {
    busy_timeout_ms: u32,
    journal_mode: String,
    synchronous: String,
}

impl r2d2::CustomizeConnection<rusqlite::Connection, rusqlite::Error> for PragmaCustomizer {
    fn on_acquire(&self, conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
        apply_pragmas(
            conn,
            self.busy_timeout_ms,
            &self.journal_mode,
            &self.synchronous,
        )
    }
}

/// Apply performance pragmas to a connection.
pub fn apply_pragmas(
    conn: &rusqlite::Connection,
    busy_timeout_ms: u32,
    journal_mode: &str,
    synchronous: &str,
) -> Result<(), rusqlite::Error> {
    const ALLOWED_JOURNAL_MODES: &[&str] =
        &["DELETE", "TRUNCATE", "PERSIST", "MEMORY", "WAL", "OFF"];
    const ALLOWED_SYNCHRONOUS: &[&str] = &["OFF", "NORMAL", "FULL", "EXTRA"];

    let safe_journal_mode = match ALLOWED_JOURNAL_MODES
        .iter()
        .find(|&&m| m.eq_ignore_ascii_case(journal_mode))
    {
        Some(&m) => m,
        None => {
            tracing::warn!(
                "Invalid journal_mode '{}', falling back to 'WAL'. Allowed values: {:?}",
                journal_mode,
                ALLOWED_JOURNAL_MODES
            );
            "WAL"
        }
    };
    let safe_synchronous = match ALLOWED_SYNCHRONOUS
        .iter()
        .find(|&&m| m.eq_ignore_ascii_case(synchronous))
    {
        Some(&m) => m,
        None => {
            tracing::warn!(
                "Invalid synchronous '{}', falling back to 'NORMAL'. Allowed values: {:?}",
                synchronous,
                ALLOWED_SYNCHRONOUS
            );
            "NORMAL"
        }
    };

    conn.execute_batch(&format!(
        "PRAGMA busy_timeout = {};\nPRAGMA journal_mode = {};\nPRAGMA synchronous = {};\nPRAGMA foreign_keys = ON;\nPRAGMA cache_size = -8000;",
        busy_timeout_ms, safe_journal_mode, safe_synchronous
    ))?;
    Ok(())
}
