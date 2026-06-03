-- V005: Add data BLOB column to cover_art table
--
-- The original schema stored only path, data_hash, width, height, and
-- mime_type for cover art. The path column was intended for on-disk cache
-- files, but the cover art extraction pipeline (tc-library) extracts image
-- binary data in memory. Without a data column, the extracted bytes were
-- discarded immediately after hashing, making the entire cover art feature
-- non-functional.
--
-- This migration adds a nullable data BLOB column to cover_art. Storing
-- cover art inline in SQLite is appropriate for typical album art sizes
-- (50–500 KB); for very large libraries the data can be written to a cache
-- directory instead and path populated instead. Both approaches now work.
--
-- Existing rows retain NULL in the data column (their art was never stored).

ALTER TABLE cover_art ADD COLUMN data BLOB;

-- Update db_metadata version
UPDATE db_metadata SET value = '0.8.10' WHERE key = 'app_version';
