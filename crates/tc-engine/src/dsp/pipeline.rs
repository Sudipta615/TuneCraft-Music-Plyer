use tc_config::{EngineConfig, LoudnessMode as ConfigLoudnessMode, PerformanceMode};

use crate::dsp::{
    convolution::ConvolutionEngine,
    crossfade::TrackMixer,
    crossfeed::Crossfeed,
    dither::{Dither, DitherType},
    equalizer::{EqBandParams, EqFilterType, ParametricEq},
    gain::{FadeProcessor, GainProcessor},
    limiter::LookaheadLimiter,
    loudness::{LoudnessMetadata, LoudnessMode, LoudnessNormalizer},
    multiband_compressor::MultibandCompressor,
    stereo::StereoEnhancer,
};

const VOLUME_RAMP_DURATION_MS: f32 = 10.0;
const PREAMP_RAMP_DURATION_MS: f32 = VOLUME_RAMP_DURATION_MS;

pub struct DspPipeline {
    // Pre-mix chain
    pub out_preamp: GainProcessor,
    pub out_loudness: LoudnessNormalizer,
    pub in_preamp: GainProcessor,
    pub in_loudness: LoudnessNormalizer,

    pub mixer: TrackMixer,

    // Post-mix chain
    pub eq: ParametricEq,
    pub multiband_compressor: MultibandCompressor,
    pub convolution: Box<ConvolutionEngine>,
    pub balance_gain_l: f32,
    pub balance_gain_r: f32,
    pub crossfeed: Crossfeed,
    pub stereo_enhancer: StereoEnhancer,
    pub limiter: LookaheadLimiter,
    pub volume: GainProcessor,
    pub seek_fade: FadeProcessor,
    pub dither: Dither,

    sample_rate: f32,
    performance_mode: PerformanceMode,
    speed: f32,
    balance: f32,
    midside_eq_enabled: bool,
    volume_fade_ms: f32,
}

