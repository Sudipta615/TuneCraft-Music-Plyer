//! Main DSP pipeline — chains all processing stages
//!
//! The pipeline processes audio in this order for single-track playback:
//!
//! ```text
//! Preamp → Loudness Normalization → Parametric EQ (with optional M/S mode) →
//! Balance Control → Stereo Enhancer → Lookahead Limiter → Volume Control →
//! Seek Fade → Dither → Output
//! ```
//!
//! v0.20.0: Added split processing methods for dual-stream crossfading:
//! - `process_outgoing()`: Preamp → Loudness → EQ → Balance → StereoEnhancer
//! - `process_incoming()`: Preamp → Loudness → EQ → Balance → StereoEnhancer
//! - `process_post_mix()`: Limiter → Volume → SeekFade → Dither
//!
//! During a crossfade, the outgoing and incoming tracks are processed
//! independently through the first half of the pipeline, then the mixer
//! combines them, and the mixed signal goes through the second half.

use tc_config::{EngineConfig, LoudnessMode as ConfigLoudnessMode, PerformanceMode};

use crate::dsp::{
    convolution::ConvolutionEngine,
    crossfade::TrackMixer,
    dither::{Dither, DitherType},
    equalizer::{EqBandParams, EqFilterType, ParametricEq},
    gain::{FadeProcessor, GainProcessor},
    limiter::LookaheadLimiter,
    loudness::{LoudnessMetadata, LoudnessMode, LoudnessNormalizer},
    stereo::StereoEnhancer,
};

/// L11: Volume ramp duration, replacing the previously hardcoded 10ms magic number.
const VOLUME_RAMP_DURATION_MS: f64 = 10.0;
/// L12: Preamp gain ramp duration. Previously hardcoded as 10.0 at the
/// call site. Now a named constant so it stays in sync with VOLUME_RAMP_DURATION_MS.
const PREAMP_RAMP_DURATION_MS: f64 = VOLUME_RAMP_DURATION_MS;

/// The complete DSP processing pipeline

/// All processing is done in f64 for maximum precision. The pipeline is
/// designed for zero allocation in the hot path.
pub struct DspPipeline {
    preamp: GainProcessor,
    loudness: LoudnessNormalizer,
    eq: ParametricEq,
    convolution: ConvolutionEngine,
    stereo_enhancer: StereoEnhancer,
    mixer: TrackMixer,
    limiter: LookaheadLimiter,
    volume: GainProcessor,
    dither: Dither,
    seek_fade: FadeProcessor,
    sample_rate: f64,
    performance_mode: PerformanceMode,
    speed: f64,
    /// Stereo balance: -1.0 = full left, 0.0 = center, +1.0 = full right
    balance: f64,
    /// Left channel gain derived from the balance setting (constant-power pan law).
    balance_gain_l: f64,
    /// Right channel gain derived from the balance setting (constant-power pan law).
    balance_gain_r: f64,
    /// Whether Mid/Side EQ mode is active.
    midside_eq_enabled: bool,
    /// Volume fade duration from EngineConfig, stored so set_sample_rate()
    /// can recompute the slew rate without overwriting it with the hardcoded
    /// VOLUME_RAMP_DURATION_MS constant (fixes config being silently ignored).
    volume_fade_ms: f64,
}

