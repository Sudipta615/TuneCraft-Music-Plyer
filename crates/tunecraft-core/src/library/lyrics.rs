//! Lyrics support module for Tunecraft.
//! Integrates with the LRCLIB API (https://lrclib.net) to fetch synced and
//! unsynced lyrics for tracks. LRCLIB is a free, open-source lyrics API
//! that does not require authentication.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::error::LyricsError;

/// Reuse a single reqwest::Client instance across requests for connection pooling.
fn http_client() -> reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::Client::builder()
        .user_agent("Tunecraft/1.0.0")
        .build()
        .unwrap_or_default())
    .clone()
}

/// A lyrics entry from LRCLIB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsEntry {
    /// Track title.
    pub track_name: String,
    /// Artist name.
    pub artist_name: String,
    /// Album name (if available).
    pub album_name: Option<String>,
    /// Duration in seconds.
    pub duration: Option<f64>,
    /// Plain (unsynced) lyrics.
    pub plain_lyrics: Option<String>,
    /// Synced lyrics in LRC format.
    pub synced_lyrics: Option<String>,
    /// Instrumental flag.
    pub instrumental: bool,
}

/// Search for lyrics by track name and artist.
/// Returns a list of matching lyrics entries from LRCLIB.
pub async fn search_lyrics(track: &str, artist: &str) -> Result<Vec<LyricsEntry>> {
    let client = http_client();
    let url = format!(
        "https://lrclib.net/api/search?q={}",
        urlencoding::encode(&format!("{} {}", artist, track))
    );

    let response = client
        .get(&url)
        .send()
        .await
        .context("failed to fetch lyrics from LRCLIB")?;

    if !response.status().is_success() {
        return Err(LyricsError::HttpError {
            status: response.status().as_u16(),
            body: format!("LRCLIB search returned HTTP {}", response.status()),
        }.into());
    }

    let entries: Vec<LyricsEntry> = response
        .json()
        .await
        .context("failed to parse LRCLIB response")?;

    Ok(entries)
}

/// Fetch lyrics for a specific track by artist, title, album, and duration.
/// This is more precise than search and returns the best match.
pub async fn get_lyrics(
    track_name: &str,
    artist_name: &str,
    album_name: Option<&str>,
    duration: Option<f64>,
) -> Result<Option<LyricsEntry>> {
    let client = http_client();
    let mut url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}",
        urlencoding::encode(artist_name),
        urlencoding::encode(track_name),
    );

    if let Some(album) = album_name {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }

    if let Some(dur) = duration {
        // Fix Bug #72: Duration parameter passed as unrounded float.
        // The LRCLIB API expects an integer number of seconds. Passing a float
        // like "215.376" causes mismatches because the server matches by integer
        // duration. Now rounded to the nearest integer before appending.
        url.push_str(&format!("&duration={}", dur.round() as u64));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .context("failed to fetch lyrics from LRCLIB")?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(LyricsError::HttpError {
            status: response.status().as_u16(),
            body: format!("LRCLIB get returned HTTP {}", response.status()),
        }.into());
    }

    let entry: LyricsEntry = response
        .json()
        .await
        .context("failed to parse LRCLIB response")?;

    Ok(Some(entry))
}

