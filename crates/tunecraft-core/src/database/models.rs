use chrono::NaiveDate;
use rusqlite::Row;
use serde::{Deserialize, Serialize};

/// A track in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Option<i64>,
    pub file_path: String,
    pub file_hash: Option<String>,
    pub file_size: Option<i64>,
    pub file_mtime: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub duration: Option<u64>,
    pub sample_rate: Option<i32>,
    pub bitrate: Option<i32>,
    pub play_count: Option<i64>,
    pub skip_count: Option<i64>,
    pub rating: Option<f64>,
    pub love: Option<i32>,
    pub bpm: Option<f64>,
    pub energy: Option<f64>,
    pub bass_ratio: Option<f64>,
    pub spectral_centroid: Option<f64>,
    pub dynamic_range: Option<f64>,
    pub mood: Option<String>,
    pub mood_override: Option<String>,
    pub date_added: NaiveDate,
    pub last_played: Option<NaiveDate>,
}

impl Track {
    /// Construct a Track from a database row.
    /// Fix Bug #4: Log a warning when non-critical column reads fail instead of
    /// silently swallowing errors. The `file_path` column is still required (hard
    /// error on failure), but optional metadata columns now emit a tracing::warn
    /// so that data issues are diagnosable from logs.
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let date_added_str: String = row.get("date_added").unwrap_or_default();
        let date_added = NaiveDate::parse_from_str(&date_added_str, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(&date_added_str, "%Y-%m-%d %H:%M:%S"))
            .unwrap_or_else(|_| {
                tracing::warn!(
                    "Unparseable date_added '{}', falling back to 1970-01-01",
                    date_added_str
                );
                NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
            });

        let last_played_str: Option<String> = row.get("last_played").ok();
        let last_played = last_played_str.and_then(|s| {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(&s, "%Y-%m-%d %H:%M:%S"))
                .ok()
        });

        macro_rules! try_col {
            ($col:expr, $ty:ty) => {
                match row.get::<_, $ty>($col) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::warn!("Column '{}' read failed in Track::from_row: {}", $col, e);
                        None
                    }
                }
            };
        }

        Ok(Self {
            id: try_col!("id", i64),
            file_path: row.get("file_path")?,
            file_hash: try_col!("file_hash", String),
            file_size: try_col!("file_size", i64),
            file_mtime: try_col!("file_mtime", i64),
            title: try_col!("title", String),
            artist: try_col!("artist", String),
            album: try_col!("album", String),
            genre: try_col!("genre", String),
            year: try_col!("year", i32),
            track_number: try_col!("track_number", i32),
            duration: row
                .get("duration")
                .ok()
                .and_then(|d: i64| if d > 0 { Some(d as u64) } else { None }),
            sample_rate: try_col!("sample_rate", i32),
            bitrate: try_col!("bitrate", i32),
            play_count: try_col!("play_count", i64),
            skip_count: try_col!("skip_count", i64),
            rating: try_col!("rating", f64),
            love: try_col!("love", i32),
            bpm: try_col!("bpm", f64),
            energy: try_col!("energy", f64),
            bass_ratio: try_col!("bass_ratio", f64),
            spectral_centroid: try_col!("spectral_centroid", f64),
            dynamic_range: try_col!("dynamic_range", f64),
            mood: try_col!("mood", String),
            mood_override: try_col!("mood_override", String),
            date_added,
            last_played,
        })
    }

    /// Display duration as "M:SS" string.
    pub fn duration_display(&self) -> String {
        match self.duration {
            Some(secs) => {
                let m = secs / 60;
                let s = secs % 60;
                format!("{}:{:02}", m, s)
            }
            None => "0:00".to_string(),
        }
    }

    /// Get the title, falling back to filename.
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("Unknown Title")
    }

    /// Get the artist, falling back to "Unknown Artist".
    pub fn display_artist(&self) -> &str {
        self.artist.as_deref().unwrap_or("Unknown Artist")
    }

    /// Get the album, falling back to "Unknown Album".
    pub fn display_album(&self) -> &str {
        self.album.as_deref().unwrap_or("Unknown Album")
    }
}

/// A user playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub rules: Option<String>, // JSON-serialized smart playlist rules
    pub created_at: String,
    pub updated_at: String,
}

impl Playlist {
    /// Construct a Playlist from a database row.
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id").ok(),
            name: row.get("name")?,
            description: row.get("description").ok(),
            is_smart: row.get("is_smart").unwrap_or(false),
            rules: row.get("rules").ok(),
            created_at: row.get("created_at").unwrap_or_default(),
            updated_at: row.get("updated_at").unwrap_or_default(),
        })
    }
}
