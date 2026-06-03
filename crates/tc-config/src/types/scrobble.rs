use serde::{Deserialize, Serialize};

/// Local play-journal (scrobble) configuration.

/// Controls whether TuneCraft records listen history to the local SQLite
/// database and the threshold at which a play is considered "complete".
/// No API keys, no network credentials — everything is local.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrobbleConfig {
    /// Master switch for local listen recording.
    /// When false, play_count and the scrobbles table are never updated.
    #[serde(default = "ScrobbleConfig::default_enabled")]
    pub enabled: bool,

    /// Fraction of track duration that must play before a listen is recorded.
    /// Range: (0.0, 1.0]. Default: 0.5 (50%).
    #[serde(default = "ScrobbleConfig::default_scrobble_threshold_pct")]
    pub scrobble_threshold_pct: f64,

    /// Hard maximum listen time (seconds) after which a play is always recorded,
    /// regardless of percentage. Default: 240 s (4 minutes), a common scrobble
    /// threshold convention. Set to u64::MAX to disable the hard cap.
    #[serde(default = "ScrobbleConfig::default_scrobble_threshold_sec")]
    pub scrobble_threshold_sec: u64,
}

impl ScrobbleConfig {
    fn default_enabled() -> bool {
        true
    }
    fn default_scrobble_threshold_pct() -> f64 {
        0.5
    }
    fn default_scrobble_threshold_sec() -> u64 {
        240
    }

    /// Validate and clamp out-of-range values.
    /// Returns a list of human-readable warnings for any adjusted field.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.scrobble_threshold_pct.is_nan()
            || self.scrobble_threshold_pct.is_infinite()
            || self.scrobble_threshold_pct <= 0.0
            || self.scrobble_threshold_pct > 1.0
        {
            warnings.push(format!(
                "scrobble_threshold_pct ({:.2}) out of range (0.0, 1.0], resetting to 0.5",
                self.scrobble_threshold_pct
            ));
            self.scrobble_threshold_pct = 0.5;
        }

        if self.scrobble_threshold_sec == 0 {
            warnings.push("scrobble_threshold_sec is 0, resetting to 240".to_string());
            self.scrobble_threshold_sec = 240;
        }

        warnings
    }
}

impl Default for ScrobbleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scrobble_threshold_pct: 0.5,
            scrobble_threshold_sec: 240,
        }
    }
}
