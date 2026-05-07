use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Current schema version for config migration detection.
/// Increment this whenever a breaking or structural change is made to the
/// config format so that `load()` can detect stale files and rewrite them.
const CURRENT_CONFIG_VERSION: u32 = 5;

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunecraftConfig {
    /// Schema version — used by `load()` to decide whether a migration/rewrite
    /// is needed. Serialized into TOML so the migration check can find it.
    #[serde(default = "default_config_version")]
    pub config_version: u32,
    pub general: GeneralConfig,
    pub library: LibraryConfig,
    pub audio: AudioConfig,
    #[serde(default)]
    pub scrobble: ScrobbleConfig,
    #[serde(default)]
    pub equalizer: EqConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default = "default_playback_speed")]
    pub playback_speed: f64,
    #[serde(default = "default_repeat_mode")]
    pub repeat_mode: String,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub volume_muted: bool,
    #[serde(default = "default_volume_before_mute")]
    pub volume_before_mute: f64,
}

fn default_theme() -> String {
    "dark".into()
}
fn default_volume() -> f64 {
    0.8
}
fn default_playback_speed() -> f64 {
    1.0
}
fn default_repeat_mode() -> String {
    "none".into()
}
fn default_volume_before_mute() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    #[serde(default)]
    pub watch_dirs: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub rescan_on_startup: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub output_device: Option<String>,
    #[serde(default)]
    pub crossfade_duration_ms: u32,
    #[serde(default)]
    pub replaygain: bool,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: u32,
    /// Decode ring buffer size in f32 samples (v3.0: configurable for low-end hardware).
    /// Default: 65536 (256K per channel at 48 kHz ≈ 0.68 seconds of buffered audio).
    /// Lower values reduce memory usage but increase risk of buffer underruns on slow systems.
    /// Valid range: 4096–262144.
    #[serde(default = "default_decode_ring_size")]
    pub decode_ring_size: u32,
    /// Output ring buffer size in f32 samples (v3.0: configurable for low-end hardware).
    /// Default: 32768 (128K per channel at 48 kHz ≈ 0.34 seconds of buffered audio).
    /// Lower values reduce latency but increase risk of buffer underruns.
    /// Valid range: 2048–131072.
    #[serde(default = "default_output_ring_size")]
    pub output_ring_size: u32,
    /// Visualization rendering mode (v3.0: Phase 4).
    /// - "always": Render visualizations at all times (GPU overhead)
    /// - "deferred": Only render visualizations when the view is visible
    /// - "disabled": Never render visualizations (saves GPU/CPU on low-end hardware)
    #[serde(default = "default_visualization_mode")]
    pub visualization_mode: String,
    /// Resampler cutoff frequency ratio (0.0–1.0, fraction of Nyquist).
    /// 0.95 matches Poweramp default. Lower = less aliasing, more HF roll-off.
    #[serde(default = "default_resampler_cutoff")]
    pub resampler_cutoff: f64,
    /// Fade-out/in duration in milliseconds applied around a seek (0 = no fade).
    #[serde(default = "default_seek_fade_ms")]
    pub seek_fade_ms: u32,
    /// User-adjustable preamp added on top of ReplayGain gain, in dB (-15.0 to +15.0).
    /// Equivalent to Poweramp's "RG preamp" knob.
    #[serde(default)]
    pub replaygain_preamp_db: f64,
    /// Preamp applied when a track has no ReplayGain tags, in dB (-15.0 to +15.0).
    /// Equivalent to Poweramp's "Preamp for songs without RG info".
    #[serde(default)]
    pub replaygain_fallback_db: f64,
    /// ReplayGain mode: "off", "track", "album", "track_prevent_clip", "album_prevent_clip".
    #[serde(default = "default_replaygain_mode_str")]
    pub replaygain_mode: String,
    /// Bit-perfect / exclusive mode — bypasses the system mixer entirely.
    /// Audio is output at the hardware's native bit depth and sample rate.
    /// Changes take effect on the next track load.
    #[serde(default)]
    pub exclusive_mode: bool,
    /// Enable EBU R128 loudness normalization for tracks without ReplayGain tags.
    #[serde(default)]
    pub loudness_normalization: bool,
    /// Target loudness level in LUFS for EBU R128 normalization.
    /// Default: -23.0 (EBU R128 broadcast standard). Streaming: -14.0.
    #[serde(default = "default_loudness_target_lufs")]
    pub loudness_target_lufs: f64,
}

