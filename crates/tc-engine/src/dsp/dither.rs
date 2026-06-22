//! Dithering for bit depth reduction
//!
//! When reducing bit depth (e.g. 64-bit float → 16-bit integer for output),
//! quantization error introduces harmonic distortion. Dither decorrelates
//! this error from the signal, replacing distortion with benign noise.
//!
//! **TPDF (Triangular Probability Density Function)** dither is the recommended
//! default for 16-bit output. It eliminates quantization distortion entirely
//! at the cost of +4.77 dB of noise, which is inaudible at 16-bit depth.
//!
//! Noise-shaped TPDF further pushes dither noise into less audible frequency
//! regions using a first-order high-pass shaping filter.

/// Dither type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DitherType {
    /// No dithering (fastest, but introduces quantization distortion)
    None,
    /// Rectangular PDF dither: one random sample added per channel
    Rectangular,
    /// Triangular PDF dither: sum of two rectangular samples (recommended)
    Triangular,
    /// TPDF with first-order noise shaping
    NoiseShaped,
}

/// Dither processor
///
/// Applies dithering before quantization to the target bit depth.
/// At 32-bit and above, dithering is a no-op (quantization error is
/// below the noise floor of any real DAC).
#[derive(Debug, Clone)]
pub struct Dither {
    dither_type: DitherType,
    bit_depth: u32,
    /// Previous quantization error for noise shaping (left channel)
    shape_left: f32,
    /// Previous quantization error for noise shaping (right channel)
    shape_right: f32,
    /// PRNG state for left channel (xorshift64)
    rng_state_left: u64,
    /// PRNG state for right channel (xorshift64)
    rng_state_right: u64,
    enabled: bool,
}

impl Dither {
    /// Create a new dither processor.
    ///
    /// # Arguments
    /// * `dither_type` - The dither algorithm to use
    /// * `bit_depth` - Target bit depth (1–32). Dither is a no-op at ≥ 32.
    pub fn new(dither_type: DitherType, bit_depth: u32) -> Self {
        Self {
            dither_type,
            bit_depth: bit_depth.clamp(1, 32),
            shape_left: 0.0,
            shape_right: 0.0,
            rng_state_left: Self::random_seed(),
            rng_state_right: Self::random_seed().wrapping_add(0xDEADBEEF_12345678),
            enabled: dither_type != DitherType::None,
        }
    }

    fn random_seed() -> u64 {
        // L11: Add a per-call counter so two Dither instances created in the
        // same nanosecond get distinct seeds, preventing correlated dither noise.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let instance_id = COUNTER.fetch_add(1, Ordering::Relaxed);
        use std::time::SystemTime;
        let ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x12345678_9ABCDEF0);
        // L11: Mix in the per-instance counter so two Dither instances created
        // within the same nanosecond produce different seeds.
        let seed = ns
            .wrapping_add(instance_id.wrapping_mul(0x9E3779B97F4A7C15))
            .wrapping_mul(0x5851F42D4C957F2D);

