use serde::{Deserialize, Serialize};

/// Engine performance presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMode {
    #[serde(alias = "UltraQuality")]
    UltraQuality,
    #[serde(alias = "Balanced")]
    Balanced,
    #[serde(alias = "LowPower")]
    LowPower,
}

impl Default for PerformanceMode {
    fn default() -> Self {
        Self::Balanced
    }
}

/// Filter types for parametric EQ bands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    #[serde(alias = "Peaking")]
    Peaking,
    #[serde(alias = "LowShelf")]
    LowShelf,
    #[serde(alias = "HighShelf")]
    HighShelf,
    #[serde(alias = "LowPass")]
    LowPass,
    #[serde(alias = "HighPass")]
    HighPass,
    #[serde(alias = "Notch")]
    Notch,
}

impl Default for FilterType {
    fn default() -> Self {
        Self::Peaking
    }
}

/// Loudness normalization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessMode {
    #[serde(alias = "Off")]
    Off,
    #[serde(alias = "TrackReplayGain")]
    TrackReplayGain,
    #[serde(alias = "AlbumReplayGain")]
    AlbumReplayGain,
    #[serde(alias = "EbuR128")]
    EbuR128,
}

impl Default for LoudnessMode {
    fn default() -> Self {
        Self::Off
    }
}

/// Crossfade curve type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfadeCurve {
    #[serde(alias = "Linear")]
    Linear,
    #[serde(alias = "EqualPower")]
    EqualPower,
    #[serde(alias = "SCurve")]
    SCurve,
}

impl Default for CrossfadeCurve {
    fn default() -> Self {
        Self::EqualPower
    }
}

/// Resampler quality setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResamplerQuality {
    #[serde(alias = "HighQuality")]
    HighQuality,
    #[serde(alias = "Balanced")]
    Balanced,
    #[serde(alias = "Fast")]
    Fast,
}

impl Default for ResamplerQuality {
    fn default() -> Self {
        Self::Balanced
    }
}

/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[serde(alias = "Off")]
    Off,
    #[serde(alias = "All")]
    All,
    #[serde(alias = "One")]
    One,
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::Off
    }
}

/// Theme selection for the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[serde(alias = "Light")]
    Light,
    #[serde(alias = "Dark")]
    Dark,
    #[serde(alias = "System")]
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Self::System
    }
}
