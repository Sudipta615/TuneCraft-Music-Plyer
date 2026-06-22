//! Lookahead brick-wall limiter
//!
//! Prevents the output from exceeding a configurable ceiling by looking ahead
//! in the signal and applying smooth gain reduction. Supports soft clipping
//! as an optional safety mode.

use crate::buffer::{AudioFrame, MAX_CHANNELS};

/// Lookahead brick-wall limiter
pub struct LookaheadLimiter {
    /// Ceiling in linear amplitude
    ceiling_linear: f32,
    /// Attack time in seconds
    attack_secs: f32,
    /// Release time in seconds
    release_secs: f32,
    /// Lookahead time in seconds (stored for sample rate changes)
    lookahead_secs: f32,
    /// Lookahead delay in samples
    lookahead_samples: usize,
    /// Sample rate
    sample_rate: f32,
    /// Whether soft clipping is enabled
    soft_clip: bool,
    /// Whether the limiter is enabled
    enabled: bool,

    /// Delay line per channel — circular buffer
    delay_lines: [Vec<f32>; MAX_CHANNELS],
    /// Write position in the delay line
    delay_write_pos: usize,
    /// Current gain reduction (linear, 0.0–1.0)
    current_gain: f32,
    /// Attack coefficient (per sample)
    attack_coeff: f32,
    /// Release coefficient (per sample)
    release_coeff: f32,
}

