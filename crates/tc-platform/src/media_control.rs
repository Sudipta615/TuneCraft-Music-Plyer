//! Cross-platform media key and transport control handling
//!
//! v0.20.0: Unified cross-platform media key integration using the `souvlaki`
//! crate. This replaces the Linux-only raw D-Bus approach with a proper
//! abstraction that works on all three desktop platforms:
//!
//! - **Linux**: Uses MPRIS D-Bus (via souvlaki's MPRIS backend)
//! - **macOS**: Uses MPRemoteCommandCenter (via souvlaki's macOS backend)
//! - **Windows**: Uses SystemMediaTransportControls (via souvlaki's Windows backend)
//!
//! The existing `mpris` module is retained for advanced D-Bus property
//! reporting (Metadata, CanGoNext, etc.) that souvlaki does not expose,
//! but media *key* handling is now fully cross-platform.

use std::sync::mpsc::Sender;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

use crate::types::{MediaKeyAction, MprisPlaybackStatus};

/// Wrapper around `souvlaki::MediaControls` that translates platform
/// media key events into `MediaKeyAction` values sent through the
/// application's action channel.
pub struct CrossPlatformMediaControls {
    /// The souvlaki MediaControls handle. None if initialization failed
    /// (e.g., no D-Bus on Linux, no window handle on some platforms).
    controls: Option<MediaControls>,
    /// Last reported playback status, so `set_position` can emit the
    /// correct `MediaPlayback` variant instead of always forcing Playing.
    current_status: MprisPlaybackStatus,
    /// Last reported track id (best-effort, used to populate
    /// `MediaKeyAction::SetPosition.track_id` which MPRIS spec requires).
    current_track_id: Option<String>,
}

impl CrossPlatformMediaControls {
    /// Create a new cross-platform media controls instance.
    ///
    /// On Linux, this creates an MPRIS D-Bus service. On macOS, it
    /// registers with MPRemoteCommandCenter. On Windows, it registers
    /// with SystemMediaTransportControls.
    ///
    /// If platform initialization fails (e.g., no D-Bus daemon on Linux),
    /// the controls will be None and media key events will not be forwarded.
    /// The application should fall back to keyboard shortcuts in this case.
    pub fn new(action_tx: Sender<MediaKeyAction>) -> Result<Self, String> {
        let config = PlatformConfig {
            // Use the canonical "TuneCraft" identity so the souvlaki-registered
            // D-Bus name `org.mpris.MediaPlayer2.TuneCraft` matches the one
            // registered by the hand-rolled zbus MPRIS service in
            // `mpris/dbus.rs`. Previously this was "tunecraft" (lowercase),
            // causing two separate D-Bus entries to appear in KDE/GNOME.
            dbus_name: "TuneCraft",
            display_name: "TuneCraft",
            hwnd: None,
        };

        let controls = match MediaControls::new(config) {
            Ok(mut ctrl) => {
                // Attach the event handler that translates souvlaki events
                // into our MediaKeyAction type.
                let tx = action_tx.clone();
                ctrl.attach(move |event: MediaControlEvent| {
                    Self::handle_event(event, &tx);
                })
                .map_err(|e| format!("Failed to attach media control handler: {}", e))?;
                Some(ctrl)
            },
            Err(e) => {
                log::warn!(
                    "Failed to initialize cross-platform media controls: {}. \
                     Media key events will not be forwarded. \
                     On Linux, ensure a D-Bus session is available. \
                     On macOS/Windows, this should not fail.",
                    e
                );
                None
            },
        };

        Ok(Self {
            controls,
            current_status: MprisPlaybackStatus::Stopped,
            current_track_id: None,
        })
    }

