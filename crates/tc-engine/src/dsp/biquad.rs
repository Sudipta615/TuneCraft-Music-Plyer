//! Biquad filter building blocks — coefficients, state, and smoothed variant.
//!
//! Implements Direct Form II Transposed (DFII-T) which has the best numerical
//! behaviour for audio-rate IIR filters at the cost of two state variables.

use crate::buffer::AudioFrame;
use std::f64::consts::PI;


/// Biquad filter coefficients (normalised, a0 = 1)
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    /// Identity / pass-through coefficients
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// Validate and clamp filter parameters to safe ranges.
    ///
    ///
    /// that would propagate through the entire DSP chain, producing
    /// silent or garbage output. Previously, `sample_rate = 0` caused
    /// division by zero producing Inf; `q = 0` caused division by zero;
    /// `freq >= sample_rate/2` produced meaningless/unstable filters.
    #[inline]
    fn validate_params(sample_rate: f64, freq: f64, q: f64) -> (f64, f64, f64) {
        // Ensure minimum sample rate to prevent division by zero
        let sr = if sample_rate <= 0.0 || !sample_rate.is_finite() {
            log::warn!("Biquad: invalid sample_rate {}, clamping to 44100", sample_rate);
            44100.0
        } else {
            sample_rate
        };
        // Clamp frequency below Nyquist (sample_rate * 0.499)
        let f = if freq <= 0.0 || !freq.is_finite() {
            log::warn!("Biquad: invalid frequency {}, clamping to 20", freq);
            20.0
        } else {
            freq.clamp(1.0, sr * 0.499)
        };
        // Clamp Q to a safe minimum
        let qv = if q <= 0.0 || !q.is_finite() {
            log::warn!("Biquad: invalid Q {}, clamping to 0.01", q);
            0.01
        } else {
            q
        };
        (sr, f, qv)
    }

    /// Second-order (biquad) low-pass filter

    pub fn lowpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Second-order (biquad) high-pass filter

    pub fn highpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Peaking EQ filter
    pub fn peaking(sample_rate: f64, freq: f64, gain_db: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// Low-shelf filter
    pub fn lowshelf(sample_rate: f64, freq: f64, gain_db: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0) / a0,
            a2: ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    /// High-shelf filter
    pub fn highshelf(sample_rate: f64, freq: f64, gain_db: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    /// Band-pass filter (constant skirt gain)
    pub fn bandpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Notch filter
    pub fn notch(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0;
        let a0 = 1.0 + alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// All-pass filter
    pub fn allpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (sample_rate, freq, q) = Self::validate_params(sample_rate, freq, q);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 - alpha;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 + alpha;
        let a0 = 1.0 + alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}


/// Supported biquad filter topologies
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Allpass,
    Peaking,
    Lowshelf,
    Highshelf,
}

impl FilterType {
    /// Compute coefficients for this filter type at the given parameters
    pub fn compute_coeffs(
        self,
        sample_rate: f64,
        freq: f64,
        gain_db: f64,
        q: f64,
    ) -> BiquadCoeffs {
        match self {
            Self::Lowpass => BiquadCoeffs::lowpass(sample_rate, freq, q),
            Self::Highpass => BiquadCoeffs::highpass(sample_rate, freq, q),
            Self::Bandpass => BiquadCoeffs::bandpass(sample_rate, freq, q),
            Self::Notch => BiquadCoeffs::notch(sample_rate, freq, q),
            Self::Allpass => BiquadCoeffs::allpass(sample_rate, freq, q),
            Self::Peaking => BiquadCoeffs::peaking(sample_rate, freq, gain_db, q),
            Self::Lowshelf => BiquadCoeffs::lowshelf(sample_rate, freq, gain_db, q),
            Self::Highshelf => BiquadCoeffs::highshelf(sample_rate, freq, gain_db, q),
        }
    }
}


/// Biquad filter state (Direct Form II Transposed)
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    pub z1: f64,
    pub z2: f64,
}

impl BiquadState {
    /// Process a single sample through the biquad filter
    #[inline]
    pub fn process(&mut self, sample: f64, coeffs: &BiquadCoeffs) -> f64 {
        let output = coeffs.b0 * sample + self.z1;
        self.z1 = crate::buffer::flush_denormal(coeffs.b1 * sample - coeffs.a1 * output + self.z2);
        self.z2 = crate::buffer::flush_denormal(coeffs.b2 * sample - coeffs.a2 * output);
        output
    }

    /// Reset filter state
    #[inline]
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}


/// A biquad filter that smoothly interpolates between coefficient sets,
/// avoiding zipper noise when parameters change during playback.
#[derive(Debug, Clone)]
pub struct SmoothedBiquad {
    /// Current (active) coefficients — updated per-sample during smoothing
    current: BiquadCoeffs,
    /// Target coefficients — set when the user changes a parameter
    target: BiquadCoeffs,
    /// Per-coefficient increment per sample during smoothing
    increment: BiquadCoeffs,
    /// Remaining smoothing steps
    remaining: u32,
    /// Number of smoothing steps (sample-rate-aware for ~1.5ms duration)
    smooth_steps: u32,
    /// Filter state (stereo: two channels)

