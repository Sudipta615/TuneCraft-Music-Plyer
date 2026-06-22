use serde::{Deserialize, Serialize};

/// LRCLIB synced-lyrics integration configuration.
///
/// Controls whether TuneCraft fetches synced lyrics from LRCLIB for tracks
/// that don't already have lyrics in the local database. All fetched lyrics
/// are cached in the `tracks.lyrics_synced` column so subsequent plays
/// never hit the network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricsConfig {
    /// Master switch for LRCLIB network access. When false, no network
    /// requests are made; the UI shows only lyrics already in the DB.
    #[serde(default = "LyricsConfig::default_enabled")]
    pub enabled: bool,

    /// Base URL for the LRCLIB instance. Defaults to the public LRCLIB
    /// at `https://lrclib.net`. Users can point this at a self-hosted
    /// instance for privacy or lower latency.
    #[serde(default = "LyricsConfig::default_base_url")]
    pub base_url: String,

    /// Fetch lyrics automatically when a track starts playing. When false,
    /// lyrics are only fetched when the user opens the lyrics panel.
    #[serde(default = "LyricsConfig::default_fetch_on_play")]
    pub fetch_on_play: bool,
}

impl LyricsConfig {
    fn default_enabled() -> bool {
        true
    }
    fn default_base_url() -> String {
        "https://lrclib.net".to_string()
    }
    fn default_fetch_on_play() -> bool {
        true
    }

    /// Validate and clamp out-of-range values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.base_url.is_empty() {
            warnings.push("lyrics.base_url is empty, resetting to https://lrclib.net".to_string());
            self.base_url = Self::default_base_url();
        }
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            warnings.push(format!(
                "lyrics.base_url ({}) missing http(s):// scheme, resetting to https://lrclib.net",
                self.base_url
            ));
            self.base_url = Self::default_base_url();
        }
        warnings
    }
}

impl Default for LyricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: Self::default_base_url(),
            fetch_on_play: true,
        }
    }
}
