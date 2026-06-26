//! Database repository — thin facade over specialized sub-modules.
//!
//! The [`Database`] struct is the public API surface. All SQL logic has been
//! decomposed into focused sub-modules, each handling one responsibility
//! domain. The public methods on `Database` remain unchanged; only the
//! internal implementation has been distributed across files.
//!
//! # Module layout
//!
//! - [`tracks`]      — Track CRUD, search, batch operations
//! - [`playlists`]   — Playlist CRUD and track-association operations
//! - [`favorites`]   — Favorite track operations
//! - [`eq_presets`]  — Equalizer preset CRUD
//! - [`waveforms`]   — Waveform cache read/write
//! - [`covers`]      — Cover art data persistence
//! - [`aggregates`]  — Album/artist/playlist counter reconciliation

mod aggregates;
mod covers;
pub(crate) mod eq_presets; // Bug #1 fix: pub(crate) so sibling modules can import from it
mod favorites;
mod playlists;
mod tracks;
mod waveforms;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use rusqlite::{params, Connection};
use thiserror::Error;
pub use tracks::TRACK_COLUMNS;

use crate::migrations;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Database mutex poisoned")]
    Poisoned,
}

/// Global counter for unique in-memory database URIs (H22 fix).
static IN_MEMORY_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Thread-safe SQLite database wrapper.
///
/// Uses a dual-connection pattern with WAL mode to minimize contention:
/// - **Write connection** (`write_conn`): used for mutations (inserts, updates, deletes)
/// - **Read connection** (`read_conn`): used for queries (selects)
///
/// Because WAL mode allows concurrent reads while a write is in progress,
///   the read connection never blocks on write transactions. This eliminates
///   the UI freezes that occurred in v0.7.x when the scan thread held the
///   single connection's Mutex for the entire scan duration.
///
/// Both connections are wrapped in `Mutex` for `Send` safety, but the
///   read Mutex is held only for the duration of a single query, never
///   for an entire scan loop.
///
/// # Lock Ordering
///
/// To prevent deadlocks, the following lock ordering MUST be observed:
/// 1. `write_conn` Mutex — always acquired first if both are needed
/// 2. `read_conn` Mutex — always acquired after write_conn
///
/// In practice, the dual-connection design means we never need both locks
///    simultaneously: read operations use `read_conn`, write operations use
///    `write_conn`, and WAL mode ensures they don't block each other.
///
/// If a future change requires holding both locks (e.g., a transaction
///    that reads from one and writes to the other), always acquire
///    `write_conn` first, then `read_conn`.
pub struct Database {
    /// Write connection — used for all mutations
    write_conn: Mutex<Connection>,
    /// Read connection — used for all queries
    read_conn: Mutex<Connection>,
}