/// Parse synced LRC format lyrics into timestamped lines.
/// Returns a vector of (time_in_seconds, text) pairs.
///
/// Handles:
/// - Standard timestamped lines: `[00:12.34]Some lyrics`
/// - Multiple timestamps per line: `[00:12.34][01:23.45]Shared text`
/// - Metadata tags like `[ti:...]`, `[ar:...]` (skipped gracefully)
/// - Continuation lines (non-bracketed text after a timestamped line,
///   as produced by some non-standard LRC generators)
pub fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    let mut lines = Vec::new();
    let mut pending_timestamps: Vec<f64> = Vec::new();

    for line in lrc.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            // If there were pending timestamps with no text on their line,
            // associate them with an empty string before discarding.
            for ts in pending_timestamps.drain(..) {
                lines.push((ts, String::new()));
            }
            continue;
        }

        if line.starts_with('[') {
            // Flush any pending timestamps before processing a new bracketed line
            for ts in pending_timestamps.drain(..) {
                lines.push((ts, String::new()));
            }

            // Parse timestamp tags like [00:12.34] or [01:23.45]
            // A line can have multiple timestamps
            let mut timestamps = Vec::new();
            let mut text_start = 0;

            let mut chars = line.char_indices().peekable();
            while let Some(&(i, c)) = chars.peek() {
                if c == '[' {
                    chars.next(); // skip '['
                    let tag_start = i + 1;
                    let mut tag_end = tag_start;
                    for (j, c) in chars.by_ref() {
                        if c == ']' {
                            tag_end = j;
                            break;
                        }
                    }

                    let tag = &line[tag_start..tag_end];
                    // Try to parse as timestamp (mm:ss.xx)
                    if let Some(secs) = parse_lrc_timestamp(tag) {
                        timestamps.push(secs);
                    }
                    // Non-timestamp tags (e.g. [ti:...], [ar:...]) are silently skipped
                    // Continue to find more timestamps or the text
                    text_start = tag_end + 1;
                } else {
                    break;
                }
            }

            let text = if text_start < line.len() {
                line[text_start..].trim().to_string()
            } else {
                String::new()
            };

            if timestamps.is_empty() {
                // This was a metadata-only line with no valid timestamps; skip it
                continue;
            }

            // If the text is empty after all timestamps, save timestamps as pending
            // in case a continuation line follows
            if text.is_empty() {
                pending_timestamps = timestamps;
            } else {
                for ts in timestamps {
                    lines.push((ts, text.clone()));
                }
            }
        } else {
            // Non-bracketed line: this is a continuation/lyrics text line.
            // Associate it with any pending timestamps from the previous line.
            if !pending_timestamps.is_empty() {
                let text = line.to_string();
                for ts in pending_timestamps.drain(..) {
                    lines.push((ts, text.clone()));
                }
            }
            // If no pending timestamps, this is orphaned text with no timing;
            // we can't meaningfully display it, so we skip it.
        }
    }

    // Flush any remaining pending timestamps
    for ts in pending_timestamps.drain(..) {
        lines.push((ts, String::new()));
    }

    // Sort by timestamp
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

/// Parse an LRC timestamp like "01:23.45" into seconds.
///
/// Fix M7: Added validation to reject negative minutes/seconds and
/// seconds >= 60. Also clamps the total timestamp to 24 hours (86400s)
/// to prevent unreasonably large values from malformed LRC files.
fn parse_lrc_timestamp(tag: &str) -> Option<f64> {
    // Format: mm:ss.xx or mm:ss.xxx
    let parts: Vec<&str> = tag.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: f64 = parts[0].parse().ok()?;
    let seconds: f64 = parts[1].parse().ok()?;

    // Fix M7: Reject negative values
    if minutes < 0.0 || seconds < 0.0 {
        return None;
    }

    // Fix M7: Reject seconds >= 60 (invalid timestamp)
    if seconds >= 60.0 {
        return None;
    }

    let total = minutes * 60.0 + seconds;

    // Fix M7: Clamp to 24 hours maximum
    if total > 86400.0 {
        return None;
    }

    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lrc_simple() {
        let lrc = "[00:12.34]First line\n[00:15.67]Second line\n[01:23.45]Third line";
        let parsed = parse_lrc(lrc);
        assert_eq!(parsed.len(), 3);
        assert!((parsed[0].0 - 12.34).abs() < 0.01);
        assert_eq!(parsed[0].1, "First line");
        assert!((parsed[2].0 - 83.45).abs() < 0.01);
    }

    #[test]
    fn test_parse_lrc_skips_metadata() {
        let lrc = "[ti:Song Title]\n[ar:Artist]\n[00:05.00]Hello";
        let parsed = parse_lrc(lrc);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].1, "Hello");
    }

    #[test]
    fn test_parse_lrc_empty() {
        let parsed = parse_lrc("");
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_parse_timestamp() {
        assert!((parse_lrc_timestamp("01:23.45").unwrap() - 83.45).abs() < 0.01);
        assert!((parse_lrc_timestamp("00:05.00").unwrap() - 5.0).abs() < 0.01);
        assert!((parse_lrc_timestamp("10:00.00").unwrap() - 600.0).abs() < 0.01);
        assert!(parse_lrc_timestamp("invalid").is_none());
    }
}
