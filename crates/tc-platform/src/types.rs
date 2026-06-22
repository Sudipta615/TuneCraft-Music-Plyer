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
#[derive(Debug, Clone, PartialEq)]
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
    SetPosition {
        track_id: String,
        position_us: i64,
    },
    /// Set volume (0.0 to 1.0 mapped from MPRIS 0.0-1.0+ range)
    SetVolume(f32),
    /// Set playback rate
    SetRate(f32),
    /// Set shuffle on/off
    SetShuffle(bool),
    /// Set loop status: "None", "Track", or "Playlist"
    SetLoopStatus(String),
    /// Open a URI for playback
    OpenUri(String),
    ToggleShuffle,
    ToggleRepeat,
    GlobalSearch,
}

// Note: previously a hand-written `PartialEq` used `f32::EPSILON` to compare
// the float-bearing variants (`SetVolume`, `SetRate`). That constant is the
// ULP at 1.0 and is wildly wrong for values far from 1.0 (e.g. it would
// consider `SetVolume(0.0)` and `SetVolume(1e-8)` equal, while also considering
// `SetVolume(1000.0)` and `SetVolume(1000.0 + 1e-5)` equal). The derived
// `PartialEq` does an exact bit-for-bit comparison, which is correct for an
// enum used in tests and in coalescing event queues where identical values
// should coalesce.

/// Playback status for MPRIS reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisPlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Track metadata for MPRIS reporting
#[derive(Debug, Clone, Default)]
pub struct MprisTrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub length_microseconds: Option<i64>,
    pub track_id: Option<String>,
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