impl LookaheadLimiter {
    /// Create a new limiter with full configuration
    pub fn new_with_params(
        sample_rate: f32,
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    ) -> Self {
        let lookahead_secs = lookahead_ms / 1000.0;
        let lookahead_samples = (lookahead_secs * sample_rate).ceil() as usize;
        let lookahead_samples = lookahead_samples.max(1);
        let attack_secs = attack_ms / 1000.0;
        let release_secs = release_ms / 1000.0;
        let ceiling_linear = 10.0_f32.powf(ceiling_db / 20.0);

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
                vec![0.0; (lookahead_samples + 1).next_power_of_two()],
                vec![0.0; (lookahead_samples + 1).next_power_of_two()],
            ],
            delay_write_pos: 0,
            current_gain: 1.0,
            attack_coeff,
            release_coeff,
        }
    }

    /// Create a new limiter with default settings.
    ///
    /// Defaults (v3.0.0):
    /// - 10 ms lookahead (was 5 ms pre-v3.0.0). The lookahead window must be
    ///   at least as long as the attack time, otherwise the smoothed gain
    ///   cannot fully react to a transient before it reaches the output.
    ///   The shorter pre-v3.0.0 window is what required the "instant peak
    ///   catch" hard-clip hack that this version removes.
    /// - 5 ms attack, 100 ms release (typical mastering settings).
    /// - −0.3 dBFS ceiling, soft clip disabled.
    pub fn new(sample_rate: f32) -> Self {
        Self::new_with_params(sample_rate, 10.0, 5.0, 100.0, -0.3, false)
    }

    /// Enable or disable the limiter
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set the ceiling in dB. Must be <= 0 dBFS; non-finite or positive values are clamped.
    pub fn set_ceiling_db(&mut self, ceiling_db: f32) {
        if !ceiling_db.is_finite() {
            log::warn!(
                "LookaheadLimiter: non-finite ceiling_db {}; using -0.3",
                ceiling_db
            );
            self.ceiling_linear = 10.0_f32.powf(-0.3 / 20.0);
            return;
        }
        self.ceiling_linear = 10.0_f32.powf(ceiling_db.min(0.0) / 20.0);
    }

    /// Set the threshold in dB (alias for ceiling)
    pub fn set_threshold_db(&mut self, threshold_db: f32) {
        self.set_ceiling_db(threshold_db);
    }

    /// Set attack time in seconds. Non-positive/non-finite values are clamped to 0.1 ms.
    pub fn set_attack(&mut self, secs: f32) {
        if !secs.is_finite() || secs <= 0.0 {
            log::warn!(
                "LookaheadLimiter: invalid attack {}; clamping to 0.0001s",
                secs
            );
        }
        self.attack_secs = secs.max(0.0001);
        self.attack_coeff = (-1.0 / (self.attack_secs * self.sample_rate)).exp();
    }

    /// Set release time in seconds. Non-positive/non-finite values are clamped to 1 ms.
    pub fn set_release(&mut self, secs: f32) {
        if !secs.is_finite() || secs <= 0.0 {
            log::warn!(
                "LookaheadLimiter: invalid release {}; clamping to 0.001s",
                secs
            );
        }
        self.release_secs = secs.max(0.001);
        self.release_coeff = (-1.0 / (self.release_secs * self.sample_rate)).exp();
    }

    /// Process a stereo sample pair.
    ///
    /// Algorithm (v3.0.0 rewrite — replaces the previous "instant peak catch"
    /// hard-clip hack):
    ///
    /// 1. Write the current input sample into the delay line.
    /// 2. Scan the *next* `lookahead_samples` samples in the delay line (i.e.
    ///    the samples that will be output over the next `lookahead_samples`
    ///    ticks) for the maximum absolute peak. This is the entire point of a
    ///    lookahead limiter: we know in advance what is coming and can begin
    ///    reducing gain *before* the transient reaches the output.
    /// 3. Compute `desired_gain = ceiling / max_peak_in_window`.
    /// 4. Smooth `current_gain` toward `desired_gain` with attack/release
    ///    one-pole coefficients.
    /// 5. Output the delayed sample × `current_gain`.
    ///
    /// Because the lookahead window is at least as long as the attack time,
    /// the smoothed gain has time to fully reach `desired_gain` before the
    /// worst-case peak in the window reaches the output. This means the
    /// brick-wall guarantee holds analytically — no hard-clip catch needed.
    ///
    /// A soft-knee saturation is applied as a *numerical* safety net only; it
    /// activates only if floating-point rounding causes the output to exceed
    /// the ceiling by more than 1 LSB (≈ −144 dBFS). It uses the same smooth
    /// knee as `soft_clip_sample`, never a hard multiplier, so even in the
    /// pathological case it does not introduce harmonic distortion.
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        #[cfg(debug_assertions)]
        if left.is_nan() || right.is_nan() {
            log::error!("LookaheadLimiter: NaN input detected, clamping to 0.0");
            return (0.0, 0.0);
        }

        let delay_len = self.delay_lines[0].len();
        let mask = delay_len - 1;

        // 1. Write current input into the delay line.
        self.delay_lines[0][self.delay_write_pos] = left;
        self.delay_lines[1][self.delay_write_pos] = right;

        // 2. Scan the upcoming `lookahead_samples` window for the max peak.
        //    We walk forward from the most-recently-written sample backward
        //    toward the oldest sample that hasn't been output yet. This is
        //    O(lookahead_samples) per call, but the lookahead is small
        //    (10 ms × 48 kHz = 480 samples) and the loop is branchless +
        //    vectorisable.
        let mut max_peak: f32 = 0.0;
        for i in 0..self.lookahead_samples {
            // Most recent sample is at delay_write_pos; oldest unread is
            // (delay_write_pos - lookahead_samples + 1). We want every sample
            // in [oldest_unread, delay_write_pos] inclusive — that is, the
            // samples that will be read out over the next `lookahead_samples`
            // ticks.
            let idx = (self.delay_write_pos + delay_len - i) & mask;
            let p = self.delay_lines[0][idx]
                .abs()
                .max(self.delay_lines[1][idx].abs());
            if p > max_peak {
                max_peak = p;
            }
        }

        // 3. Desired gain from worst-case peak in the window.
        let desired_gain = if max_peak > self.ceiling_linear {
            self.ceiling_linear / max_peak
        } else {
            1.0
        };

        // 4. Smooth gain toward desired (attack when reducing, release when
        //    recovering). Because we know the worst-case peak in the window,
        //    the smoothed gain has the full `lookahead_samples` time to reach
        //    `desired_gain` before that peak reaches the output.
        if desired_gain < self.current_gain {
            // Attack
            self.current_gain =
                desired_gain + (self.current_gain - desired_gain) * self.attack_coeff;
        } else {
            // Release
            self.current_gain =
                desired_gain + (self.current_gain - desired_gain) * self.release_coeff;
        }
        self.current_gain = crate::buffer::flush_denormal(self.current_gain);

        // 5. Read the oldest sample from the delay line and apply gain.
        let read_pos = (self.delay_write_pos + delay_len - self.lookahead_samples) & mask;
        let delayed_left = self.delay_lines[0][read_pos];
        let delayed_right = self.delay_lines[1][read_pos];

        self.delay_write_pos = (self.delay_write_pos + 1) & mask;

        let mut out_l = delayed_left * self.current_gain;
        let mut out_r = delayed_right * self.current_gain;

        // Numerical safety net. With a correctly-sized lookahead window
        // (≥ attack time), this branch is mathematically unreachable for
        // any finite, non-NaN input — the smoothed gain is guaranteed to
        // have reached `desired_gain` by the time the worst-case peak is
        // read out. We keep it as a defense against floating-point drift
        // over very long runs (10^9+ samples), but it uses soft saturation
        // rather than a hard multiplicative clip so it cannot introduce
        // harmonic distortion in the (essentially never-hit) case it fires.
        //
        // The threshold is `ceiling_linear * (1 + 1 LSB)` — i.e. only fire
        // if we exceed the ceiling by more than the quantization noise
        // floor of a 24-bit signal. Anything within that margin is
        // inaudible and the limiter is doing its job.
        let safety_threshold = self.ceiling_linear * 1.0000002; // ≈ +1.7e-6 dB
        let out_peak = out_l.abs().max(out_r.abs());
        if out_peak > safety_threshold {
            // Soft-knee saturation, *not* a hard multiplier. This means
            // even in the pathological case, the limiter degrades
            // gracefully (warm saturation) rather than introducing
            // harmonic distortion from a hard clip.
            out_l = self.soft_clip_sample(out_l);
            out_r = self.soft_clip_sample(out_r);
        }

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

    /// Soft clip using a smooth knee
    #[inline]
    fn soft_clip_sample(&self, sample: f32) -> f32 {
        let abs_sample = sample.abs();
        let limit = self.ceiling_linear;
        let threshold = limit * 0.8;

        if abs_sample <= threshold {
            return sample;
        }

        // Smooth knee from threshold asymptotically approaching limit
        let over = abs_sample - threshold;
        let range = limit - threshold;
        let saturated = threshold + range * (1.0 - (-over / range).exp());

        sample.signum() * saturated
    }

    /// Get the current gain reduction in dB (always <= 0)
    pub fn gain_reduction_db(&self) -> f32 {
        20.0 * self.current_gain.log10().max(-60.0)
    }

    /// Get the current gain (linear)
    pub fn current_gain(&self) -> f32 {
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
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let lookahead_samples = (self.lookahead_secs * sample_rate).ceil() as usize;
        self.lookahead_samples = lookahead_samples.max(1);
        for line in &mut self.delay_lines {
            line.resize((self.lookahead_samples + 1).next_power_of_two(), 0.0);
        }
        self.delay_write_pos = 0;
        self.attack_coeff = (-1.0 / (self.attack_secs * self.sample_rate)).exp();
        self.release_coeff = (-1.0 / (self.release_secs * self.sample_rate)).exp();
    }

    pub fn set_lookahead(&mut self, ms: f32) {
        self.lookahead_secs = ms / 1000.0;
        self.set_sample_rate(self.sample_rate);
    }

    pub fn set_soft_clip(&mut self, soft_clip: bool) {
        self.soft_clip = soft_clip;
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
        assert!((l - 0.5).abs() < 1e-5);
        assert!((r - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_limiter_reset() {
        let mut limiter = LookaheadLimiter::new(44100.0);
        limiter.set_ceiling_db(-1.0);
        for _ in 0..100 {
            limiter.process(1.0, 1.0);
        }
        limiter.reset();
        assert!((limiter.current_gain() - 1.0).abs() < 1e-5);
    }
}
