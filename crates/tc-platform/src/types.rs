//! Core types for platform integration
//!
//! Defines the shared types used across the platform integration layer:
//! error types, media key actions, MPRIS status types, and keyboard shortcuts.

use thiserror::Error;

/// Platform-specific error type
#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("Platform feature not available: {0}")]
    NotAvailable(String),
    #[error("Platform error: {0}")]
    Other(String),
    #[error("D-Bus error: {0}")]
    Dbus(String),
}

/// Media key actions that the platform can emit
#[derive(Debug, Clone)]
pub enum MediaKeyAction {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
    Quit,
    /// Seek by offset in microseconds (positive = forward, negative = backward)
    Seek(i64),
    /// Set absolute position for a given track in microseconds
    SetPosition { track_id: String, position_us: i64 },
    /// Set volume (0.0 to 1.0 mapped from MPRIS 0.0-1.0+ range)
    SetVolume(f64),
    /// Set playback rate
    SetRate(f64),
    /// Set shuffle on/off
    SetShuffle(bool),
    /// Set loop status: "None", "Track", or "Playlist"
    SetLoopStatus(String),
    /// Open a URI for playback
    OpenUri(String),
}

impl PartialEq for MediaKeyAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::PlayPause, Self::PlayPause) => true,
            (Self::Play, Self::Play) => true,
            (Self::Pause, Self::Pause) => true,
            (Self::Next, Self::Next) => true,
            (Self::Previous, Self::Previous) => true,
            (Self::Stop, Self::Stop) => true,
            (Self::VolumeUp, Self::VolumeUp) => true,
            (Self::VolumeDown, Self::VolumeDown) => true,
            (Self::Mute, Self::Mute) => true,
            (Self::Quit, Self::Quit) => true,
            (Self::Seek(a), Self::Seek(b)) => a == b,
            (Self::SetPosition { track_id: a1, position_us: a2 }, Self::SetPosition { track_id: b1, position_us: b2 }) => a1 == b1 && a2 == b2,
            (Self::SetVolume(a), Self::SetVolume(b)) => (a - b).abs() < f64::EPSILON,
            (Self::SetRate(a), Self::SetRate(b)) => (a - b).abs() < f64::EPSILON,
            (Self::SetShuffle(a), Self::SetShuffle(b)) => a == b,
            (Self::SetLoopStatus(a), Self::SetLoopStatus(b)) => a == b,
            (Self::OpenUri(a), Self::OpenUri(b)) => a == b,
            _ => false,
        }
    }
}

/// Playback status for MPRIS reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisPlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Track metadata for MPRIS reporting
#[derive(Debug, Clone)]
pub struct MprisTrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub length_microseconds: Option<i64>,
    pub track_id: Option<String>,
}

impl Default for MprisTrackInfo {
    fn default() -> Self {
        Self {
            title: None,
            artist: None,
            album: None,
            art_url: None,
            length_microseconds: None,
            track_id: None,
        }
    }
}

/// Notification that MPRIS properties have changed, sent from the
/// main thread to the D-Bus service thread so it can emit
/// PropertiesChanged signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisPropertyChanged {
    PlaybackStatus,
    TrackMetadata,
    Volume,
    Shuffle,
    LoopStatus,
}

/// A keyboard shortcut definition
#[derive(Debug, Clone)]
pub struct KeyboardShortcut {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub action: MediaKeyAction,
}

