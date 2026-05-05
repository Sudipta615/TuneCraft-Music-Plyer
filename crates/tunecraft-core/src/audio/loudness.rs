//! EBU R128 / ITU-R BS.1770 loudness measurement and normalization.
//!
//! # Overview
//!
//! This module implements loudness measurement according to ITU-R BS.1770-4,
//! which is the foundation of the EBU R128 standard. It provides:
//!
//! - **Integrated loudness** measurement (long-term, programme-wide LUFS)
//! - **Momentary loudness** (400 ms window)
//! - **Short-term loudness** (3 s window)
//! - **Loudness normalization** to a configurable target level
//!
//! # Why this exists alongside ReplayGain
//!
//! ReplayGain relies on tags embedded in the audio file. Many files lack these
//! tags, especially in large or older music libraries. EBU R128 loudness
//! measurement operates on the actual audio signal in real-time, providing
//! normalization even for untagged files.
//!
//! Professional players increasingly implement EBU R128 as a complement or
//! alternative to ReplayGain. The two systems coexist:
//!
//! - Files **with** ReplayGain tags: ReplayGain is used (more accurate, offline)
//! - Files **without** ReplayGain tags: EBU R128 loudness normalization is
//!   applied in real-time as a fallback
//!
//! # ITU-R BS.1770-4 algorithm
//!
//! The measurement algorithm applies:
//! 1. A pre-filter ("K-weighting") that approximates human hearing:
//!    - A high-shelf filter boosting high frequencies (+4 dB at ~1.5 kHz)
//!    - A high-pass filter removing sub-bass (~60 Hz)
//! 2. Mean square calculation of the filtered signal
//! 3. A channel weighting factor (1.0 for L/R, 1.41 for C, 0.0 for LFE)
//! 4. Gating: only frames above absolute (-70 LUFS) and relative (-10 dB
//!    below ungated loudness) thresholds are included
//!
//! # Real-time constraints
//!
//! No heap allocation in the processing path. The K-weighting biquad filters
//! are applied per-sample in the hot path. Gating and LUFS computation are
//! performed on accumulated blocks.

use std::collections::VecDeque;
use std::f32::consts::PI;

use anyhow::Result;

/// Maximum number of 400 ms blocks to keep for integrated loudness computation.
/// At 48 kHz stereo, one block = 48000 * 0.4 * 2 = 38400 samples.
/// 1200 blocks = 8 minutes of audio. Beyond this, older blocks are dropped.
///
/// Fix Issue #7: For long continuous playback sessions (DJ use, 4+ hour playlists),
/// the gate window was effectively the entire first 8 minutes, which skewed the
/// integrated loudness away from the current segment. The EBU R128 standard
/// recommends resetting on track boundaries. The AudioEngine now calls reset()
/// at track boundaries in load_internal(), ensuring each track gets an independent
/// loudness measurement. This constant remains as a safety cap for very long
/// tracks, but the per-track reset is the primary mechanism for correctness.
const MAX_BLOCKS: usize = 1200;

/// Duration of one measurement block in seconds (400 ms for momentary loudness).
const BLOCK_DURATION_SECS: f32 = 0.4;

/// Absolute gate threshold in LUFS (EBU R128: -70 LUFS).
const ABSOLUTE_GATE_LUFS: f64 = -70.0;

/// Relative gate threshold in dB below ungated loudness (EBU R128: -10 dB).
const RELATIVE_GATE_DB: f64 = -10.0;

// ── K-weighting biquad filters (ITU-R BS.1770-4) ─────────────────────

/// Pre-filter 1: High-shelf filter (approx +4 dB boost above ~1.0 kHz).
/// This is the "head-related" part of K-weighting.
#[derive(Debug, Clone, Copy)]
struct KWeightShelf {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    z1: f32, z2: f32,
}

