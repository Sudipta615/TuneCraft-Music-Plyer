-- Add a UNIQUE constraint on albums(title, artist) so that UPSERT
-- operations in reconcile_aggregates() can use ON CONFLICT.
-- SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we recreate
-- the table (data is always re-derived from tracks, so this is safe).

CREATE TABLE IF NOT EXISTS albums_v2 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL DEFAULT '',
    artist TEXT,
    year INTEGER,
    genre TEXT,
    track_count INTEGER NOT NULL DEFAULT 0,
    duration_secs REAL NOT NULL DEFAULT 0.0,
    has_cover INTEGER NOT NULL DEFAULT 0,
    date_added TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(title, artist)
);

-- Migrate existing data
INSERT OR IGNORE INTO albums_v2 (id, title, artist, year, genre, track_count, duration_secs, has_cover, date_added)
    SELECT id, title, artist, year, genre, track_count, duration_secs, has_cover, date_added
    FROM albums;

DROP TABLE IF EXISTS albums;
ALTER TABLE albums_v2 RENAME TO albums;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_albums_title ON albums(title);
CREATE INDEX IF NOT EXISTS idx_albums_artist ON albums(artist);
