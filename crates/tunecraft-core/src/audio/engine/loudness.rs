//! EBU R128 loudness normalization control methods.

use super::AudioEngine;

impl AudioEngine {
    /// Enable or disable EBU R128 loudness normalization.
    /// When enabled, this operates as a complement to ReplayGain — tracks with
    /// ReplayGain tags use ReplayGain; tracks without tags get EBU R128
    /// loudness measurement and normalization to the configured target LUFS.
    pub fn set_loudness_enabled(&self, enabled: bool) {
        let mut state = self
            .loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.enabled = enabled;
        if !enabled {
            drop(state);
            self.dsp_arc()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_loudness_gain(1.0);
        }
    }
    pub fn loudness_enabled(&self) -> bool {
        self.loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .enabled
    }

    /// Set the target loudness level in LUFS (typically -23.0 for EBU R128 or -14.0 for streaming).
    pub fn set_loudness_target_lufs(&self, lufs: f64) {
        self.loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .target_lufs = lufs.clamp(-30.0, 0.0);
    }
    pub fn loudness_target_lufs(&self) -> f64 {
        self.loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .target_lufs
    }

    /// Compute EBU R128 loudness for a buffer of interleaved stereo samples.
    #[deprecated(
        since = "5.1.0",
        note = "process_loudness corrupts DSP thread measurement; the DSP thread handles this internally"
    )]
    pub fn process_loudness(&self, _buf: &[f32]) {}

    /// Reset the loudness measurement state (call at track boundaries).
    pub fn reset_loudness(&self) {
        self.loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .loudness
            .reset();
    }

    /// Get the current measured loudness in LUFS, if enough samples have been processed.
    pub fn measured_loudness_lufs(&self) -> Option<f64> {
        self.loudness_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .loudness
            .integrated_loudness()
    }
}
