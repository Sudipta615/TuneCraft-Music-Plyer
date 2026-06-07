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

const VOLUME_RAMP_DURATION_MS: f32 = 10.0;
const PREAMP_RAMP_DURATION_MS: f32 = VOLUME_RAMP_DURATION_MS;

pub enum DspNode {
    Preamp(GainProcessor),
    Volume(GainProcessor),
    Loudness(LoudnessNormalizer),
    Eq(ParametricEq),
    MidSideEq(ParametricEq), // Wraps an EQ but processes in M/S space
    Convolution(ConvolutionEngine),
    Stereo(StereoEnhancer),
    Limiter(LookaheadLimiter),
    SeekFade(FadeProcessor),
    Dither(Dither),
    Balance(f32, f32), // Custom node for stereo balance (gain_l, gain_r)
}

impl DspNode {
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        match self {
            DspNode::Preamp(p) | DspNode::Volume(p) => p.process_stereo(l, r),
            DspNode::Loudness(p) => p.process(l, r),
            DspNode::Eq(p) => p.process(l, r),
            DspNode::MidSideEq(p) => {
                let mid = (l + r) * 0.5;
                let side = (l - r) * 0.5;
                let (eq_mid, eq_side) = p.process(mid, side);
                (eq_mid + eq_side, eq_mid - eq_side)
            },
            DspNode::Convolution(p) => p.process(l, r),
            DspNode::Stereo(p) => p.process(l, r),
            DspNode::Limiter(p) => p.process(l, r),
            DspNode::SeekFade(p) => p.process(l, r),
            DspNode::Dither(p) => p.process(l, r),
            DspNode::Balance(gain_l, gain_r) => (l * *gain_l, r * *gain_r),
        }
    }

    pub fn reset(&mut self) {
        match self {
            DspNode::Preamp(p) | DspNode::Volume(p) => p.reset(),
            DspNode::Loudness(p) => p.reset(),
            DspNode::Eq(p) | DspNode::MidSideEq(p) => p.reset(),
            DspNode::Convolution(p) => p.reset(),
            DspNode::Stereo(p) => p.reset(),
            DspNode::Limiter(p) => p.reset(),
            DspNode::SeekFade(p) => p.reset(),
            DspNode::Dither(p) => p.reset(),
            DspNode::Balance(_, _) => {},
        }
    }
}

pub struct DspPipeline {
    pub pre_mix_chain: Vec<DspNode>,
    pub post_mix_chain: Vec<DspNode>,
    pub mixer: TrackMixer,

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

        let preamp = GainProcessor::with_ramp(1.0, PREAMP_RAMP_DURATION_MS, sample_rate);
        let volume = GainProcessor::with_ramp(1.0, config.volume_fade_ms as f32, sample_rate);
        let seek_fade = FadeProcessor::new(config.seek_fade_ms as f32, sample_rate);

        let mut pre_mix_chain = vec![
            DspNode::Preamp(preamp),
            DspNode::Loudness(loudness),
            DspNode::Eq(eq), // We swap this to MidSideEq dynamically if enabled
            DspNode::Convolution(convolution),
            DspNode::Balance(1.0, 1.0),
            DspNode::Stereo(stereo_enhancer),
        ];

        let post_mix_chain = vec![
            DspNode::Limiter(limiter),
            DspNode::Volume(volume),
            DspNode::SeekFade(seek_fade),
            DspNode::Dither(dither),
        ];