impl DspPipeline {
    /// Construct a DSP pipeline from an [`EngineConfig`].
    pub fn from_config(config: &EngineConfig, sample_rate: f64) -> Self {
        let mut eq = ParametricEq::default_10_band(sample_rate);
        eq.set_enabled(config.eq.enabled);
        eq.set_preamp_db(config.eq.preamp_db);
        eq.set_post_gain_db(config.eq.post_gain_db);
        eq.set_headroom_db(config.eq.headroom_db);

        // Apply EQ band settings from config
        for (i, band_cfg) in config.eq.bands.iter().enumerate() {
            if i >= eq.num_bands() {
                break;
            }
            let filter_type = match band_cfg.filter_type {
                tc_config::FilterType::Peaking => EqFilterType::Peaking,
                tc_config::FilterType::LowShelf => EqFilterType::LowShelf,
                tc_config::FilterType::HighShelf => EqFilterType::HighShelf,
                tc_config::FilterType::LowPass => EqFilterType::LowPass,
                tc_config::FilterType::HighPass => EqFilterType::HighPass,
                tc_config::FilterType::Notch => EqFilterType::Notch,
            };
            eq.set_band(
                i,
                EqBandParams {
                    enabled: band_cfg.enabled,
                    filter_type,
                    frequency: band_cfg.frequency,
                    gain_db: band_cfg.gain_db,
                    q: band_cfg.q,
                },
            );
        }

        let mut loudness = LoudnessNormalizer::new(sample_rate);
        loudness.set_mode(match config.loudness.mode {
            ConfigLoudnessMode::Off => LoudnessMode::Off,
            ConfigLoudnessMode::TrackReplayGain => LoudnessMode::TrackReplayGain,
            ConfigLoudnessMode::AlbumReplayGain => LoudnessMode::AlbumReplayGain,
            ConfigLoudnessMode::EbuR128 => LoudnessMode::EbuR128,
        });
        loudness.set_target_lufs(config.loudness.target_lufs);
        loudness.set_true_peak_guard(
            config.loudness.true_peak_guard,
            config.loudness.true_peak_dbtp,
        );

        let mut limiter = LookaheadLimiter::new_with_params(
            sample_rate,
            config.limiter.lookahead_ms,
            config.limiter.attack_ms,
            config.limiter.release_ms,
            config.limiter.ceiling_db,
            config.limiter.soft_clip,
        );
        limiter.set_enabled(config.limiter.enabled);

        let mut stereo_enhancer = StereoEnhancer::new();
        stereo_enhancer.set_enabled(config.stereo_enhancer.enabled);
        stereo_enhancer.set_width(config.stereo_enhancer.width);

        let dither = if config.dither_enabled {
            Dither::new(DitherType::Triangular, 24)
        } else {
            Dither::new(DitherType::None, 32)
        };

        // Initialize convolution engine with max IR length of 8192 samples
        let mut convolution = ConvolutionEngine::new(sample_rate, 8192);
        if config.convolution.enabled {
            convolution.set_enabled(true);
            convolution.set_wet_mix(config.convolution.wet_mix);
            // Try loading IR from configured path
            if let Some(ref ir_path) = config.convolution.ir_path {
                let path = std::path::Path::new(ir_path);
                if path.exists() {
                    match convolution.load_ir_from_file(path) {
                        Ok(()) => log::info!("Loaded IR: {}", ir_path.display()),
                        Err(e) => log::warn!("Failed to load IR {}: {}", ir_path.display(), e),
                    }
                } else {
                    log::warn!("IR file not found: {}", ir_path.display());
                }
            }
        }

        // Initialize crossfade/gapless mixer
        let mixer = TrackMixer::new(sample_rate);

        let mut pipeline = Self {
            preamp: GainProcessor::with_ramp(1.0, PREAMP_RAMP_DURATION_MS, sample_rate),
            loudness,
            eq,
            convolution,
            stereo_enhancer,
            mixer,
            limiter,
            volume: GainProcessor::with_ramp(1.0, config.volume_fade_ms as f64, sample_rate),
            dither,
            seek_fade: FadeProcessor::new(config.seek_fade_ms as f64, sample_rate),
            sample_rate,
            performance_mode: config.performance_mode,
            speed: 1.0,
            balance: 0.0,
            balance_gain_l: 1.0, // center: both gains at unity
            balance_gain_r: 1.0, // center: both gains at unity
            midside_eq_enabled: false,
            volume_fade_ms: config.volume_fade_ms as f64,
        };

        pipeline.apply_performance_mode();
        pipeline
    }

