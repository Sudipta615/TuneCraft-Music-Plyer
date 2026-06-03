use serde::{Deserialize, Serialize};

use super::enums::RepeatMode;

/// Playback configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackConfig {
    /// 0.0 to 1.0
    #[serde(default = "PlaybackConfig::default_volume")]
    pub volume: f64,
    /// 0.25 to 4.0
    #[serde(default = "PlaybackConfig::default_speed")]
    pub speed: f64,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default = "PlaybackConfig::default_resume_on_start")]
    pub resume_on_start: bool,
}

impl PlaybackConfig {
    fn default_volume() -> f64 { 1.0 }
    fn default_speed() -> f64 { 1.0 }
    fn default_resume_on_start() -> bool { true }

    /// Validate playback config, clamping out-of-range values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.volume.is_nan() || self.volume.is_infinite() {
            warnings.push(format!("Playback volume ({:.2}) is NaN/inf, resetting to 1.0", self.volume));
            self.volume = 1.0;
        } else if self.volume < 0.0 || self.volume > 1.0 {
            warnings.push(format!(
                "Playback volume ({:.2}) out of range [0.0, 1.0], clamped",
                self.volume
            ));
            self.volume = self.volume.clamp(0.0, 1.0);
        }

        if self.speed.is_nan() || self.speed.is_infinite() || self.speed <= 0.0 {
            warnings.push(format!("Playback speed ({:.2}) is invalid, resetting to 1.0", self.speed));
            self.speed = 1.0;
        } else if self.speed < 0.25 || self.speed > 4.0 {
            warnings.push(format!(
                "Playback speed ({:.2}) out of range [0.25, 4.0], clamped",
                self.speed
            ));
            self.speed = self.speed.clamp(0.25, 4.0);
        }

        warnings
    }
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            speed: 1.0,
            shuffle: false,
            repeat: RepeatMode::Off,
            resume_on_start: true,
        }
    }
}

