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

        // Fix H2: Use volume_gain (user volume control) for the seek fade
        // instead of replaygain_factor. The previous code ramped
        // replaygain_factor to 0, which corrupted the ReplayGain state —
        // after the seek, a stale RG value was restored, or worse, RG was
        // left at 0. The seek fade should only attenuate the output volume,
        // not touch ReplayGain at all.
        //
        // Fix H1: Move fade to a non-blocking approach. The previous code
        // blocked the UI thread for up to 200ms (seek_fade_ms up to 200ms)
        // with sleep(1ms) in a loop, competing for the DSP lock and causing
        // audio underruns/clicks. Instead, we use the DSP engine's built-in
        // seek-fade mechanism which ramps volume in the audio thread.
        if fade_ms > 0 {
            let vol = self
                .volume_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .volume as f32;
            // Fix seek-fade race: acquire the DSP lock once and perform both
            // start_seek_fade and reset_state under the same lock acquisition.
            // Previously these were two separate lock acquisitions, allowing the
            // DSP thread to process samples between the two calls with stale state.
            let dsp_arc = self.dsp_arc();
            let mut dsp = dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
            // Fix C1: Reset DSP state BEFORE starting the seek fade.
            // Previously, start_seek_fade() set up a volume ramp, then reset_state()
            // immediately cleared it — making the seek fade inaudible.
            // Now we reset first (flushing old state), then start the fade
            // so the ramp survives and the seek fade is applied correctly.
            dsp.reset_state();
            // Use the DSP engine's volume gain for the fade, NOT replaygain_factor
            dsp.start_seek_fade(vol, fade_ms);
        } else {
            // No fade — still need to reset state under a single lock.
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
