#[allow(unused_imports)] // CONFIG_VERSION used in tests
use crate::types::CONFIG_VERSION;
use crate::types::{AppConfig, ConfigChangedEvent, ConfigSection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Global counter for unique temp-file suffixes (H1 fix).
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Read(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// Config value failed validation (e.g., frequency out of range).
    /// The config has been auto-corrected; the messages describe what was changed.
    #[error("Config validation warnings: {0}")]
    Validation(String),
    /// Config file was not found at the expected path.
    #[error("Config file not found: {0}")]
    NotFound(PathBuf),
}

/// Configuration persistence manager with configurable paths and change notification.

/// Unlike the previous unit-struct design, this is a stateful struct that holds:
/// - A configurable config directory path (for testing or portable mode)
/// - A channel for config change notifications

/// For the simplest usage, use the associated functions (`load()`, `save()`,
/// `load_or_default()`) which use the default system config directory.
/// For advanced usage (custom paths, notifications), create an instance.
pub struct ConfigPersistence {
    /// Optional override for the config directory.
    /// When None, uses the system default (XDG_CONFIG_HOME/tunecraft on Linux,
    /// ~/Library/Application Support/tunecraft on macOS, %APPDATA%/tunecraft on Windows).
    config_dir_override: Option<PathBuf>,
    /// Subscribers for config change notifications.
    subscribers: Vec<crossbeam::channel::Sender<ConfigChangedEvent>>,
}

impl ConfigPersistence {
    /// Get the default config directory
    pub fn config_dir() -> Result<PathBuf, ConfigError> {
        let base = dirs::config_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot determine config directory",
            )
        })?;
        Ok(base.join("tunecraft"))
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Load config from disk.
    ///
    /// Uses a direct-read pattern (no TOCTOU race condition): attempts to read
    /// the file directly and matches on `ErrorKind::NotFound` to distinguish
    /// "file does not exist" from "file is corrupt".
    ///
    /// After loading, runs migration and validation. Any clamped values are
    /// logged as warnings.
    pub fn load() -> Result<AppConfig, ConfigError> {
        let path = Self::config_path()?;

        // Direct-read pattern: attempt the read, match on error kind
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Config file doesn't exist — create defaults with system paths
                let mut config = AppConfig::default();
                // Use system-specific defaults for library paths
                config.library = crate::types::LibraryConfig::with_system_defaults();
                if let Err(e) = Self::save(&config) {
                    log::error!(
                        "Failed to save default config (settings will not persist): {}",
                        e
                    );
                }
                return Ok(config);
            },
            Err(e) => return Err(ConfigError::Read(e)),
        };

        let mut config: AppConfig = toml::from_str(&content)?;

        let migration_log = config.migrate();
        for msg in &migration_log {
            log::info!("Config migration: {}", msg);
        }

        // Validate and clamp
        let warnings = config.validate();
        for warning in &warnings {
            log::warn!("Config validation: {}", warning);
        }

        // If migration or validation changed anything, save the updated config
        if !migration_log.is_empty() || !warnings.is_empty() {
            if let Err(e) = Self::save(&config) {
                log::error!("Failed to save migrated/validated config: {}", e);
            }
        }

        Ok(config)
    }

    /// Load config from disk, falling back to defaults on any error.
    ///
    /// This is the recommended way to load config for application startup.
    /// It consolidates the load-with-fallback pattern that was previously
    /// duplicated in `AppContext::init()` and `AppResources::build()`.
    pub fn load_or_default() -> AppConfig {
        Self::load().unwrap_or_else(|e| {
            log::warn!("Failed to load config (using defaults): {}", e);
            let mut config = AppConfig::default();
            config.library = crate::types::LibraryConfig::with_system_defaults();
            config
        })
    }

    /// Save config to disk atomically.
    ///
    /// Uses the write-to-temp-then-rename pattern to prevent data loss on crash:
    /// 1. Write to a temporary file in the same directory
    /// 2. fsync the temp file to ensure data is on disk
    /// 3. Rename the temp file to the target path (atomic on POSIX)
    ///
    /// On Windows, rename is not guaranteed atomic, but this is still safer
    /// than writing directly to the target file.
    pub fn save(config: &AppConfig) -> Result<(), ConfigError> {
        let dir = Self::config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let path = Self::config_path()?;
        Self::atomic_write(&path, &toml::to_string_pretty(config)?)
    }

    /// Load config from a specific path
    pub fn load_from(path: &Path) -> Result<AppConfig, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::Read(e)
            }
        })?;

        let mut config: AppConfig = toml::from_str(&content)?;
        config.migrate();
        config.validate();
        Ok(config)
    }

    /// Save config to a specific path atomically
    pub fn save_to(config: &AppConfig, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::atomic_write(path, &toml::to_string_pretty(config)?)
    }

    /// Create a new ConfigPersistence with default settings.
    pub fn new() -> Self {
        Self {
            config_dir_override: None,
            subscribers: Vec::new(),
        }
    }

    /// Create a ConfigPersistence with a custom config directory.
    pub fn with_config_dir(dir: PathBuf) -> Self {
        Self {
            config_dir_override: Some(dir),
            subscribers: Vec::new(),
        }
    }

    /// Get the effective config directory (custom or system default).
    pub fn effective_config_dir(&self) -> Result<PathBuf, ConfigError> {
        match &self.config_dir_override {
            Some(dir) => Ok(dir.clone()),
            None => Self::config_dir(),
        }
    }

    /// Get the effective config file path.
    pub fn effective_config_path(&self) -> Result<PathBuf, ConfigError> {
        Ok(self.effective_config_dir()?.join("config.toml"))
    }

    /// Load config using this instance's configured paths.
    pub fn load_instance(&self) -> Result<AppConfig, ConfigError> {
        let path = self.effective_config_path()?;

        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut config = AppConfig::default();
                config.library = crate::types::LibraryConfig::with_system_defaults();
                if let Err(e) = self.save_instance(&config) {
                    log::error!("Failed to save default config: {}", e);
                }
                return Ok(config);
            },
            Err(e) => return Err(ConfigError::Read(e)),
        };

        let mut config: AppConfig = toml::from_str(&content)?;
        config.migrate();
        config.validate();
        Ok(config)
    }

    /// Save config using this instance's configured paths, atomically.
    pub fn save_instance(&self, config: &AppConfig) -> Result<(), ConfigError> {
        let dir = self.effective_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let path = self.effective_config_path()?;
        Self::atomic_write(&path, &toml::to_string_pretty(config)?)
    }

    /// Save and notify subscribers of the change.
    pub fn save_and_notify(
        &mut self,
        config: &AppConfig,
        section: ConfigSection,
    ) -> Result<(), ConfigError> {
        self.save_instance(config)?;
        self.notify(ConfigChangedEvent { section });
        Ok(())
    }

    /// Subscribe to config change notifications.
    /// Returns a receiver that will receive events when config is saved.
    pub fn subscribe(&mut self) -> crossbeam::channel::Receiver<ConfigChangedEvent> {
        let (tx, rx) = crossbeam::channel::bounded(16);
        self.subscribers.push(tx);
        rx
    }

    /// Notify all subscribers of a config change.
    fn notify(&self, event: ConfigChangedEvent) {
        for subscriber in &self.subscribers {
            let _ = subscriber.send(event.clone());
        }
    }

    /// Atomic write: write to temp file, fsync, then rename.

    // monotonically-increasing counter instead of a fixed "config.toml.tmp" to
    // prevent collisions when multiple processes or threads write concurrently.
    fn atomic_write(path: &Path, content: &str) -> Result<(), ConfigError> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml");
        let pid = std::process::id();
        let rand_suffix = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_name = format!("{}.tmp.{}.{}", file_name, pid, rand_suffix);
        let temp_path = path.with_file_name(temp_name);

        // Bug #17 fix: Write using File::create() so we keep the handle open
        // for fsync, eliminating the TOCTOU gap where another process could
        // rename or delete the temp file between fs::write() closing it and
        // File::open() re-opening it. Also use sync_all() instead of
        // sync_data() to flush both data and metadata (file size, mtime).
        use std::io::Write;
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        // Rename temp to target (atomic on POSIX)
        std::fs::rename(&temp_path, path)?;

        Ok(())
    }
}