    fn apply_performance_mode(&mut self) {
        match self.performance_mode {
            PerformanceMode::UltraQuality => {},
            PerformanceMode::Balanced => {},
            PerformanceMode::LowPower => {
                self.stereo_enhancer.set_enabled(false);
                self.dither.set_enabled(false);
            },
        }
    }

    /// Process a single stereo frame through the entire pipeline.
    /// Used for single-track (non-crossfading) playback.
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let (l, r) = self.process_pre_mix(left, right);
        // In Single mode, the mixer is in PlayingCurrent and simply
        // passes through. We skip the mixer call entirely.
        self.process_post_mix(l, r)
    }

    /// Process the first half of the pipeline (pre-mix stages):
    /// Preamp → Loudness → EQ → Convolution → Balance → StereoEnhancer
    ///
    /// This is used for both outgoing and incoming tracks during crossfade,
    /// allowing each to be processed independently before mixing.
    #[inline]
    pub fn process_outgoing(&mut self, left: f64, right: f64) -> (f64, f64) {
        self.process_pre_mix(left, right)
    }

    /// Alias for process_outgoing — processes an incoming track through
    /// the same pre-mix stages. Both tracks receive identical DSP treatment
    /// (same EQ, loudness, etc.) since they share the pipeline config.
    #[inline]
    pub fn process_incoming(&mut self, left: f64, right: f64) -> (f64, f64) {
        self.process_pre_mix(left, right)
    }

    /// Internal: process all stages up to (but not including) the mixer.
    #[inline]
    fn process_pre_mix(&mut self, left: f64, right: f64) -> (f64, f64) {
        let (l, r) = self.preamp.process_stereo(left, right);
        let (l, r) = self.loudness.process(l, r);

        // Mid/Side EQ mode: encode to M/S, apply EQ, decode back to L/R
        let (l, r) = if self.midside_eq_enabled {
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5;
            let (eq_mid, eq_side) = self.eq.process(mid, side);
            (eq_mid + eq_side, eq_mid - eq_side)
        } else {
            self.eq.process(l, r)
        };

        let (l, r) = self.convolution.process(l, r);

        let (l, r) = if self.balance.abs() > 0.001 {
            (l * self.balance_gain_l, r * self.balance_gain_r)
        } else {
            (l, r)
        };

        let (l, r) = self.stereo_enhancer.process(l, r);
        (l, r)
    }

    /// Process the second half of the pipeline (post-mix stages):
    /// Limiter → Volume → SeekFade → Dither
    ///
    /// This is applied to the mixed (crossfaded) output signal.
    #[inline]
    pub fn process_post_mix(&mut self, left: f64, right: f64) -> (f64, f64) {
        let (l, r) = self.limiter.process(left, right);
        let (l, r) = self.volume.process_stereo(l, r);
        let (l, r) = self.seek_fade.process(l, r);
        let (l, r) = self.dither.process(l, r);
        (l, r)
    }

    /// Process a batch of stereo frames
    pub fn process_batch(&mut self, frames: &mut [(f64, f64)]) {
        for frame in frames.iter_mut() {
            *frame = self.process(frame.0, frame.1);
        }
    }

    /// Set volume (0.0 to 1.0) with smooth ramping
    pub fn set_volume(&mut self, volume: f64) {
        self.volume.set_gain(volume.clamp(0.0, 1.5));
    }

    /// Get the current volume (linear)
    pub fn volume(&self) -> f64 {
        self.volume.current_gain()
    }

    /// Set playback speed.
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.25, 4.0);
    }

    /// Get the current playback speed setting
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Update EQ band parameters
    pub fn set_eq_band(&mut self, index: usize, params: EqBandParams) {
        self.eq.set_band(index, params);
    }

    /// Enable/disable the EQ
    pub fn set_eq_enabled(&mut self, enabled: bool) {
        self.eq.set_enabled(enabled);
    }

    /// Whether EQ is enabled
    pub fn is_eq_enabled(&self) -> bool {
        self.eq.is_enabled()
    }

    /// Update loudness metadata for the current track
    pub fn set_loudness_metadata(&mut self, meta: &LoudnessMetadata) {
        self.loudness.set_track_metadata(meta);
    }

    /// Set the loudness normalization mode
    pub fn set_loudness_mode(&mut self, mode: LoudnessMode) {
        self.loudness.set_mode(mode);
    }

    /// Begin a seek fade-out
    pub fn begin_seek_fade_out(&mut self) {
        self.seek_fade.fade_out();
    }

    /// Begin a seek fade-in
    pub fn begin_seek_fade_in(&mut self) {
        self.seek_fade.fade_in();
    }

    /// Whether the seek fade-out has completed
    pub fn is_seek_faded_out(&self) -> bool {
        self.seek_fade.is_faded_out()
    }

    /// Get the current limiter gain reduction in dB
    pub fn limiter_gain_reduction_db(&self) -> f64 {
        self.limiter.gain_reduction_db()
    }

    /// Get the current loudness gain in dB
    pub fn loudness_gain_db(&self) -> f64 {
        self.loudness.current_gain_db()
    }

    /// Update the sample rate
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.eq.set_sample_rate(sample_rate);
        self.limiter.set_sample_rate(sample_rate);
        self.loudness.set_sample_rate(sample_rate);
        self.seek_fade.set_sample_rate(sample_rate);
        self.convolution.set_sample_rate(sample_rate);

        self.preamp
            .set_slew_rate(1.0 / (VOLUME_RAMP_DURATION_MS * 0.001 * sample_rate));

        self.volume
            .set_slew_rate(1.0 / (self.volume_fade_ms * 0.001 * sample_rate));

        // Recalculate the crossfade frame count for the new sample rate so that
        // the configured crossfade duration in milliseconds stays accurate after
        // a device change. Without this the mixer still uses the old frame count
        // which was computed for the previous (possibly different) sample rate.
        let crossfade_ms = self.mixer.duration_ms(self.sample_rate.max(1.0));
        // Reuse set_duration_ms so all derived state (duration_frames) is updated atomically.
        self.mixer.set_duration_ms(crossfade_ms, sample_rate);
    }

    /// Reset all processing state
    pub fn reset(&mut self) {
        self.preamp.reset();
        self.loudness.reset();
        self.eq.reset();
        self.convolution.reset();
        self.stereo_enhancer.reset();
        self.mixer.reset();
        self.limiter.reset();
        self.volume.reset();
        self.dither.reset();
        self.seek_fade.reset();
    }

    /// Enable or disable the convolution engine
    pub fn set_convolution_enabled(&mut self, enabled: bool) {
        self.convolution.set_enabled(enabled);
    }

    /// Set stereo balance (-1.0 = full left, 0.0 = center, +1.0 = full right)
    pub fn set_balance(&mut self, balance: f64) {
        self.balance = balance.clamp(-1.0, 1.0);

        let angle = (self.balance + 1.0) * std::f64::consts::FRAC_PI_4;
        self.balance_gain_l = angle.cos();
        self.balance_gain_r = angle.sin();
    }

    /// Get the current balance setting
    pub fn balance(&self) -> f64 {
        self.balance
    }

    /// Enable or disable Mid/Side EQ mode
    pub fn set_midside_eq(&mut self, enabled: bool) {
        self.midside_eq_enabled = enabled;
    }

    /// Whether Mid/Side EQ mode is active
    pub fn is_midside_eq(&self) -> bool {
        self.midside_eq_enabled
    }

    /// Set the convolution wet/dry mix
    pub fn set_convolution_wet_mix(&mut self, mix: f64) {
        self.convolution.set_wet_mix(mix);
    }

    pub fn eq_mut(&mut self) -> &mut crate::dsp::equalizer::ParametricEq {
        &mut self.eq
    }
    pub fn eq_ref(&self) -> &crate::dsp::equalizer::ParametricEq {
        &self.eq
    }
    pub fn limiter_mut(&mut self) -> &mut crate::dsp::limiter::LookaheadLimiter {
        &mut self.limiter
    }
    pub fn stereo_enhancer_mut(&mut self) -> &mut crate::dsp::stereo::StereoEnhancer {
        &mut self.stereo_enhancer
    }
    pub fn mixer_mut(&mut self) -> &mut crate::dsp::crossfade::TrackMixer {
        &mut self.mixer
    }
    pub fn dither_mut(&mut self) -> &mut crate::dsp::dither::Dither {
        &mut self.dither
    }

    /// Whether the loaded convolution IR needs to be reloaded due to a
    /// sample rate change. The UI should display a warning when true.
    pub fn convolution_ir_needs_reload(&self) -> bool {
        self.convolution.ir_needs_reload()
    }

    /// Set stereo width. Disables the enhancer if width is 1.0 (unity).
    pub fn set_stereo_width(&mut self, width: f64) {
        self.stereo_enhancer
            .set_enabled((width - 1.0).abs() > 0.001);
        self.stereo_enhancer.set_width(width);
    }

    /// Enable or disable dither via the pipeline API.
    pub fn set_dither_enabled(&mut self, enabled: bool) {
        self.dither.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_passthrough() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        for _ in 0..1000 {
            pipeline.process(0.1, 0.1);
        }
        let (l, r) = pipeline.process(0.1, 0.1);
        assert!(
            (l - 0.1).abs() < 0.05,
            "Small signal should pass through pipeline, got l={}",
            l
        );
        assert!(
            (r - 0.1).abs() < 0.05,
            "Small signal should pass through pipeline, got r={}",
            r
        );
    }

    #[test]
    fn test_pipeline_volume_control() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        pipeline.set_volume(0.5);
        for _ in 0..10000 {
            pipeline.process(0.5, 0.5);
        }
        let (l, _r) = pipeline.process(0.5, 0.5);
        assert!(
            (l - 0.25).abs() < 0.05,
            "Volume 0.5 should roughly halve output, got {}",
            l
        );
    }

    #[test]
    fn test_pipeline_reset() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        for _ in 0..1000 {
            pipeline.process(0.8, 0.8);
        }
        pipeline.reset();
        let (l, r) = pipeline.process(0.1, 0.1);
        assert!(l.is_finite());
        assert!(r.is_finite());
    }

    #[test]
    fn test_pipeline_seek_fade() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        pipeline.begin_seek_fade_in();
        for _ in 0..50000 {
            pipeline.process(0.5, 0.5);
        }
        pipeline.begin_seek_fade_out();
        for _ in 0..50000 {
            pipeline.process(0.5, 0.5);
        }
        assert!(pipeline.is_seek_faded_out());
    }

    #[test]
    fn test_pipeline_batch_processing() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        let mut frames: [(f64, f64); 256] = [(0.1, 0.1); 256];
        pipeline.process_batch(&mut frames);
        for (l, r) in &frames {
            assert!(l.is_finite());
            assert!(r.is_finite());
        }
    }

    #[test]
    fn test_split_processing_pre_post_mix() {
        let config = EngineConfig::default();
        let mut pipeline = DspPipeline::from_config(&config, 44100.0);
        // Warm up the pipeline
        for _ in 0..1000 {
            pipeline.process(0.1, 0.1);
        }
        // Compare full pipeline vs split pipeline
        let (full_l, full_r) = pipeline.process(0.5, 0.5);
        let (pre_l, pre_r) = pipeline.process_outgoing(0.5, 0.5);
        let (split_l, split_r) = pipeline.process_post_mix(pre_l, pre_r);
        // Results should be very close (within floating-point tolerance)
        assert!(
            (full_l - split_l).abs() < 0.01,
            "Split processing should match full pipeline, got full={} split={}",
            full_l,
            split_l
        );
        assert!(
            (full_r - split_r).abs() < 0.01,
            "Split processing should match full pipeline for right channel"
        );
    }
}
