//! Helper methods for AudioEngine — utility functions for playback info,
//! state management, and URI decoding.

use log::error;

use super::AudioEngine;
use crate::buffer::{PlaybackInfo, PlaybackState};

impl AudioEngine {
    pub(super) fn current_state(&self) -> PlaybackState {
        match self.playback_info.read() {
            Ok(pb) => pb.state,
            Err(e) => {
                error!("PlaybackInfo RwLock poisoned in current_state; using Stopped");
                e.into_inner().state
            },
        }
    }

    pub(super) fn update_playback_state(&self, state: PlaybackState) {
        match self.playback_info.write() {
            Ok(mut pb) => {
                pb.state = state;
            },
            Err(e) => {
                error!("PlaybackInfo RwLock poisoned in update_playback_state; resetting");
                e.into_inner().state = state;
            },
        }
    }

    /// Helper for short one-field writes to PlaybackInfo.
    pub(super) fn write_playback_info<F: FnOnce(&mut PlaybackInfo)>(&self, f: F) {
        match self.playback_info.write() {
            Ok(mut pb) => f(&mut pb),
            Err(e) => {
                error!("PlaybackInfo RwLock poisoned; resetting and continuing");
                f(&mut e.into_inner());
            },
        }
    }
}

/// Percent-decode a URI-encoded string (e.g. `%20` → space).
/// Returns `None` if the encoding is malformed.
pub(super) fn percent_decode(s: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter().copied();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next()?;
            let h2 = chars.next()?;
            let pair = [h1, h2];
            let hex = std::str::from_utf8(&pair).ok()?;
            let val = u8::from_str_radix(hex, 16).ok()?;
            bytes.push(val);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}
