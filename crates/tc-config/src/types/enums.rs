use serde::{Deserialize, Serialize};

/// Engine performance presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMode {
    #[serde(alias = "UltraQuality")]
    UltraQuality,
    #[serde(alias = "Balanced")]
    #[default]
    Balanced,
    #[serde(alias = "LowPower")]
    LowPower,
}

/// Filter types for parametric EQ bands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    #[serde(alias = "Peaking")]
    #[default]
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

/// Loudness normalization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessMode {
    #[serde(alias = "Off")]
    #[default]
    Off,
    #[serde(alias = "TrackReplayGain")]
    TrackReplayGain,
    #[serde(alias = "AlbumReplayGain")]
    AlbumReplayGain,
    #[serde(alias = "EbuR128")]
    EbuR128,
}

/// Crossfade curve type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfadeCurve {
    #[serde(alias = "Linear")]
    Linear,
    #[serde(alias = "EqualPower")]
    #[default]
    EqualPower,
    #[serde(alias = "SCurve")]
    SCurve,
}

/// Resampler quality setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResamplerQuality {
    #[serde(alias = "HighQuality")]
    HighQuality,
    #[serde(alias = "Balanced")]
    #[default]
    Balanced,
    #[serde(alias = "Fast")]
    Fast,
}

/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[serde(alias = "Off")]
    #[default]
    Off,
    #[serde(alias = "All")]
    All,
    #[serde(alias = "One")]
    One,
}

/// Theme selection for the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[serde(alias = "Light")]
    Light,
    #[serde(alias = "Dark")]
    Dark,
    #[serde(alias = "System")]
    #[default]
    System,
    #[serde(alias = "Ocean")]
    Ocean,
    #[serde(alias = "Forest")]
    Forest,
    #[serde(alias = "Sunset")]
    Sunset,
    #[serde(alias = "Berry")]
    Berry,
    #[serde(alias = "Midnight")]
    Midnight,
    #[serde(alias = "Rose")]
    Rose,
    #[serde(alias = "Coffee")]
    Coffee,
    #[serde(alias = "Mint")]
    Mint,
}

/// Audio backend selection for output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackend {
    #[serde(alias = "Auto")]
    #[default]
    Auto,
    #[serde(alias = "ExclusiveAlsa")]
    ExclusiveAlsa,
    #[serde(alias = "ExclusiveAsio")]
    ExclusiveAsio,
    #[serde(alias = "ExclusiveCoreAudioHog")]
    ExclusiveCoreAudioHog,
}

/// Profile for the Crossfeed DSP node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfeedProfile {
    #[serde(alias = "Bauer")]
    #[default]
    Bauer,
    #[serde(alias = "ChuMoy")]
    ChuMoy,
    #[serde(alias = "Jmeier")]
    Jmeier,
    #[serde(alias = "Custom")]
    Custom,
}
