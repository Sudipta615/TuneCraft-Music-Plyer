use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Library configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryConfig {
    /// Directories to watch for music files.
    /// Empty means "not configured" — callers should use `with_system_defaults()`
    /// to populate system-specific paths.
    #[serde(default)]
    pub watch_dirs: Vec<PathBuf>,
    #[serde(default = "LibraryConfig::default_scan_on_startup")]
    pub scan_on_startup: bool,
    #[serde(default)]
    pub auto_rescan_interval_min: Option<u64>,
    #[serde(default = "LibraryConfig::default_cover_art_cache_size_mb")]
    pub cover_art_cache_size_mb: u64,
    #[serde(default = "LibraryConfig::default_waveform_cache_size_mb")]
    pub waveform_cache_size_mb: u64,
    /// Number of tracks shown per page in the track list.
    /// Smaller values improve rendering performance for very large libraries.
    /// Defaults to 500.
    #[serde(default = "LibraryConfig::default_tracks_per_page")]
    pub tracks_per_page: usize,
}

impl LibraryConfig {
    fn default_scan_on_startup() -> bool {
        true
    }
    fn default_cover_art_cache_size_mb() -> u64 {
        100
    }
    fn default_waveform_cache_size_mb() -> u64 {
        50
    }
    fn default_tracks_per_page() -> usize {
        500
    }

    /// Create a LibraryConfig with system-specific default paths.
    ///
    /// Unlike `Default::default()`, which returns an empty `watch_dirs`,
    /// this method queries the filesystem for the user's music directory
    /// and is therefore non-deterministic and performs I/O.
    pub fn with_system_defaults() -> Self {
        let music_dir = dirs::audio_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
            .unwrap_or_else(|| {
                log::warn!(
                    "Cannot determine music directory. Falling back to ~/Music/tunecraft. \
                     Set library.watch_dirs in config.toml to override."
                );

                // /tmp is volatile and may be cleared on reboot, losing the music path.
                dirs::home_dir()
                    .map(|h| h.join("Music").join("tunecraft"))
                    .unwrap_or_else(|| PathBuf::from("/tmp/tunecraft-music"))
            });

        Self {
            watch_dirs: vec![music_dir],
            scan_on_startup: true,
            auto_rescan_interval_min: None,
            cover_art_cache_size_mb: 100,
            waveform_cache_size_mb: 50,
            tracks_per_page: 500,
        }
    }

    /// Validate library config values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Validate watch_dirs exist (informational, not an error)
        for dir in &self.watch_dirs {
            if !dir.exists() {
                warnings.push(format!(
                    "Library watch_dir '{}' does not exist",
                    dir.display()
                ));
            }
        }

        if self.cover_art_cache_size_mb == 0 {
            warnings.push("cover_art_cache_size_mb is 0, resetting to 100".to_string());
            self.cover_art_cache_size_mb = 100;
        }

        warnings
    }
}

/// Default implementation returns a deterministic, pure config.
/// `watch_dirs` is empty, indicating "not configured".
/// Use `LibraryConfig::with_system_defaults()` for system-specific paths.
impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            watch_dirs: vec![],
            scan_on_startup: true,
            auto_rescan_interval_min: None,
            cover_art_cache_size_mb: 100,
            waveform_cache_size_mb: 50,
            tracks_per_page: 500,
        }
    }
}
