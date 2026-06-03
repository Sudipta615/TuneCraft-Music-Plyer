-- Local play history (offline scrobbling).
--
-- Every qualifying listen (≥ 50% played OR ≥ 4 minutes) gets one row.
-- This is the permanent, append-only play journal — never updated, only
-- inserted and (rarely) deleted when the referenced track is removed.
--
-- Design notes:
--   - `played_at` is stored as a Unix timestamp (INTEGER) for portability
--     and efficient range queries.
--   - `duration_played_secs` is the actual seconds heard, not the track
--     length.  Used for total-listening-time statistics.
--   - `completed` = 1 means the track played past the scrobble threshold
--     (50% or 4 min).  = 0 means it was interrupted but we still logged it
--     (for partial-listen stats — not used in play_count).
CREATE TABLE IF NOT EXISTS scrobbles (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    track_id         INTEGER NOT NULL,
    played_at        INTEGER NOT NULL,   -- Unix timestamp (UTC)
    duration_played_secs REAL NOT NULL DEFAULT 0.0,
    completed        INTEGER NOT NULL DEFAULT 1,  -- 1 = threshold reached
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

-- Fast lookup by track (most common query: "how many times did I play X?")
CREATE INDEX IF NOT EXISTS idx_scrobbles_track_id  ON scrobbles(track_id);
-- Fast lookup by date (history view, "on this day")
CREATE INDEX IF NOT EXISTS idx_scrobbles_played_at ON scrobbles(played_at);

-- Materialised per-track aggregate stats.
--
-- Updated by the application on every completed scrobble via upsert.
-- Avoids scanning the full scrobbles table for common UI queries
-- (most-played list, last-played date on the track row).
--
-- `first_played_at` and `last_played_at` are Unix timestamps.
CREATE TABLE IF NOT EXISTS listening_stats (
    track_id              INTEGER PRIMARY KEY,
    play_count            INTEGER NOT NULL DEFAULT 0,
    total_seconds_listened REAL    NOT NULL DEFAULT 0.0,
    first_played_at       INTEGER,   -- NULL until first completed play
    last_played_at        INTEGER,   -- NULL until first completed play
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);
