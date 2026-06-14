//! Config service — configuration persistence and dirty tracking
//!
//! Encapsulates config loading, saving, and change notification,
//! removing these concerns from the UI layer.
//!
//! Uses `parking_lot::RwLock` — non-poisonable, infallible `.read()` / `.write()`.
//! The `recover_from_poison` helpers are no longer needed and have been removed.

use std::sync::Arc;

use log::warn;
use parking_lot::RwLock;
use tc_config::AppConfig;

/// The config service manages application configuration.
///
/// Uses `parking_lot::RwLock` internally, which is non-poisonable and
/// provides infallible guard acquisition — no `PoisonError` recovery needed.
pub struct ConfigService {
    config: Arc<RwLock<AppConfig>>,
    /// Whether config has been modified since last save
    dirty: RwLock<bool>,
    /// Time of last config save
    last_save: RwLock<std::time::Instant>,
    /// Minimum interval between saves (seconds)
    save_interval_secs: u64,
}

impl ConfigService {
    /// Create a new ConfigService.
    pub fn new(config: Arc<RwLock<AppConfig>>) -> Self {
        Self {
            config,
            dirty: RwLock::new(false),
            last_save: RwLock::new(std::time::Instant::now()),
            save_interval_secs: 30,
        }
    }

    /// Read a value from the config (non-blocking read lock).
    pub fn read<F, T>(&self, f: F) -> Option<T>
    where
        F: FnOnce(&AppConfig) -> T,
    {
        Some(f(&self.config.read()))
    }

    /// Read the entire config as a clone.
    pub fn read_config(&self) -> AppConfig {
        self.config.read().clone()
    }

    /// Write a value to the config and mark it dirty.
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut AppConfig),
    {
        f(&mut self.config.write());
        *self.dirty.write() = true;
    }

    /// Mark the config as dirty (to be saved periodically).
    pub fn mark_dirty(&self) {
        *self.dirty.write() = true;
    }

    /// Check if the config is dirty.
    pub fn is_dirty(&self) -> bool {
        *self.dirty.read()
    }

    /// Save config to disk if it has been modified since last save.
    pub fn save_if_dirty(&self) -> bool {
        if !*self.dirty.read() {
            return false;
        }

        let should_save = self.last_save.read().elapsed()
            >= std::time::Duration::from_secs(self.save_interval_secs);

        if !should_save {
            return false;
        }

        match tc_config::ConfigPersistence::save(&self.config.read()) {
            Ok(()) => {
                *self.dirty.write() = false;
                *self.last_save.write() = std::time::Instant::now();
                true
            },
            Err(e) => {
                warn!("Failed to save config: {}", e);
                false
            },
        }
    }

    /// Force-save the config regardless of dirty state.
    pub fn force_save(&self) -> Result<(), String> {
        tc_config::ConfigPersistence::save(&self.config.read())
            .map_err(|e| format!("Failed to save config: {}", e))
    }

    /// Get the current theme setting.
    pub fn theme(&self) -> tc_config::Theme {
        self.config.read().ui.theme
    }
}
