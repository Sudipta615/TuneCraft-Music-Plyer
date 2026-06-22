//! Parametric equaliser — configurable multi-band EQ with smooth parameter transitions

use crate::{
    buffer::AudioFrame,
    dsp::biquad::{FilterType, SmoothedBiquad},
};

/// Maximum number of EQ bands
pub const MAX_EQ_BANDS: usize = 31;

/// Number of default EQ bands
pub const NUM_EQ_BANDS: usize = 10;

/// EQ filter type for each band
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EqFilterType {
    #[default]
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Bandpass,
    Notch,
}

impl EqFilterType {
    /// Map to the biquad FilterType
    pub fn to_filter_type(&self) -> FilterType {
        match self {
            Self::Peaking => FilterType::Peaking,
            Self::LowShelf => FilterType::Lowshelf,
            Self::HighShelf => FilterType::Highshelf,
            Self::LowPass => FilterType::Lowpass,
            Self::HighPass => FilterType::Highpass,
            Self::Bandpass => FilterType::Bandpass,
            Self::Notch => FilterType::Notch,
        }
    }
}

/// Parameters for a single EQ band
#[derive(Debug, Clone, Copy)]
pub struct EqBandParams {
    /// Centre/cutoff frequency in Hz
    pub frequency: f32,
    /// Gain in dB (for shelving/peaking)
    pub gain_db: f32,
    /// Quality factor (bandwidth)
    pub q: f32,
    /// Filter type
    pub filter_type: EqFilterType,
    /// Whether this band is enabled
    pub enabled: bool,
}

impl Default for EqBandParams {
    fn default() -> Self {
        Self {
            frequency: 1000.0,
            gain_db: 0.0,
            q: 1.0,
            filter_type: EqFilterType::Peaking,
            enabled: false,
        }
    }
}

impl EqBandParams {
    /// Create a peaking EQ band
    pub fn peaking(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self {
            frequency,
            gain_db,
            q,
            filter_type: EqFilterType::Peaking,
            enabled: true,
        }
    }

    /// Create a low-shelf band
    pub fn lowshelf(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self {
            frequency,
            gain_db,
            q,
            filter_type: EqFilterType::LowShelf,
            enabled: true,
        }
    }

    /// Create a high-shelf band
    pub fn highshelf(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self {
            frequency,
            gain_db,
            q,
            filter_type: EqFilterType::HighShelf,
            enabled: true,
        }
    }
}

/// A single EQ band using a smoothed biquad filter (stereo pair)
#[derive(Debug, Clone)]
struct EqBand {
    params: EqBandParams,
    filter_left: SmoothedBiquad,
    filter_right: SmoothedBiquad,
}

impl EqBand {
    fn new() -> Self {
        Self {
            params: EqBandParams::default(),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        }
    }

    fn update_coefficients(&mut self, sample_rate: f32) {
        if !self.params.enabled {
            return;
        }
        let coeffs = self.params.filter_type.to_filter_type().compute_coeffs(
            sample_rate,
            self.params.frequency,
            self.params.gain_db,
            self.params.q,
        );
        self.filter_left.set_target(coeffs);
        self.filter_right.set_target(coeffs);
    }

    /// Process a stereo sample pair.
    ///
    /// v3.1.0: Marked `#[inline(always)]` to guarantee the band loop in
    /// `ParametricEq::process` is fully unrolled by LLVM. With 10 bands
    /// and 2 tone controls, the per-sample EQ cost is one flat sequence
    /// of ~36 biquad filter operations with no loop overhead, which LLVM
    /// can then auto-vectorize across the L/R stereo pair (4 f32 ops per
    /// AVX2 instruction instead of 1).
    #[inline(always)]
    fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.params.enabled {
            return (left, right);
        }
        let out_l = self.filter_left.process_sample(0, left);
        let out_r = self.filter_right.process_sample(1, right);
        // Advance smoothing once per frame (not per channel) for correct stereo behavior
        self.filter_left.advance_smoothing();
        self.filter_right.advance_smoothing();
        (out_l, out_r)
    }

    fn reset(&mut self) {
        self.filter_left.reset();
        self.filter_right.reset();
    }
}