fn default_backend() -> String {
    "gstreamer".into()
}
fn default_buffer_size() -> u32 {
    4096
}
fn default_decode_ring_size() -> u32 {
    65_536
}
fn default_output_ring_size() -> u32 {
    32_768
}
fn default_visualization_mode() -> String {
    "deferred".into()
}
fn default_resampler_cutoff() -> f64 {
    0.95
}
fn default_seek_fade_ms() -> u32 {
    20
}
fn default_replaygain_mode_str() -> String {
    "track".into()
}
fn default_loudness_target_lufs() -> f64 {
    -23.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_min_duration")]
    pub min_duration_secs: u32,
    #[serde(default = "default_min_percent")]
    pub min_percent: u32,
}

impl Default for ScrobbleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_duration_secs: default_min_duration(),
            min_percent: default_min_percent(),
        }
    }
}

fn default_min_duration() -> u32 {
    30
}
fn default_min_percent() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub preset: Option<String>,
    /// Bass shelf gain in dB (-12.0 to +12.0). Default: 0.0.
    #[serde(default)]
    pub bass_db: f64,
    /// Treble shelf gain in dB (-12.0 to +12.0). Default: 0.0.
    #[serde(default)]
    pub treble_db: f64,
    /// Stereo width: 0.0=mono, 1.0=original, >1.0=wider. Default: 1.0.
    #[serde(default = "default_stereo_width")]
    pub stereo_width: f64,
    /// Channel balance: -1.0=full left, 0.0=center, +1.0=full right. Default: 0.0.
    #[serde(default)]
    pub balance: f64,
    /// TPDF dither enabled (for 16-bit output). Default: false.
    #[serde(default)]
    pub dither_enabled: bool,
    /// Bass shelf corner frequency in Hz (20–500). Default: 90 Hz (matches Poweramp).
    #[serde(default = "default_bass_freq")]
    pub bass_freq_hz: f64,
    /// Treble shelf corner frequency in Hz (1000–20000). Default: 10000 Hz.
    #[serde(default = "default_treble_freq")]
    pub treble_freq_hz: f64,
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: None,
            bass_db: 0.0,
            treble_db: 0.0,
            stereo_width: default_stereo_width(),
            balance: 0.0,
            dither_enabled: false,
            bass_freq_hz: default_bass_freq(),
            treble_freq_hz: default_treble_freq(),
        }
    }
}

fn default_stereo_width() -> f64 {
    1.0
}
fn default_bass_freq() -> f64 {
    90.0
}
fn default_treble_freq() -> f64 {
    10_000.0
}

/// Helper: if `val` is NaN, log a warning and replace with `default_val`.
/// NaN comparisons always return false, so `clamp()` is a no-op for NaN —
/// the value passes through unclamped, which can cause panics or UB in
/// DSP code that expects finite values.
fn sanitize_f64(val: f64, field_name: &str, default_val: f64) -> f64 {
    if val.is_nan() {
        tracing::warn!(
            "Config field '{}' is NaN — resetting to default {}",
            field_name,
            default_val
        );
        default_val
    } else {
        val
    }
}

