use tc_config::CrossfeedProfile;

use super::biquad::{BiquadCoeffs, BiquadState};

/// Headphone Crossfeed DSP node
/// Reduces listening fatigue on hard-panned stereo tracks by blending
/// low-pass filtered and delayed audio from the opposite channel.
pub struct Crossfeed {
    enabled: bool,
    level: f32,
    profile: CrossfeedProfile,
    custom_freq: f32,
    custom_q: f32,
    custom_delay_ms: f32,
    sample_rate: f32,
    
    /// Biquad coefficients for the crossfeed low-pass filter
    coeffs: BiquadCoeffs,
    /// State for the Left-to-Right crossfeed filter
    state_lr: BiquadState,
    /// State for the Right-to-Left crossfeed filter
    state_rl: BiquadState,
    
    /// Delay ring buffer for Left-to-Right
    delay_lr: Vec<f32>,
    /// Delay ring buffer for Right-to-Left
    delay_rl: Vec<f32>,
    delay_pos: usize,
    delay_len: usize,
}

impl Crossfeed {
    pub fn new(sample_rate: f32) -> Self {
        let mut cf = Self {
            enabled: false,
            level: 1.0,
            profile: CrossfeedProfile::Bauer,
            custom_freq: 700.0,
            custom_q: 0.707,
            custom_delay_ms: 0.3,
            sample_rate,
            coeffs: BiquadCoeffs::IDENTITY,
            state_lr: BiquadState::default(),
            state_rl: BiquadState::default(),
            delay_lr: Vec::new(),
            delay_rl: Vec::new(),
            delay_pos: 0,
            delay_len: 0,
        };
        cf.update_params();
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
            self.update_params();
        }
    }

    pub fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
    }
    
    pub fn set_custom_params(&mut self, freq: f32, q: f32, delay_ms: f32) {
        self.custom_freq = freq;
        self.custom_q = q;
        self.custom_delay_ms = delay_ms;
        if self.profile == CrossfeedProfile::Custom {
            self.update_params();
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() > 0.01 {
            self.sample_rate = sample_rate;
            self.update_params();
            self.reset();
        }
    }

    fn update_params(&mut self) {
        let (freq, q, delay_ms) = match self.profile {
            CrossfeedProfile::Bauer => (700.0, 0.5, 0.3),
            CrossfeedProfile::ChuMoy => (700.0, 0.707, 0.25),
            CrossfeedProfile::Jmeier => (600.0, 0.6, 0.35),
            CrossfeedProfile::Custom => (self.custom_freq, self.custom_q, self.custom_delay_ms),
        };
        
        self.coeffs = BiquadCoeffs::lowpass(self.sample_rate, freq, q);
        
        // Calculate delay in samples
        self.delay_len = ((delay_ms / 1000.0) * self.sample_rate) as usize;
        if self.delay_len == 0 {
            self.delay_len = 1; // Minimum 1 sample to avoid 0-capacity ring buffer logic
        }
        
        if self.delay_lr.len() != self.delay_len {
            self.delay_lr = vec![0.0; self.delay_len];
            self.delay_rl = vec![0.0; self.delay_len];
            self.delay_pos = 0;
        }
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled || self.level <= 0.001 {
            return (left, right);
        }

        // Apply low-pass filter to each channel to create the crossfeed signal
        let filtered_to_right = self.state_lr.process(left, &self.coeffs);
        let filtered_to_left = self.state_rl.process(right, &self.coeffs);

        // Process delay line
        let cross_to_right = self.delay_lr[self.delay_pos];
        let cross_to_left = self.delay_rl[self.delay_pos];
        
        self.delay_lr[self.delay_pos] = filtered_to_right;
        self.delay_rl[self.delay_pos] = filtered_to_left;
        
        self.delay_pos += 1;
        if self.delay_pos >= self.delay_len {
            self.delay_pos = 0;
        }

        // Blend the crossfeed signals
        let direct_level = 1.0 - (0.2 * self.level);
        let cross_level = 0.5 * self.level;

        let out_l = (left * direct_level) + (cross_to_left * cross_level);
        let out_r = (right * direct_level) + (cross_to_right * cross_level);

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.state_lr.reset();
        self.state_rl.reset();
        self.delay_lr.fill(0.0);
        self.delay_rl.fill(0.0);
        self.delay_pos = 0;
    }
}
