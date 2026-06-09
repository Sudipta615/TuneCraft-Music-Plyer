use tc_config::CrossfeedProfile;

use super::biquad::{BiquadCoeffs, BiquadState};

/// Headphone Crossfeed DSP node
/// Reduces listening fatigue on hard-panned stereo tracks by blending
/// low-pass filtered audio from the opposite channel.
pub struct Crossfeed {
    enabled: bool,
    level: f32,
    profile: CrossfeedProfile,
    sample_rate: f32,
    /// Biquad coefficients for the crossfeed low-pass filter
    coeffs: BiquadCoeffs,
    /// State for the Left-to-Right crossfeed filter
    state_lr: BiquadState,
    /// State for the Right-to-Left crossfeed filter
    state_rl: BiquadState,
}

impl Crossfeed {
    pub fn new(sample_rate: f32) -> Self {
        let mut cf = Self {
            enabled: false,
            level: 1.0,
            profile: CrossfeedProfile::Bauer,
            sample_rate,
            coeffs: BiquadCoeffs::IDENTITY,
            state_lr: BiquadState::default(),
            state_rl: BiquadState::default(),
        };
        cf.update_coeffs();
        cf
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.reset();
            }
        }
    }

    pub fn set_profile(&mut self, profile: CrossfeedProfile) {
        if self.profile != profile {
            self.profile = profile;
            self.update_coeffs();
        }
    }

    pub fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.update_coeffs();
    }

    fn update_coeffs(&mut self) {
        let (freq, q) = match self.profile {
            CrossfeedProfile::Bauer => (700.0, 0.5),
            CrossfeedProfile::ChuMoy => (700.0, 0.707), // Standard Butterworth
            CrossfeedProfile::Jmeier => (600.0, 0.6),
        };
        self.coeffs = BiquadCoeffs::lowpass(self.sample_rate, freq, q);
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled || self.level <= 0.001 {
            return (left, right);
        }

        // Apply low-pass filter to each channel to create the crossfeed signal
        let cross_to_right = self.state_lr.process(left, &self.coeffs);
        let cross_to_left = self.state_rl.process(right, &self.coeffs);

        // Blend the crossfeed signals
        // We use a slight attenuation to the direct signal to maintain overall loudness
        let direct_level = 1.0 - (0.2 * self.level);
        let cross_level = 0.5 * self.level;

        let out_l = (left * direct_level) + (cross_to_left * cross_level);
        let out_r = (right * direct_level) + (cross_to_right * cross_level);

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.state_lr.reset();
        self.state_rl.reset();
    }
}
