//! Lyrics retrieval and synced lyric parsing
//!
//! Supports searching for lyrics via the LRCLIB API (https://lrclib.net),
//! which provides both synced (LRC format) and unsynced lyrics for free.
//! Also includes a robust LRC parser for converting LRC text into structured
//! timestamped lyric lines.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LyricsError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("No lyrics found")]
    NotFound,
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("API error: {0}")]
    Api(String),
}

/// A synced lyric line with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedLyricLine {
    pub timestamp_ms: u64,
    pub text: String,
}

/// Lyrics result containing both synced and unsynced versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsResult {
    /// Timestamp-synced lyrics (LRC format parsed)
    pub synced: Option<Vec<SyncedLyricLine>>,
    /// Plain text unsynced lyrics
    pub unsynced: Option<String>,
    /// Source of the lyrics
    pub source: String,
}

/// LRCLIB API search response item
#[derive(Debug, Clone, Deserialize)]
struct LrclibSearchResult {
    id: i64,
    track_name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

/// Lyrics client using the LRCLIB API
pub struct LyricsClient {
    client: reqwest::Client,
    base_url: String,
}

impl LyricsClient {
    /// Create a new lyrics client pointing to the default LRCLIB instance
    pub fn new() -> Self {
        let user_agent = format!(
            "TuneCraft/{} (https://github.com/tunecraft)",
            env!("CARGO_PKG_VERSION")
        );
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .unwrap_or_else(|e| {
                log::error!("Failed to build HTTP client for LyricsClient: {}", e);
                reqwest::Client::new()
            });
        Self {
            client,
            base_url: "https://lrclib.net/api".to_string(),
        }
    }

    /// Create a lyrics client with a custom LRCLIB API base URL
    pub fn with_base_url(base_url: String) -> Self {
        let user_agent = format!("TuneCraft/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .unwrap_or_else(|e| {
                log::error!("Failed to build HTTP client for LyricsClient: {}", e);
                reqwest::Client::new()
            });
        Self { client, base_url }
    }