impl Database {
    /// Open database at the given path, creating it if needed.
    ///
    /// Opens two connections to the same database file with WAL mode
    /// enabled, allowing concurrent reads and writes.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DbError::Migration(format!("Cannot create database directory: {}", e))
            })?;
        }

        // Primary (write) connection
        let mut write_conn = Connection::open(path)?;
        write_conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        migrations::run_migrations(&mut write_conn)
            .map_err(|e| DbError::Migration(e.to_string()))?;

        Self::check_version_compatibility(&write_conn)?;

        // WAL mode allows this connection to read while the write
        // connection has an active transaction.
        let read_conn = match Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(conn) => conn,
            Err(e) => {
                // Fallback: if read-only connection fails (e.g., first run
                // before WAL files exist), open a normal read-write connection.
                log::warn!(
                    "Read-only DB connection failed ({}), falling back to read-write connection",
                    e
                );
                Connection::open(path).map_err(DbError::Sqlite)?
            },
        };

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
        })
    }

    /// Check that the database was not created by a newer version of the application.
    /// If the stored version is newer than the current version, return an error
    /// to prevent silent data corruption from schema mismatches.
    fn check_version_compatibility(conn: &Connection) -> Result<(), DbError> {
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='db_metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if table_exists {
            let stored_version: Option<String> = conn
                .query_row(
                    "SELECT value FROM db_metadata WHERE key = 'app_version'",
                    [],
                    |row| row.get(0),
                )
                .ok();

            if let Some(version) = stored_version {
                let current_version = env!("CARGO_PKG_VERSION");
                if version.as_str() != current_version {
                    let stored_parts: Vec<u32> =
                        version.split('.').filter_map(|s| s.parse().ok()).collect();
                    let current_parts: Vec<u32> = current_version
                        .split('.')
                        .filter_map(|s| s.parse().ok())
                        .collect();

                    let max_len = stored_parts.len().max(current_parts.len());
                    let mut stored_padded = stored_parts.clone();
                    let mut current_padded = current_parts.clone();
                    stored_padded.resize(max_len, 0);
                    current_padded.resize(max_len, 0);

                    // If stored version is newer (higher major.minor.patch), refuse to open in release builds
                    if stored_padded > current_padded {
                        #[cfg(not(debug_assertions))]
                        return Err(DbError::Migration(format!(
                            "Database was created by TuneCraft v{}, but this is v{}. \
                             Downgrading is not supported — please upgrade the application.",
                            version, current_version
                        )));
                        #[cfg(debug_assertions)]
                        log::warn!(
                            "Database was created by TuneCraft v{}, but this is v{}. \
                             Allowing downgrade in debug build.",
                            version, current_version
                        );
                    }

                    let _ = conn.execute(
                        "UPDATE db_metadata SET value = ?1 WHERE key = 'app_version'",
                        params![current_version],
                    );
                }
            }
        }

        Ok(())
    }

    /// Open in-memory database (for testing)
    ///
    ///
    /// separate in-memory databases — the read connection couldn't see
    /// data written by the write connection. Now both connections share
    /// the same underlying in-memory database via `file:mem?cache=shared`
    /// URI, and both are opened in read-write mode so the read connection
    /// can access data inserted by the write connection.
    ///
    ///
    /// behavior. Note: shared-cache mode uses table-level locking
    /// rather than WAL mode's database-level locking, so concurrency
    /// behavior in tests may differ from production. Test authors
    /// should be aware of this difference.
    pub fn open_in_memory() -> Result<Self, DbError> {
        // shared "file:mem?mode=memory&cache=shared" so that parallel test
        // instances each get their own database. The static counter ensures
        // uniqueness; the thread ID is also included as an extra guard.
        let unique_id = IN_MEMORY_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let thread_id = std::thread::current().id();
        let mem_uri = format!(
            "file:mem_{}_{}?mode=memory&cache=shared",
            unique_id,
            format!("{:?}", thread_id).replace(':', "_")
        );

        let mut write_conn = Connection::open_with_flags(
            &mem_uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        write_conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
        migrations::run_migrations(&mut write_conn)
            .map_err(|e| DbError::Migration(e.to_string()))?;

        // Both read and write use the same shared cache, so reads see writes.
        let read_conn = Connection::open_with_flags(
            &mem_uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
        })
    }

    /// Acquire the read connection lock.
    pub(crate) fn read_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, DbError> {
        self.read_conn.lock().map_err(|_| DbError::Poisoned)
    }

    /// Acquire the write connection lock.
    pub(crate) fn write_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, DbError> {
        self.write_conn.lock().map_err(|_| DbError::Poisoned)
    }

    /// Execute a closure with the write connection.
    ///
    /// This is a convenience method for callers that need to perform
    /// arbitrary database operations without managing the lock manually.
    /// The closure receives a `&Connection` with the write lock already held.
    /// For read-only operations, prefer `read_lock()` directly.
    ///
    /// # Errors
    ///
    /// Returns `DbError::Poisoned` if the mutex is poisoned, or any
    /// error the closure produces.
    pub fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, rusqlite::Error>,
    {
        let conn = self.write_lock()?;
        f(&conn).map_err(DbError::Sqlite)
    }

    /// Execute a closure within a database transaction.
    ///
    /// If the closure returns `Ok`, the transaction is committed.
    /// If the closure returns `Err`, the transaction is rolled back.
    ///
    /// The closure receives a `&Connection` that is already inside a
    /// transaction; callers must **not** attempt to open a nested
    /// transaction on the same connection.
    pub fn transaction<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, E>,
        E: From<DbError>,
    {
        let conn = self.write_lock().map_err(E::from)?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| E::from(DbError::Sqlite(e)))?;
        let result = f(&tx)?;
        tx.commit().map_err(|e| E::from(DbError::Sqlite(e)))?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = Database::open_in_memory();
        assert!(db.is_ok(), "Should be able to open in-memory database");
    }
}
