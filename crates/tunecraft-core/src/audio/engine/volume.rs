//! Volume, playback speed, and exclusive mode control methods.

use anyhow::Result;

use super::AudioEngine;

impl AudioEngine {
    pub fn set_volume(&self, volume: f64) -> Result<()> {
        let vol = volume.clamp(0.0, 1.0);
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .volume = vol;
        {
            let dsp_arc = self.dsp_arc();
            let mut dsp = dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
            dsp.set_volume_gain(vol as f32);
        }
        if self.crossfade_active() {
            let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref e) = *cf {
                e.set_volume(vol);
            }
        }
        Ok(())
    }

    pub fn volume(&self) -> f64 {
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .volume
    }

    pub fn set_playback_speed(&self, speed: f64) -> Result<()> {
        let speed = speed.clamp(0.25, 4.0);
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .playback_speed = speed;
        let s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref sess) = *s {
            sess.pipeline.set_rate(speed);
        }
        if self.crossfade_active() {
            let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref e) = *cf {
                e.set_rate(speed);
            }
        }
        Ok(())
    }

    pub fn playback_speed(&self) -> f64 {
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .playback_speed
    }

    /// Enable or disable bit-perfect / exclusive mode.
    ///
    /// When enabled, the audio output opens the device in exclusive mode,
    /// bypassing the system mixer entirely. Audio is output at the hardware's
    /// native bit depth and sample rate with no kernel resampling.
    ///
    /// On Linux this means ALSA direct or PipeWire exclusive access.
    /// On macOS this uses CoreAudio exclusive mode.
    /// Changes take effect on the next track load.
    pub fn set_exclusive_mode(&self, enabled: bool) {
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .exclusive_mode = enabled;
    }
    pub fn exclusive_mode(&self) -> bool {
        self.volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .exclusive_mode
    }
}