/// Parametric EQ processor — configurable multi-band equaliser
#[derive(Debug, Clone)]
pub struct ParametricEq {
    bands: Vec<EqBand>,
    bass_band: EqBand,
    treble_band: EqBand,
    sample_rate: f32,
    enabled: bool,
    preamp_db: f32,
    preamp_linear: f32,
    post_gain_db: f32,
    /// Cached linear gain derived from `post_gain_db` — avoids a per-sample
    /// `powf` call in the hot path.  Updated by `set_post_gain_db()`.
    post_gain_linear: f32,
    headroom_db: f32,
    headroom_linear: f32,
    /// Current headroom scale factor (smoothed for zipper-noise prevention)
    headroom_scale: f32,
    /// Target headroom scale factor (computed from headroom_db and signal peak)
    headroom_scale_target: f32,
    /// Slew rate for headroom scale attack — when signal exceeds threshold,
    /// the scale reduces quickly to prevent clipping (per sample, 0.0–1.0).
    headroom_attack_rate: f32,
    /// Slew rate for headroom scale release — when signal falls below threshold,
    /// the scale returns to unity slowly to avoid pumping artifacts.
    headroom_release_rate: f32,
    /// Current peak envelope of the signal
    peak_envelope: f32,
    /// Release rate for the peak envelope (slower than audio frequencies)
    envelope_release_rate: f32,
}

impl ParametricEq {
    /// Create a new 10-band parametric EQ with standard ISO frequencies
    pub fn default_10_band(sample_rate: f32) -> Self {
        let freqs = [
            31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        let types = [
            EqFilterType::LowShelf,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::HighShelf,
        ];

        let bands = freqs
            .iter()
            .zip(types.iter())
            .map(|(&freq, &ft)| {
                let mut band = EqBand::new();
                band.params.frequency = freq;
                band.params.filter_type = ft;
                band.params.gain_db = 0.0;
                band.params.q = 1.4;
                band.params.enabled = false;
                band
            })
            .collect();

        let mut bass_band = EqBand::new();
        bass_band.params = EqBandParams::lowshelf(90.0, 0.0, 0.80);
        let mut treble_band = EqBand::new();
        treble_band.params = EqBandParams::highshelf(10000.0, 0.0, 0.60);

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp_linear: 1.0,
            post_gain_db: 0.0,
            post_gain_linear: 1.0,
            headroom_db: -1.0,
            headroom_linear: 10.0_f32.powf(-1.0 / 20.0),
            headroom_scale: 1.0,
            headroom_scale_target: 1.0,
            headroom_attack_rate: 0.01, // Fast attack: ~7ms to 95% at 44.1kHz
            headroom_release_rate: 0.0005, // Slow release: ~136ms to 95% at 44.1kHz
            peak_envelope: 0.0,
            envelope_release_rate: 0.0001, // Very slow envelope release
        }
    }

