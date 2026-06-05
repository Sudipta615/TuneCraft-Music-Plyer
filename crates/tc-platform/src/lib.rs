//! Platform integration — media keys, MPRIS D-Bus, system tray, notifications
//!
//! On Linux, media key handling and MPRIS integration use D-Bus.
//! On macOS and Windows, platform-specific APIs are used.
//! This module provides a cross-platform abstraction for these features.
//!
//! ## Features
//! - Media key action channel (play/pause, next, prev, stop, volume)
//! - MPRIS D-Bus playback status reporting (Linux)
//! - Desktop notifications (async dispatch)
//! - Global keyboard shortcut registration
//! - Background thread for platform event processing
//! - Cross-platform media controls via souvlaki (v0.20.0)
//!
//! v0.20.0: Added `CrossPlatformMediaControls` using the `souvlaki` crate
//! for true cross-platform media key handling. Previously, media key
//! integration was Linux-only (MPRIS D-Bus). Now it works on macOS
//! (MPRemoteCommandCenter) and Windows (SystemMediaTransportControls) too.
//! The existing `mpris` module is retained for advanced D-Bus property
//! reporting that souvlaki does not expose.

// Submodules
mod types;
mod media_keys;
mod shortcuts;
mod notifications;
mod media_control;

#[cfg(target_os = "linux")]
pub mod mpris;

// Re-exports: maintain backward-compatible public API
pub use types::{
    PlatformError,
    MediaKeyAction,
    MprisPlaybackStatus,
    MprisTrackInfo,
    MprisPropertyChanged,
    KeyboardShortcut,
};
pub use media_keys::MediaKeyReceiver;
pub use media_control::CrossPlatformMediaControls;
pub use shortcuts::default_shortcuts;
pub use notifications::{applescript_escape, xml_escape};

use std::sync::mpsc::{self, Sender};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Platform integration handle

/// Manages MPRIS D-Bus registration, system tray integration,
/// keyboard shortcuts, notifications, and cross-platform media controls.
/// Media key actions are received through a separate [`MediaKeyReceiver`]
/// returned by [`PlatformIntegration::new()`].
pub struct PlatformIntegration {
    /// Channel for sending media key actions
    action_tx: Sender<MediaKeyAction>,
    /// Whether system tray is available
    tray_available: bool,
    /// Whether MPRIS is registered
    mpris_registered: bool,
    /// Current MPRIS playback status (local mirror)
    mpris_status: MprisPlaybackStatus,
    /// Current track info for MPRIS (local mirror)
    mpris_track: MprisTrackInfo,
    /// Current volume (0.0 to 1.0) for MPRIS (local mirror)
    mpris_volume: f64,
    /// Registered keyboard shortcuts
    shortcuts: Vec<KeyboardShortcut>,
    /// Background thread running flag
    bg_running: Arc<AtomicBool>,
    /// Cross-platform media controls (souvlaki wrapper)
    media_controls: Option<CrossPlatformMediaControls>,
    /// Shared MPRIS state for D-Bus service thread (Linux only)
    #[cfg(target_os = "linux")]
    mpris_state: Option<std::sync::Arc<parking_lot::Mutex<mpris::MprisState>>>,
    /// Channel for notifying D-Bus thread of property changes (Linux only)
    #[cfg(target_os = "linux")]
    mpris_notify_tx: Option<Sender<MprisPropertyChanged>>,
}

impl PlatformIntegration {
    /// Create a new PlatformIntegration and its associated MediaKeyReceiver.
    ///
    /// Returns a tuple of `(PlatformIntegration, MediaKeyReceiver)` so that
    /// the receiver can be polled independently without borrowing the
    /// integration struct.
    ///
    /// v0.20.0: Now initializes `CrossPlatformMediaControls` (souvlaki)
    /// for cross-platform media key handling. Falls back gracefully if
    /// the platform doesn't support media controls.
    pub fn new() -> Result<(Self, MediaKeyReceiver), PlatformError> {
        let (action_tx, action_rx) = mpsc::channel();

        // Initialize cross-platform media controls.
        let media_controls = match CrossPlatformMediaControls::new(action_tx.clone()) {
            Ok(ctrl) => {
                log::info!("Cross-platform media controls initialized successfully");
                Some(ctrl)
            }
            Err(e) => {
                log::warn!(
                    "Cross-platform media controls initialization failed: {}. \
                     Falling back to keyboard shortcuts only.",
                    e
                );
                None
            }
        };

        let integration = Self {
            action_tx,
            tray_available: false,
            mpris_registered: false,
            mpris_status: MprisPlaybackStatus::Stopped,
            mpris_track: MprisTrackInfo::default(),
            mpris_volume: 1.0,
            shortcuts: default_shortcuts(),
            bg_running: Arc::new(AtomicBool::new(false)),
            media_controls,
            #[cfg(target_os = "linux")]
            mpris_state: None,
            #[cfg(target_os = "linux")]
            mpris_notify_tx: None,
        };

        let receiver = MediaKeyReceiver::new(action_rx);

        Ok((integration, receiver))
    }

