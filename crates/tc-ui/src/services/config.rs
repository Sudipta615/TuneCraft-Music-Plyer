//! Config service — configuration persistence and dirty tracking
//!
//! Encapsulates config loading, saving, and change notification,
//! removing these concerns from the UI layer.
//!
//! thread safety. Now consistent with PlaybackService and
//! EqService which use RwLock. The overhead of RwLock at 60fps is
//! negligible (one lock/unlock per frame).

use std::sync::Arc;

use log::warn;
use tc_config::AppConfig;

/// Recover from a poisoned RwLock by extracting the inner value.
pub fn recover_from_poison<'a, T>(
    result: Result<
        std::sync::RwLockReadGuard<'a, T>,
        std::sync::PoisonError<std::sync::RwLockReadGuard<'a, T>>,
    >,
) -> std::sync::RwLockReadGuard<'a, T> {
    match result {
        Ok(guard) => guard,
        Err(e) => {
            warn!("RwLock poisoned — recovering (data may be inconsistent)");
            e.into_inner()
        },
    }
}

/// Recover from a poisoned write lock.
pub fn recover_from_poison_write<'a, T>(
    result: Result<
        std::sync::RwLockWriteGuard<'a, T>,
        std::sync::PoisonError<std::sync::RwLockWriteGuard<'a, T>>,
    >,
) -> std::sync::RwLockWriteGuard<'a, T> {
    match result {
        Ok(guard) => guard,
        Err(e) => {
            warn!("RwLock poisoned — recovering (data may be inconsistent)");
            e.into_inner()
        },
    }
}

/// The config service manages application configuration.
///
/// Uses `RwLock` internally. This makes the service both `Send` and `Sync`, consistent
/// with PlaybackService and EqService, and prevents runtime panics from
/// cross-thread access.
pub struct ConfigService {
    config: Arc<std::sync::RwLock<AppConfig>>,
    /// Whether config has been modified since last save
    dirty: std::sync::RwLock<bool>,
    /// Time of last config save
    last_save: std::sync::RwLock<std::time::Instant>,
    /// Minimum interval between saves (seconds)
    save_interval_secs: u64,
}

impl ConfigService {
    /// Create a new ConfigService.
    pub fn new(config: Arc<std::sync::RwLock<AppConfig>>) -> Self {
        Self {
            config,
            dirty: std::sync::RwLock::new(false),
            last_save: std::sync::RwLock::new(std::time::Instant::now()),
            save_interval_secs: 30,
        }
    }

    /// Read a value from the config (non-blocking read lock).
    pub fn read<F, T>(&self, f: F) -> Option<T>
    where
        F: FnOnce(&AppConfig) -> T,
    {
        let guard = recover_from_poison(self.config.read());
        Some(f(&guard))
    }

    /// Read the entire config as a clone (handles poisoning gracefully).
    pub fn read_config(&self) -> AppConfig {
        let guard = recover_from_poison(self.config.read());
        guard.clone()
    }

    /// Write a value to the config and mark it dirty.
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut guard = recover_from_poison_write(self.config.write());
        f(&mut guard);
        *recover_from_poison_write(self.dirty.write()) = true;
    }

    /// Mark the config as dirty (to be saved periodically).
    pub fn mark_dirty(&self) {
        *recover_from_poison_write(self.dirty.write()) = true;
    }

    /// Check if the config is dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.read().map(|d| *d).unwrap_or(false)
    }

    /// Save config to disk if it has been modified since last save.
    pub fn save_if_dirty(&self) -> bool {
        let is_dirty = self.dirty.read().map(|d| *d).unwrap_or(false);
        if !is_dirty {
            return false;
        }

        let should_save = self
            .last_save
            .read()
            .map(|last| last.elapsed() >= std::time::Duration::from_secs(self.save_interval_secs))
            .unwrap_or(true);
        if !should_save {
            return false;
        }

        let guard = recover_from_poison(self.config.read());
        match tc_config::ConfigPersistence::save(&guard) {
            Ok(()) => {
                *recover_from_poison_write(self.dirty.write()) = false;
                *recover_from_poison_write(self.last_save.write()) = std::time::Instant::now();
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
        let guard = recover_from_poison(self.config.read());
        tc_config::ConfigPersistence::save(&guard)
            .map_err(|e| format!("Failed to save config: {}", e))
    }

    /// Get the current theme setting.
    pub fn theme(&self) -> tc_config::Theme {
        self.read(|c| c.ui.theme).unwrap_or(tc_config::Theme::Dark)
    }
}