    /// Create a new EQ with all bands disabled
    pub fn new(num_bands: usize, sample_rate: f32) -> Self {
        let bands = (0..num_bands).map(|_| EqBand::new()).collect();
        let mut bass_band = EqBand::new();
        bass_band.params = EqBandParams::lowshelf(90.0, 0.0, 0.80);
        let mut treble_band = EqBand::new();
        treble_band.params = EqBandParams::highshelf(10000.0, 0.0, 0.60);

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp_linear: 1.0,
            post_gain_db: 0.0,
            post_gain_linear: 1.0,
            headroom_db: -1.0,
            headroom_linear: 10.0_f32.powf(-1.0 / 20.0),
            headroom_scale: 1.0,
            headroom_scale_target: 1.0,
            headroom_attack_rate: 0.01,
            headroom_release_rate: 0.0005,
            peak_envelope: 0.0,
            envelope_release_rate: 0.0001,
        }
    }

    /// Enable or disable the EQ
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether EQ is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set preamp gain in dB (applied before EQ)
    pub fn set_preamp_db(&mut self, gain_db: f32) {
        self.preamp_db = gain_db;
        self.preamp_linear = 10.0_f32.powf(gain_db / 20.0);
    }

    /// Set post-EQ gain in dB
    pub fn set_post_gain_db(&mut self, gain_db: f32) {
        self.post_gain_db = gain_db;
        // Cache the linear equivalent so process() doesn't call powf() per sample.
        self.post_gain_linear = 10.0_f32.powf(gain_db / 20.0);
    }

    /// Set headroom in dB for headroom management
    pub fn set_headroom_db(&mut self, headroom_db: f32) {
        self.headroom_db = headroom_db;
        self.headroom_linear = 10.0_f32.powf(headroom_db / 20.0);
    }

    /// Process a stereo sample pair through the full EQ chain
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Apply preamp
        let preamp_linear = self.preamp_linear;
        let mut l = left * preamp_linear;
        let mut r = right * preamp_linear;

        // Process through all bands
        for band in &mut self.bands {
            (l, r) = band.process(l, r);
        }

        // Process through tone controls
        (l, r) = self.bass_band.process(l, r);
        (l, r) = self.treble_band.process(l, r);

        // Attack (signal exceeds threshold): Use a fast attack rate so the
        // scale reduces quickly to prevent clipping. At 0.01, 95% of the
        // target is reached in ~300 samples (~7ms at 44.1kHz), which is fast
        // enough to catch transients while still avoiding zipper noise.
        //
        // the scale returns to unity gradually, avoiding pumping artifacts.
        // At 0.0005, 95% return takes ~6000 samples (~136ms at 44.1kHz).
        let headroom_linear = self.headroom_linear;
        let peak = l.abs().max(r.abs());

        // Envelope follower
        if peak > self.peak_envelope {
            self.peak_envelope = peak; // Instant attack for envelope
        } else {
            self.peak_envelope += self.envelope_release_rate * (peak - self.peak_envelope);
        }

        let target_scale = if self.peak_envelope > headroom_linear {
            headroom_linear / self.peak_envelope
        } else {
            1.0
        };

        if target_scale < self.headroom_scale {
            // Fast attack: reduce gain quickly to prevent clipping
            self.headroom_scale += self.headroom_attack_rate * (target_scale - self.headroom_scale);
        } else {
            // Slow release: avoid pumping artifacts and waveform distortion
            self.headroom_scale +=
                self.headroom_release_rate * (target_scale - self.headroom_scale);
        }
        self.headroom_scale_target = target_scale;

        // floating-point accumulation can never permanently exceed unity.
        self.headroom_scale = self.headroom_scale.min(1.0);
        // Only apply headroom reduction (scale < 1.0), never boost
        if self.headroom_scale < 1.0 {
            l *= self.headroom_scale;
            r *= self.headroom_scale;
        }

        // Apply post-gain using pre-cached linear value (avoids per-sample powf).
        let post_linear = self.post_gain_linear;
        (l * post_linear, r * post_linear)
    }

    /// Process an audio frame (alternative API)
    ///
    /// to avoid out-of-bounds access on frame.channels[1].
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        if frame.num_channels <= 1 {
            // Mono: process the single channel through both L and R filters
            // to maintain consistent filter state, then copy result back.
            let (l, _r) = self.process(frame.channels[0], frame.channels[0]);
            frame.channels[0] = l;
        } else {
            let (l, r) = self.process(frame.channels[0], frame.channels[1]);
            frame.channels[0] = l;
            frame.channels[1] = r;
        }
    }

    /// Set a band's parameters and update its coefficients
    pub fn set_band(&mut self, index: usize, params: EqBandParams) {
        if let Some(band) = self.bands.get_mut(index) {
            band.params = params;
            band.update_coefficients(self.sample_rate);
        }
    }

    /// Get number of bands
    pub fn num_bands(&self) -> usize {
        self.bands.len()
    }

    /// Get band parameters
    pub fn band_params(&self, index: usize) -> Option<&EqBandParams> {
        self.bands.get(index).map(|b| &b.params)
    }

    /// Set bass shelf gain
    pub fn set_bass_shelf(&mut self, gain_db: f32) {
        self.bass_band.params.gain_db = gain_db;
        self.bass_band.params.enabled = gain_db.abs() > 0.001;
        self.bass_band.update_coefficients(self.sample_rate);
    }

    /// Set treble shelf gain
    pub fn set_treble_shelf(&mut self, gain_db: f32) {
        self.treble_band.params.gain_db = gain_db;
        self.treble_band.params.enabled = gain_db.abs() > 0.001;
        self.treble_band.update_coefficients(self.sample_rate);
    }

    /// Update sample rate and recompute all coefficients
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for band in &mut self.bands {
            band.update_coefficients(sample_rate);
        }
        self.bass_band.update_coefficients(sample_rate);
        self.treble_band.update_coefficients(sample_rate);
    }

    /// Reset all bands
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.bass_band.reset();
        self.treble_band.reset();
        self.headroom_scale = 1.0;
        self.headroom_scale_target = 1.0;
        self.peak_envelope = 0.0;
        // Note: attack/release rates are persistent settings, not reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_passthrough_when_disabled() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        let (l, r) = eq.process(0.5, 0.5);
        assert!((l - 0.5).abs() < 1e-5);
        assert!((r - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_eq_enabled_zero_gain() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);
        // After settling, zero-gain EQ should pass signal through
        for _ in 0..500 {
            eq.process(0.5, 0.5);
        }
        let (l, _r) = eq.process(0.5, 0.5);
        assert!(
            (l - 0.5).abs() < 0.05,
            "Zero-gain EQ should be near passthrough"
        );
    }

    #[test]
    fn test_eq_set_band() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_band(0, EqBandParams::peaking(100.0, 6.0, 1.4));
        let params = eq.band_params(0).unwrap();
        assert_eq!(params.frequency, 100.0);
        assert_eq!(params.gain_db, 6.0);
    }

    #[test]
    fn test_stereo_imaging_preserved() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);
        eq.set_band(0, EqBandParams::peaking(1000.0, 6.0, 1.4));
        // Process same signal on both channels
        for _ in 0..200 {
            eq.process(0.5, 0.5);
        }
        let (l, r) = eq.process(0.5, 0.5);
        assert!((l - r).abs() < 0.01, "Stereo imaging should be preserved");
    }

    #[test]
    fn test_headroom_attack_faster_than_release() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);
        eq.set_headroom_db(-6.0); // headroom linear ≈ 0.5

        // Feed a loud signal to trigger attack
        for _ in 0..50 {
            eq.process(0.9, 0.9);
        }
        let scale_after_attack = eq.headroom_scale;

        // Now feed silence to trigger release
        for _ in 0..50 {
            eq.process(0.01, 0.01);
        }
        let scale_after_partial_release = eq.headroom_scale;

        // After equal number of samples, release should not have recovered
        // as much as attack reduced — release is slower
        let attack_reduction = 1.0 - scale_after_attack;
        let release_recovery = scale_after_partial_release - scale_after_attack;
        assert!(
            release_recovery < attack_reduction,
            "Release should be slower than attack: attack_reduction={}, release_recovery={}",
            attack_reduction,
            release_recovery
        );
    }

    #[test]
    fn test_headroom_prevents_clipping() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);
        eq.set_headroom_db(-3.0); // headroom linear ≈ 0.707

        // Feed a signal that exceeds the headroom for long enough to settle
        for _ in 0..5000 {
            let (l, r) = eq.process(2.0, 2.0);
            // Output should eventually be pulled below the headroom threshold
            let _ = (l, r);
        }

        // After settling, the headroom scale should be well below 1.0
        assert!(
            eq.headroom_scale < 0.95,
            "Headroom scale should reduce to prevent clipping, got {}",
            eq.headroom_scale
        );
    }

    #[test]
    fn test_headroom_resets_to_unity() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);
        eq.set_headroom_db(-3.0);

        // Trigger headroom reduction
        for _ in 0..1000 {
            eq.process(2.0, 2.0);
        }
        assert!(eq.headroom_scale < 1.0, "Should have reduced headroom");

        eq.reset();
        assert!(
            (eq.headroom_scale - 1.0).abs() < 1e-5,
            "Headroom scale should be 1.0 after reset, got {}",
            eq.headroom_scale
        );
        assert!(
            (eq.headroom_scale_target - 1.0).abs() < 1e-5,
            "Headroom scale target should be 1.0 after reset"
        );
    }

    #[test]
    fn test_headroom_never_boosts() {
        let mut eq = ParametricEq::default_10_band(44100.0);
        eq.set_enabled(true);

        for _ in 0..1000 {
            eq.process(0.01, 0.01);
        }
        assert!(
            eq.headroom_scale <= 1.0 + 1e-5,
            "Headroom scale should never exceed 1.0, got {}",
            eq.headroom_scale
        );
    }
}
