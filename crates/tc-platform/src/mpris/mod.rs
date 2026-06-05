//! MPRIS D-Bus service implementation (Linux only)
//!
//! Exposes org.mpris.MediaPlayer2 and org.mpris.MediaPlayer2.Player
//! interfaces on the session bus so that desktop panels and media
//! player applets can control TuneCraft.
//!
//! `PropertiesChanged` signals (C2 fix), uses `parking_lot::Mutex`
//! for poison-resistant state access (C5 fix), and the `play()` and
//! `pause()` MPRIS methods now send dedicated `MediaKeyAction::Play`
//! and `MediaKeyAction::Pause` actions instead of `PlayPause` (C3 fix).
//! The `Quit()` method now sends `MediaKeyAction::Quit` (C4 fix).

mod dbus;
mod signals;

use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};

use parking_lot::Mutex;

use crate::types::{MediaKeyAction, MprisPlaybackStatus, MprisPropertyChanged, MprisTrackInfo};

/// Shared MPRIS state that can be updated from the main thread
/// and read from the D-Bus service thread.
///
/// Fields are public to enforce validation boundaries (M7 partial fix).
#[derive(Debug, Clone)]
pub struct MprisState {
    pub playback_status: MprisPlaybackStatus,
    pub track_info: MprisTrackInfo,
    pub volume: f64,
    pub identity: String,
    pub desktop_entry: String,
    /// Whether shuffle is enabled
    pub shuffle: bool,
    /// Loop status string: must be one of "None", "Track", "Playlist"
    pub loop_status: String,
    /// Playback rate (must be > 0)
    pub rate: f64,
    /// Playback position in microseconds
    pub position_microseconds: i64,
}

impl Default for MprisState {
    fn default() -> Self {
        Self {
            playback_status: MprisPlaybackStatus::Stopped,
            track_info: MprisTrackInfo::default(),
            volume: 1.0,
            identity: "TuneCraft".to_string(),
            desktop_entry: "tunecraft".to_string(),
            shuffle: false,
            loop_status: "None".to_string(),
            rate: 1.0,
            position_microseconds: 0,
        }
    }
}

/// MPRIS D-Bus service handle
pub struct MprisService {
    identity: String,
    action_tx: Sender<MediaKeyAction>,
}

impl MprisService {
    pub fn new(identity: &str, action_tx: Sender<MediaKeyAction>) -> Self {
        Self {
            identity: identity.to_string(),
            action_tx,
        }
    }

    /// Create the shared MprisState Arc.
    pub fn state(&self) -> Arc<Mutex<MprisState>> {
        Arc::new(Mutex::new(MprisState {
            identity: self.identity.clone(),
            desktop_entry: self.identity.to_lowercase(),
            ..MprisState::default()
        }))
    }

    /// Attempt to register the MPRIS service on D-Bus.
    ///
    /// This spawns a background thread that owns the D-Bus connection.
    /// The thread exits when the notification channel is disconnected
    /// (i.e., when `PlatformIntegration` drops the sender).
    pub fn start(
        &self,
        state: Arc<Mutex<MprisState>>,
        notify_rx: Receiver<MprisPropertyChanged>,
    ) -> Result<(), String> {
        let identity = self.identity.clone();
        let action_tx = self.action_tx.clone();

        std::thread::Builder::new()
            .name("tunecraft-mpris-dbus".to_string())
            .spawn(
                move || match dbus::run_dbus_server(&identity, &action_tx, &state, &notify_rx) {
                    Ok(()) => log::info!("MPRIS D-Bus service stopped"),
                    Err(e) => log::warn!("MPRIS D-Bus service error: {}", e),
                },
            )
            .map_err(|e| format!("Failed to spawn MPRIS thread: {}", e))?;

        Ok(())
    }
}
