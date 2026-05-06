use anyhow::{Context, Result};
use rusqlite::Connection;

/// Run all database migrations.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Create schema_version tracking table first
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT (datetime('now'))
        );
        ",
    )
    .context("failed to create schema_version table")?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .context("failed to query schema version")?;

    // Version 1: Core tables (tracks, playlists, playlist_tracks, scrobble_queue)
    // Fix Bug #18: Wrap migration in an explicit transaction so partial failure
    // doesn't leave the DB in a half-migrated state.
    if current_version < 1 {
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v1 migration")?;
        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tracks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path   TEXT NOT NULL UNIQUE,
                file_hash   TEXT NOT NULL DEFAULT '',
                file_size   INTEGER,
                file_mtime  INTEGER,
                title       TEXT,
                artist      TEXT,
                album       TEXT,
                genre       TEXT,
                year        INTEGER,
                track_number INTEGER,
                duration    INTEGER,
                sample_rate INTEGER,
                bitrate     INTEGER,
                play_count  INTEGER DEFAULT 0,
                skip_count  INTEGER DEFAULT 0,
                rating      REAL DEFAULT 0.0,
                love        INTEGER DEFAULT 0,
                date_added  TEXT NOT NULL DEFAULT (datetime('now')),
                last_played TEXT,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                description TEXT,
                is_smart    INTEGER DEFAULT 0,
                rules       TEXT,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id INTEGER NOT NULL,
                track_id    INTEGER NOT NULL,
                position    INTEGER NOT NULL DEFAULT 0,
                added_at    TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (playlist_id, track_id),
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS scrobble_queue (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id    INTEGER NOT NULL,
                artist      TEXT NOT NULL DEFAULT '',
                title       TEXT NOT NULL DEFAULT '',
                album       TEXT,
                timestamp   INTEGER NOT NULL,
                status      TEXT DEFAULT 'pending',
                error_message TEXT,
                created_at  TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_file_path ON tracks(file_path);
            CREATE INDEX IF NOT EXISTS idx_tracks_file_hash ON tracks(file_hash);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
            CREATE INDEX IF NOT EXISTS idx_tracks_date_added ON tracks(date_added);
            CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist ON playlist_tracks(playlist_id);
            CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track ON playlist_tracks(track_id);
            CREATE INDEX IF NOT EXISTS idx_scrobble_queue_status ON scrobble_queue(status);

            CREATE TABLE IF NOT EXISTS cover_art_cache (
                file_hash   TEXT PRIMARY KEY,
                data        BLOB NOT NULL,
                mime_type   TEXT NOT NULL DEFAULT 'image/jpeg',
                created_at  TEXT DEFAULT (datetime('now'))
            );

            INSERT INTO schema_version (version) VALUES (1);
            ",
        )
        .context("failed to run v1 migrations")?;
        tx.commit().context("failed to commit v1 migration")?;
    }

    // Version 2: Add eq_presets, user_prefs, waveforms tables
    // Fix Bug #18: Wrap migration in an explicit transaction.
    if current_version < 2 {
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v2 migration")?;
        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS eq_presets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                bands TEXT NOT NULL,
                preamp REAL DEFAULT 0.0,
                is_builtin INTEGER DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS user_prefs (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS waveforms (
                file_hash TEXT PRIMARY KEY,
                peaks TEXT NOT NULL,
                sample_rate INTEGER NOT NULL DEFAULT 44100
            );

            INSERT INTO schema_version (version) VALUES (2);
            ",
        )
        .context("failed to run v2 migrations")?;
        tx.commit().context("failed to commit v2 migration")?;
    }

    // Version 3: Add mood analysis columns to tracks
    // Fix Bug #18: Wrap migration in an explicit transaction.
    if current_version < 3 {
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v3 migration")?;
        tx.execute_batch(
            "
            ALTER TABLE tracks ADD COLUMN bpm              REAL;
            ALTER TABLE tracks ADD COLUMN energy           REAL;
            ALTER TABLE tracks ADD COLUMN bass_ratio       REAL;
            ALTER TABLE tracks ADD COLUMN spectral_centroid REAL;
            ALTER TABLE tracks ADD COLUMN dynamic_range    REAL;
            ALTER TABLE tracks ADD COLUMN mood             TEXT;
            ALTER TABLE tracks ADD COLUMN mood_override    TEXT;

            CREATE INDEX IF NOT EXISTS idx_tracks_mood ON tracks(mood);
            CREATE INDEX IF NOT EXISTS idx_tracks_mood_effective ON tracks(COALESCE(mood_override, mood));

            INSERT INTO schema_version (version) VALUES (3);
            ",
        )
        .context("failed to run v3 migrations")?;
        tx.commit().context("failed to commit v3 migration")?;
    }

    // Version 4: Add track_number column to tracks (for databases created before v1 included it)
    // Fix Bug #18: Wrap migration in an explicit transaction.
    if current_version < 4 {
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v4 migration")?;

        // Check if track_number column already exists (it may have been added in v1 for fresh DBs)
        let has_track_number: bool = tx
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('tracks') WHERE name = 'track_number'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if !has_track_number {
            tx.execute_batch(
                "
                ALTER TABLE tracks ADD COLUMN track_number INTEGER;
                ",
            )
            .context("failed to add track_number column")?;
        }

        tx.execute_batch("INSERT INTO schema_version (version) VALUES (4);")
            .context("failed to update schema version to 4")?;

        tx.commit().context("failed to commit v4 migration")?;
    }

    // Version 5: Migrate cover art out of the tracks table into a separate
    // cover_art_cache table keyed by file_hash. This prevents large BLOBs from
    // being loaded on every SELECT * and allows lazy loading with an LRU cache.
    //
    // Fix Bug #9: On fresh installs, the `cover_art` column doesn't exist in the
    // v1 CREATE TABLE, so `SELECT cover_art FROM tracks` crashes. We now check
    // whether the column exists before attempting the migration.
    // Fix Bug #37: Wrap multi-step migrations in a transaction so partial failure
    // doesn't leave the DB in a half-migrated state.
    if current_version < 5 {
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v5 migration")?;

        // Create the cover_art_cache table if it doesn't exist (idempotent)
        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS cover_art_cache (
                file_hash   TEXT PRIMARY KEY,
                data        BLOB NOT NULL,
                mime_type   TEXT NOT NULL DEFAULT 'image/jpeg',
                created_at  TEXT DEFAULT (datetime('now'))
            );
            ",
        )
        .context("failed to create cover_art_cache table")?;

        // Only attempt to migrate cover_art from tracks if the column exists.
        // On fresh installs (v1 → v5 in one pass), the `cover_art` column
        // was never added, so this SELECT would crash.
        let has_cover_art_col: bool = tx
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('tracks') WHERE name = 'cover_art'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if has_cover_art_col {
            tx.execute_batch(
                "
                -- Move existing art into the new table
                INSERT OR IGNORE INTO cover_art_cache (file_hash, data)
                SELECT file_hash, cover_art
                FROM tracks
                WHERE cover_art IS NOT NULL AND file_hash != '';
                ",
            )
            .context("failed to migrate cover_art data")?;

            // SQLite 3.35.0+ (released 2021-03-12) is required for DROP COLUMN.
            // Older SQLite versions (e.g. on older Linux distros or bundled
            // SQLite builds) will fail with a syntax error. Check the version
            // and skip the column drop if unsupported — the data has already
            // been migrated above, so leaving the column in-place is harmless.
            let sqlite_version: i64 = tx
                .query_row("SELECT sqlite_version()", [], |row| {
                    let v: String = row.get(0)?;
                    // Parse "3.35.0" → 3_035_000 (major*1_000_000 + minor*1_000 + patch)
                    let parts: Vec<i64> = v.split('.').filter_map(|p| p.parse().ok()).collect();
                    Ok(parts.get(0).unwrap_or(&3) * 1_000_000
                        + parts.get(1).unwrap_or(&0) * 1_000
                        + parts.get(2).unwrap_or(&0))
                })
                .unwrap_or(0);

            if sqlite_version >= 3_035_000 {
                tx.execute_batch("ALTER TABLE tracks DROP COLUMN cover_art;")
                    .context("failed to drop cover_art column")?;
            } else {
                tracing::warn!(
                    "SQLite version {} does not support DROP COLUMN (requires 3.35.0+). \
                     The cover_art column in tracks will remain but is unused — no data loss.",
                    sqlite_version
                );
            }
        }

        tx.execute_batch("INSERT INTO schema_version (version) VALUES (5);")
            .context("failed to update schema version to 5")?;

        tx.commit().context("failed to commit v5 migration")?;
    }

    // Version 6: Encrypt sensitive credentials in user_prefs.
    // Last.fm API keys, API secrets, and session keys that were previously
    // stored as plaintext are now encrypted with AES-256-GCM using a
    // machine-derived key. This prevents credential extraction from stolen
    // database files. The `enc:v1:` prefix identifies encrypted values.
    // Unencrypted values (no prefix) are still readable for backward compat.
    if current_version < 6 {
        // Fix H6: Wrap the V6 credential encryption migration in a transaction
        // to prevent partial migration on failure (e.g. encrypt fails midway).
        let tx = conn
            .unchecked_transaction()
            .context("failed to begin transaction for v6 migration")?;

        // Migrate credentials from plaintext to encrypted storage
        let credential_keys = ["lastfm_api_key", "lastfm_api_secret", "lastfm_session_key"];

        for key in &credential_keys {
            // Read the current value
            let value: Option<String> = tx
                .query_row(
                    "SELECT value FROM user_prefs WHERE key = ?1",
                    rusqlite::params![key],
                    |row| row.get(0),
                )
                .ok();

            if let Some(plain_value) = value {
                // Only encrypt if not already encrypted
                if !crate::util::crypto::is_encrypted(&plain_value) {
                    match crate::util::crypto::encrypt(&plain_value) {
                        Ok(encrypted) => {
                            tx.execute(
                                "UPDATE user_prefs SET value = ?1 WHERE key = ?2",
                                rusqlite::params![encrypted, key],
                            )
                            .context(format!("failed to encrypt credential '{}'", key))?;
                        }
                        Err(e) => {
                            // Log but don't fail the migration — the credential
                            // will still work unencrypted, and will be encrypted
                            // on the next save
                            tracing::warn!(
                                "Failed to encrypt credential '{}' during migration (will encrypt on next save): {}",
                                key, e
                            );
                        }
                    }
                }
            }
        }

        tx.execute_batch("INSERT INTO schema_version (version) VALUES (6);")
            .context("failed to update schema version to 6")?;

        tx.commit().context("failed to commit v6 migration")?;
    }

    Ok(())
}