    /// Search for lyrics by artist and title using the LRCLIB API.
    ///
    /// Returns the best-matching result with both synced and unsynced lyrics
    /// when available. The search prioritizes results with synced lyrics.
    pub async fn search(&self, artist: &str, title: &str) -> Result<LyricsResult, LyricsError> {
        let url = format!("{}/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("q", format!("{} {}", artist, title))])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(LyricsError::Api(format!(
                "LRCLIB returned status {}",
                response.status()
            )));
        }

        let results: Vec<LrclibSearchResult> = response.json().await?;

        if results.is_empty() {
            return Err(LyricsError::NotFound);
        }

        // Find the best match: prefer results with synced lyrics and matching artist/title
        let best = match self.find_best_match(&results, artist, title) {
            Some(b) => b,
            None => return Err(LyricsError::NotFound),
        };

        let synced = best
            .synced_lyrics
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| {
                // L14: Previously .ok() silently discarded LRC parse errors,
                // giving no indication of why synced lyrics were unavailable.
                match Self::parse_lrc(s) {
                    Ok(lines) => Some(lines),
                    Err(e) => {
                        log::warn!("Failed to parse LRC synced lyrics: {}", e);
                        None
                    },
                }
            })
            .flatten();

        // Use plain lyrics as unsynced fallback
        let unsynced = best
            .plain_lyrics
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned();

        if synced.is_none() && unsynced.is_none() {
            // Instrumental track or no lyrics available
            if best.instrumental.unwrap_or(false) {
                return Err(LyricsError::NotFound);
            }
            return Err(LyricsError::NotFound);
        }

        Ok(LyricsResult {
            synced,
            unsynced,
            source: "lrclib.net".to_string(),
        })
    }

    /// Get lyrics for a specific track by LRCLIB ID
    pub async fn get_by_id(&self, id: i64) -> Result<LyricsResult, LyricsError> {
        let url = format!("{}/get/{}", self.base_url, id);
        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(LyricsError::NotFound);
        }

        if !response.status().is_success() {
            return Err(LyricsError::Api(format!(
                "LRCLIB returned status {}",
                response.status()
            )));
        }

        let result: LrclibSearchResult = response.json().await?;

        let synced = result
            .synced_lyrics
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| Self::parse_lrc(s).ok())
            .flatten();

        let unsynced = result
            .plain_lyrics
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned();

        Ok(LyricsResult {
            synced,
            unsynced,
            source: "lrclib.net".to_string(),
        })
    }

    /// Find the best matching result from a list of search results.
    ///
    /// Prioritizes:
    /// 1. Results with synced lyrics
    /// 2. Results matching both artist and title
    /// 3. Results matching title only
    fn find_best_match<'a>(
        &self,
        results: &'a [LrclibSearchResult],
        artist: &str,
        title: &str,
    ) -> Option<&'a LrclibSearchResult> {
        // results instead of indexing results[0] as a fallback.
        if results.is_empty() {
            return None;
        }

        let artist_lower = artist.to_lowercase();
        let title_lower = title.to_lowercase();

        // Score each result: higher is better
        let score = |r: &LrclibSearchResult| {
            let mut s = 0i32;

            // Synced lyrics available: big bonus
            if r.synced_lyrics.as_ref().map_or(false, |l| !l.is_empty()) {
                s += 100;
            }

            // Plain lyrics available: moderate bonus
            if r.plain_lyrics.as_ref().map_or(false, |l| !l.is_empty()) {
                s += 50;
            }

            // Artist match
            if r.artist_name
                .as_ref()
                .map_or(false, |a| a.to_lowercase().contains(&artist_lower))
            {
                s += 30;
            }

            // Title match
            if r.track_name
                .as_ref()
                .map_or(false, |t| t.to_lowercase().contains(&title_lower))
            {
                s += 40;
            }

            // Exact title match bonus
            if r.track_name
                .as_ref()
                .map_or(false, |t| t.to_lowercase() == title_lower)
            {
                s += 20;
            }

            s
        };

        results.iter().max_by_key(|r| score(r)).or(results.first())
    }

    /// Parse LRC format synced lyrics into structured lines
    ///
    /// LRC format uses `[mm:ss.xx]` timestamps before each line:
    /// ```text
    /// [00:12.00]First line
    /// [00:17.20]Second line
    /// ```
    ///
    /// Also handles metadata tags like `[ti:Title]`, `[ar:Artist]`, etc.
    pub fn parse_lrc(lrc_text: &str) -> Result<Vec<SyncedLyricLine>, LyricsError> {
        let mut lines = Vec::new();

        for line in lrc_text.lines() {
            let line = line.trim();
            if line.is_empty() || !line.starts_with('[') {
                continue;
            }

            // A single physical LRC line can carry multiple timestamps before
            // the lyric text, e.g. "[00:10.00][00:45.00]Hello world".
            // We extract all leading timestamp tags and emit one entry per tag,
            // all sharing the same lyric text.  Previously only the first ']'
            // was consumed, so lines with N>1 timestamps silently lost N-1 of them.
            let mut timestamps: Vec<u64> = Vec::new();
            let mut rest = line;

            while rest.starts_with('[') {
                if let Some(close) = rest.find(']') {
                    let time_str = &rest[1..close];
                    let after = &rest[close + 1..];

                    let is_timestamp = time_str.contains(':')
                        && !time_str
                            .chars()
                            .next()
                            .map(|c| c.is_alphabetic())
                            .unwrap_or(false);

                    if is_timestamp {
                        let parts: Vec<&str> = time_str.split(':').collect();
                        if parts.len() >= 2 {
                            if let (Ok(mins), Ok(secs)) =
                                (parts[0].parse::<f64>(), parts[1].parse::<f64>())
                            {
                                let timestamp_ms = ((mins * 60.0 + secs) * 1000.0) as u64;
                                timestamps.push(timestamp_ms);
                            }
                        }
                        // Advance past this tag whether or not it parsed
                        rest = after;
                    } else {
                        // Metadata tag (e.g. [ti:Song title]) — skip the entire line
                        break;
                    }
                } else {
                    break;
                }
            }

            if !timestamps.is_empty() {
                let text = rest.trim().to_string();
                for timestamp_ms in timestamps {
                    lines.push(SyncedLyricLine {
                        timestamp_ms,
                        text: text.clone(),
                    });
                }
            }
        }

        lines.sort_by_key(|l| l.timestamp_ms);
        Ok(lines)
    }

    /// Find the lyric line that should be displayed at a given timestamp.
    ///
    /// Returns the index of the current line (the last line whose timestamp
    /// is <= the given position). Returns None if no lines match.
    pub fn find_current_line(lines: &[SyncedLyricLine], position_ms: u64) -> Option<usize> {
        if lines.is_empty() {
            return None;
        }

        // Binary search for the last line <= position_ms
        let mut lo = 0;
        let mut hi = lines.len();

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if lines[mid].timestamp_ms <= position_ms {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo == 0 {
            // Position is before the first line
            return None;
        }

        Some(lo - 1)
    }
}

