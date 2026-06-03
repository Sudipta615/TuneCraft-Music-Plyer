-- V004: Remove redundant indexes and add database metadata table
--
-- 1. Remove redundant indexes on UNIQUE-constrained columns.
--    SQLite automatically creates indexes for UNIQUE constraints, so
--    the explicit indexes are duplicates that waste disk space and
--    slow down insertions.
-- 2. Add a db_metadata table for version compatibility checks,
--    preventing data corruption when opening a database created by
--    a newer version of the application.

-- Remove redundant index on tracks.path (UNIQUE constraint already creates one)
DROP INDEX IF EXISTS idx_tracks_path;

-- Remove redundant index on artists.name (UNIQUE constraint already creates one)
DROP INDEX IF EXISTS idx_artists_name;

-- Create metadata table for version tracking and other key-value data
CREATE TABLE IF NOT EXISTS db_metadata (
    key TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
);

-- Store the application version for compatibility checks
INSERT OR IGNORE INTO db_metadata (key, value) VALUES ('app_version', '0.8.9');
