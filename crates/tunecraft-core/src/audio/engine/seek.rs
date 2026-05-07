//! Seek-with-fade logic.

use anyhow::Result;

use super::AudioEngine;

impl AudioEngine {
    pub fn seek(&self, position: std::time::Duration) -> Result<()> {
        if self.crossfade_active() {
            let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref e) = *cf {
                return e.seek(position);
            }
        }

        let (rate, fade_ms) = {
            let vs = self.volume_state.lock().unwrap_or_else(|e| e.into_inner());
            let ts = self
                .transport_state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            (vs.playback_speed, ts.seek_fade_ms)
        };

        if fade_ms > 0 {
            let vol = self
                .volume_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .volume as f32;
            let dsp_arc = self.dsp_arc();
            let mut dsp = dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
            dsp.reset_state();
            dsp.start_seek_fade(vol, fade_ms);
        } else {
            self.dsp_arc()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reset_state();
        }
        let s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let result = match s.as_ref() {
            Some(sess) => sess.pipeline.seek(position, rate),
            None => anyhow::bail!("no track loaded"),
        };
        drop(s); // release session lock

        result
    }

    /// Set the fade duration (in milliseconds) applied before and after a seek.
    /// 0 disables seek fading. Typical value: 20 ms (one crossfade step).
    pub fn set_seek_fade_ms(&self, ms: u32) {
        self.transport_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .seek_fade_ms = ms.clamp(0, 200);
    }
    pub fn seek_fade_ms(&self) -> u32 {
        self.transport_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .seek_fade_ms
    }
}
