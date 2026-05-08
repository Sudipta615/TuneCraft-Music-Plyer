//! Room correction convolution control methods.

use anyhow::Result;

use super::AudioEngine;

impl AudioEngine {
    /// Load a WAV impulse response for room correction convolution.
    /// Creates a ConvolutionEngine from the IR file and stores it.
    pub fn load_convolution_ir(&self, path: &std::path::Path) -> Result<()> {
        let engine = crate::audio::convolution::ConvolutionEngine::load_from_wav(path)?;
        *self.convolution.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);
        Ok(())
    }

    /// Enable or disable the room correction convolution engine.
    /// When disabled, the convolution is bypassed even if an IR is loaded.
    pub fn set_convolution_enabled(&self, enabled: bool) {
        let mut conv = self.convolution.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut engine) = *conv {
            engine.enabled = enabled;
        }
    }

    /// Returns whether room correction convolution is currently enabled.
    pub fn convolution_enabled(&self) -> bool {
        self.convolution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|e| e.enabled)
    }
}