    /// Get a sender for injecting media key actions (e.g., from UI shortcuts)
    pub fn action_sender(&self) -> Sender<MediaKeyAction> {
        self.action_tx.clone()
    }

    /// Update MPRIS playback status.
    ///
    /// v0.20.0: Now also updates the cross-platform media controls
    /// (souvlaki), so the status is reflected in the OS media UI on
    /// all platforms, not just Linux.
    pub fn set_mpris_status(&mut self, status: MprisPlaybackStatus) {
        self.mpris_status = status;

        // Update cross-platform media controls.
        if let Some(ref mut ctrl) = self.media_controls {
            ctrl.set_playback_status(status);
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.playback_status = status;
            }
            if let Some(ref tx) = self.mpris_notify_tx {
                let _ = tx.send(MprisPropertyChanged::PlaybackStatus);
            }
        }
    }

    /// Update MPRIS track metadata.
    ///
    /// v0.20.0: Now also updates the cross-platform media controls
    /// (souvlaki), so track metadata is shown in the OS media UI on
    /// all platforms.
    pub fn set_mpris_track(&mut self, info: MprisTrackInfo) {
        // Update cross-platform media controls.
        if let Some(ref mut ctrl) = self.media_controls {
            let duration = info.length_microseconds
                .map(|us| std::time::Duration::from_micros(us as u64));
            ctrl.set_metadata(
                info.title.as_deref(),
                info.artist.as_deref(),
                info.album.as_deref(),
                duration,
                info.art_url.as_deref(),
            );
        }

        self.mpris_track = info;

        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.track_info = self.mpris_track.clone();
            }
            if let Some(ref tx) = self.mpris_notify_tx {
                let _ = tx.send(MprisPropertyChanged::TrackMetadata);
            }
        }
    }

    /// Update MPRIS volume.
    pub fn set_mpris_volume(&mut self, volume: f64) {
        self.mpris_volume = volume.clamp(0.0, 1.0);

        // Update cross-platform media controls.
        if let Some(ref mut ctrl) = self.media_controls {
            ctrl.set_volume(self.mpris_volume);
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.volume = self.mpris_volume;
            }
            if let Some(ref tx) = self.mpris_notify_tx {
                let _ = tx.send(MprisPropertyChanged::Volume);
            }
        }
    }

    /// Update MPRIS shuffle state.
    pub fn set_mpris_shuffle(&mut self, shuffle: bool) {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.shuffle = shuffle;
            }
            if let Some(ref tx) = self.mpris_notify_tx {
                let _ = tx.send(MprisPropertyChanged::Shuffle);
            }
        }
    }

    /// Update MPRIS loop status.
    pub fn set_mpris_loop_status(&mut self, status: &str) {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.loop_status = status.to_string();
            }
            if let Some(ref tx) = self.mpris_notify_tx {
                let _ = tx.send(MprisPropertyChanged::LoopStatus);
            }
        }
    }

    /// Get the current MPRIS volume
    pub fn mpris_volume(&self) -> f64 {
        self.mpris_volume
    }

    /// Check if system tray is available
    pub fn is_tray_available(&self) -> bool {
        self.tray_available
    }

    /// Set tray availability
    pub fn set_tray_available(&mut self, available: bool) {
        self.tray_available = available;
    }

    /// Check if MPRIS is registered
    pub fn is_mpris_registered(&self) -> bool {
        self.mpris_registered
    }

    /// Check if cross-platform media controls are available.
    /// This returns true on all platforms where souvlaki successfully
    /// initialized, not just Linux.
    pub fn is_media_controls_available(&self) -> bool {
        self.media_controls.as_ref().map_or(false, |c| c.is_available())
    }

    /// Attempt to register MPRIS on D-Bus (Linux only)
    #[cfg(target_os = "linux")]
    pub fn register_mpris(&mut self, identity: &str) -> Result<(), PlatformError> {
        let (notify_tx, notify_rx) = mpsc::channel::<MprisPropertyChanged>();

        let service = mpris::MprisService::new(identity, self.action_tx.clone());
        let state = service.state();
        self.mpris_state = Some(Arc::clone(&state));
        self.mpris_notify_tx = Some(notify_tx);

        match service.start(state, notify_rx) {
            Ok(()) => {
                log::info!("MPRIS D-Bus service started for '{}'", identity);
                self.mpris_registered = true;
                Ok(())
            }
            Err(e) => {
                log::warn!("MPRIS D-Bus service failed to start: {}", e);
                self.mpris_registered = false;
                Err(PlatformError::Dbus(format!(
                    "MPRIS D-Bus service failed to start for '{}': {}",
                    identity, e
                )))
            }
        }
    }

    /// Non-Linux: MPRIS D-Bus registration is not available, but
    /// cross-platform media controls (souvlaki) may still work.
    ///
    /// v0.20.0: This no longer returns an error unconditionally. Instead,
    /// it checks if cross-platform media controls are available and returns
    /// Ok if they are, Err only if neither MPRIS nor souvlaki is working.
    #[cfg(not(target_os = "linux"))]
    pub fn register_mpris(&mut self, identity: &str) -> Result<(), PlatformError> {
        if self.media_controls.as_ref().map_or(false, |c| c.is_available()) {
            log::info!(
                "Cross-platform media controls active for '{}' (no MPRIS D-Bus needed on this platform)",
                identity
            );
            self.mpris_registered = true;
            Ok(())
        } else {
            log::warn!(
                "Neither MPRIS D-Bus nor cross-platform media controls are available"
            );
            Err(PlatformError::NotAvailable(
                "Neither MPRIS D-Bus nor cross-platform media controls are available on this platform".to_string()
            ))
        }
    }

    /// Get the MPRIS identity string
    pub fn mpris_identity() -> &'static str {
        "TuneCraft"
    }

    /// Get the MPRIS desktop entry
    pub fn mpris_desktop_entry() -> &'static str {
        "tunecraft"
    }

    /// Update the playback position for MPRIS clients.
    pub fn set_mpris_position(&mut self, position_microseconds: i64) {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref state) = self.mpris_state {
                let mut s = state.lock();
                s.position_microseconds = position_microseconds;
            }
        }

        // Update cross-platform media controls.
        if let Some(ref mut ctrl) = self.media_controls {
            ctrl.set_position(std::time::Duration::from_micros(
                position_microseconds.max(0) as u64
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_creation() {
        let (platform, _rx) = PlatformIntegration::new().unwrap();
        assert!(!platform.is_tray_available());
    }

    #[test]
    fn test_media_key_channel() {
        let (platform, rx) = PlatformIntegration::new().unwrap();
        let sender = platform.action_sender();

        sender.send(MediaKeyAction::PlayPause).unwrap();
        sender.send(MediaKeyAction::Next).unwrap();

        assert_eq!(rx.try_recv(), Some(MediaKeyAction::PlayPause));
        assert_eq!(rx.try_recv(), Some(MediaKeyAction::Next));
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn test_play_pause_quit_actions() {
        let (platform, rx) = PlatformIntegration::new().unwrap();
        let sender = platform.action_sender();

        sender.send(MediaKeyAction::Play).unwrap();
        sender.send(MediaKeyAction::Pause).unwrap();
        sender.send(MediaKeyAction::Quit).unwrap();

        assert_eq!(rx.try_recv(), Some(MediaKeyAction::Play));
        assert_eq!(rx.try_recv(), Some(MediaKeyAction::Pause));
        assert_eq!(rx.try_recv(), Some(MediaKeyAction::Quit));
    }

    #[test]
    fn test_mpris_status_update() {
        let (mut platform, _rx) = PlatformIntegration::new().unwrap();
        platform.set_mpris_status(MprisPlaybackStatus::Playing);
        assert_eq!(platform.mpris_status, MprisPlaybackStatus::Playing);
    }

    #[test]
    fn test_mpris_track_update() {
        let (mut platform, _rx) = PlatformIntegration::new().unwrap();
        let info = MprisTrackInfo {
            title: Some("Test Song".to_string()),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            art_url: None,
            length_microseconds: Some(180_000_000),
            track_id: Some("/org/tunecraft/track/1".to_string()),
        };
        platform.set_mpris_track(info);
        assert_eq!(platform.mpris_track.title, Some("Test Song".to_string()));
    }

    #[test]
    fn test_mpris_volume() {
        let (mut platform, _rx) = PlatformIntegration::new().unwrap();
        platform.set_mpris_volume(0.5);
        assert!((platform.mpris_volume() - 0.5).abs() < 0.001);

        platform.set_mpris_volume(-0.1);
        assert!(platform.mpris_volume() >= 0.0);
        platform.set_mpris_volume(1.5);
        assert!(platform.mpris_volume() <= 1.0);
    }

    #[test]
    fn test_keyboard_shortcuts() {
        let (platform, _rx) = PlatformIntegration::new().unwrap();

        let action = platform.process_key_event("Space", false, false, false, false);
        assert_eq!(action, Some(MediaKeyAction::PlayPause));

        let action = platform.process_key_event("Right", true, false, false, false);
        assert_eq!(action, Some(MediaKeyAction::Next));

        let action = platform.process_key_event("X", false, false, false, false);
        assert_eq!(action, None);
    }

    #[test]
    fn test_custom_shortcut() {
        let (mut platform, _rx) = PlatformIntegration::new().unwrap();
        platform.add_shortcut(
            KeyboardShortcut::new("P", MediaKeyAction::PlayPause).ctrl().alt()
        );

        let action = platform.process_key_event("P", true, true, false, false);
        assert_eq!(action, Some(MediaKeyAction::PlayPause));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("hello & world"), "hello &amp; world");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
        assert_eq!(xml_escape(""), "");
        assert_eq!(xml_escape("no special chars"), "no special chars");
    }

    #[test]
    fn test_applescript_escape() {
        assert_eq!(applescript_escape("hello"), "hello");
        assert_eq!(applescript_escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(applescript_escape("back\\slash"), "back\\\\slash");
        assert_eq!(applescript_escape(""), "");
    }

    #[test]
    fn test_async_notification() {
        let (platform, _rx) = PlatformIntegration::new().unwrap();
        let _ = platform.send_notification("Test", "Async notification test");
    }
}
