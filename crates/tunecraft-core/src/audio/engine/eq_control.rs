//! EQ control methods: equalizer state, bass, treble, stereo width, balance, dither.

use crate::audio::equalizer::EqualizerState;

use super::AudioEngine;

impl AudioEngine {
    // -- Equalizer -----------------------------------------------------------

    pub fn eq_state(&self) -> EqualizerState { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).clone() }

    /// Apply a full `EqualizerState` — pushes all parameters to `DspEngine`.
    pub fn set_eq_state(&self, state: EqualizerState) {
        *self.eq_state.lock().unwrap_or_else(|e| e.into_inner()) = state.clone();
        let mut dsp = self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner());
        let gains: Vec<f32> = state.bands.iter().map(|b| b.gain as f32).collect();
        dsp.set_all_gains(&gains);
        dsp.enabled = state.enabled;
        // Apply shelf frequencies before gains so the biquads are built at the right freq.
        dsp.set_bass_freq(state.bass_freq_hz as f32);
        dsp.set_treble_freq(state.treble_freq_hz as f32);
        dsp.set_bass(state.bass_db as f32);
        dsp.set_treble(state.treble_db as f32);
        dsp.set_stereo_width(state.stereo_width as f32);
        dsp.set_balance(state.balance as f32);
        dsp.set_dither_enabled(state.dither_enabled);
        // Apply MS EQ bands if present
        dsp.apply_ms_eq_from_state(&state);
    }

    pub fn set_eq_band_gain(&self, band_index: usize, gain: f64) {
        {
            let mut eq = self.eq_state.lock().unwrap_or_else(|e| e.into_inner());
            if band_index < eq.bands.len() { eq.bands[band_index].gain = gain.clamp(-24.0, 24.0); }
        }
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_band_gain(band_index, gain as f32);
    }

    pub fn set_eq_enabled(&self, enabled: bool) {
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).enabled = enabled;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).enabled = enabled;
    }

    // -- Bass / Treble (Tier 1 #1) ------------------------------------------

    pub fn set_bass(&self, gain_db: f64) {
        let g = gain_db.clamp(-12.0, 12.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).bass_db = g;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_bass(g as f32);
    }
    pub fn bass(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).bass_db }

    /// Set bass shelf corner frequency in Hz (20-500). Default: 90 Hz (matches Poweramp).
    pub fn set_bass_freq_hz(&self, freq_hz: f64) {
        let f = freq_hz.clamp(20.0, 500.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).bass_freq_hz = f;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_bass_freq(f as f32);
    }
    pub fn bass_freq_hz(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).bass_freq_hz }

    pub fn set_treble(&self, gain_db: f64) {
        let g = gain_db.clamp(-12.0, 12.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).treble_db = g;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_treble(g as f32);
    }
    pub fn treble(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).treble_db }

    /// Set treble shelf corner frequency in Hz (1000-20 000). Default: 10 000 Hz.
    pub fn set_treble_freq_hz(&self, freq_hz: f64) {
        let f = freq_hz.clamp(1000.0, 20_000.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).treble_freq_hz = f;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_treble_freq(f as f32);
    }
    pub fn treble_freq_hz(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).treble_freq_hz }

    // -- Stereo width (Tier 2 #5) --------------------------------------------

    pub fn set_stereo_width(&self, width: f64) {
        let w = width.clamp(0.0, 3.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).stereo_width = w;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_stereo_width(w as f32);
    }
    pub fn stereo_width(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).stereo_width }

    // -- Balance (Tier 2 #6) -------------------------------------------------

    pub fn set_balance(&self, balance: f64) {
        let b = balance.clamp(-1.0, 1.0);
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).balance = b;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_balance(b as f32);
    }
    pub fn balance(&self) -> f64 { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).balance }

    // -- Dither (Tier 1 #4) --------------------------------------------------

    pub fn set_dither_enabled(&self, enabled: bool) {
        self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).dither_enabled = enabled;
        self.dsp_arc().lock().unwrap_or_else(|e| e.into_inner()).set_dither_enabled(enabled);
    }
    pub fn dither_enabled(&self) -> bool { self.eq_state.lock().unwrap_or_else(|e| e.into_inner()).dither_enabled }
}
