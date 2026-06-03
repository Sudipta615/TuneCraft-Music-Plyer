//! Lookahead brick-wall limiter
//!
//! Prevents the output from exceeding a configurable ceiling by looking ahead
//! in the signal and applying smooth gain reduction. Supports soft clipping
//! as an optional safety mode.

use crate::buffer::{AudioFrame, MAX_CHANNELS};

/// Lookahead brick-wall limiter
pub struct LookaheadLimiter {
    /// Ceiling in linear amplitude
    ceiling_linear: f64,
    /// Attack time in seconds
    attack_secs: f64,
    /// Release time in seconds
    release_secs: f64,
    /// Lookahead time in seconds (stored for sample rate changes)
    lookahead_secs: f64,
    /// Lookahead delay in samples
    lookahead_samples: usize,
    /// Sample rate
    sample_rate: f64,
    /// Whether soft clipping is enabled
    soft_clip: bool,
    /// Whether the limiter is enabled
    enabled: bool,

    /// Delay line per channel — circular buffer
    delay_lines: [Vec<f64>; MAX_CHANNELS],
    /// Write position in the delay line
    delay_write_pos: usize,
    /// Current gain reduction (linear, 0.0–1.0)
    current_gain: f64,
    /// Attack coefficient (per sample)
    attack_coeff: f64,
    /// Release coefficient (per sample)
    release_coeff: f64,
}

impl LookaheadLimiter {
    /// Create a new limiter with full configuration
    pub fn new_with_params(
        sample_rate: f64,
        lookahead_ms: f64,
        attack_ms: f64,
        release_ms: f64,
        ceiling_db: f64,
        soft_clip: bool,
    ) -> Self {
        let lookahead_secs = lookahead_ms / 1000.0;
        let lookahead_samples = (lookahead_secs * sample_rate).ceil() as usize;
        let lookahead_samples = lookahead_samples.max(1);
        let attack_secs = attack_ms / 1000.0;
        let release_secs = release_ms / 1000.0;
        let ceiling_linear = 10.0_f64.powf(ceiling_db / 20.0);

        let attack_coeff = if attack_secs > 0.0 {
            (-1.0 / (attack_secs * sample_rate)).exp()
        } else {
            0.0
        };
        let release_coeff = if release_secs > 0.0 {
            (-1.0 / (release_secs * sample_rate)).exp()
        } else {
            0.0
        };

        Self {
            ceiling_linear,
            attack_secs,
            release_secs,
            lookahead_secs,
            lookahead_samples,
            sample_rate,
            soft_clip,
            enabled: true,
            delay_lines: [
                vec![0.0; lookahead_samples + 1],
                vec![0.0; lookahead_samples + 1],
            ],
            delay_write_pos: 0,
            current_gain: 1.0,
            attack_coeff,
            release_coeff,
        }
    }

    /// Create a new limiter with default settings
    pub fn new(sample_rate: f64) -> Self {
        Self::new_with_params(sample_rate, 5.0, 5.0, 50.0, -0.3, false)
    }

    /// Enable or disable the limiter
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set the ceiling in dB. Must be <= 0 dBFS; non-finite or positive values are clamped.
    pub fn set_ceiling_db(&mut self, ceiling_db: f64) {
        if !ceiling_db.is_finite() {
            log::warn!("LookaheadLimiter: non-finite ceiling_db {}; using -0.3", ceiling_db);
            self.ceiling_linear = 10.0_f64.powf(-0.3 / 20.0);
            return;
        }
        self.ceiling_linear = 10.0_f64.powf(ceiling_db.min(0.0) / 20.0);
    }

    /// Set the threshold in dB (alias for ceiling)
    pub fn set_threshold_db(&mut self, threshold_db: f64) {
        self.set_ceiling_db(threshold_db);
    }

    /// Set attack time in seconds. Non-positive/non-finite values are clamped to 0.1 ms.
    pub fn set_attack(&mut self, secs: f64) {
        if !secs.is_finite() || secs <= 0.0 {
            log::warn!("LookaheadLimiter: invalid attack {}; clamping to 0.0001s", secs);
        }
        self.attack_secs = secs.max(0.0001);
        self.attack_coeff = (-1.0 / (self.attack_secs * self.sample_rate)).exp();
    }

    /// Set release time in seconds. Non-positive/non-finite values are clamped to 1 ms.
    pub fn set_release(&mut self, secs: f64) {
        if !secs.is_finite() || secs <= 0.0 {
            log::warn!("LookaheadLimiter: invalid release {}; clamping to 0.001s", secs);
        }
        self.release_secs = secs.max(0.001);
        self.release_coeff = (-1.0 / (self.release_secs * self.sample_rate)).exp();
    }

