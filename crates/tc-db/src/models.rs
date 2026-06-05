use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// A music track in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration_secs: f64,
    pub sample_rate: i32,
    pub channels: i32,
    pub bitrate_kbps: Option<i32>,
    pub format: String,
    pub file_size: i64,
    pub file_modified: i64, // Unix timestamp
    pub crc32: Option<u32>,
    pub replaygain_track_db: Option<f64>,
    pub replaygain_album_db: Option<f64>,
    pub replaygain_track_peak: Option<f64>,
    pub replaygain_album_peak: Option<f64>,
    pub ebu_r128_loudness: Option<f64>,
    pub ebu_r128_peak: Option<f64>,
    pub bpm: Option<f64>,
    pub mood: Option<String>,
    pub lyrics_synced: Option<String>,
    pub lyrics_unsynced: Option<String>,
    pub last_played: Option<NaiveDateTime>,
    pub play_count: i32,
    pub date_added: NaiveDateTime,
    pub date_scanned: NaiveDateTime,
}

/// An album in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub track_count: i32,
    pub duration_secs: f64,
    pub has_cover: bool,
    pub date_added: NaiveDateTime,
}

/// An artist in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub album_count: i32,
    pub track_count: i32,
}

/// A playlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub smart_rules: Option<String>, // JSON
    pub track_count: i32,
    pub duration_secs: f64,
    pub date_created: NaiveDateTime,
    pub date_modified: NaiveDateTime,
}

/// A track's position in a playlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub playlist_id: i64,
    pub track_id: i64,
    pub position: i32,
}

/// Cover art cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverArt {
    pub id: i64,
    pub album_id: Option<i64>,
    pub track_id: Option<i64>,
    pub path: Option<String>,
    pub data: Option<Vec<u8>>,
    pub data_hash: Option<String>,
    pub width: i32,
    pub height: i32,
    pub mime_type: String,
}

/// Loudness metadata cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoudnessMeta {
    pub track_id: i64,
    pub replaygain_track_db: Option<f64>,
    pub replaygain_album_db: Option<f64>,
    pub replaygain_track_peak: Option<f64>,
    pub replaygain_album_peak: Option<f64>,
    pub ebu_r128_loudness: Option<f64>,
    pub ebu_r128_peak: Option<f64>,
}

/// Waveform cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformCache {
    pub track_id: i64,
    pub samples_per_pixel: i32,
    pub data: Vec<u8>, // Serialized min/max pairs
    pub date_generated: NaiveDateTime,
}

/// EQ preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqPreset {
    pub id: i64,
    pub name: String,
    pub config_json: String, // Serialized EqConfig
    pub is_builtin: bool,
    pub date_created: NaiveDateTime,
}
