//! Transport control methods: play, pause, stop, toggle, state queries.

use anyhow::Result;
use std::sync::atomic::Ordering;

use super::{AudioEngine, PlayerState};
use crate::audio::engine::notify_state;

impl AudioEngine {
    pub fn play(&self) -> Result<()> {
        if self.crossfade_active() {
            let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref e) = *cf {
                e.play();
                return Ok(());
            }
        }
        let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        match s.as_mut() {
            Some(sess) => {
                sess.pipeline.play()?;
                sess.playing.store(true, Ordering::Relaxed);
                sess.is_playing = true;
                notify_state(&self.state_cb, PlayerState::Playing);
                Ok(())
            }
            None => anyhow::bail!("no track loaded"),
        }
    }

    pub fn pause(&self) -> Result<()> {
        if self.crossfade_active() {
            let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref e) = *cf {
                e.pause();
                return Ok(());
            }
        }
        let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        match s.as_mut() {
            Some(sess) => {
                sess.pipeline.pause()?;
                sess.playing.store(false, Ordering::Relaxed);
                sess.is_playing = false;
                notify_state(&self.state_cb, PlayerState::Paused);
                Ok(())
            }
            None => anyhow::bail!("no track loaded"),
        }
    }

    pub fn stop(&self) -> Result<()> {
        self.transport_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .use_crossfade = false;
        let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut sess) = s.take() {
            sess.stop_and_join();
        }
        *self.crossfade.lock().unwrap_or_else(|e| e.into_inner()) = None;
        notify_state(&self.state_cb, PlayerState::Stopped);
        Ok(())
    }

    pub fn toggle_playback(&self) -> Result<()> {
        if self.is_playing() {
            self.pause()
        } else {
            self.play()
        }
    }

    pub fn is_playing(&self) -> bool {
        if self.crossfade_active() {
            return self
                .crossfade
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|e| e.is_playing());
        }
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|s| s.is_playing)
    }

    pub fn state(&self) -> PlayerState {
        if self.crossfade_active() {
            return self
                .crossfade
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .map_or(PlayerState::Stopped, |e| {
                    if e.is_playing() {
                        PlayerState::Playing
                    } else {
                        PlayerState::Paused
                    }
                });
        }
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(PlayerState::Stopped, |s| {
                if s.is_playing {
                    PlayerState::Playing
                } else {
                    PlayerState::Paused
                }
            })
    }

    pub fn position(&self) -> Option<std::time::Duration> {
        if self.crossfade_active() {
            return self
                .crossfade
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|e| e.position());
        }
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|s| s.pipeline.position())
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        if self.crossfade_active() {
            return self
                .crossfade
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|e| e.duration());
        }
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|s| s.pipeline.duration())
    }

    /// Returns the cumulative underrun count from the current audio output stream.
    /// Useful for diagnostics and buffer tuning. Returns 0 if no session is active.
    pub fn underrun_count(&self) -> u64 {
        let s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        match s.as_ref() {
            Some(sess) => sess.underrun_count.load(Ordering::Relaxed),
            None => 0,
        }
    }

    pub(crate) fn crossfade_active(&self) -> bool {
        let use_crossfade = {
            let ts = self
                .transport_state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            ts.use_crossfade
        };
        if !use_crossfade {
            return false;
        }
        self.crossfade
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
}