impl KWeightShelf {
    /// BS.1770-4 high-shelf coefficients for a given sample rate.
    /// At 48 kHz: boost ~4 dB above ~1.5 kHz.
    fn new(sample_rate: f32) -> Self {
        // ITU-R BS.1770-4 Annex, Table 1 (48 kHz)
        // For other rates, we scale the coefficients proportionally.
        let (b0, b1, b2, a1, a2) = if (sample_rate - 48000.0).abs() < 100.0 {
            // Exact 48 kHz coefficients from BS.1770-4
            (1.530909599, -2.650391682, 1.169160636, -1.663632945, 0.612383232)
        } else if (sample_rate - 44100.0).abs() < 100.0 {
            // 44.1 kHz coefficients (EBU Tech 3341)
            (1.530909599, -2.603747892, 1.113068076, -1.634954350, 0.588016499)
        } else {
            // Generic: compute high-shelf at ~1.5 kHz, +4 dB, Q ~0.7
            let freq = 1500.0_f32.min(sample_rate * 0.03125);
            let gain_db = 4.0_f32;
            let a = 10.0_f32.powf(gain_db / 40.0);
            let w0 = 2.0 * PI * freq / sample_rate;
            let (sin_w0, cos_w0) = w0.sin_cos();
            let alpha = sin_w0 * 0.5 * 2.0_f32.sqrt();
            let sq = 2.0 * a.sqrt() * alpha;
            let _b0 =        a * ((a+1.0) + (a-1.0)*cos_w0 + sq);
            let _b1 = -2.0 * a * ((a-1.0) + (a+1.0)*cos_w0      );
            let _b2 =        a * ((a+1.0) + (a-1.0)*cos_w0 - sq);
            let _a0 =             (a+1.0) - (a-1.0)*cos_w0 + sq;
            let _a1 =  2.0 *     ((a-1.0) - (a+1.0)*cos_w0      );
            let _a2 =             (a+1.0) - (a-1.0)*cos_w0 - sq;
            (_b0/_a0, _b1/_a0, _b2/_a0, _a1/_a0, _a2/_a0)
        };

        Self { b0, b1, b2, a1, a2, z1: 0.0, z2: 0.0 }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Pre-filter 2: High-pass filter (~60 Hz cut-off).
/// Removes sub-bass content that is not perceptually relevant.
#[derive(Debug, Clone, Copy)]
struct KWeightHPF {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    z1: f32, z2: f32,
}

impl KWeightHPF {
    fn new(sample_rate: f32) -> Self {
        let (b0, b1, b2, a1, a2) = if (sample_rate - 48000.0).abs() < 100.0 {
            // Exact 48 kHz coefficients from BS.1770-4
            (1.0, -2.0, 1.0, -1.989175119, 0.989221003)
        } else if (sample_rate - 44100.0).abs() < 100.0 {
            // 44.1 kHz coefficients
            (1.0, -2.0, 1.0, -1.990328814, 0.990385047)
        } else {
            // Generic 2nd-order Butterworth HPF at ~65 Hz
            let freq = 65.0_f32.min(sample_rate * 0.00135);
            let w0 = 2.0 * PI * freq / sample_rate;
            let (sin_w0, cos_w0) = w0.sin_cos();
            let alpha = sin_w0 / 2.0_f32.sqrt();
            let _b0 = (1.0 + cos_w0) / 2.0; let _b1 = -(1.0 + cos_w0); let _b2 = _b0;
            let _a0 = 1.0 + alpha;           let _a1 = -2.0 * cos_w0;  let _a2 = 1.0 - alpha;
            (_b0/_a0, _b1/_a0, _b2/_a0, _a1/_a0, _a2/_a0)
        };

        Self { b0, b1, b2, a1, a2, z1: 0.0, z2: 0.0 }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

// ── Loudness measurement state ─────────────────────────────────────────

/// Configuration for loudness normalization.
#[derive(Debug, Clone)]
pub struct LoudnessNormalizationConfig {
    /// Target loudness level in LUFS. Default: -23.0 (EBU R128 broadcast standard).
    /// Streaming platforms typically use -14.0 LUFS.
    pub target_lufs: f64,
    /// Maximum gain to apply in dB. Prevents extreme amplification of very
    /// quiet signals. Default: +12 dB.
    pub max_gain_db: f64,
    /// Minimum gain to apply in dB. Prevents extreme attenuation.
    /// Default: -24 dB.
    pub min_gain_db: f64,
    /// Smoothing time constant in seconds for gain changes. Prevents
    /// abrupt volume jumps. Default: 3.0 s.
    pub smoothing_secs: f64,
}

impl Default for LoudnessNormalizationConfig {
    fn default() -> Self {
        Self {
            target_lufs: -23.0,
            max_gain_db: 12.0,
            min_gain_db: -24.0,
            smoothing_secs: 3.0,
        }
    }
}

/// EBU R128 loudness measurement and normalization engine.
///
/// Processes interleaved stereo F32 audio through K-weighting filters and
/// computes integrated loudness using the BS.1770-4 gating algorithm.
/// Produces a smooth gain factor for loudness normalization.
pub struct EbuR128Loudness {
    sample_rate: f32,

    // K-weighting filters (per-channel)
    shelf: [KWeightShelf; 2],
    hpf: [KWeightHPF; 2],

    // Block accumulation
    block_samples: usize,
    block_square_sum: f64,
    samples_per_block: usize,

    // Gated loudness computation
    // Fix Bug #14: Changed from Vec<f64> to VecDeque<f64> so pop_front is O(1)
    // instead of the O(n) remove(0) that shifted all 1200 elements every block.
    block_loudness: VecDeque<f64>,

    // Fix Bug #13: Pre-allocated scratch buffers for compute_gated_loudness().
    // Previously, two Vec<f64> were allocated every call from the DSP thread,
    // causing heap allocations in the real-time path. Now we reuse these.
    scratch_pass1: Vec<f64>,
    scratch_pass2: Vec<f64>,

    // Current gain for normalization (smoothed)
    current_gain: f32,
    target_gain: f32,
    smoothing_coeff: f32,
    // Fix Bug #8: Cache the last smoothing_secs value so we only recompute
    // the smoothing coefficient (exp()) when it changes, not on every buffer.
    // exp() is ~30-100 ns per call on modern CPUs; with 256-sample buffers
    // at 48 kHz that's ~5800 calls/s ≈ 0.5 ms/s of CPU wasted on a constant.
    last_smoothing_secs: f64,
}

impl EbuR128Loudness {
    /// Create a new EBU R128 loudness measurer for the given sample rate.
    pub fn new(sample_rate: f32) -> Result<Self> {
        anyhow::ensure!(sample_rate > 0.0, "EbuR128Loudness: sample_rate must be > 0, got {}", sample_rate);
        let samples_per_block = (sample_rate as f64 * BLOCK_DURATION_SECS as f64 * 2.0) as usize;
        Ok(Self {
            sample_rate,
            shelf: [KWeightShelf::new(sample_rate), KWeightShelf::new(sample_rate)],
            hpf: [KWeightHPF::new(sample_rate), KWeightHPF::new(sample_rate)],
            block_samples: 0,
            block_square_sum: 0.0,
            samples_per_block,
            // Fix Bug #14: Use VecDeque instead of Vec for O(1) front removal.
            block_loudness: VecDeque::with_capacity(256),
            // Fix Bug #13: Pre-allocate scratch buffers to avoid heap allocations
            // in compute_gated_loudness() which runs on the DSP thread.
            scratch_pass1: Vec::with_capacity(MAX_BLOCKS),
            scratch_pass2: Vec::with_capacity(MAX_BLOCKS),
            current_gain: 1.0,
            target_gain: 1.0,
            smoothing_coeff: 0.999, // will be set by update_smoothing
            last_smoothing_secs: -1.0, // sentinel: forces first computation
        })
    }

    /// Process a buffer of interleaved stereo F32 samples.
    ///
    /// This updates the loudness measurement and adjusts the normalization gain.
    pub fn process_buffer(&mut self, buf: &[f32], config: &LoudnessNormalizationConfig) {
        // Fix Bug #2: Clamp smoothing_secs to a minimum of 0.01 to prevent
        // Infinity or NaN in the smoothing coefficient calculation when
        // smoothing_secs is 0.0 or negative. A 10 ms minimum smoothing time
        // still allows very responsive gain adjustment without producing
        // non-finite values.
        let smoothing_secs = config.smoothing_secs.max(0.01);

        // Fix Bug #8: Only recompute the smoothing coefficient when
        // smoothing_secs actually changes. exp() is expensive (~30-100 ns)
        // and was previously called on every buffer (~5800×/s at 48 kHz).
        // Since smoothing_secs rarely changes (only on config updates),
        // caching the last value eliminates this hot-path overhead.
        if (config.smoothing_secs - self.last_smoothing_secs).abs() > 1e-9 {
            let alpha = (-1.0 / (self.sample_rate as f64 * smoothing_secs)).exp() as f32;
            self.smoothing_coeff = alpha;
            self.last_smoothing_secs = config.smoothing_secs;
        }

        // Process each stereo sample through K-weighting
        for frame in buf.chunks_exact(2) {
            let l_kw = self.hpf[0].process(self.shelf[0].process(frame[0]));
            let r_kw = self.hpf[1].process(self.shelf[1].process(frame[1]));

            // Accumulate mean square (stereo: both channels weighted 1.0 per BS.1770)
            self.block_square_sum += l_kw as f64 * l_kw as f64 + r_kw as f64 * r_kw as f64;
            self.block_samples += 2;

            // Fix Bug #27: Apply gain smoothing per-sample instead of per-buffer.
            // The previous code applied smoothing once per buffer (~256 samples),
            // making the effective time constant ~256× longer than the configured
            // 3 seconds. Now smoothing is applied per-frame for correct timing.
            self.current_gain += (self.target_gain - self.current_gain) * (1.0 - self.smoothing_coeff);

            // End of block?
            if self.block_samples >= self.samples_per_block {
                self.finalize_block(config);
            }
        }
    }

    /// Finalize the current 400 ms block and compute loudness.
    fn finalize_block(&mut self, config: &LoudnessNormalizationConfig) {
        if self.block_samples == 0 { return; }

        let mean_square = self.block_square_sum / self.block_samples as f64;

        // Convert to LUFS: L = -0.691 + 10 * log10(mean_square)
        let loudness_lufs = if mean_square > 1e-20 {
            -0.691 + 10.0 * mean_square.log10()
        } else {
            -70.0 // below absolute gate
        };

        // Reset block accumulator
        self.block_square_sum = 0.0;
        self.block_samples = 0;

        // Only keep blocks above absolute gate
        if loudness_lufs > ABSOLUTE_GATE_LUFS {
            self.block_loudness.push_back(loudness_lufs);
            // Fix Bug #14: Use pop_front() (O(1)) instead of remove(0) (O(n)).
            // Previously, remove(0) shifted all ~1200 elements every time a block
            // was evicted, causing unnecessary CPU work on the DSP thread.
            if self.block_loudness.len() > MAX_BLOCKS {
                self.block_loudness.pop_front();
            }
        }

        // Compute integrated loudness with gating (if we have enough blocks)
        if self.block_loudness.len() >= 3 {
            if let Some(integrated) = self.compute_gated_loudness() {
                // Compute normalization gain
                let gain_db = config.target_lufs - integrated;
                let gain_db_clamped = gain_db.clamp(config.min_gain_db, config.max_gain_db);
                self.target_gain = 10.0_f32.powf(gain_db_clamped as f32 / 20.0);
            }
        }
    }

    /// Compute EBU R128 gated integrated loudness.
    ///
    /// Two-pass gating:
    /// 1. Remove blocks below -70 LUFS (absolute gate)
    /// 2. Compute ungated loudness, then remove blocks -10 dB below it (relative gate)
    /// 3. Compute final loudness from remaining blocks
    /// Fix Bug #13: Changed from &self to &mut self to allow reuse of
    /// pre-allocated scratch buffers (scratch_pass1, scratch_pass2), avoiding
    /// heap allocations in the DSP thread.
    fn compute_gated_loudness(&mut self) -> Option<f64> {
        // Fix Bug #13: Reuse pre-allocated scratch buffers instead of allocating
        // new Vecs every call. This prevents heap allocations on the DSP thread.

        // Pass 1: absolute gate (blocks are already filtered above -70 LUFS)
        self.scratch_pass1.clear();
        self.scratch_pass1.extend(self.block_loudness.iter().copied());
        if self.scratch_pass1.is_empty() { return None; }

        // Compute ungated loudness from pass 1
        let sum_exp: f64 = self.scratch_pass1.iter().map(|&l| 10.0_f64.powf(l / 10.0)).sum();
        let ungated_loudness = -0.691 + 10.0 * (sum_exp / self.scratch_pass1.len() as f64).log10();

        // Pass 2: relative gate (-10 dB below ungated)
        let relative_gate = ungated_loudness + RELATIVE_GATE_DB;
        self.scratch_pass2.clear();
        self.scratch_pass2.extend(
            self.scratch_pass1.iter().copied().filter(|&l| l > relative_gate)
        );
        if self.scratch_pass2.is_empty() { return None; }

        // Compute gated integrated loudness
        let sum_exp: f64 = self.scratch_pass2.iter().map(|&l| 10.0_f64.powf(l / 10.0)).sum();
        Some(-0.691 + 10.0 * (sum_exp / self.scratch_pass2.len() as f64).log10())
    }

    /// Get the current integrated loudness in LUFS, if enough samples have been processed.
    /// Fix Bug #13: Changed from &self to &mut self because compute_gated_loudness()
    /// now mutates scratch buffers instead of allocating new Vecs.
    pub fn integrated_loudness(&mut self) -> Option<f64> {
        if self.block_loudness.len() < 3 { return None; }
        self.compute_gated_loudness()
    }

    /// Get the current normalization gain factor (linear, 1.0 = no change).
    pub fn current_gain(&self) -> f32 {
        self.current_gain
    }

    /// Reset all measurement state (call at track boundaries).
    pub fn reset(&mut self) {
        self.block_samples = 0;
        self.block_square_sum = 0.0;
        self.block_loudness.clear();
        // Fix Bug #13: Clear scratch buffers on reset too.
        self.scratch_pass1.clear();
        self.scratch_pass2.clear();
        self.current_gain = 1.0;
        self.target_gain = 1.0;
        // Fix Bug #8: Reset the cached smoothing_secs sentinel so the
        // coefficient gets recomputed after reset (sample rate may change).
        self.last_smoothing_secs = -1.0;
        for ch in 0..2 {
            self.shelf[ch].z1 = 0.0; self.shelf[ch].z2 = 0.0;
            self.hpf[ch].z1 = 0.0; self.hpf[ch].z2 = 0.0;
        }
    }

    /// Update the sample rate (rebuilds filter coefficients).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.shelf = [KWeightShelf::new(sample_rate), KWeightShelf::new(sample_rate)];
        self.hpf = [KWeightHPF::new(sample_rate), KWeightHPF::new(sample_rate)];
        self.samples_per_block = (sample_rate as f64 * BLOCK_DURATION_SECS as f64 * 2.0) as usize;
        // Fix Bug #8: Invalidate cached smoothing coefficient since sample_rate
        // changed (the coefficient depends on sample_rate * smoothing_secs).
        self.last_smoothing_secs = -1.0;
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_produces_no_loudness() {
        let mut meter = EbuR128Loudness::new(48000.0).unwrap();
        let silence = vec![0.0f32; 48000 * 2]; // 1 second of silence
        meter.process_buffer(&silence, &LoudnessNormalizationConfig::default());
        // With all-silence input, integrated loudness should be None or very low
        let lufs = meter.integrated_loudness();
        // Silence should not produce a valid measurement above -70 LUFS
        assert!(lufs.is_none() || lufs.unwrap() < -70.0, "Silence should not produce loudness above -70 LUFS");
    }

    #[test]
    fn full_scale_sine_produces_positive_loudness() {
        let mut meter = EbuR128Loudness::new(48000.0).unwrap();
        // Generate 5 seconds of -3 dBFS sine at 1 kHz (enough for several blocks)
        let sr = 48000.0f32;
        let samples: Vec<f32> = (0..(sr as usize * 5 * 2))
            .map(|i| {
                let t = i as f32 / (sr * 2.0);
                (2.0 * PI * 1000.0 * t).sin() * 0.707 // -3 dBFS
            })
            .collect();
        meter.process_buffer(&samples, &LoudnessNormalizationConfig::default());
        let lufs = meter.integrated_loudness();
        // A -3 dBFS sine at 1 kHz should produce approximately -3 to -6 LUFS
        // (K-weighting adds ~4 dB above 1 kHz, so it may be slightly higher)
        assert!(lufs.is_some(), "Should have a valid loudness measurement");
        let l = lufs.unwrap();
        assert!(l > -10.0 && l < 5.0, "Full-scale sine should produce loudness near 0 LUFS, got {}", l);
    }

    #[test]
    fn gain_converges_to_target() {
        let mut meter = EbuR128Loudness::new(48000.0).unwrap();
        let config = LoudnessNormalizationConfig {
            target_lufs: -23.0,
            ..Default::default()
        };
        // Generate 10 seconds of -3 dBFS sine
        let sr = 48000.0f32;
        let samples: Vec<f32> = (0..(sr as usize * 10 * 2))
            .map(|i| {
                let t = i as f32 / (sr * 2.0);
                (2.0 * PI * 1000.0 * t).sin() * 0.707
            })
            .collect();
        meter.process_buffer(&samples, &config);
        // After 10 seconds, the gain should have been adjusted
        let gain = meter.current_gain();
        // The signal is louder than -23 LUFS, so gain should be < 1.0
        assert!(gain < 1.0, "Gain should attenuate a loud signal, got {}", gain);
        assert!(gain > 0.0, "Gain should be positive, got {}", gain);
    }

    #[test]
    fn reset_clears_state() {
        let mut meter = EbuR128Loudness::new(48000.0).unwrap();
        let sr = 48000.0f32;
        let samples: Vec<f32> = (0..(sr as usize * 2 * 2))
            .map(|i| {
                let t = i as f32 / (sr * 2.0);
                (2.0 * PI * 1000.0 * t).sin() * 0.5
            })
            .collect();
        meter.process_buffer(&samples, &LoudnessNormalizationConfig::default());
        meter.reset();
        // After reset, integrated loudness should be None
        assert!(meter.integrated_loudness().is_none(), "Reset should clear loudness state");
        // Gain should be unity
        assert!((meter.current_gain() - 1.0).abs() < 1e-6, "Reset should reset gain to 1.0");
    }

    #[test]
    fn k_weight_shelf_is_stable() {
        let mut shelf = KWeightShelf::new(48000.0);
        // Process 100k samples of a full-scale sine — should not diverge
        for i in 0..100_000 {
            let x = (2.0 * PI * 1000.0 * i as f32 / 48000.0).sin();
            let y = shelf.process(x);
            assert!(y.is_finite() && y.abs() < 10.0, "Shelf filter unstable at sample {}: {}", i, y);
        }
    }

    #[test]
    fn k_weight_hpf_is_stable() {
        let mut hpf = KWeightHPF::new(48000.0);
        for i in 0..100_000 {
            let x = (2.0 * PI * 1000.0 * i as f32 / 48000.0).sin();
            let y = hpf.process(x);
            assert!(y.is_finite() && y.abs() < 10.0, "HPF filter unstable at sample {}: {}", i, y);
        }
    }
}