        let mut pipeline = Self {
            pre_mix_chain,
            post_mix_chain,
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
            for node in self
                .pre_mix_chain
                .iter_mut()
                .chain(self.post_mix_chain.iter_mut())
            {
                match node {
                    DspNode::Stereo(p) => p.set_enabled(false),
                    DspNode::Dither(p) => p.set_enabled(false),
                    _ => {},
                }
            }
        }
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (l, r) = self.process_pre_mix(left, right);
        self.process_post_mix(l, r)
    }

    #[inline]
    pub fn process_outgoing(&mut self, left: f32, right: f32) -> (f32, f32) {
        self.process_pre_mix(left, right)
    }

    #[inline]
    pub fn process_incoming(&mut self, left: f32, right: f32) -> (f32, f32) {
        self.process_pre_mix(left, right)
    }

    #[inline]
    fn process_pre_mix(&mut self, mut l: f32, mut r: f32) -> (f32, f32) {
        for node in &mut self.pre_mix_chain {
            let (out_l, out_r) = node.process(l, r);
            l = out_l;
            r = out_r;
        }
        (l, r)
    }

    #[inline]
    pub fn process_post_mix(&mut self, mut l: f32, mut r: f32) -> (f32, f32) {
        for node in &mut self.post_mix_chain {
            let (out_l, out_r) = node.process(l, r);
            l = out_l;
            r = out_r;
        }
        (l, r)
    }

    pub fn process_batch(&mut self, frames: &mut [(f32, f32)]) {
        for frame in frames.iter_mut() {
            *frame = self.process(frame.0, frame.1);
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        for node in &mut self.post_mix_chain {
            if let DspNode::Volume(p) = node {
                p.set_gain(volume.clamp(0.0, 1.0));
            }
        }
    }

    pub fn volume(&self) -> f32 {
        for node in &self.post_mix_chain {
            if let DspNode::Volume(p) = node {
                return p.current_gain();
            }
        }
        1.0
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.25, 4.0);
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_eq_band(&mut self, index: usize, params: EqBandParams) {
        for node in &mut self.pre_mix_chain {
            match node {
                DspNode::Eq(p) | DspNode::MidSideEq(p) => p.set_band(index, params.clone()),
                _ => {},
            }
        }
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        for node in &mut self.pre_mix_chain {
            match node {
                DspNode::Eq(p) | DspNode::MidSideEq(p) => p.set_enabled(enabled),
                _ => {},
            }
        }
    }

    pub fn is_eq_enabled(&self) -> bool {
        for node in &self.pre_mix_chain {
            match node {
                DspNode::Eq(p) | DspNode::MidSideEq(p) => return p.is_enabled(),
                _ => {},
            }
        }
        false
    }

    pub fn set_loudness_metadata(&mut self, meta: &LoudnessMetadata) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Loudness(p) = node {
                p.set_track_metadata(meta);
            }
        }
    }

    pub fn set_loudness_mode(&mut self, mode: LoudnessMode) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Loudness(p) = node {
                p.set_mode(mode);
            }
        }
    }

    pub fn begin_seek_fade_out(&mut self) {
        for node in &mut self.post_mix_chain {
            if let DspNode::SeekFade(p) = node {
                p.fade_out();
            }
        }
    }

    pub fn begin_seek_fade_in(&mut self) {
        for node in &mut self.post_mix_chain {
            if let DspNode::SeekFade(p) = node {
                p.fade_in();
            }
        }
    }

    pub fn is_seek_faded_out(&self) -> bool {
        for node in &self.post_mix_chain {
            if let DspNode::SeekFade(p) = node {
                return p.is_faded_out();
            }
        }
        false
    }

    pub fn limiter_gain_reduction_db(&self) -> f32 {
        for node in &self.post_mix_chain {
            if let DspNode::Limiter(p) = node {
                return p.gain_reduction_db();
            }
        }
        0.0
    }

    pub fn loudness_gain_db(&self) -> f32 {
        for node in &self.pre_mix_chain {
            if let DspNode::Loudness(p) = node {
                return p.current_gain_db();
            }
        }
        0.0
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for node in self
            .pre_mix_chain
            .iter_mut()
            .chain(self.post_mix_chain.iter_mut())
        {
            match node {
                DspNode::Preamp(p) => {
                    p.set_slew_rate(1.0 / (PREAMP_RAMP_DURATION_MS * 0.001 * sample_rate))
                },
                DspNode::Volume(p) => {
                    p.set_slew_rate(1.0 / (self.volume_fade_ms * 0.001 * sample_rate))
                },
                DspNode::Loudness(p) => p.set_sample_rate(sample_rate),
                DspNode::Eq(p) | DspNode::MidSideEq(p) => p.set_sample_rate(sample_rate),
                DspNode::Convolution(p) => p.set_sample_rate(sample_rate),
                DspNode::Limiter(p) => p.set_sample_rate(sample_rate),
                DspNode::SeekFade(p) => p.set_sample_rate(sample_rate),
                _ => {},
            }
        }
        let crossfade_ms = self.mixer.duration_ms(self.sample_rate.max(1.0));
        self.mixer.set_duration_ms(crossfade_ms, sample_rate);
    }

    pub fn reset(&mut self) {
        for node in self
            .pre_mix_chain
            .iter_mut()
            .chain(self.post_mix_chain.iter_mut())
        {
            node.reset();
        }
        self.mixer.reset();
    }

    pub fn set_limiter_enabled(&mut self, enabled: bool) {
        for node in &mut self.post_mix_chain {
            if let DspNode::Limiter(p) = node {
                p.set_enabled(enabled);
            }
        }
    }

    pub fn set_preamp_db(&mut self, db: f32) {
        for node in &mut self.pre_mix_chain {
            match node {
                DspNode::Eq(p) | DspNode::MidSideEq(p) => p.set_preamp_db(db),
                _ => {},
            }
        }
    }

    pub fn mixer_mut(&mut self) -> &mut TrackMixer {
        &mut self.mixer
    }

    pub fn set_stereo_enhancer_enabled(&mut self, enabled: bool) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Stereo(p) = node {
                p.set_enabled(enabled);
            }
        }
    }

    pub fn set_convolution_enabled(&mut self, enabled: bool) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Convolution(p) = node {
                p.set_enabled(enabled);
            }
        }
    }

    pub fn set_balance(&mut self, balance: f32) {
        self.balance = balance.clamp(-1.0, 1.0);
        let angle = (self.balance + 1.0) * std::f32::consts::FRAC_PI_4;
        let gain_l = angle.cos();
        let gain_r = angle.sin();
        for node in &mut self.pre_mix_chain {
            if let DspNode::Balance(l, r) = node {
                *l = gain_l;
                *r = gain_r;
            }
        }
    }

    pub fn balance(&self) -> f32 {
        self.balance
    }

    pub fn set_midside_eq(&mut self, enabled: bool) {
        if self.midside_eq_enabled == enabled {
            return;
        }
        self.midside_eq_enabled = enabled;
        // Find the EQ and swap its wrapper type
        for node in &mut self.pre_mix_chain {
            let new_node = match node {
                DspNode::Eq(p) if enabled => Some(DspNode::MidSideEq(p.clone())),
                DspNode::MidSideEq(p) if !enabled => Some(DspNode::Eq(p.clone())),
                _ => None,
            };
            if let Some(n) = new_node {
                *node = n;
                break;
            }
        }
    }

    pub fn is_midside_eq(&self) -> bool {
        self.midside_eq_enabled
    }

    pub fn set_convolution_wet_mix(&mut self, mix: f32) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Convolution(p) = node {
                p.set_wet_mix(mix);
            }
        }
    }

    pub fn convolution_ir_needs_reload(&self) -> bool {
        for node in &self.pre_mix_chain {
            if let DspNode::Convolution(p) = node {
                return p.ir_needs_reload();
            }
        }
        false
    }

    pub fn set_stereo_width(&mut self, width: f32) {
        for node in &mut self.pre_mix_chain {
            if let DspNode::Stereo(p) = node {
                p.set_enabled((width - 1.0).abs() > 0.001);
                p.set_width(width);
            }
        }
    }

    pub fn set_dither_enabled(&mut self, enabled: bool) {
        for node in &mut self.post_mix_chain {
            if let DspNode::Dither(p) = node {
                p.set_enabled(enabled);
            }
        }
    }
}
