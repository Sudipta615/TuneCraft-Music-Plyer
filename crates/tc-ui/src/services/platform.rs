//! Platform service — MPRIS status, media keys, OS integration
//!
//! Encapsulates platform integration, using the separated
//! `MediaKeyReceiver` pattern introduced in v0.9.1.
//!
//! Uses `parking_lot::RwLock` / `parking_lot::Mutex` — non-poisonable,
//! infallible guard acquisition. No poison-recovery boilerplate needed.

use parking_lot::{Mutex, RwLock};
use tc_platform::{
    MediaKeyAction, MediaKeyReceiver, MprisPlaybackStatus, MprisTrackInfo, PlatformIntegration,
};

/// The platform service manages OS integration features.
///
/// Uses `parking_lot::RwLock<PlatformIntegration>` — non-poisonable,
/// consistent with ConfigService and EqService.
pub struct PlatformService {
    inner: RwLock<PlatformIntegration>,
    media_key_rx: Mutex<MediaKeyReceiver>,
}

impl PlatformService {
    /// Create a new PlatformService.
    pub fn new(platform: PlatformIntegration, media_key_rx: MediaKeyReceiver) -> Self {
        Self {
            inner: RwLock::new(platform),
            media_key_rx: Mutex::new(media_key_rx),
        }
    }

    /// Try to receive a media key action (non-blocking).
    pub fn try_recv_action(&self) -> Option<MediaKeyAction> {
        self.media_key_rx.lock().try_recv()
    }

    /// Update MPRIS status to Playing with track info.
    pub fn update_mpris_playing(
        &self,
        title: &str,
        artist: Option<&str>,
        album: Option<&str>,
        duration_secs: f32,
        track_id: i64,
    ) {
        let mut platform = self.inner.write();
        platform.set_mpris_status(MprisPlaybackStatus::Playing);
        platform.set_mpris_track(MprisTrackInfo {
            title: Some(title.to_string()),
            artist: artist.map(|s| s.to_string()),
            album: album.map(|s| s.to_string()),
            art_url: None,
            length_microseconds: Some((duration_secs * 1_000_000.0) as i64),
            track_id: Some(format!("/org/tunecraft/track/{}", track_id)),
        });
    }

    /// Update MPRIS status to Playing (resume, no track change).
    pub fn update_mpris_playing_by_state(&self) {
        self.inner
            .write()
            .set_mpris_status(MprisPlaybackStatus::Playing);
    }

    /// Update MPRIS status to Paused.
    pub fn update_mpris_paused(&self) {
        self.inner
            .write()
            .set_mpris_status(MprisPlaybackStatus::Paused);
    }

    /// Update MPRIS status to Stopped.
    pub fn update_mpris_stopped(&self) {
        self.inner
            .write()
            .set_mpris_status(MprisPlaybackStatus::Stopped);
    }

    /// Update MPRIS volume.
    pub fn update_mpris_volume(&self, volume: f32) {
        self.inner.write().set_mpris_volume(volume);
    }

    /// Send a desktop notification.
    pub fn send_notification(
        &self,
        title: &str,
        body: &str,
    ) -> Result<(), tc_platform::PlatformError> {
        self.inner.read().send_notification(title, body)
    }

    /// Process a keyboard event for shortcuts.
    pub fn process_key_event(
        &self,
        key: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) -> Option<MediaKeyAction> {
        self.inner
            .read()
            .process_key_event(key, ctrl, alt, shift, meta)
    }

    /// Called periodically from the UI sync loop to keep MPRIS clients
    /// (desktop panels, media applets) in sync with the actual position.
    pub fn update_mpris_position(&self, position_secs: f32) {
        let position_us = (position_secs * 1_000_000.0) as i64;
        self.inner.write().set_mpris_position(position_us);
    }
}
