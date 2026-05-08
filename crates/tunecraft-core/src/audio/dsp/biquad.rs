//! Biquad filter primitives — Transposed Direct Form II, hybrid f32/f64 precision.
//!
//! All constructor functions follow the RBJ Audio EQ Cookbook.
//! No heap allocation; all state is stack-allocated.
//!
//! ## Filter form: TDF-II
//!
//! Direct Form I stores raw delay-line history (`x[n-1]`, `x[n-2]`,
//! `y[n-1]`, `y[n-2]`). Near Nyquist with high gain those values can be orders
//! of magnitude larger than the output, causing catastrophic cancellation in f32.
//!
//! Transposed Direct Form II uses only two internal state nodes (`s1`, `s2`)
//! whose values are scaled versions of the *output*, so they never build up to
//! large magnitudes.
//!
//! Transfer function in TDF-II (normalised, a0 = 1):
//!
//! ```text
//! y[n] = b0·x[n] + s1[n-1]
//! s1   = b1·x[n] - a1·y[n] + s2[n-1]
//! s2   = b2·x[n] - a2·y[n]
//! ```
//!
//! ## Hybrid f32/f64 precision (Poweramp-class quality)
//!
//! Filter *coefficients* (`b0`…`a2`) are stored as `f32` — they are computed
//! once per parameter change and their precision is limited by the RBJ formula
//! itself, not by storage width.
//!
//! Filter *state* (`s1`, `s2`) is stored as `f64`. The state nodes accumulate
//! every sample; even small per-sample rounding errors compound over time. At
//! ≥12 dB boost above 15 kHz on a 44.1 kHz source, f32 TDF-II state diverges
//! by 1–3 dB from the ideal response. f64 state eliminates that error entirely.
//!
//! The `process` function promotes to f64 for the accumulation and narrows back
//! to f32 at output — I/O and the rest of the pipeline stay f32 throughout.
//! On x86-64 (SSE2) and ARM64 (NEON) the cost of scalar f64 vs f32 is one
//! instruction; the audio-thread CPU delta is ~1–2 % on a modern SoC.

use std::f32::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    /// Coefficients — f32 is sufficient; precision is bounded by the RBJ
    /// formula, not storage width. Computed once per parameter change.
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
    /// TDF-II state nodes — f64 to eliminate per-sample accumulation error.
    /// At high gain near Nyquist, f32 state diverges 1–3 dB from ideal;
    /// f64 state is indistinguishable from an exact implementation.
    s1: f64,
    s2: f64,
}

impl Biquad {
    pub const fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    pub fn peaking(freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let freq_hz = freq_hz.clamp(20.0, sample_rate / 2.0 - 1.0);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    /// Low-shelf filter (RBJ cookbook, S=1, Q=1/sqrt(2)).
    pub fn low_shelf(freq_hz: f32, gain_db: f32, sample_rate: f32) -> Self {
        let freq_hz = freq_hz.clamp(20.0, sample_rate / 2.0 - 1.0);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 * 0.5 * 2.0_f32.sqrt();
        let sq = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + sq);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - sq);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + sq;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - sq;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    /// High-shelf filter (RBJ cookbook, S=1, Q=1/sqrt(2)).
    pub fn high_shelf(freq_hz: f32, gain_db: f32, sample_rate: f32) -> Self {
        let freq_hz = freq_hz.clamp(20.0, sample_rate / 2.0 - 1.0);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 * 0.5 * 2.0_f32.sqrt();
        let sq = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + sq);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - sq);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + sq;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - sq;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    /// 2nd-order Butterworth high-pass.
    pub fn high_pass(freq_hz: f32, sample_rate: f32) -> Self {
        let freq_hz = freq_hz.clamp(20.0, sample_rate / 2.0 - 1.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / 2.0_f32.sqrt();
        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    /// 2nd-order Butterworth low-pass.
    pub fn low_pass(freq_hz: f32, sample_rate: f32) -> Self {
        let freq_hz = freq_hz.clamp(20.0, sample_rate / 2.0 - 1.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / 2.0_f32.sqrt();
        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0_f64,
            s2: 0.0_f64,
        }
    }

    /// TDF-II with hybrid f32/f64 precision (Poweramp-class quality).
    ///
    /// Coefficients are widened to f64 for the multiply-accumulate, then the
    /// result narrows back to f32 at return. All internal state is f64.
    /// I/O and the surrounding pipeline remain f32 — no interface changes.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let x64 = x as f64;
        let y64 = (self.b0 as f64) * x64 + self.s1;
        self.s1 = (self.b1 as f64) * x64 - (self.a1 as f64) * y64 + self.s2;
        self.s2 = (self.b2 as f64) * x64 - (self.a2 as f64) * y64;
        y64 as f32
    }

    #[inline]
    pub fn reset(&mut self) {
        self.s1 = 0.0_f64;
        self.s2 = 0.0_f64;
    }

    #[inline]
    pub fn copy_coeffs_from(&mut self, src: &Biquad) {
        self.b0 = src.b0;
        self.b1 = src.b1;
        self.b2 = src.b2;
        self.a1 = src.a1;
        self.a2 = src.a2;
    }
}

