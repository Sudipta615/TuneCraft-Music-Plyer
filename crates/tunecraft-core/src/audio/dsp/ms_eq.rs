//! Parametric Mid/Side EQ stage.
//!
//! Applied after stereo-width encoding. Allows independent per-band gain on
//! the Mid (centre) and Side (stereo difference) components — mastering-grade
//! processing without affecting the overall L/R balance.

use super::biquad::{Biquad, SmoothedBand};

pub const MAX_MS_EQ_BANDS: usize = 5;

/// Parameters for a single M/S EQ band.
#[derive(Debug, Clone, Copy)]
pub struct MsEqBand {
    pub freq_hz: f32,
    pub mid_gain_db: f32,
    pub side_gain_db: f32,
    pub q: f32,
    pub enabled: bool,
}

impl MsEqBand {
    pub fn flat() -> Self {
        Self {
            freq_hz: 1000.0,
            mid_gain_db: 0.0,
            side_gain_db: 0.0,
            q: 1.0,
            enabled: false,
        }
    }
}

/// The full M/S EQ stage: up to `MAX_MS_EQ_BANDS` smoothed biquad pairs
/// (one for Mid, one for Side per band).
pub struct MsEqStage {
    pub bands: [MsEqBand; MAX_MS_EQ_BANDS],
    mid_filters: [SmoothedBand; MAX_MS_EQ_BANDS],
    side_filters: [SmoothedBand; MAX_MS_EQ_BANDS],
    pub enabled: bool,
    sample_rate: f32,
}

impl MsEqStage {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            bands: [MsEqBand::flat(); MAX_MS_EQ_BANDS],
            mid_filters: std::array::from_fn(|_| SmoothedBand::new()),
            side_filters: std::array::from_fn(|_| SmoothedBand::new()),
            enabled: false,
            sample_rate,
        }
    }

    /// Update a single band's parameters and schedule coefficient smoothing.
    pub fn set_band(&mut self, index: usize, band: MsEqBand) {
        if index >= MAX_MS_EQ_BANDS {
            return;
        }
        self.bands[index] = band;
        self.recompute(index);
    }

    pub fn band(&self, index: usize) -> MsEqBand {
        if index < MAX_MS_EQ_BANDS {
            self.bands[index]
        } else {
            MsEqBand::flat()
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for i in 0..MAX_MS_EQ_BANDS {
            self.recompute(i);
        }
    }

    /// Apply M/S EQ bands from an `EqualizerState`.
    pub fn apply_from_state(&mut self, state: &crate::audio::equalizer::EqualizerState) {
        for (i, ms) in state.ms_eq_bands.iter().enumerate() {
            if i >= MAX_MS_EQ_BANDS {
                break;
            }
            self.bands[i] = MsEqBand {
                freq_hz: ms.frequency as f32,
                mid_gain_db: ms.mid_gain_db as f32,
                side_gain_db: ms.side_gain_db as f32,
                q: ms.bandwidth as f32,
                enabled: ms.enabled,
            };
            self.recompute(i);
        }
        self.enabled = state.ms_eq_enabled;
    }

    /// Process one stereo frame through the M/S EQ stage.
    ///
    /// Decodes L/R → M/S, applies per-band peaking filters independently on
    /// Mid and Side, then re-encodes back to L/R.
    #[inline(always)]
    pub fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.enabled {
            return (l, r);
        }

        let mut m = (l + r) * 0.5;
        let mut s = (l - r) * 0.5;

        for b in 0..MAX_MS_EQ_BANDS {
            if self.bands[b].enabled {
                let (nm, _) = self.mid_filters[b].process(m, m);
                let (_, ns) = self.side_filters[b].process(s, s);
                m = nm;
                s = ns;
            }
        }

        (m + s, m - s)
    }

    pub fn reset(&mut self) {
        for b in 0..MAX_MS_EQ_BANDS {
            self.mid_filters[b].reset();
            self.side_filters[b].reset();
        }
    }

    /// Fix Bug #10: Copy all settings from another MsEqStage.
    /// Copies band parameters, enabled flag, and target coefficients
    /// (snapping current to target to avoid smoothing ramps).
    pub fn copy_settings_from(&mut self, other: &MsEqStage) {
        self.enabled = other.enabled;
        self.sample_rate = other.sample_rate;
        for i in 0..MAX_MS_EQ_BANDS {
            self.bands[i] = other.bands[i];
            self.mid_filters[i].target[0].copy_coeffs_from(&other.mid_filters[i].target[0]);
            self.mid_filters[i].target[1].copy_coeffs_from(&other.mid_filters[i].target[1]);
            self.mid_filters[i].current[0].copy_coeffs_from(&other.mid_filters[i].target[0]);
            self.mid_filters[i].current[1].copy_coeffs_from(&other.mid_filters[i].target[1]);
            self.mid_filters[i].steps = 0;
            self.side_filters[i].target[0].copy_coeffs_from(&other.side_filters[i].target[0]);
            self.side_filters[i].target[1].copy_coeffs_from(&other.side_filters[i].target[1]);
            self.side_filters[i].current[0].copy_coeffs_from(&other.side_filters[i].target[0]);
            self.side_filters[i].current[1].copy_coeffs_from(&other.side_filters[i].target[1]);
            self.side_filters[i].steps = 0;
        }
    }

    fn recompute(&mut self, index: usize) {
        let b = &self.bands[index];
        if !b.enabled || (b.mid_gain_db.abs() < 0.001 && b.side_gain_db.abs() < 0.001) {
            let id = Biquad::identity();
            self.mid_filters[index].set_target(id);
            self.side_filters[index].set_target(id);
            return;
        }
        self.mid_filters[index].set_target(Biquad::peaking(
            b.freq_hz,
            b.mid_gain_db,
            b.q,
            self.sample_rate,
        ));
        self.side_filters[index].set_target(Biquad::peaking(
            b.freq_hz,
            b.side_gain_db,
            b.q,
            self.sample_rate,
        ));
    }
}