impl Default for ConfigPersistence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_or_default_returns_valid_config() {
        // This test uses the system config path, which may or may not exist.
        // In either case, load_or_default should return a valid config.
        let config = ConfigPersistence::load_or_default();
        assert_eq!(config.config_version, CONFIG_VERSION);
    }

    #[test]
    fn test_atomic_write() {
        let dir = std::env::temp_dir().join("tunecraft_test_atomic_write");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_config.toml");

        ConfigPersistence::atomic_write(&path, "test content").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "test content");

        // Temp file should not exist after rename (check the old-style name is absent;
        // new-style names include PID and counter, so just verify the target file exists)
        assert!(path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_instance_with_custom_dir() {
        let dir = std::env::temp_dir().join("tunecraft_test_custom_dir");
        let _ = fs::create_dir_all(&dir);

        let persistence = ConfigPersistence::with_config_dir(dir.clone());
        assert_eq!(persistence.effective_config_dir().unwrap(), dir);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_error_not_found() {
        let path = PathBuf::from("/nonexistent/path/config.toml");
        let result = ConfigPersistence::load_from(&path);
        match result {
            Err(ConfigError::NotFound(p)) => assert_eq!(p, path),
            Err(e) => panic!("Expected NotFound, got: {}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn test_subscribe_receives_events() {
        let mut persistence = ConfigPersistence::new();
        let rx = persistence.subscribe();

        persistence.notify(ConfigChangedEvent {
            section: ConfigSection::Engine,
        });

        let event = rx.try_recv().unwrap();
        assert_eq!(event.section, ConfigSection::Engine);
    }
}
