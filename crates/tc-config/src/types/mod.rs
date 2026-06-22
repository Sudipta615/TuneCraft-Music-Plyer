//! Configuration type definitions.
//!
//! This module re-exports all config types from their respective submodules.
//! Consumers can use `use tc_config::types::*` or import individual types.

mod app;
mod engine;
pub mod enums;
mod library;
mod lyrics;
mod playback;
mod scrobble;
mod ui;

/// Current config schema version.
/// Increment when the config schema changes; migration functions
/// will transform older versions to the current one.
pub const CONFIG_VERSION: u32 = 1;

// Enums
// App types
pub use app::{AppConfig, ConfigChangedEvent, ConfigSection};
// Engine types
pub use engine::{
    BandCompressorConfig, ConvolutionConfig, CrossfadeConfig, CrossfeedConfig, EngineConfig,
    EqBand, EqConfig, LimiterConfig, LoudnessConfig, MultibandCompressorConfig,
    StereoEnhancerConfig,
};
pub use enums::{
    AudioBackend, CrossfadeCurve, CrossfeedProfile, FilterType, LoudnessMode, PerformanceMode,
    RepeatMode, ResamplerQuality, Theme,
};
// Library types
pub use library::LibraryConfig;
// Lyrics types
pub use lyrics::LyricsConfig;
// Playback types
pub use playback::PlaybackConfig;
// Scrobble types
pub use scrobble::ScrobbleConfig;
// UI types
pub use ui::UiConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validates_cleanly() {
        let mut config = AppConfig::default();
        let warnings = config.validate();
        assert!(
            warnings.is_empty(),
            "Default config should validate without warnings: {:?}",
            warnings
        );
    }

    #[test]
    fn test_eq_band_validate_clamps_frequency() {
        let mut band = EqBand {
            frequency: 10.0,
            ..Default::default()
        };
        let warnings = band.validate();
        assert!(!warnings.is_empty());
        assert_eq!(band.frequency, 20.0);

        let mut band = EqBand {
            frequency: 50000.0,
            ..Default::default()
        };
        band.validate();
        assert_eq!(band.frequency, 20000.0);
    }

    #[test]
    fn test_eq_band_validate_rejects_zero_q() {
        let mut band = EqBand {
            q: 0.0,
            ..Default::default()
        };
        let warnings = band.validate();
        assert!(!warnings.is_empty());
        assert_eq!(band.q, 1.414);
    }

    #[test]
    fn test_eq_band_validate_rejects_nan_q() {
        let mut band = EqBand {
            q: f32::NAN,
            ..Default::default()
        };
        band.validate();
        assert_eq!(band.q, 1.414);
    }

    #[test]
    fn test_playback_validate_clamps_volume() {
        let mut playback = PlaybackConfig {
            volume: 1.5,
            ..Default::default()
        };
        playback.validate();
        assert_eq!(playback.volume, 1.0);

        let mut playback = PlaybackConfig {
            volume: -0.5,
            ..Default::default()
        };
        playback.validate();
        assert_eq!(playback.volume, 0.0);
    }

    #[test]
    fn test_playback_validate_clamps_speed() {
        let mut playback = PlaybackConfig {
            speed: 0.1,
            ..Default::default()
        };
        playback.validate();
        assert_eq!(playback.speed, 0.25);

        let mut playback = PlaybackConfig {
            speed: 10.0,
            ..Default::default()
        };
        playback.validate();
        assert_eq!(playback.speed, 4.0);
    }

    #[test]
    fn test_playback_validate_rejects_zero_speed() {
        let mut playback = PlaybackConfig {
            speed: 0.0,
            ..Default::default()
        };
        playback.validate();
        assert_eq!(playback.speed, 1.0);
    }

    #[test]
    fn test_eq_config_with_band_count() {
        let eq = EqConfig::with_band_count(5);
        assert_eq!(eq.bands.len(), 5);
        assert!(eq.bands[0].frequency < eq.bands[4].frequency);

        let eq = EqConfig::with_band_count(0);
        assert!(eq.bands.is_empty());

        let eq = EqConfig::with_band_count(1);
        assert_eq!(eq.bands.len(), 1);
    }

    #[test]
    fn test_library_config_default_is_deterministic() {
        let config1 = LibraryConfig::default();
        let config2 = LibraryConfig::default();
        assert_eq!(config1, config2);
        assert!(config1.watch_dirs.is_empty());
    }

    #[test]
    fn test_config_version_defaults_to_current() {
        let config = AppConfig::default();
        assert_eq!(config.config_version, CONFIG_VERSION);
    }

    #[test]
    fn test_migrate_updates_version() {
        let mut config = AppConfig {
            config_version: 0,
            ..Default::default()
        };
        let log = config.migrate();
        assert_eq!(config.config_version, CONFIG_VERSION);
        assert!(!log.is_empty());
    }

    #[test]
    fn test_serde_rename_all_snake_case() {
        let json = serde_json::to_string(&PerformanceMode::UltraQuality).unwrap();
        assert!(json.contains("ultra_quality"));

        let json = serde_json::to_string(&ResamplerQuality::HighQuality).unwrap();
        assert!(json.contains("high_quality"));
    }

    #[test]
    fn test_serde_alias_backward_compat() {
        // Old CamelCase format should still deserialize
        let old_format = "\"ultra_quality\"";
        let mode: PerformanceMode = serde_json::from_str(old_format).unwrap();
        assert_eq!(mode, PerformanceMode::UltraQuality);

        // Old alias should work
        let old_alias = "\"UltraQuality\"";
        let mode: PerformanceMode = serde_json::from_str(old_alias).unwrap();
        assert_eq!(mode, PerformanceMode::UltraQuality);
    }

    #[test]
    fn test_serde_default_missing_fields() {
        // Empty TOML should deserialize to default config
        let empty = "";
        let config: AppConfig = toml::from_str(empty).unwrap();
        assert_eq!(config.config_version, 0); // serde(default) gives 0
        assert!(!config.engine.eq.enabled);
        assert!(config.engine.gapless_enabled);
    }

    #[test]
    fn test_convolution_config_ir_path_is_pathbuf() {
        let config = ConvolutionConfig::default();
        assert!(config.ir_path.is_none());
        // PathBuf should serialize/deserialize as string in TOML
        let config = ConvolutionConfig {
            ir_path: Some(std::path::PathBuf::from("/path/to/ir.wav")),
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: ConvolutionConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            deserialized.ir_path,
            Some(std::path::PathBuf::from("/path/to/ir.wav"))
        );
    }

    #[test]
    fn test_eq_band_slope_skip_serializing_none() {
        let band = EqBand::default();
        let toml_str = toml::to_string(&band).unwrap();
        assert!(!toml_str.contains("slope"));
    }
}
