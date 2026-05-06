//! Per-output presets and AutoEQ integration methods.

use anyhow::{Context, Result};

use crate::audio::equalizer::OutputDeviceId;

use super::AudioEngine;

impl AudioEngine {
    /// Save the current EQ state for a specific output device.
    pub fn save_preset_for_device(&self, device: OutputDeviceId) {
        let state = self
            .eq_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.output_presets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .save(device, state);
    }

    /// Load and apply the saved EQ state for a device.
    /// Call this from cpal's device-change callback.
    pub fn apply_preset_for_device(&self, device: OutputDeviceId) {
        *self.active_device.lock().unwrap_or_else(|e| e.into_inner()) = device.clone();
        let state = self
            .output_presets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .load(&device);
        self.set_eq_state(state);
    }

    /// Returns the active output device ID.
    pub fn active_device(&self) -> OutputDeviceId {
        self.active_device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Serialize the full preset store to JSON for persistence.
    pub fn serialize_presets(&self) -> Result<String> {
        let store = self
            .output_presets
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        Ok(serde_json::to_string(&*store)?)
    }

    /// Restore the preset store from JSON.
    pub fn deserialize_presets(&self, json: &str) -> Result<()> {
        let store: crate::audio::equalizer::OutputPresetStore = serde_json::from_str(json)?;
        *self
            .output_presets
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = store;
        Ok(())
    }

    /// Apply a headphone correction profile from AutoEQ JSON export.
    /// See https://autoeq.app -- export as "Parametric EQ" JSON.
    pub fn apply_autoeq_profile(&self, json: &str) -> Result<()> {
        let state = crate::audio::equalizer::load_autoeq_profile(json)
            .context("Failed to parse AutoEQ profile")?;
        self.set_eq_state(state);
        Ok(())
    }
}