        // if the seed comes out 0, reinitialize with a non-zero fallback.
        if seed == 0 {
            0x12345678_9ABCDEF0
        } else {
            seed
        }
    }

    /// xorshift64 PRNG — fast, one multiplication, three shifts.
    /// Quality is more than sufficient for dither noise generation.
    ///
    /// if state becomes 0, reinitialize with a non-zero seed.
    #[inline]
    fn next_random(state: &mut u64) -> u64 {
        if *state == 0 {
            *state = 0x12345678_9ABCDEF0;
        }
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// Generate a random value in [-1.0, 1.0] using the PRNG.
    #[inline]
    fn next_random_f32(state: &mut u64) -> f32 {
        let bits = Self::next_random(state);
        // Map u64 → i64 → f32 in [-1, 1]
        (bits as i64 as f32) / (i64::MIN as f32).abs()
    }

    /// Process a stereo sample with dithering and quantization.
    ///
    /// Returns the dithered and quantized sample pair, clamped to [-1.0, 1.0].
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled || self.bit_depth >= 32 {
            return (left, right);
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f32;
        let half_lsb = 0.5 / quant_steps_f;

        let (dithered_l, dithered_r) = match self.dither_type {
            DitherType::None => (left, right),

            DitherType::Rectangular => {
                let noise_l = Self::next_random_f32(&mut self.rng_state_left) * half_lsb;
                let noise_r = Self::next_random_f32(&mut self.rng_state_right) * half_lsb;
                (left + noise_l, right + noise_r)
            },

            DitherType::Triangular => {
                // TPDF: sum of two independent rectangular noise sources
                // produces a triangular distribution centered on zero.

                // that introduced a DC bias of -0.5 LSB. The formula
                // `(rng1 + rng2) * half_lsb` produces a triangular distribution
                // with mean 0, which is the correct TPDF dither.
                let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;
                (left + noise_l, right + noise_r)
            },

            DitherType::NoiseShaped => {
                // TPDF with first-order high-pass noise shaping
                // Feeds quantization error back with a negative coefficient
                // to push dither energy into less audible high frequencies.

                let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;
                let shaped_l = left + noise_l - self.shape_left * 0.5;
                let shaped_r = right + noise_r - self.shape_right * 0.5;

                // Quantize to compute the new error
                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;

                self.shape_left = q_l - shaped_l + self.shape_left * 0.5;
                self.shape_right = q_r - shaped_r + self.shape_right * 0.5;

                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            },
        };

        // Quantize
        let ql = (dithered_l * quant_steps_f).round() / quant_steps_f;
        let qr = (dithered_r * quant_steps_f).round() / quant_steps_f;

        (ql.clamp(-1.0, 1.0), qr.clamp(-1.0, 1.0))
    }

    /// Enable or disable dithering
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether dithering is currently enabled (the user's actual setting).
    /// v3.1.2: added so `DspPipeline::apply_performance_mode` can restore
    /// the preference after a LowPower round-trip without having mutated
    /// it directly.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Reset the dither state (PRNG seed and shaping memory)
    pub fn reset(&mut self) {
        self.shape_left = 0.0;
        self.shape_right = 0.0;
        self.rng_state_left = Self::random_seed();
        self.rng_state_right = Self::random_seed().wrapping_add(0xDEADBEEF_12345678);
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dither_output_bounded() {
        let mut dither = Dither::new(DitherType::Triangular, 16);
        for _ in 0..10000 {
            let (l, r) = dither.process(0.5, -0.5);
            assert!(
                l.abs() <= 1.0,
                "Dithered output should be bounded, got {}",
                l
            );
            assert!(
                r.abs() <= 1.0,
                "Dithered output should be bounded, got {}",
                r
            );
        }
    }

    #[test]
    fn test_no_dither_at_high_bit_depth() {
        let mut dither = Dither::new(DitherType::Triangular, 32);
        let (l, r) = dither.process(0.5, 0.5);
        assert!((l - 0.5).abs() < 1e-5, "32-bit should pass through");
        assert!((r - 0.5).abs() < 1e-5, "32-bit should pass through");
    }

    #[test]
    fn test_none_dither_is_quantize_only() {
        let mut dither = Dither::new(DitherType::None, 16);
        // With no dither, the output should be quantized but not have noise added
        let (l, r) = dither.process(0.5, 0.5);
        // 0.5 should be exactly representable at 16-bit
        assert!((l - 0.5).abs() < 1e-4, "0.5 should quantize cleanly");
        assert!((r - 0.5).abs() < 1e-4, "0.5 should quantize cleanly");
    }

    #[test]
    fn test_tpdf_statistics() {
        // TPDF dither should produce a triangular distribution of noise.
        // The mean should be approximately zero.
        let mut dither = Dither::new(DitherType::Triangular, 16);
        let n = 100000;
        let mut sum = 0.0;
        for _ in 0..n {
            let (l, _) = dither.process(0.0, 0.0);
            sum += l;
        }
        let mean = sum / n as f32;
        assert!(
            mean.abs() < 0.001,
            "TPDF dither mean should be near zero, got {}",
            mean
        );
    }

    #[test]
    fn test_noise_shaped_dither() {
        let mut dither = Dither::new(DitherType::NoiseShaped, 16);
        for _ in 0..10000 {
            let (l, r) = dither.process(0.3, -0.3);
            assert!(l.abs() <= 1.0, "Noise-shaped output should be bounded");
            assert!(r.abs() <= 1.0, "Noise-shaped output should be bounded");
            assert!(l.is_finite(), "Output should be finite");
            assert!(r.is_finite(), "Output should be finite");
        }
    }

    #[test]
    fn test_rectangular_dither() {
        let mut dither = Dither::new(DitherType::Rectangular, 16);
        for _ in 0..10000 {
            let (l, r) = dither.process(0.5, 0.5);
            assert!(
                l.abs() <= 1.0,
                "Rectangular dither output should be bounded"
            );
            assert!(
                r.abs() <= 1.0,
                "Rectangular dither output should be bounded"
            );
        }
    }

    #[test]
    fn test_clamping_at_boundary() {
        let mut dither = Dither::new(DitherType::Triangular, 16);
        // Input near 1.0 should be clamped, not wrap
        for _ in 0..1000 {
            let (l, _r) = dither.process(0.999, 0.999);
            assert!(l <= 1.0, "Output should never exceed 1.0, got {}", l);
            assert!(l >= -1.0, "Output should never go below -1.0");
        }
    }
}