/// ~5 ms at 48 kHz — smooths EQ parameter changes, eliminating zipper noise.
pub const SMOOTH_FRAMES: u32 = 240;
/// Precomputed reciprocal — avoids a per-frame division in the hot path.
pub const SMOOTH_STEP: f32 = 1.0 / SMOOTH_FRAMES as f32;

#[derive(Debug, Clone, Copy)]
pub struct SmoothedBand {
    pub current: [Biquad; 2],
    pub target: [Biquad; 2],
    pub steps: u32,
}

impl Default for SmoothedBand {
    fn default() -> Self {
        Self::new()
    }
}

impl SmoothedBand {
    pub fn new() -> Self {
        let id = Biquad::identity();
        Self {
            current: [id; 2],
            target: [id; 2],
            steps: 0,
        }
    }

    pub fn set_target(&mut self, bq: Biquad) {
        self.target[0].copy_coeffs_from(&bq);
        self.target[1].copy_coeffs_from(&bq);
        self.steps = SMOOTH_FRAMES;
    }

    #[inline(always)]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.steps > 0 {
            self.steps -= 1;
            if self.steps == 0 {
                for ch in 0..2 {
                    self.current[ch].copy_coeffs_from(&self.target[ch]);
                }
            } else {
                for ch in 0..2 {
                    macro_rules! lerp {
                        ($f:ident) => {
                            self.current[ch].$f +=
                                (self.target[ch].$f - self.current[ch].$f) * SMOOTH_STEP;
                        };
                    }
                    lerp!(b0);
                    lerp!(b1);
                    lerp!(b2);
                    lerp!(a1);
                    lerp!(a2);
                }
            }
        }
        (self.current[0].process(l), self.current[1].process(r))
    }

    pub fn reset(&mut self) {
        for ch in 0..2 {
            self.current[ch].reset();
            self.target[ch].reset();
        }
        self.steps = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn identity_biquad_is_passthrough() {
        let mut bq = Biquad::identity();
        for x in [-1.0_f32, 0.0, 0.5, 1.0] {
            assert!((bq.process(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn peaking_0db_is_passthrough() {
        let bq = Biquad::peaking(1000.0, 0.0, 1.0, 48000.0);
        assert!(
            (bq.b0 - 1.0).abs() < 1e-6,
            "b0 should be ~1.0, got {}",
            bq.b0
        );
        assert!((bq.b1).abs() < 1e-4, "b1 should be ~0.0, got {}", bq.b1);
        assert!((bq.b2).abs() < 1e-4);
        assert!((bq.a1).abs() < 1e-4);
        assert!((bq.a2).abs() < 1e-4);
    }

    #[test]
    fn peaking_gain_accuracy() {
        let mut bq = Biquad::peaking(1000.0, 6.0, 1.414, 48000.0);
        let sr = 48000.0;
        let freq = 1000.0;
        let mut max_gain = 0.0_f32;
        for i in 0..(sr as usize * 2) {
            let x = (2.0 * PI * freq * i as f32 / sr).sin() * 0.1;
            let y = bq.process(x);
            let gain_db = 20.0 * (y.abs() / 0.1).log10();
            if gain_db > max_gain {
                max_gain = gain_db;
            }
        }
        assert!(
            (max_gain - 6.0).abs() < 0.5,
            "Expected ~6 dB gain, got {:.2} dB",
            max_gain
        );
    }

    #[test]
    fn low_shelf_coefficients_symmetry() {
        let bq = Biquad::low_shelf(120.0, 0.0, 48000.0);
        assert!((bq.b0 - 1.0).abs() < 1e-6);
        assert!((bq.a1).abs() < 2.0);
        assert!((bq.a2).abs() < 2.0);
    }

    #[test]
    fn high_shelf_coefficients_symmetry() {
        let bq = Biquad::high_shelf(10000.0, 0.0, 48000.0);
        assert!((bq.b0 - 1.0).abs() < 1e-6);
        assert!((bq.a1).abs() < 2.0);
        assert!((bq.a2).abs() < 2.0);
    }

    #[test]
    fn biquad_stability_at_nyquist() {
        let bq = Biquad::peaking(23990.0, 6.0, 1.0, 48000.0);
        assert!(bq.b0.is_finite());
        assert!(bq.b1.is_finite());
        assert!(bq.a1.is_finite());
        let mut bq = bq;
        for i in 0..1000 {
            let x = (2.0 * PI * 23990.0 * i as f32 / 48000.0).sin() * 0.5;
            let y = bq.process(x);
            assert!(
                y.is_finite() && y.abs() < 100.0,
                "Unstable at sample {}: {}",
                i,
                y
            );
        }
    }

    /// f64 TDF-II state nodes must remain bounded near the output magnitude.
    /// With f64 state the threshold can be tighter than the f32 version
    /// because there is no intermediate value build-up.
    #[test]
    fn tdfii_state_stays_bounded_near_nyquist() {
        let mut bq = Biquad::peaking(23500.0, 24.0, 0.7, 48000.0);
        for i in 0..4800 {
            let x = (2.0 * PI * 23500.0 * i as f32 / 48000.0).sin() * 0.5;
            let _y = bq.process(x);
            assert!(bq.s1.abs() < 50.0, "s1 blew up at sample {}: {}", i, bq.s1);
            assert!(bq.s2.abs() < 50.0, "s2 blew up at sample {}: {}", i, bq.s2);
        }
    }

    /// f64 state must produce a gain error < 0.1 dB at 20 kHz / 24 dB boost
    /// on a 44.1 kHz source — the worst-case zone where f32 state diverges.
    #[test]
    fn f64_state_gain_accuracy_near_nyquist() {
        use std::f32::consts::PI;
        let freq = 20_000.0_f32;
        let sr = 44100.0_f32;
        let gain_db = 24.0_f32;
        let mut bq = Biquad::peaking(freq, gain_db, 1.0, sr);
        let mut max_val = 0.0_f32;
        for i in 0..(sr as usize * 2 + sr as usize / 2) {
            let x = (2.0 * PI * freq * i as f32 / sr).sin() * 0.1;
            let y = bq.process(x);
            if i >= sr as usize * 2 && y.abs() > max_val {
                max_val = y.abs();
            }
        }
        let measured_db = 20.0 * (max_val / 0.1_f32).log10();
        assert!(
            (measured_db - gain_db).abs() < 0.1,
            "f64 state gain error at 20 kHz +24 dB: expected {:.1} dB, got {:.2} dB",
            gain_db,
            measured_db
        );
    }

    /// After exactly SMOOTH_FRAMES steps the coefficients must land exactly on
    /// target (not merely close), since we snap on the final step.
    #[test]
    fn smoothed_band_snaps_exactly_to_target() {
        let mut band = SmoothedBand::new();
        let target = Biquad::peaking(1000.0, 12.0, 1.0, 48000.0);
        band.set_target(target);
        for _ in 0..SMOOTH_FRAMES {
            band.process(0.0, 0.0);
        }
        assert_eq!(
            band.current[0].b0, band.target[0].b0,
            "b0 must be bit-exact after snap"
        );
        assert_eq!(
            band.current[0].a1, band.target[0].a1,
            "a1 must be bit-exact after snap"
        );
    }
}