    /// Translate a souvlaki MediaControlEvent into our MediaKeyAction
    /// and send it through the action channel.
    fn handle_event(event: MediaControlEvent, tx: &Sender<MediaKeyAction>) {
        let action = match event {
            MediaControlEvent::Play => MediaKeyAction::Play,
            MediaControlEvent::Pause => MediaKeyAction::Pause,
            MediaControlEvent::Toggle => MediaKeyAction::PlayPause,
            MediaControlEvent::Next => MediaKeyAction::Next,
            MediaControlEvent::Previous => MediaKeyAction::Previous,
            MediaControlEvent::Stop => MediaKeyAction::Stop,
            MediaControlEvent::Seek(direction) => {
                let amount = match direction {
                    SeekDirection::Forward => 5_000_000,
                    SeekDirection::Backward => -5_000_000,
                };
                MediaKeyAction::Seek(amount)
            },
            MediaControlEvent::SeekBy(direction, duration) => {
                let sign = match direction {
                    SeekDirection::Forward => 1,
                    SeekDirection::Backward => -1,
                };
                MediaKeyAction::Seek(sign * duration.as_micros() as i64)
            },
            MediaControlEvent::Raise => {
                // Bring window to front — not directly a media key action
                log::info!("Media control: Raise (bring to front)");
                return;
            },
            MediaControlEvent::Quit => MediaKeyAction::Quit,
            MediaControlEvent::SetVolume(volume) => MediaKeyAction::SetVolume(volume as f32),
            MediaControlEvent::SetPosition(position) => {
                let pos_us = position.0.as_micros() as i64;
                // We don't have access to the actual track_id here (souvlaki
                // doesn't pass it through), so we send an empty string and
                // let the consumer ignore it. The MPRIS spec requires the
                // track_id to be present, but the engine's SetPosition
                // handler treats it as informational only.
                MediaKeyAction::SetPosition {
                    track_id: String::new(),
                    position_us: pos_us,
                }
            },
            MediaControlEvent::OpenUri(uri) => MediaKeyAction::OpenUri(uri),
        };

        if let Err(e) = tx.send(action) {
            log::warn!("Failed to send media key action: {}", e);
        }
    }

    /// Update the playback status shown in the OS media controls.
    pub fn set_playback_status(&mut self, status: MprisPlaybackStatus) {
        self.current_status = status;
        if let Some(ref mut ctrl) = self.controls {
            let playback = match status {
                MprisPlaybackStatus::Playing => MediaPlayback::Playing { progress: None },
                MprisPlaybackStatus::Paused => MediaPlayback::Paused { progress: None },
                MprisPlaybackStatus::Stopped => MediaPlayback::Stopped,
            };
            if let Err(e) = ctrl.set_playback(playback) {
                log::warn!("Failed to update media playback status: {}", e);
            }
        }
    }

    /// Update the track metadata shown in the OS media controls.
    pub fn set_metadata(
        &mut self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration: Option<std::time::Duration>,
        art_url: Option<&str>,
    ) {
        // Track the title so we can populate SetPosition.track_id later
        // (the MPRIS spec requires SetPosition to include the track_id).
        self.current_track_id = title.map(|s| s.to_string());
        if let Some(ref mut ctrl) = self.controls {
            let metadata = MediaMetadata {
                title,
                artist,
                album,
                cover_url: art_url,
                duration,
            };
            if let Err(e) = ctrl.set_metadata(metadata) {
                log::warn!("Failed to update media metadata: {}", e);
            }
        }
    }

    /// Update the current playback position.
    ///
    /// Preserves the current playback status (Playing / Paused / Stopped)
    /// rather than unconditionally forcing `Playing` — previously this
    /// caused the OS media overlay to flicker back to "Playing" on every
    /// position update tick even while paused.
    pub fn set_position(&mut self, position: std::time::Duration) {
        if let Some(ref mut ctrl) = self.controls {
            let pos = Some(MediaPosition(position));
            let playback = match self.current_status {
                MprisPlaybackStatus::Playing => MediaPlayback::Playing { progress: pos },
                MprisPlaybackStatus::Paused => MediaPlayback::Paused { progress: pos },
                MprisPlaybackStatus::Stopped => MediaPlayback::Stopped,
            };
            if let Err(e) = ctrl.set_playback(playback) {
                log::warn!("Failed to update media position: {}", e);
            }
        }
    }

    /// Update the volume shown in the OS media controls.
    pub fn set_volume(&mut self, volume: f32) {
        if let Some(ref mut ctrl) = self.controls {
            if let Err(e) = ctrl.set_volume(volume as f64) {
                log::warn!("Failed to update media volume: {}", e);
            }
        }
    }

    /// Check if media controls are available on this platform.
    pub fn is_available(&self) -> bool {
        self.controls.is_some()
    }
}