impl DspPipeline {
    pub fn from_config(config: &EngineConfig, sample_rate: f32) -> Self {
        let mut eq = ParametricEq::default_10_band(sample_rate);
        eq.set_enabled(config.eq.enabled);
        eq.set_preamp_db(config.eq.preamp_db);
        eq.set_post_gain_db(config.eq.post_gain_db);
        eq.set_headroom_db(config.eq.headroom_db);

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

        let mode = match config.loudness.mode {
            ConfigLoudnessMode::Off => LoudnessMode::Off,
            ConfigLoudnessMode::TrackReplayGain => LoudnessMode::TrackReplayGain,
            ConfigLoudnessMode::AlbumReplayGain => LoudnessMode::AlbumReplayGain,
            ConfigLoudnessMode::EbuR128 => LoudnessMode::EbuR128,
        };

        let mut loudness_out = LoudnessNormalizer::new(sample_rate);
        loudness_out.set_mode(mode);
        loudness_out.set_target_lufs(config.loudness.target_lufs);
        loudness_out.set_true_peak_guard(
            config.loudness.true_peak_guard,
            config.loudness.true_peak_dbtp,
        );

        let mut loudness_in = LoudnessNormalizer::new(sample_rate);
        loudness_in.set_mode(mode);
        loudness_in.set_target_lufs(config.loudness.target_lufs);
        loudness_in.set_true_peak_guard(
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

        let mut crossfeed = Crossfeed::new(sample_rate);
        crossfeed.set_enabled(config.crossfeed.enabled);
        crossfeed.set_custom_params(
            config.crossfeed.custom_freq,
            config.crossfeed.custom_q,
            config.crossfeed.custom_delay_ms,
        );
        crossfeed.set_profile(config.crossfeed.profile);

        let mut multiband_compressor = MultibandCompressor::new(sample_rate);
        multiband_compressor.set_enabled(config.multiband_compressor.enabled);
        multiband_compressor.set_band_params(
            0,
            config.multiband_compressor.low_band.threshold_db,
            config.multiband_compressor.low_band.ratio,
            config.multiband_compressor.low_band.attack_ms,
            config.multiband_compressor.low_band.release_ms,
            config.multiband_compressor.low_band.makeup_gain_db,
        );
        multiband_compressor.set_band_params(
            1,
            config.multiband_compressor.mid_band.threshold_db,
            config.multiband_compressor.mid_band.ratio,
            config.multiband_compressor.mid_band.attack_ms,
            config.multiband_compressor.mid_band.release_ms,
            config.multiband_compressor.mid_band.makeup_gain_db,
        );
        multiband_compressor.set_band_params(
            2,
            config.multiband_compressor.high_band.threshold_db,
            config.multiband_compressor.high_band.ratio,
            config.multiband_compressor.high_band.attack_ms,
            config.multiband_compressor.high_band.release_ms,
            config.multiband_compressor.high_band.makeup_gain_db,
        );

        let dither = if config.dither_enabled {
            Dither::new(DitherType::Triangular, 24)
        } else {
            Dither::new(DitherType::None, 32)
        };

        let mut convolution = ConvolutionEngine::new(sample_rate, 8192);
        if config.convolution.enabled {
            convolution.set_enabled(true);
            convolution.set_wet_mix(config.convolution.wet_mix);
            if let Some(ref ir_path) = config.convolution.ir_path {
                let path = std::path::Path::new(ir_path);
                if path.exists() {
                    match convolution.load_ir_from_file(path) {
                        Ok(()) => log::info!("Loaded IR: {}", ir_path.display()),
                        Err(e) => log::warn!("Failed to load IR {}: {}", ir_path.display(), e),
                    }
                }
            }
        }

        let mixer = TrackMixer::new(sample_rate);

        let preamp_out = GainProcessor::with_ramp(1.0, PREAMP_RAMP_DURATION_MS, sample_rate);
        let preamp_in = GainProcessor::with_ramp(1.0, PREAMP_RAMP_DURATION_MS, sample_rate);
        let volume = GainProcessor::with_ramp(1.0, config.volume_fade_ms as f32, sample_rate);
        let seek_fade = FadeProcessor::new(config.seek_fade_ms as f32, sample_rate);

        let mut pipeline = Self {
            out_preamp: preamp_out,
            out_loudness: loudness_out,
            in_preamp: preamp_in,
            in_loudness: loudness_in,
            eq,
            multiband_compressor,
            convolution: Box::new(convolution),
            balance_gain_l: 1.0,
            balance_gain_r: 1.0,
            crossfeed,
            stereo_enhancer,
            limiter,
            volume,
            seek_fade,
            dither,
            mixer,
            sample_rate,
            performance_mode: config.performance_mode,
            speed: 1.0,
            balance: 0.0,
            midside_eq_enabled: false,
            volume_fade_ms: config.volume_fade_ms as f32,
        };

        pipeline.apply_performance_mode();
        pipeline
    }

    fn apply_performance_mode(&mut self) {
        if self.performance_mode == PerformanceMode::LowPower {
            self.stereo_enhancer.set_enabled(false);
            self.dither.set_enabled(false);
        }
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (l, r) = self.process_outgoing(left, right);
        self.process_post_mix(l, r)
    }

    pub fn mixer_mut(&mut self) -> &mut TrackMixer {
        &mut self.mixer
    }

    pub fn mixer(&self) -> &TrackMixer {
        &self.mixer
    }

    #[inline]
    pub fn process_outgoing(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (l, r) = self.out_preamp.process_stereo(left, right);
        self.out_loudness.process(l, r)
    }

    #[inline]
    pub fn process_incoming(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (l, r) = self.in_preamp.process_stereo(left, right);
        self.in_loudness.process(l, r)
    }

    #[inline]
    pub fn process_post_mix(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (mut l, mut r) = (left, right);
        if self.midside_eq_enabled {
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5;
            let (eq_mid, eq_side) = self.eq.process(mid, side);
            l = eq_mid + eq_side;
            r = eq_mid - eq_side;
        } else {
            let (eq_l, eq_r) = self.eq.process(l, r);
            l = eq_l;
            r = eq_r;
        }
        let (l_c, r_c) = self.multiband_compressor.process(l, r);
        let (l_cv, r_cv) = self.convolution.process(l_c, r_c);
        let (l_b, r_b) = (l_cv * self.balance_gain_l, r_cv * self.balance_gain_r);
        let (l_x, r_x) = self.crossfeed.process(l_b, r_b);
        let (l_s, r_s) = self.stereo_enhancer.process(l_x, r_x);
        let (l_lm, r_lm) = self.limiter.process(l_s, r_s);
        let (l_v, r_v) = self.volume.process_stereo(l_lm, r_lm);
        let (l_f, r_f) = self.seek_fade.process(l_v, r_v);
        self.dither.process(l_f, r_f)
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume.set_gain(volume.clamp(0.0, 1.0));
    }

    pub fn set_balance(&mut self, balance: f32) {
        self.balance = balance.clamp(-1.0, 1.0);
        let gain_l = if self.balance > 0.0 {
            1.0 - self.balance
        } else {
            1.0
        };
        let gain_r = if self.balance < 0.0 {
            1.0 + self.balance
        } else {
            1.0
        };
        self.balance_gain_l = gain_l;
        self.balance_gain_r = gain_r;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.25, 4.0);
    }

    pub fn begin_seek_fadeout(&mut self) {
        self.seek_fade.fade_out();
    }

    pub fn begin_seek_fadein(&mut self) {
        self.seek_fade.fade_in();
    }

    pub fn is_seek_fadeout_complete(&self) -> bool {
        self.seek_fade.is_faded_out()
    }

    pub fn apply_loudness_metadata_outgoing(&mut self, metadata: Option<LoudnessMetadata>) {
        self.out_loudness
            .set_track_metadata(&metadata.unwrap_or_default());
    }