impl TunecraftConfig {
    /// Clamp all config values to safe ranges.
    ///
    /// Fix: NaN values bypass `clamp()` because all comparisons with NaN
    /// return false (e.g. `NaN.clamp(0.0, 1.0)` returns NaN). This can
    /// cause panics or undefined behavior in DSP code that expects finite
    /// values. Now we check for NaN first and replace with a sensible
    /// default before clamping.
    pub fn validate(&mut self) {
        self.general.volume =
            sanitize_f64(self.general.volume, "general.volume", 0.8).clamp(0.0, 1.0);
        self.general.playback_speed =
            sanitize_f64(self.general.playback_speed, "general.playback_speed", 1.0)
                .clamp(0.25, 4.0);
        self.general.volume_before_mute = sanitize_f64(
            self.general.volume_before_mute,
            "general.volume_before_mute",
            1.0,
        )
        .clamp(0.0, 1.0);
        if !["none", "all", "one"].contains(&self.general.repeat_mode.as_str()) {
            tracing::warn!(
                "Invalid repeat_mode '{}', resetting to 'none'",
                self.general.repeat_mode
            );
            self.general.repeat_mode = "none".to_string();
        }
        self.audio.buffer_size = self.audio.buffer_size.clamp(512, 16384);
        self.audio.decode_ring_size = self.audio.decode_ring_size.clamp(4096, 262_144);
        self.audio.output_ring_size = self.audio.output_ring_size.clamp(2048, 131_072);
        if !["always", "deferred", "disabled"].contains(&self.audio.visualization_mode.as_str()) {
            self.audio.visualization_mode = "deferred".to_string();
        }
        self.audio.crossfade_duration_ms = self.audio.crossfade_duration_ms.clamp(0, 10000);
        self.audio.resampler_cutoff =
            sanitize_f64(self.audio.resampler_cutoff, "audio.resampler_cutoff", 0.95)
                .clamp(0.5, 1.0);
        self.audio.seek_fade_ms = self.audio.seek_fade_ms.clamp(0, 200);
        self.audio.replaygain_preamp_db = sanitize_f64(
            self.audio.replaygain_preamp_db,
            "audio.replaygain_preamp_db",
            0.0,
        )
        .clamp(-15.0, 15.0);
        self.audio.replaygain_fallback_db = sanitize_f64(
            self.audio.replaygain_fallback_db,
            "audio.replaygain_fallback_db",
            0.0,
        )
        .clamp(-15.0, 15.0);
        if ![
            "off",
            "track",
            "album",
            "track_prevent_clip",
            "album_prevent_clip",
        ]
        .contains(&self.audio.replaygain_mode.as_str())
        {
            tracing::warn!(
                "Invalid replaygain_mode '{}', resetting to 'track'",
                self.audio.replaygain_mode
            );
            self.audio.replaygain_mode = "track".to_string();
        }
        self.audio.loudness_target_lufs = sanitize_f64(
            self.audio.loudness_target_lufs,
            "audio.loudness_target_lufs",
            -23.0,
        )
        .clamp(-30.0, 0.0);
        if self.audio.backend != "gstreamer" {
            tracing::warn!(
                "Invalid backend '{}', resetting to 'gstreamer'",
                self.audio.backend
            );
            self.audio.backend = "gstreamer".to_string();
        }
        self.equalizer.bass_db =
            sanitize_f64(self.equalizer.bass_db, "equalizer.bass_db", 0.0).clamp(-12.0, 12.0);
        self.equalizer.treble_db =
            sanitize_f64(self.equalizer.treble_db, "equalizer.treble_db", 0.0).clamp(-12.0, 12.0);
        self.equalizer.stereo_width =
            sanitize_f64(self.equalizer.stereo_width, "equalizer.stereo_width", 1.0)
                .clamp(0.0, 3.0);
        self.equalizer.balance =
            sanitize_f64(self.equalizer.balance, "equalizer.balance", 0.0).clamp(-1.0, 1.0);
        self.equalizer.bass_freq_hz =
            sanitize_f64(self.equalizer.bass_freq_hz, "equalizer.bass_freq_hz", 90.0)
                .clamp(20.0, 500.0);
        self.equalizer.treble_freq_hz = sanitize_f64(
            self.equalizer.treble_freq_hz,
            "equalizer.treble_freq_hz",
            10_000.0,
        )
        .clamp(1000.0, 20_000.0);
        self.scrobble.min_duration_secs = self.scrobble.min_duration_secs.clamp(10, 300);
        self.scrobble.min_percent = self.scrobble.min_percent.clamp(10, 100);
    }
}

impl Default for TunecraftConfig {
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            general: GeneralConfig {
                theme: "dark".into(),
                volume: 0.8,
                playback_speed: 1.0,
                repeat_mode: "none".into(),
                shuffle: false,
                volume_muted: false,
                volume_before_mute: 1.0,
            },
            library: LibraryConfig {
                watch_dirs: vec![],
                exclude_dirs: vec![],
                rescan_on_startup: true,
            },
            audio: AudioConfig {
                backend: "gstreamer".into(),
                output_device: None,
                crossfade_duration_ms: 0,
                replaygain: false,
                buffer_size: 4096,
                decode_ring_size: 65_536,
                output_ring_size: 32_768,
                visualization_mode: "deferred".into(),
                resampler_cutoff: 0.95,
                seek_fade_ms: 20,
                replaygain_preamp_db: 0.0,
                replaygain_fallback_db: 0.0,
                replaygain_mode: "track".into(),
                exclusive_mode: false,
                loudness_normalization: false,
                loudness_target_lufs: -23.0,
            },
            scrobble: ScrobbleConfig {
                enabled: false,
                min_duration_secs: 30,
                min_percent: 50,
            },
            equalizer: EqConfig {
                enabled: false,
                preset: None,
                bass_db: 0.0,
                treble_db: 0.0,
                stereo_width: 1.0,
                balance: 0.0,
                dither_enabled: false,
                bass_freq_hz: 90.0,
                treble_freq_hz: 10_000.0,
            },
        }
    }
}

