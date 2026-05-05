//! ReplayGain control methods.

use anyhow::Result;

use crate::audio::replaygain::{ReplayGainInfo, ReplayGainMode, ReplayGainConfig, ReplayGainApplyMode};

use super::AudioEngine;

impl AudioEngine {
    /// Build a ReplayGainConfig from the consolidated rg_state.
    ///
    /// Fix Bug #33: Previously this read 4 separate Mutex fields individually,
    /// so the config could be inconsistent if another thread modified a field
    /// between reads. Now all fields are read atomically from a single Mutex.
    pub(crate) fn build_rg_config(&self) -> ReplayGainConfig {
        let rg = self.rg_state.lock().unwrap_or_else(|e| e.into_inner());
        ReplayGainConfig {
            apply_mode: rg.apply,
            source:     rg.mode,
            preamp_db:  rg.preamp_db,
            fallback_preamp_db: rg.fallback_db,
        }
    }

    pub(crate) fn apply_replaygain_for(&self, path: &std::path::Path) -> Result<()> {
        let info   = ReplayGainInfo::from_path(path)?;
        let cfg    = self.build_rg_config();
        let factor = info.scaling_factor_full(&cfg);
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).factor = factor;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_replaygain_factor(factor as f32);
        tracing::info!(
            "ReplayGain: mode={:?} apply={:?} preamp={:.1} fallback={:.1} factor={:.4}",
            cfg.source, cfg.apply_mode, cfg.preamp_db, cfg.fallback_preamp_db, factor
        );
        Ok(())
    }

    pub fn set_replaygain_enabled(&self, enabled: bool) {
        let was_enabled = {
            let mut rg = self.rg_state.lock().unwrap_or_else(|e| e.into_inner());
            let prev = rg.enabled;
            rg.enabled = enabled;
            if !enabled {
                rg.factor = 1.0;
            }
            prev
        };
        if !enabled {
            // Release rg_state lock before acquiring dsp lock (avoid nested lock ordering)
            self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_replaygain_factor(1.0);
        } else if !was_enabled {
            // Fix Bug #15: When enabling ReplayGain on a playing track, compute
            // and apply the RG factor immediately. Previously, setting enabled=true
            // had no effect until the next track load, because no factor was
            // computed for the currently-loaded track.
            let path = self.current_track_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(ref p) = path {
                if let Err(e) = self.apply_replaygain_for(p) {
                    tracing::warn!("ReplayGain enable-apply: {}", e);
                }
            }
        }
    }
    pub fn replaygain_enabled(&self) -> bool { self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).enabled }

    pub fn set_replaygain_mode(&self, mode: ReplayGainMode) {
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).mode = mode;
    }
    pub fn replaygain_mode(&self) -> ReplayGainMode { self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).mode }

    /// Set the ReplayGain apply mode (Don't apply / Apply Gain / Apply Gain + prevent clipping).
    pub fn set_replaygain_apply_mode(&self, mode: ReplayGainApplyMode) {
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).apply = mode;
    }
    pub fn replaygain_apply_mode(&self) -> ReplayGainApplyMode {
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).apply
    }

    /// Set the RG preamp added on top of the track/album gain (-15..+15 dB).
    /// Equivalent to Poweramp's "RG preamp" knob.
    pub fn set_replaygain_preamp_db(&self, db: f64) {
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).preamp_db = db.clamp(-15.0, 15.0);
    }
    pub fn replaygain_preamp_db(&self) -> f64 { self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).preamp_db }

    /// Set the preamp applied to tracks that have no RG tags (-15..+15 dB).
    /// Equivalent to Poweramp's "Preamp for songs without RG info".
    pub fn set_replaygain_fallback_db(&self, db: f64) {
        self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).fallback_db = db.clamp(-15.0, 15.0);
    }
    pub fn replaygain_fallback_db(&self) -> f64 { self.rg_state.lock().unwrap_or_else(|e| e.into_inner()).fallback_db }
}