    /// prevents mono audio support. For mono, only channel 0 is used.
    /// A dynamic Vec<BiquadState> would be needed for full N-channel support.
    states: [BiquadState; 2],
}

impl SmoothedBiquad {
    /// Create a new smoothed biquad initialised to identity (pass-through)
    pub fn new() -> Self {
        Self {
            current: BiquadCoeffs::IDENTITY,
            target: BiquadCoeffs::IDENTITY,
            increment: BiquadCoeffs::default(),
            remaining: 0,
            smooth_steps: 64, // default for 44.1kHz (~1.45ms)
            states: [BiquadState::default(); 2],
        }
    }

    /// Update the sample rate, recomputing smooth steps for ~1.5ms duration
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        // Target ~1.5ms smoothing duration regardless of sample rate
        self.smooth_steps = (sample_rate * 0.0015).max(8.0) as u32;
    }

    /// Set new target coefficients — begins smooth transition
    pub fn set_target(&mut self, coeffs: BiquadCoeffs) {
        let steps = self.smooth_steps as f64;
        self.target = coeffs;
        self.increment = BiquadCoeffs {
            b0: (coeffs.b0 - self.current.b0) / steps,
            b1: (coeffs.b1 - self.current.b1) / steps,
            b2: (coeffs.b2 - self.current.b2) / steps,
            a1: (coeffs.a1 - self.current.a1) / steps,
            a2: (coeffs.a2 - self.current.a2) / steps,
        };
        self.remaining = self.smooth_steps;
    }

    /// Process an audio frame through the filter (both channels)
    ///
    /// processing channel 0, avoiding out-of-bounds access on states[1].
    #[inline]
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        if frame.num_channels <= 1 {
            // Mono: only process channel 0
            frame.channels[0] = self.states[0].process(frame.channels[0], &self.current);
        } else {
            for (ch, state) in self.states.iter_mut().enumerate().take(frame.num_channels as usize) {
                frame.channels[ch] = state.process(frame.channels[ch], &self.current);
            }
        }
        self.advance_smoothing();
    }

    /// Process a single sample on a given channel
    #[inline]
    pub fn process_sample(&mut self, ch: usize, sample: f64) -> f64 {
        if ch < 2 {
            self.states[ch].process(sample, &self.current)
        } else {
            sample
        }
    }

    /// Advance coefficient interpolation by one sample
    #[inline]
    pub(crate) fn advance_smoothing(&mut self) {
        if self.remaining > 0 {
            self.current.b0 += self.increment.b0;
            self.current.b1 += self.increment.b1;
            self.current.b2 += self.increment.b2;
            self.current.a1 += self.increment.a1;
            self.current.a2 += self.increment.a2;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
    }

    /// Reset filter state (but not coefficients)
    pub fn reset(&mut self) {
        self.states[0].reset();
        self.states[1].reset();
    }

    /// Reset both state and coefficients to identity
    pub fn reset_all(&mut self) {
        self.reset();
        self.current = BiquadCoeffs::IDENTITY;
        self.target = BiquadCoeffs::IDENTITY;
        self.increment = BiquadCoeffs::default();
        self.remaining = 0;
    }
}

impl Default for SmoothedBiquad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_identity_passes_signal() {
        let coeffs = BiquadCoeffs::IDENTITY;
        let mut state = BiquadState::default();
        let input = 0.5;
        let output = state.process(input, &coeffs);
        assert_relative_eq!(output, input, epsilon = 1e-12);
    }

    #[test]
    fn test_lowpass_attenuates_high_freq() {
        let coeffs = BiquadCoeffs::lowpass(44100.0, 1000.0, 0.707);
        let mut state = BiquadState::default();
        // Feed a constant (DC) signal — after settling the output should approach the input
        let mut output = 0.0;
        for _ in 0..1000 {
            output = state.process(1.0, &coeffs);
        }
        // Lowpass at 1kHz should pass DC (output close to 1.0 after settling)
        assert!(output > 0.5, "DC should pass through lowpass, got {}", output);
    }

    #[test]
    fn test_smoothed_biquad_converges() {
        let mut bq = SmoothedBiquad::new();
        let target = BiquadCoeffs::lowpass(44100.0, 1000.0, 0.707);
        bq.set_target(target);
        // Process enough samples to finish smoothing
        for _ in 0..100 {
            let mut frame = AudioFrame::stereo(0.5, 0.5);
            bq.process_frame(&mut frame);
        }
        assert_eq!(bq.remaining, 0);
        assert_relative_eq!(bq.current.b0, target.b0, epsilon = 1e-12);
    }

    #[test]
    fn test_filter_type_dispatch() {
        let coeffs = FilterType::Lowpass.compute_coeffs(44100.0, 1000.0, 0.0, 0.707);
        assert!(coeffs.b0 != 0.0);

        let coeffs = FilterType::Peaking.compute_coeffs(44100.0, 1000.0, 3.0, 1.0);
        assert!(coeffs.b0 != 0.0);
    }
}