/// Load configuration from disk. Creates default if missing.
///
/// v3.0 Migration: If a v2.1 config file is found that lacks the new v3.0
/// fields (`decode_ring_size`, `output_ring_size`, `visualization_mode`),
/// those fields are populated with defaults via serde's `#[serde(default)]`
/// attributes. The migrated config is then written back to disk so future
/// loads are clean and users can see the new options.
pub fn load() -> Result<TunecraftConfig> {
    let config_dir = config_dir()?;
    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("tunecraft.toml");
    if !config_path.exists() {
        let default_config = TunecraftConfig::default();
        let toml_str =
            toml::to_string_pretty(&default_config).context("failed to serialize config")?;
        std::fs::write(&config_path, &toml_str).context("failed to write default config")?;
        info!("Created default config at {:?}", config_path);
        return Ok(default_config);
    }

    let content = std::fs::read_to_string(&config_path).context("failed to read config file")?;
    let mut config: TunecraftConfig =
        toml::from_str(&content).context("failed to parse config file")?;

    let needs_migration = config.config_version < CURRENT_CONFIG_VERSION;

    config.validate();

    if needs_migration {
        info!(
            "Migrating config from v{} to v{} format",
            config.config_version, CURRENT_CONFIG_VERSION
        );
        config.config_version = CURRENT_CONFIG_VERSION;
        if let Err(e) = save(&config) {
            tracing::warn!("Failed to write migrated config: {}", e);
        }
    }

    info!("Loaded config from {:?}", config_path);
    Ok(config)
}

/// Save configuration to disk.
pub fn save(config: &TunecraftConfig) -> Result<()> {
    let config_dir = config_dir()?;
    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("tunecraft.toml");
    let toml_str = toml::to_string_pretty(config).context("failed to serialize config")?;
    std::fs::write(&config_path, toml_str).context("failed to write config")?;
    Ok(())
}

pub fn config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "tunecraft", "TuneCraft")
        .context("failed to determine project directories")?;
    Ok(dirs.config_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TunecraftConfig::default();
        assert_eq!(config.general.theme, "dark");
        assert_eq!(config.general.volume, 0.8);
        assert!(config.general.shuffle == false);
        assert_eq!(config.audio.backend, "gstreamer");
        assert_eq!(config.audio.crossfade_duration_ms, 0);
        assert!(!config.audio.replaygain);
        assert!(!config.scrobble.enabled);
        assert!(!config.equalizer.enabled);
        assert_eq!(config.equalizer.bass_db, 0.0);
        assert_eq!(config.equalizer.stereo_width, 1.0);
        assert!(!config.equalizer.dither_enabled);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = TunecraftConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        let parsed: TunecraftConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(parsed.general.theme, config.general.theme);
        assert_eq!(parsed.general.volume, config.general.volume);
        assert_eq!(
            parsed.audio.crossfade_duration_ms,
            config.audio.crossfade_duration_ms
        );
        assert_eq!(
            parsed.scrobble.min_duration_secs,
            config.scrobble.min_duration_secs
        );
    }

    #[test]
    fn test_config_custom_values() {
        let toml_str = r#"
[general]
theme = "light"
volume = 0.5
repeat_mode = "all"
shuffle = true

[library]
watch_dirs = ["/home/user/Music"]
exclude_dirs = []
rescan_on_startup = false

[audio]
backend = "gstreamer"
crossfade_duration_ms = 5000
replaygain = true
buffer_size = 8192

[scrobble]
enabled = true
min_duration_secs = 60
min_percent = 75

[equalizer]
enabled = true
preset = "rock"
bass_db = 3.0
treble_db = -1.5
stereo_width = 1.5
balance = 0.2
dither_enabled = true
"#;
        let config: TunecraftConfig = toml::from_str(toml_str).expect("parse custom config");
        assert_eq!(config.general.theme, "light");
        assert_eq!(config.general.volume, 0.5);
        assert!(config.general.shuffle);
        assert!(config.audio.replaygain);
        assert_eq!(config.audio.crossfade_duration_ms, 5000);
        assert!(config.scrobble.enabled);
        assert!(config.equalizer.enabled);
        assert_eq!(config.equalizer.preset, Some("rock".to_string()));
        assert!((config.equalizer.bass_db - 3.0).abs() < 1e-9);
        assert!((config.equalizer.treble_db - (-1.5)).abs() < 1e-9);
        assert!((config.equalizer.stereo_width - 1.5).abs() < 1e-9);
        assert!((config.equalizer.balance - 0.2).abs() < 1e-9);
        assert!(config.equalizer.dither_enabled);
        assert_eq!(config.library.watch_dirs, vec!["/home/user/Music"]);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = TunecraftConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize config");
        let parsed: TunecraftConfig = toml::from_str(&toml_str).expect("deserialize config");
        assert_eq!(config.general.theme, parsed.general.theme);
        assert_eq!(config.general.volume, parsed.general.volume);
        assert_eq!(
            config.audio.crossfade_duration_ms,
            parsed.audio.crossfade_duration_ms
        );
        assert_eq!(config.audio.replaygain, parsed.audio.replaygain);
        assert_eq!(config.equalizer.enabled, parsed.equalizer.enabled);
        assert_eq!(config.scrobble.enabled, parsed.scrobble.enabled);
        assert_eq!(
            config.scrobble.min_duration_secs,
            parsed.scrobble.min_duration_secs
        );
        assert_eq!(config.library.watch_dirs, parsed.library.watch_dirs);
    }
}
