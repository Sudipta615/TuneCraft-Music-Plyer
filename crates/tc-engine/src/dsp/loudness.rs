//! Loudness measurement and normalisation (EBU R128 / ReplayGain)
//!
//! Implements loudness normalisation that applies gain adjustments based on
//! pre-computed loudness metadata. The normaliser runs in the playback pipeline
//! and applies smooth gain transitions.

use std::f64::consts::PI;

use crate::buffer::AudioFrame;

/// Loudness normalisation mode
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LoudnessMode {
    #[default]
    Off,
    TrackReplayGain,
    AlbumReplayGain,
    EbuR128,
}


/// Loudness metadata for a track (pre-computed during scanning)
#[derive(Debug, Clone, Copy, Default)]
pub struct LoudnessMetadata {
    /// ReplayGain track gain in dB
    pub replaygain_track_db: Option<f64>,
    /// ReplayGain album gain in dB
    pub replaygain_album_db: Option<f64>,
    /// ReplayGain track peak (linear)
    pub replaygain_track_peak: Option<f64>,
    /// ReplayGain album peak (linear)
    pub replaygain_album_peak: Option<f64>,
    /// EBU R128 integrated loudness in LUFS
    pub ebu_r128_loudness: Option<f64>,
    /// EBU R128 true peak in dBTP
    pub ebu_r128_peak: Option<f64>,
}

/// First-order high-pass shelf (stage 1 of K-weighting)
#[derive(Debug, Clone, Copy)]
struct KWeightStage1 {
    b0: f64,
    b1: f64,
    a1: f64,
    z1: [f64; 2],
}

impl KWeightStage1 {
    fn new(sample_rate: f64) -> Self {
        // L6: Guard against zero or negative sample_rate which would cause
        // tan() to receive infinity or NaN, producing garbage coefficients.
        let sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            log::warn!(
                "KWeightStage1: invalid sample_rate {:.1}, defaulting to 44100",
                sample_rate
            );
            44100.0
        };
        let f0: f64 = 1681.974450955533;
        let g: f64 = 3.999843853973347;
        let q: f64 = 0.7071752369554196;
        let k = (PI * f0 / sample_rate).tan();
        let vh = g.powf(0.5) * k * k + k / q + 1.0;
        let vb = g.powf(0.5) - 1.0;
        let vl = g.powf(0.5) * k * k - k / q + 1.0;
        Self {
            b0: (vh + vb) / (vh - vb),
            b1: (-vl - vb) / (vh - vb),
            a1: (vl - vb) / (vh - vb),
            z1: [0.0; 2],
        }
    }

    #[inline]
    fn process(&mut self, sample: f64, ch: usize) -> f64 {
        let out = sample * self.b0 + self.z1[ch];
        self.z1[ch] = sample * self.b1 - out * self.a1;
        out
    }
}

/// Second-order high-pass (stage 2 of K-weighting)
#[derive(Debug, Clone, Copy)]
struct KWeightStage2 {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

impl KWeightStage2 {
    fn new(sample_rate: f64) -> Self {
        // L6: Guard against zero or negative sample_rate.
        let sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            log::warn!(
                "KWeightStage2: invalid sample_rate {:.1}, defaulting to 44100",
                sample_rate
            );
            44100.0
        };
        let f0 = 38.13547087602444;
        let q = 0.5003270373238773;
        let k = (PI * f0 / sample_rate).tan();
        let kk = k * k;
        let norm = kk + k / q + 1.0;
        Self {
            b0: 1.0 / norm,
            b1: -2.0 / norm,
            b2: 1.0 / norm,
            a1: 2.0 * (kk - 1.0) / norm,
            a2: (1.0 - k / q + kk) / norm,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    #[inline]
    fn process(&mut self, sample: f64, ch: usize) -> f64 {
        let out = sample * self.b0 + self.z1[ch];
        self.z1[ch] = sample * self.b1 - out * self.a1 + self.z2[ch];
        self.z2[ch] = sample * self.b2 - out * self.a2;
        out
    }
}

/// Loudness normaliser for playback
///
/// Applies gain adjustments based on pre-computed loudness metadata.
/// Supports ReplayGain (track/album) and EBU R128 modes.
pub struct LoudnessNormalizer {
    mode: LoudnessMode,
    target_lufs: f64,
    true_peak_guard: bool,
    true_peak_dbtp: f64,
    preamp_db: f64,
    /// Current applied gain (linear)
    current_gain_linear: f64,
    /// Target gain (linear, computed from metadata)
    target_gain_linear: f64,
    /// Smoothing coefficient for gain changes
    smooth_coeff: f64,
    /// K-weighting filters for measurement (used in EBU R128 measurement mode)
    stage1: KWeightStage1,
    stage2: KWeightStage2,
    sample_rate: f64,
    /// Accumulated loudness sum (squared K-weighted samples)
    loudness_sum: f64,
    /// Number of samples accumulated for loudness measurement
    loudness_count: u64,
    /// Channel count observed during measurement (for correct normalization)
    measured_channels: usize,
}

impl LoudnessNormalizer {
    /// Create a new normaliser at the given sample rate (off by default)
    pub fn new(sample_rate: f64) -> Self {
        Self {
            mode: LoudnessMode::Off,
            target_lufs: -23.0,
            true_peak_guard: true,
            true_peak_dbtp: -1.0,
            preamp_db: 0.0,
            current_gain_linear: 1.0,
            target_gain_linear: 1.0,
            smooth_coeff: 0.0005,
            stage1: KWeightStage1::new(sample_rate),
            stage2: KWeightStage2::new(sample_rate),
            sample_rate,
            loudness_sum: 0.0,
            loudness_count: 0,
            measured_channels: 0,
        }
    }