    pub fn apply_loudness_metadata_incoming(&mut self, metadata: Option<LoudnessMetadata>) {
        self.in_loudness
            .set_track_metadata(&metadata.unwrap_or_default());
    }

    pub fn update_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() < 1.0 {
            return;
        }
        self.sample_rate = sample_rate;
        self.eq.set_sample_rate(sample_rate);
        self.out_loudness.set_sample_rate(sample_rate);
        self.in_loudness.set_sample_rate(sample_rate);
        self.multiband_compressor.set_sample_rate(sample_rate);
        self.convolution.set_sample_rate(sample_rate);
        self.crossfeed.set_sample_rate(sample_rate);
        self.limiter.set_sample_rate(sample_rate);
        self.out_preamp
            .set_slew_rate(1.0 / (PREAMP_RAMP_DURATION_MS * 0.001 * sample_rate));
        self.in_preamp
            .set_slew_rate(1.0 / (PREAMP_RAMP_DURATION_MS * 0.001 * sample_rate));
        self.volume
            .set_slew_rate(1.0 / (self.volume_fade_ms * 0.001 * sample_rate));
        self.seek_fade.set_sample_rate(sample_rate);
        let crossfade_ms = self.mixer.duration_ms(self.sample_rate.max(1.0));
        self.mixer.set_duration_ms(crossfade_ms, sample_rate);
    }

    pub fn reset(&mut self) {
        self.out_preamp.reset();
        self.out_loudness.reset();
        self.in_preamp.reset();
        self.in_loudness.reset();
        self.eq.reset();
        self.multiband_compressor.reset();
        self.convolution.reset();
        self.crossfeed.reset();
        self.stereo_enhancer.reset();
        self.limiter.reset();
        self.volume.reset();
        self.seek_fade.reset();
        self.dither.reset();
        self.mixer.reset();
    }

    pub fn set_limiter_enabled(&mut self, enabled: bool) {
        self.limiter.set_enabled(enabled);
    }

    pub fn set_preamp_db(&mut self, db: f32) {
        self.eq.set_preamp_db(db);
    }

    pub fn set_bass_shelf(&mut self, gain_db: f32) {
        self.eq.set_bass_shelf(gain_db);
    }

    pub fn set_treble_shelf(&mut self, gain_db: f32) {
        self.eq.set_treble_shelf(gain_db);
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        self.eq.set_enabled(enabled);
    }

    pub fn set_eq_band(&mut self, index: usize, params: EqBandParams) {
        self.eq.set_band(index, params);
    }

    pub fn eq_num_bands(&self) -> usize {
        self.eq.num_bands()
    }

    pub fn set_midside_eq(&mut self, enabled: bool) {
        self.midside_eq_enabled = enabled;
    }

    pub fn is_midside_eq(&self) -> bool {
        self.midside_eq_enabled
    }

    pub fn set_convolution_wet_mix(&mut self, mix: f32) {
        self.convolution.set_wet_mix(mix);
    }

    pub fn convolution_ir_needs_reload(&self) -> bool {
        self.convolution.ir_needs_reload()
    }

    pub fn set_stereo_width(&mut self, width: f32) {
        self.stereo_enhancer
            .set_enabled((width - 1.0).abs() > 0.001);
        self.stereo_enhancer.set_width(width);
    }

    pub fn set_stereo_enhancer_enabled(&mut self, enabled: bool) {
        self.stereo_enhancer.set_enabled(enabled);
    }

    pub fn set_dither_enabled(&mut self, enabled: bool) {
        self.dither.set_enabled(enabled);
    }

    pub fn set_limiter_params(
        &mut self,
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    ) {
        self.limiter.set_lookahead(lookahead_ms);
        self.limiter.set_attack(attack_ms);
        self.limiter.set_release(release_ms);
        self.limiter.set_ceiling_db(ceiling_db);
        self.limiter.set_soft_clip(soft_clip);
    }

    pub fn set_crossfeed_enabled(&mut self, enabled: bool) {
        self.crossfeed.set_enabled(enabled);
    }

    pub fn set_crossfeed_profile(&mut self, profile: tc_config::types::enums::CrossfeedProfile) {
        self.crossfeed.set_profile(profile);
    }

    pub fn set_crossfeed_custom_params(
        &mut self,
        frequency_hz: f32,
        q: f32,
        delay_ms: f32,
        _mix_db: f32,
    ) {
        self.crossfeed.set_custom_params(frequency_hz, q, delay_ms);
    }

    pub fn set_compressor_enabled(&mut self, enabled: bool) {
        self.multiband_compressor.set_enabled(enabled);
    }

    pub fn set_compressor_band_params(
        &mut self,
        band: usize,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    ) {
        self.multiband_compressor.set_band_params(
            band,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
        );
    }

    pub fn set_loudness_mode(&mut self, mode: LoudnessMode) {
        self.out_loudness.set_mode(mode);
        self.in_loudness.set_mode(mode);
    }
}
