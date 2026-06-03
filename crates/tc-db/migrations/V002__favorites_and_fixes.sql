-- Favorites table: persistent user favorites (replaces play_count > 5 heuristic)
CREATE TABLE IF NOT EXISTS favorites (
    track_id INTEGER NOT NULL,
    date_favorited TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (track_id),
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

-- Fix playlist_tracks: use (playlist_id, position) as the primary key
-- so the same track CAN appear more than once in a playlist at different
-- positions. v0.8.6: Changed from (playlist_id, track_id) to allow
-- duplicate track entries in a single playlist, preserving the ability
-- to create manual playlists with repeated tracks.
-- SQLite doesn't support ALTER TABLE DROP CONSTRAINT, so we recreate:
CREATE TABLE IF NOT EXISTS playlist_tracks_v2 (
    playlist_id INTEGER NOT NULL,
    track_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position),
    FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

-- Migrate existing data (preserve all entries including duplicates)
INSERT OR IGNORE INTO playlist_tracks_v2 (playlist_id, track_id, position)
    SELECT playlist_id, track_id, position
    FROM playlist_tracks;

DROP TABLE IF EXISTS playlist_tracks;
ALTER TABLE playlist_tracks_v2 RENAME TO playlist_tracks;

-- Index for fast favorites lookup
CREATE INDEX IF NOT EXISTS idx_favorites_track ON favorites(track_id);

-- Album population: insert albums derived from tracks
INSERT OR IGNORE INTO albums (title, artist, year, genre, track_count, duration_secs)
    SELECT
        album,
        album_artist,
        MAX(year),
        MAX(genre),
        COUNT(*),
        SUM(duration_secs)
    FROM tracks
    WHERE album IS NOT NULL AND album != ''
    GROUP BY album, album_artist;

-- Artist population: insert artists derived from tracks
INSERT OR IGNORE INTO artists (name, album_count, track_count)
    SELECT
        artist,
        COUNT(DISTINCT album),
        COUNT(*)
    FROM tracks
    WHERE artist IS NOT NULL AND artist != ''
    GROUP BY artist;