    /// Process a stereo sample pair
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled {
            return (left, right);
        }

        let input_peak = left.abs().max(right.abs());

        let desired_gain = if input_peak > self.ceiling_linear {
            self.ceiling_linear / input_peak
        } else {
            1.0
        };

        if desired_gain < self.current_gain {
            // Attack
            self.current_gain =
                desired_gain + (self.current_gain - desired_gain) * self.attack_coeff;
        } else {
            self.current_gain =
                desired_gain + (self.current_gain - desired_gain) * self.release_coeff;
        }

        // giving exactly lookahead_samples of delay
        let delay_len = self.delay_lines[0].len();
        let read_pos = (self.delay_write_pos + delay_len - self.lookahead_samples) % delay_len;

        let delayed_left = self.delay_lines[0][read_pos];
        let delayed_right = self.delay_lines[1][read_pos];

        self.delay_lines[0][self.delay_write_pos] = left;
        self.delay_lines[1][self.delay_write_pos] = right;

        self.delay_write_pos = (self.delay_write_pos + 1) % delay_len;

        let mut out_l = delayed_left * self.current_gain;
        let mut out_r = delayed_right * self.current_gain;

        if self.soft_clip {
            out_l = self.soft_clip_sample(out_l);
            out_r = self.soft_clip_sample(out_r);
        }

        (out_l, out_r)
    }

    /// Process an audio frame (alternative API)
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        let (l, r) = self.process(frame.channels[0], frame.channels[1]);
        frame.channels[0] = l;
        frame.channels[1] = r;
    }

    /// Soft clip using tanh saturation
    #[inline]
    fn soft_clip_sample(&self, sample: f64) -> f64 {
        if sample.abs() <= self.ceiling_linear {
            return sample;
        }
        let normalized = sample / self.ceiling_linear;
        normalized.tanh() * self.ceiling_linear
    }

    /// Get the current gain reduction in dB (always <= 0)
    pub fn gain_reduction_db(&self) -> f64 {
        20.0 * self.current_gain.log10().max(-60.0)
    }

    /// Get the current gain (linear)
    pub fn current_gain(&self) -> f64 {
        self.current_gain
    }

    /// Reset the limiter state
    pub fn reset(&mut self) {
        for line in &mut self.delay_lines {
            line.fill(0.0);
        }
        self.delay_write_pos = 0;
        self.current_gain = 1.0;
    }

    /// Update sample rate (rebuilds delay lines)
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let lookahead_samples = (self.lookahead_secs * sample_rate).ceil() as usize;
        self.lookahead_samples = lookahead_samples.max(1);
        for line in &mut self.delay_lines {
            line.resize(self.lookahead_samples + 1, 0.0);
        }
        self.delay_write_pos = 0;
        self.attack_coeff = (-1.0 / (self.attack_secs * self.sample_rate)).exp();
        self.release_coeff = (-1.0 / (self.release_secs * self.sample_rate)).exp();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_prevents_clipping() {
        let mut limiter = LookaheadLimiter::new(44100.0);
        for _ in 0..1000 {
            let (l, r) = limiter.process(1.5, 1.5);
            // After delay fills, output should be controlled
            assert!(l.abs() <= 1.5);
            assert!(r.abs() <= 1.5);
        }
    }

    #[test]
    fn test_limiter_passes_quiet_signal() {
        let mut limiter = LookaheadLimiter::new(44100.0);
        for _ in 0..1000 {
            let (_l, _r) = limiter.process(0.1, 0.1);
        }
        let (l, r) = limiter.process(0.1, 0.1);
        assert!((l - 0.1).abs() < 0.05, "Quiet signal should pass through");
        assert!((r - 0.1).abs() < 0.05, "Quiet signal should pass through");
    }

    #[test]
    fn test_limiter_with_params() {
        let mut limiter = LookaheadLimiter::new_with_params(44100.0, 5.0, 5.0, 50.0, -0.3, true);
        limiter.set_enabled(true);
        for _ in 0..10000 {
            let (l, r) = limiter.process(2.0, 2.0);
            assert!(l.is_finite());
            assert!(r.is_finite());
        }
    }

    #[test]
    fn test_limiter_disabled_passthrough() {
        let mut limiter = LookaheadLimiter::new(44100.0);
        limiter.set_enabled(false);
        let (l, r) = limiter.process(0.5, 0.5);
        assert!((l - 0.5).abs() < 1e-10);
        assert!((r - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_limiter_reset() {
        let mut limiter = LookaheadLimiter::new(44100.0);
        limiter.set_ceiling_db(-1.0);
        for _ in 0..100 {
            limiter.process(1.0, 1.0);
        }
        limiter.reset();
        assert!((limiter.current_gain() - 1.0).abs() < 1e-10);
    }
}