impl Default for LyricsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lrc() {
        let lrc = "[00:12.00]First line\n[00:17.20]Second line\n[01:05.50]Third line";
        let result = LyricsClient::parse_lrc(lrc).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].timestamp_ms, 12000);
        assert_eq!(result[0].text, "First line");
        assert_eq!(result[1].timestamp_ms, 17200);
    }

    #[test]
    fn test_parse_lrc_with_metadata() {
        let lrc = "[ti:Song Title]\n[ar:Artist Name]\n[00:05.00]Hello world";
        let result = LyricsClient::parse_lrc(lrc).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Hello world");
    }

    #[test]
    fn test_parse_lrc_millisecond_precision() {
        let lrc = "[00:05.123]Precise timing\n[01:02.456]Another line";
        let result = LyricsClient::parse_lrc(lrc).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].timestamp_ms, 5123);
        assert_eq!(result[1].timestamp_ms, 62456);
    }

    #[test]
    fn test_find_current_line() {
        let lines = vec![
            SyncedLyricLine {
                timestamp_ms: 5000,
                text: "Line 1".into(),
            },
            SyncedLyricLine {
                timestamp_ms: 10000,
                text: "Line 2".into(),
            },
            SyncedLyricLine {
                timestamp_ms: 15000,
                text: "Line 3".into(),
            },
        ];

        assert_eq!(LyricsClient::find_current_line(&lines, 3000), None);
        assert_eq!(LyricsClient::find_current_line(&lines, 5000), Some(0));
        assert_eq!(LyricsClient::find_current_line(&lines, 7000), Some(0));
        assert_eq!(LyricsClient::find_current_line(&lines, 12000), Some(1));
        assert_eq!(LyricsClient::find_current_line(&lines, 20000), Some(2));
    }

    #[test]
    fn test_find_current_line_empty() {
        let lines: Vec<SyncedLyricLine> = vec![];
        assert_eq!(LyricsClient::find_current_line(&lines, 5000), None);
    }

    #[test]
    fn test_find_best_match() {
        let client = LyricsClient::new();
        let results = vec![
            LrclibSearchResult {
                id: 1,
                track_name: Some("Wrong Song".into()),
                artist_name: Some("Other Artist".into()),
                album_name: None,
                duration: None,
                instrumental: None,
                plain_lyrics: Some("Lyrics here".into()),
                synced_lyrics: None,
            },
            LrclibSearchResult {
                id: 2,
                track_name: Some("Test Song".into()),
                artist_name: Some("Test Artist".into()),
                album_name: None,
                duration: None,
                instrumental: None,
                plain_lyrics: Some("Lyrics here".into()),
                synced_lyrics: Some("[00:05.00]Synced line".into()),
            },
        ];

        let best = client
            .find_best_match(&results, "Test Artist", "Test Song")
            .unwrap();
        assert_eq!(best.id, 2);
    }
}
