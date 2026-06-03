//! Media key receiver and listener management
//!
//! Contains [`MediaKeyReceiver`] for polling media key actions and
//! the start/stop listener methods on [`PlatformIntegration`].
//!
//! v0.20.0: The `start_media_key_listener` method now returns `Ok(())`
//! on ALL platforms (not just Linux) because cross-platform media key
//! handling is provided by the `souvlaki`-based `CrossPlatformMediaControls`.
//! The Linux-only MPRIS D-Bus approach is supplemented (not replaced) by
//! souvlaki, which handles media key events on macOS and Windows too.

use std::sync::mpsc::Receiver;
use std::sync::atomic::Ordering;

use crate::types::MediaKeyAction;
use crate::PlatformIntegration;

/// Standalone receiver for media key actions.

/// Extracted from `PlatformIntegration` to fix the Sender+Receiver
/// co-location issue. This type can be polled independently without
/// borrowing the integration struct, eliminating the need for `RefCell`
/// wrapping in the UI layer and preventing `BorrowMutError` panics.
pub struct MediaKeyReceiver {
    rx: Receiver<MediaKeyAction>,
}

impl MediaKeyReceiver {
    /// Create a new MediaKeyReceiver wrapping the given channel receiver.
    pub(crate) fn new(rx: Receiver<MediaKeyAction>) -> Self {
        Self { rx }
    }

    /// Try to receive a media key action (non-blocking)
    pub fn try_recv(&self) -> Option<MediaKeyAction> {
        self.rx.try_recv().ok()
    }
}

impl PlatformIntegration {
    /// Start listening for media keys.
    ///
    /// v0.20.0: Now returns `Ok(())` on ALL platforms. Cross-platform media
    /// key handling is provided by `CrossPlatformMediaControls` (souvlaki),
    /// which uses:
    /// - MPRIS D-Bus on Linux
    /// - MPRemoteCommandCenter on macOS
    /// - SystemMediaTransportControls on Windows
    ///
    /// If souvlaki initialization failed, media keys will not be forwarded,
    /// but the method still returns Ok since keyboard shortcuts work as a
    /// fallback.
    pub fn start_media_key_listener(&mut self) -> Result<(), crate::types::PlatformError> {
        if self.bg_running.load(Ordering::Relaxed) {
            return Ok(()); // Already running
        }

        // Cross-platform media controls (souvlaki) are already initialized
        // in PlatformIntegration::new(). They work on all platforms.
        if self.media_controls.as_ref().map_or(false, |c| c.is_available()) {
            log::info!(
                "Media key listener active via cross-platform controls (souvlaki)"
            );
        } else {
            log::warn!(
                "Cross-platform media controls not available. \
                 Media keys will not be forwarded. Use keyboard shortcuts instead."
            );
        }

        // On Linux, also start the MPRIS D-Bus service for advanced
        // property reporting (Metadata, CanGoNext, etc.) if registered.
        #[cfg(target_os = "linux")]
        {
            if self.mpris_registered {
                log::info!("MPRIS D-Bus service active for advanced property reporting");
            }
        }

        self.bg_running.store(true, Ordering::Relaxed);
        log::info!("Media key listener started");
        Ok(())
    }

    /// Stop listening for media keys.
    pub fn stop_media_key_listener(&mut self) {
        self.bg_running.store(false, Ordering::Relaxed);

        #[cfg(target_os = "linux")]
        {
            // The D-Bus thread's event loop detects RecvTimeoutError::Disconnected
            // and breaks out of the loop, allowing the thread to exit.
            self.mpris_notify_tx = None;
        }

        log::info!("Media key listener stopped");
    }
}
