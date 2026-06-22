//! Converters — translate Rust domain types into Slint-compatible structs.
//!
//! Slint generates Rust structs for every `struct` declared in .slint files.
//! These structs have `Default` impls and public fields, so we can construct
//! them directly. This module isolates the conversion logic so the rest of
//! the codebase stays clean.

use chrono::NaiveDateTime;

use crate::{
    NavItem, PlaylistItem, ToastItem, TrackItem, PlayerState, EqBandItem,
    FolderItem, LyricsLine,
};

/// Convert a `tc_db::Track` to a `TrackItem` for Slint.
///
/// `is_playing` / `is_paused` / `is_favorite` flags are computed by the caller
/// based on current playback state, not stored in the DB row.
pub fn track_to_item(
    track: &tc_db::Track,
    is_playing: bool,
    is_paused: bool,
    is_favorite: bool,
    cover_art: slint::Image,
    has_cover: bool,
) -> TrackItem {
    TrackItem {
        id: track.id as i32,
        title: track.title.clone().into(),
        artist: track.artist.clone().unwrap_or_default().into(),
        album: track.album.clone().unwrap_or_default().into(),
        duration_text: format_duration(track.duration_secs).into(),
        format: track.format.clone().into(),
        bitrate_kbps: track.bitrate_kbps.unwrap_or(0),
        year: track.year.unwrap_or(0),
        genre: track.genre.clone().unwrap_or_default().into(),
        track_number: track.track_number.unwrap_or(0),
        disc_number: track.disc_number.unwrap_or(0),
        is_favorite,
        is_playing,
        is_paused,
        play_count: track.play_count,
        cover_art,
        has_cover,
        file_path: track.path.clone().into(),
    }
}

pub fn playlist_to_item(pl: &tc_db::Playlist, track_count: u32) -> PlaylistItem {
    PlaylistItem {
        id: pl.id as i32,
        name: pl.name.clone().into(),
        track_count: track_count as i32,
    }
}

pub fn folder_to_item(path: &str, name: &str, track_count: u32, is_parent: bool) -> FolderItem {
    FolderItem {
        path: path.to_string().into(),
        name: name.to_string().into(),
        track_count: track_count as i32,
        is_parent,
    }
}

pub fn eq_band_to_item(index: i32, label: &str, gain_db: f32) -> EqBandItem {
    EqBandItem {
        index,
        label: label.to_string().into(),
        gain_db,
        filter_type: "peaking".into(),
    }
}

pub fn toast_to_item(id: u64, message: &str, level: &str) -> ToastItem {
    ToastItem {
        id: id as i32,
        message: message.to_string().into(),
        level: level.to_string().into(),
    }
}

pub fn lyrics_line_to_item(timestamp_ms: i64, text: &str, is_current: bool) -> LyricsLine {
    LyricsLine {
        timestamp_ms: timestamp_ms as i32,
        text: text.to_string().into(),
        is_current,
    }
}

/// Parse LRC-format synced lyrics (`[mm:ss.xx] text` per line, optionally
/// with multiple timestamp tags on one line) into `(timestamp_ms, text)`
/// pairs sorted by timestamp. Non-timestamp metadata tags (`[ar:...]`,
/// `[ti:...]`, etc.) are skipped since they don't parse as `mm:ss[.xx]`.
pub fn parse_lrc(lrc: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for raw_line in lrc.lines() {
        let line = raw_line.trim();
        if !line.starts_with('[') {
            continue;
        }
        let mut rest = line;
        let mut timestamps_ms = Vec::new();
        while let Some(start) = rest.find('[') {
            let Some(rel_end) = rest[start..].find(']') else { break };
            let end = start + rel_end;
            let tag = &rest[start + 1..end];
            if let Some(ms) = parse_lrc_timestamp(tag) {
                timestamps_ms.push(ms);
            }
            rest = &rest[end + 1..];
        }
        let text = rest.trim();
        if timestamps_ms.is_empty() || text.is_empty() {
            continue;
        }
        for ms in timestamps_ms {
            out.push((ms, text.to_string()));
        }
    }
    out.sort_by_key(|(ms, _)| *ms);
    out
}

/// Parse a single LRC timestamp tag body (e.g. `"01:23.45"` or `"01:23"`)
/// into milliseconds. Returns `None` for non-timestamp tags like `"ar:Foo"`.
fn parse_lrc_timestamp(tag: &str) -> Option<i64> {
    let (mins_str, secs_str) = tag.split_once(':')?;
    let mins: i64 = mins_str.trim().parse().ok()?;
    let secs: f64 = secs_str.trim().parse().ok()?;
    Some(mins * 60_000 + (secs * 1000.0).round() as i64)
}

/// Format seconds as M:SS for display.
pub fn format_duration(secs: f32) -> String {
    let total = secs as u32;
    let m = total / 60;
    let s = total % 60;
    format!("{}:{:02}", m, s)
}

/// Format a NaiveDateTime as "YYYY-MM-DD HH:MM".
pub fn format_datetime(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// Convert a `RepeatMode` enum to the lowercase string Slint expects.
pub fn repeat_mode_str(mode: tc_config::RepeatMode) -> &'static str {
    match mode {
        tc_config::RepeatMode::Off => "off",
        tc_config::RepeatMode::All => "all",
        tc_config::RepeatMode::One => "one",
    }
}

/// Build an empty `PlayerState` (used when no track is loaded).
pub fn empty_player_state() -> PlayerState {
    PlayerState {
        current_track_id: -1,
        title: "No track selected".into(),
        artist: "—".into(),
        album: "".into(),
        album_art: slint::Image::default(),
        has_album_art: false,
        duration_secs: 0.0,
        position_secs: 0.0,
        is_playing: false,
        is_favorited: false,
        volume: 0.0,
        shuffle: false,
        repeat: "off".into(),
        speed: 1.0,
        has_track: false,
    }
}

/// Build a `NavItem` for the sidebar.
pub fn nav_item(section: &str, label: &str, icon: slint::Image, badge: u32, active: bool) -> NavItem {
    NavItem {
        section: section.to_string().into(),
        label: label.to_string().into(),
        icon,
        badge: badge as i32,
        active,
    }
}