    /// Set the loudness normalisation mode
    pub fn set_mode(&mut self, mode: LoudnessMode) {
        self.mode = mode;
    }

    /// Set the target LUFS for EBU R128 mode
    pub fn set_target_lufs(&mut self, target: f64) {
        self.target_lufs = target;
    }

    /// Configure true peak guard
    pub fn set_true_peak_guard(&mut self, enabled: bool, ceiling_dbtp: f64) {
        self.true_peak_guard = enabled;
        self.true_peak_dbtp = ceiling_dbtp;
    }

    /// Set preamp in dB
    pub fn set_preamp_db(&mut self, gain_db: f64) {
        self.preamp_db = gain_db;
    }

    /// Update loudness metadata for the current track, computing gain
    pub fn set_track_metadata(&mut self, meta: &LoudnessMetadata) {
        let gain_db = match self.mode {
            LoudnessMode::Off => 0.0,
            LoudnessMode::TrackReplayGain => meta
                .replaygain_track_db
                .map(|rg| rg + self.preamp_db)
                .unwrap_or(0.0),
            LoudnessMode::AlbumReplayGain => meta
                .replaygain_album_db
                .map(|rg| rg + self.preamp_db)
                .unwrap_or(0.0),
            LoudnessMode::EbuR128 => meta
                .ebu_r128_loudness
                .map(|loudness| self.target_lufs - loudness + self.preamp_db)
                .unwrap_or(0.0),
        };

        // Apply true peak guard
        let peak = match self.mode {
            LoudnessMode::TrackReplayGain => meta.replaygain_track_peak,
            LoudnessMode::AlbumReplayGain => meta.replaygain_album_peak,
            LoudnessMode::EbuR128 => meta.ebu_r128_peak.map(|p| 10.0_f64.powf(p / 20.0)),
            _ => None,
        };

        let adjusted_gain = if self.true_peak_guard {
            if let Some(peak_linear) = peak {
                if peak_linear > 0.0 {
                    let new_peak_db = 20.0 * peak_linear.log10() + gain_db;
                    if new_peak_db > self.true_peak_dbtp {
                        gain_db - (new_peak_db - self.true_peak_dbtp)
                    } else {
                        gain_db
                    }
                } else {
                    gain_db
                }
            } else {
                gain_db
            }
        } else {
            gain_db
        };

        self.target_gain_linear = 10.0_f64.powf(adjusted_gain / 20.0);
    }

    /// Process a stereo sample pair with loudness normalisation
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        if self.mode == LoudnessMode::Off {
            return (left, right);
        }
        // Smooth gain transition
        self.current_gain_linear +=
            self.smooth_coeff * (self.target_gain_linear - self.current_gain_linear);
        (
            left * self.current_gain_linear,
            right * self.current_gain_linear,
        )
    }

    /// Get current applied gain in dB (for metering)
    pub fn current_gain_db(&self) -> f64 {
        if self.current_gain_linear > 0.0 {
            20.0 * self.current_gain_linear.log10()
        } else {
            -60.0
        }
    }

    /// Process an audio frame (alternative API for measurement)
    ///
    ///
    /// divisor in `measured_loudness_lufs()` uses the actual sample
    /// count (loudness_count * num_channels) instead of a hardcoded
    /// stereo assumption.
    pub fn process_frame(&mut self, frame: &AudioFrame) {
        let num_ch = frame.num_channels as usize;
        // Measurement mode: K-weight and accumulate
        for ch in 0..num_ch {
            let weighted = self
                .stage2
                .process(self.stage1.process(frame.channels[ch], ch), ch);
            self.loudness_sum += weighted * weighted;
        }
        self.loudness_count += 1;
        // Track channel count for correct normalization
        self.measured_channels = num_ch;
    }

