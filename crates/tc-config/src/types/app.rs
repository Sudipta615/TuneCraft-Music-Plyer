use serde::{Deserialize, Serialize};

use super::engine::EngineConfig;
use super::library::LibraryConfig;
use super::playback::PlaybackConfig;
use super::scrobble::ScrobbleConfig;
use super::ui::UiConfig;
use super::CONFIG_VERSION;

/// Root application configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Config schema version for migration support.
    /// Defaults to 1 (current version). When loading an older config,
    /// migration functions will transform it to the current version.
    #[serde(default)]
    pub config_version: u32,
    #[serde(default)]
    pub engine: EngineConfig,
    /// Library config. Note: Default::default() returns empty watch_dirs.
    /// Use LibraryConfig::with_system_defaults() for system paths.
    #[serde(default)]
    pub library: LibraryConfig,
    #[serde(default)]
    pub playback: PlaybackConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub scrobble: ScrobbleConfig,
}

impl AppConfig {
    /// Validate the entire config, clamping out-of-range values.
    /// Returns a list of warnings for any values that were adjusted.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Validate engine
        warnings.extend(self.engine.validate());

        // Validate library
        warnings.extend(self.library.validate());

        // Validate playback
        warnings.extend(self.playback.validate());

        // Validate UI
        warnings.extend(self.ui.validate());

        // Validate scrobble
        warnings.extend(self.scrobble.validate());

        if self.config_version < CONFIG_VERSION {
            warnings.push(format!(
                "Config version {} upgraded to {}",
                self.config_version, CONFIG_VERSION
            ));
            self.config_version = CONFIG_VERSION;
        }

        warnings
    }

    /// Run config migrations from the loaded version to the current version.
    /// Currently only version 1 exists, so this is a no-op for version 1 configs.
    /// Add migration functions here when the schema changes.
    pub fn migrate(&mut self) -> Vec<String> {
        let mut migration_log = Vec::new();

        // Future migrations go here:
        // if self.config_version < 2 { self.migrate_v1_to_v2(); migration_log.push("..."); }
        // if self.config_version < 3 { self.migrate_v2_to_v3(); migration_log.push("..."); }

        // Ensure version is up to date
        if self.config_version < CONFIG_VERSION {
            migration_log.push(format!(
                "Migrated config from version {} to {}",
                self.config_version, CONFIG_VERSION
            ));
            self.config_version = CONFIG_VERSION;
        }

        migration_log
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            engine: EngineConfig::default(),
            library: LibraryConfig::default(),
            playback: PlaybackConfig::default(),
            ui: UiConfig::default(),
            scrobble: ScrobbleConfig::default(),
        }
    }
}


/// A notification that the configuration has changed.
/// Consumers can subscribe to receive these notifications.
#[derive(Debug, Clone)]
pub struct ConfigChangedEvent {
    /// Which section of the config changed
    pub section: ConfigSection,
}

/// Sections of the configuration that can change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSection {
    Engine,
    Library,
    Playback,
    Ui,
    Scrobble,
    All,
}