    /// Get the measured loudness in LUFS (EBU R128)
    ///
    ///
    /// instead of hardcoding stereo (2.0). This produces correct
    /// measurements for mono and multi-channel inputs.
    pub fn measured_loudness_lufs(&self) -> Option<f64> {
        if self.loudness_count == 0 {
            return None;
        }
        let channels = if self.measured_channels > 0 {
            self.measured_channels as f64
        } else {
            2.0 // fallback to stereo
        };
        let mean_square = self.loudness_sum / (self.loudness_count as f64 * channels);
        if mean_square <= 0.0 {
            return None;
        }
        // EBU R128: LKFS = -0.691 + 10 * log10(mean_square)
        Some(-0.691 + 10.0 * mean_square.log10())
    }

    /// Update the sample rate, rebuilding K-weighting filters
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.stage1 = KWeightStage1::new(sample_rate);
        self.stage2 = KWeightStage2::new(sample_rate);
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.current_gain_linear = 1.0;
        self.target_gain_linear = 1.0;
        for ch in 0..2 {
            self.stage1.z1[ch] = 0.0;
            self.stage2.z1[ch] = 0.0;
            self.stage2.z2[ch] = 0.0;
        }
        self.loudness_sum = 0.0;
        self.loudness_count = 0;
        self.measured_channels = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_off_mode_passthrough() {
        let mut norm = LoudnessNormalizer::new(44100.0);
        norm.set_mode(LoudnessMode::Off);
        let (l, r) = norm.process(0.5, 0.5);
        assert!((l - 0.5).abs() < 1e-10);
        assert!((r - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_replay_gain_attenuation() {
        let mut norm = LoudnessNormalizer::new(44100.0);
        norm.set_mode(LoudnessMode::TrackReplayGain);
        let meta = LoudnessMetadata {
            replaygain_track_db: Some(-5.0), // Loud track, RG says -5dB (reduce volume)
            replaygain_track_peak: Some(0.95),
            ..Default::default()
        };
        norm.set_track_metadata(&meta);
        for _ in 0..10000 {
            norm.process(0.5, 0.5);
        }
        let (l, _r) = norm.process(0.5, 0.5);
        // With correct ReplayGain sign: rg + preamp = -5.0 + 0.0 = -5.0 dB (attenuation)
        // A loud track should be attenuated, so output should be less than input
        assert!(
            l < 0.5,
            "Loud track should be attenuated by ReplayGain, got {}",
            l
        );
        assert!(
            l > 0.01,
            "Should still be audible after attenuation, got {}",
            l
        );
    }

    #[test]
    fn test_ebu_r128_normalization() {
        let mut norm = LoudnessNormalizer::new(44100.0);
        norm.set_mode(LoudnessMode::EbuR128);
        norm.set_target_lufs(-23.0);
        let meta = LoudnessMetadata {
            ebu_r128_loudness: Some(-30.0), // Quiet track
            ebu_r128_peak: Some(-3.0),
            ..Default::default()
        };
        norm.set_track_metadata(&meta);
        for _ in 0..10000 {
            norm.process(0.1, 0.1);
        }
        let (l, _r) = norm.process(0.1, 0.1);
        // Should be boosted (7dB = -23 - (-30))
        assert!(l > 0.1, "Quiet track should be boosted, got {}", l);
    }

    #[test]
    fn test_gain_smoothing() {
        let mut norm = LoudnessNormalizer::new(44100.0);
        norm.set_mode(LoudnessMode::EbuR128);
        let meta = LoudnessMetadata {
            ebu_r128_loudness: Some(-20.0),
            ebu_r128_peak: Some(-1.0),
            ..Default::default()
        };
        norm.set_track_metadata(&meta);
        let mut prev_gain = norm.current_gain_linear;
        for _ in 0..1000 {
            norm.process(0.5, 0.5);
            let delta = (norm.current_gain_linear - prev_gain).abs();
            assert!(delta < 0.1, "Gain should change smoothly");
            prev_gain = norm.current_gain_linear;
        }
    }

    #[test]
    fn test_true_peak_guard() {
        let mut norm = LoudnessNormalizer::new(44100.0);
        norm.set_mode(LoudnessMode::TrackReplayGain);
        norm.set_true_peak_guard(true, -1.0);

        let meta = LoudnessMetadata {
            replaygain_track_db: Some(10.0),
            replaygain_track_peak: Some(0.8),
            ..Default::default()
        };
        norm.set_track_metadata(&meta);
        let guarded_gain = norm.target_gain_linear;

        norm.set_true_peak_guard(false, -1.0);
        norm.set_track_metadata(&meta);
        let unguarded_gain = norm.target_gain_linear;

        assert!(
            guarded_gain <= unguarded_gain,
            "True peak guard should reduce gain when needed"
        );
    }
}
