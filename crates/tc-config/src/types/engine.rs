use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::enums::{
    AudioBackend, CrossfadeCurve, CrossfeedProfile, FilterType, LoudnessMode, PerformanceMode,
    ResamplerQuality,
};

/// A single parametric EQ band configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub filter_type: FilterType,
    /// Hz, 20.0 - 20000.0
    #[serde(default = "EqBand::default_frequency")]
    pub frequency: f32,
    /// dB, -20.0 to +20.0
    #[serde(default)]
    pub gain_db: f32,
    /// Q factor, 0.1 to 30.0
    #[serde(default = "EqBand::default_q")]
    pub q: f32,
    /// Optional slope for shelf filters (dB/oct).
    /// NOTE: This field is currently unused by the DSP pipeline. Shelf filter
    /// slope is determined by the Q factor. The field is retained for forward
    /// compatibility but is not serialized when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope: Option<f32>,
}

impl EqBand {
    fn default_frequency() -> f32 {
        1000.0
    }
    fn default_q() -> f32 {
        1.414
    }

    /// Validate this EQ band's parameters, clamping out-of-range values.
    /// Returns a list of warnings for any values that were clamped.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Reject NaN/inf in frequency
        if self.frequency.is_nan() || self.frequency.is_infinite() {
            warnings.push(format!(
                "EQ band frequency is NaN/inf ({:.2}), resetting to 1000 Hz",
                self.frequency
            ));
            self.frequency = 1000.0;
        } else if self.frequency < 20.0 {
            warnings.push(format!(
                "EQ band frequency {:.2} Hz is below minimum 20 Hz, clamped",
                self.frequency
            ));
            self.frequency = 20.0;
        } else if self.frequency > 20000.0 {
            warnings.push(format!(
                "EQ band frequency {:.2} Hz is above maximum 20000 Hz, clamped",
                self.frequency
            ));
            self.frequency = 20000.0;
        }

        // Reject NaN/inf and zero in Q
        if self.q.is_nan() || self.q.is_infinite() || self.q <= 0.0 {
            warnings.push(format!(
                "EQ band Q ({:.4}) is invalid (must be > 0 and finite), resetting to 1.414",
                self.q
            ));
            self.q = 1.414;
        } else if self.q < 0.1 {
            warnings.push(format!(
                "EQ band Q ({:.4}) is below minimum 0.1, clamped",
                self.q
            ));
            self.q = 0.1;
        } else if self.q > 30.0 {
            warnings.push(format!(
                "EQ band Q ({:.4}) is above maximum 30.0, clamped",
                self.q
            ));
            self.q = 30.0;
        }

        // Reject NaN/inf in gain
        if self.gain_db.is_nan() || self.gain_db.is_infinite() {
            warnings.push(format!(
                "EQ band gain ({:.2} dB) is NaN/inf, resetting to 0 dB",
                self.gain_db
            ));
            self.gain_db = 0.0;
        } else if self.gain_db < -20.0 {
            warnings.push(format!(
                "EQ band gain ({:.2} dB) is below minimum -20 dB, clamped",
                self.gain_db
            ));
            self.gain_db = -20.0;
        } else if self.gain_db > 20.0 {
            warnings.push(format!(
                "EQ band gain ({:.2} dB) is above maximum +20 dB, clamped",
                self.gain_db
            ));
            self.gain_db = 20.0;
        }

        // Validate slope if present
        if let Some(s) = self.slope {
            if s.is_nan() || s.is_infinite() || s <= 0.0 {
                warnings.push(format!(
                    "EQ band slope ({:.2} dB/oct) is invalid, removing",
                    s
                ));
                self.slope = None;
            }
        }

        warnings
    }
}

impl Default for EqBand {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_type: FilterType::Peaking,
            frequency: 1000.0,
            gain_db: 0.0,
            q: 1.414, // Butterworth Q
            slope: None,
        }
    }
}

/// Equalizer configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bands: Vec<EqBand>,
    /// Pre-EQ gain
    #[serde(default)]
    pub preamp_db: f32,
    /// Post-EQ gain
    #[serde(default)]
    pub post_gain_db: f32,
    /// Headroom management
    #[serde(default = "EqConfig::default_headroom_db")]
    pub headroom_db: f32,
}

impl EqConfig {
    fn default_headroom_db() -> f32 {
        1.0
    }

    /// Create an EQ config with a specified number of evenly-spaced bands.
    /// Frequencies are logarithmically spaced from 31 Hz to 16000 Hz.
    pub fn with_band_count(n: usize) -> Self {
        if n == 0 {
            return Self {
                enabled: false,
                bands: vec![],
                preamp_db: 0.0,
                post_gain_db: 0.0,
                headroom_db: 1.0,
            };
        }

        let min_freq: f32 = 31.0;
        let max_freq: f32 = 16000.0;
        let log_min = min_freq.ln();
        let log_max = max_freq.ln();

        let bands: Vec<EqBand> = (0..n)
            .map(|i| {
                let t = if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.5
                };
                let freq = (log_min + t * (log_max - log_min)).exp();
                EqBand {
                    enabled: false,
                    filter_type: FilterType::Peaking,
                    frequency: freq,
                    gain_db: 0.0,
                    q: 1.414,
                    slope: None,
                }
            })
            .collect();

        Self {
            enabled: false,
            bands,
            preamp_db: 0.0,
            post_gain_db: 0.0,
            headroom_db: 1.0,
        }
    }

    /// Validate all EQ bands, clamping out-of-range values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Validate preamp
        if self.preamp_db.is_nan() || self.preamp_db.is_infinite() {
            warnings.push(format!(
                "EQ preamp ({:.2} dB) is NaN/inf, resetting to 0 dB",
                self.preamp_db
            ));
            self.preamp_db = 0.0;
        } else if self.preamp_db < -20.0 {
            warnings.push(format!(
                "EQ preamp ({:.2} dB) below -20 dB, clamped",
                self.preamp_db
            ));
            self.preamp_db = -20.0;
        } else if self.preamp_db > 20.0 {
            warnings.push(format!(
                "EQ preamp ({:.2} dB) above +20 dB, clamped",
                self.preamp_db
            ));
            self.preamp_db = 20.0;
        }

        // Validate post_gain
        if self.post_gain_db.is_nan() || self.post_gain_db.is_infinite() {
            warnings.push(format!(
                "EQ post_gain ({:.2} dB) is NaN/inf, resetting to 0 dB",
                self.post_gain_db
            ));
            self.post_gain_db = 0.0;
        } else if self.post_gain_db < -20.0 {
            warnings.push(format!(
                "EQ post_gain ({:.2} dB) below -20 dB, clamped",
                self.post_gain_db
            ));
            self.post_gain_db = -20.0;
        } else if self.post_gain_db > 20.0 {
            warnings.push(format!(
                "EQ post_gain ({:.2} dB) above +20 dB, clamped",
                self.post_gain_db
            ));
            self.post_gain_db = 20.0;
        }

        // Validate headroom
        if self.headroom_db.is_nan() || self.headroom_db.is_infinite() || self.headroom_db < 0.0 {
            warnings.push(format!(
                "EQ headroom ({:.2} dB) is invalid, resetting to 1.0 dB",
                self.headroom_db
            ));
            self.headroom_db = 1.0;
        }

        // Validate each band
        for (i, band) in self.bands.iter_mut().enumerate() {
            for w in band.validate() {
                warnings.push(format!("Band {}: {}", i, w));
            }
        }

        warnings
    }
}

impl Default for EqConfig {
    fn default() -> Self {
        // Default 10-band EQ at standard ISO frequencies
        let frequencies = [
            31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        let bands = frequencies
            .iter()
            .map(|&freq| EqBand {
                enabled: false,
                filter_type: FilterType::Peaking,
                frequency: freq,
                gain_db: 0.0,
                q: 1.414,
                slope: None,
            })
            .collect();
        Self {
            enabled: false,
            bands,
            preamp_db: 0.0,
            post_gain_db: 0.0,
            headroom_db: 1.0,
        }
    }
}

/// Loudness configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoudnessConfig {
    #[serde(default)]
    pub mode: LoudnessMode,
    /// Target loudness in LUFS (default -23)
    #[serde(default = "LoudnessConfig::default_target_lufs")]
    pub target_lufs: f32,
    #[serde(default = "LoudnessConfig::default_true_peak_guard")]
    pub true_peak_guard: bool,
    /// True peak ceiling in dBTP
    #[serde(default = "LoudnessConfig::default_true_peak_dbtp")]
    pub true_peak_dbtp: f32,
    #[serde(default)]
    pub preamp_db: f32,
}

impl LoudnessConfig {
    fn default_target_lufs() -> f32 {
        -23.0
    }
    fn default_true_peak_guard() -> bool {
        true
    }
    fn default_true_peak_dbtp() -> f32 {
        -1.0
    }

    /// Validate loudness config values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.target_lufs.is_nan() || self.target_lufs.is_infinite() {
            warnings.push(format!(
                "Loudness target_lufs ({:.2}) is NaN/inf, resetting to -23",
                self.target_lufs
            ));
            self.target_lufs = -23.0;
        } else if self.target_lufs < -70.0 || self.target_lufs > 0.0 {
            warnings.push(format!(
                "Loudness target_lufs ({:.2}) out of range [-70, 0], clamped",
                self.target_lufs
            ));
            self.target_lufs = self.target_lufs.clamp(-70.0, 0.0);
        }

        if self.true_peak_dbtp.is_nan() || self.true_peak_dbtp.is_infinite() {
            warnings.push(format!(
                "Loudness true_peak_dbtp ({:.2}) is NaN/inf, resetting to -1.0",
                self.true_peak_dbtp
            ));
            self.true_peak_dbtp = -1.0;
        } else if self.true_peak_dbtp > 0.0 {
            warnings.push(format!(
                "Loudness true_peak_dbtp ({:.2}) must be <= 0 dBTP, clamped",
                self.true_peak_dbtp
            ));
            self.true_peak_dbtp = 0.0;
        }

        if self.preamp_db.is_nan() || self.preamp_db.is_infinite() {
            warnings.push(format!(
                "Loudness preamp_db ({:.2}) is NaN/inf, resetting to 0",
                self.preamp_db
            ));
            self.preamp_db = 0.0;
        } else {
            self.preamp_db = self.preamp_db.clamp(-20.0, 20.0);
        }

        warnings
    }
}

impl Default for LoudnessConfig {
    fn default() -> Self {
        Self {
            mode: LoudnessMode::Off,
            target_lufs: -23.0,
            true_peak_guard: true,
            true_peak_dbtp: -1.0,
            preamp_db: 0.0,
        }
    }
}

/// Limiter configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimiterConfig {
    #[serde(default = "LimiterConfig::default_enabled")]
    pub enabled: bool,
    /// Output ceiling in dB
    #[serde(default = "LimiterConfig::default_ceiling_db")]
    pub ceiling_db: f32,
    /// Attack time in ms
    #[serde(default = "LimiterConfig::default_attack_ms")]
    pub attack_ms: f32,
    /// Release time in ms
    #[serde(default = "LimiterConfig::default_release_ms")]
    pub release_ms: f32,
    /// Lookahead time in ms
    #[serde(default = "LimiterConfig::default_lookahead_ms")]
    pub lookahead_ms: f32,
    /// Soft clipping safety mode
    #[serde(default)]
    pub soft_clip: bool,
}

impl LimiterConfig {
    fn default_enabled() -> bool {
        true
    }
    fn default_ceiling_db() -> f32 {
        -0.3
    }
    fn default_attack_ms() -> f32 {
        5.0
    }
    fn default_release_ms() -> f32 {
        50.0
    }
    fn default_lookahead_ms() -> f32 {
        5.0
    }

    /// Validate limiter config values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.ceiling_db.is_nan() || self.ceiling_db.is_infinite() {
            warnings.push("Limiter ceiling_db is NaN/inf, resetting to -0.3".to_string());
            self.ceiling_db = -0.3;
        } else if self.ceiling_db > 0.0 {
            warnings.push(format!(
                "Limiter ceiling_db ({:.2}) must be <= 0 dB, clamped",
                self.ceiling_db
            ));
            self.ceiling_db = 0.0;
        }

        if self.attack_ms <= 0.0 || self.attack_ms.is_nan() {
            warnings.push(format!(
                "Limiter attack_ms ({:.2}) must be > 0, resetting to 5.0",
                self.attack_ms
            ));
            self.attack_ms = 5.0;
        }

        if self.release_ms <= 0.0 || self.release_ms.is_nan() {
            warnings.push(format!(
                "Limiter release_ms ({:.2}) must be > 0, resetting to 50.0",
                self.release_ms
            ));
            self.release_ms = 50.0;
        }

        if self.lookahead_ms < 0.0 || self.lookahead_ms.is_nan() {
            warnings.push(format!(
                "Limiter lookahead_ms ({:.2}) must be >= 0, resetting to 5.0",
                self.lookahead_ms
            ));
            self.lookahead_ms = 5.0;
        }

        warnings
    }
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ceiling_db: -0.3,
            attack_ms: 5.0,
            release_ms: 50.0,
            lookahead_ms: 5.0,
            soft_clip: true,
        }
    }
}

/// Crossfade configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossfadeConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Crossfade duration in ms
    #[serde(default = "CrossfadeConfig::default_duration_ms")]
    pub duration_ms: u64,
    #[serde(default)]
    pub curve: CrossfadeCurve,
    /// Avoid cutting intros/outros
    #[serde(default = "CrossfadeConfig::default_smart_boundaries")]
    pub smart_boundaries: bool,
}

impl CrossfadeConfig {
    fn default_duration_ms() -> u64 {
        2000
    }
    fn default_smart_boundaries() -> bool {
        true
    }

    /// Validate crossfade config.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.duration_ms == 0 {
            warnings.push("Crossfade duration_ms is 0, resetting to 2000".to_string());
            self.duration_ms = 2000;
        } else if self.duration_ms > 30000 {
            warnings.push(format!(
                "Crossfade duration_ms ({}) exceeds 30000, clamped",
                self.duration_ms
            ));
            self.duration_ms = 30000;
        }
        warnings
    }
}

impl Default for CrossfadeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            duration_ms: 2000,
            curve: CrossfadeCurve::EqualPower,
            smart_boundaries: true,
        }
    }
}

/// Convolution configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvolutionConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Path to impulse response file
    #[serde(default)]
    pub ir_path: Option<PathBuf>,
    /// Wet/dry mix (0.0 - 1.0)
    #[serde(default = "ConvolutionConfig::default_wet_mix")]
    pub wet_mix: f32,
    /// Auto-disable when in low power mode
    #[serde(default = "ConvolutionConfig::default_auto_disable_low_power")]
    pub auto_disable_low_power: bool,
}

impl ConvolutionConfig {
    fn default_wet_mix() -> f32 {
        1.0
    }
    fn default_auto_disable_low_power() -> bool {
        true
    }

    /// Validate convolution config.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.wet_mix.is_nan() || self.wet_mix.is_infinite() {
            warnings.push(format!(
                "Convolution wet_mix ({:.2}) is NaN/inf, resetting to 1.0",
                self.wet_mix
            ));
            self.wet_mix = 1.0;
        } else {
            self.wet_mix = self.wet_mix.clamp(0.0, 1.0);
        }
        warnings
    }
}

impl Default for ConvolutionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ir_path: None,
            wet_mix: 1.0,
            auto_disable_low_power: true,
        }
    }
}

/// Stereo enhancer configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StereoEnhancerConfig {
    /// Disabled by default as required
    #[serde(default)]
    pub enabled: bool,
    /// 0.0 to 2.0, 1.0 = no change
    #[serde(default = "StereoEnhancerConfig::default_width")]
    pub width: f32,
}

impl StereoEnhancerConfig {
    fn default_width() -> f32 {
        1.0
    }

    /// Validate stereo enhancer config.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.width.is_nan() || self.width.is_infinite() {
            warnings.push(format!(
                "StereoEnhancer width ({:.2}) is NaN/inf, resetting to 1.0",
                self.width
            ));
            self.width = 1.0;
        } else {
            self.width = self.width.clamp(0.0, 2.0);
        }
        warnings
    }
}

impl Default for StereoEnhancerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 1.0,
        }
    }
}

/// Crossfeed configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossfeedConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: CrossfeedProfile,
    /// 0.0 to 1.0, blend level
    #[serde(default = "CrossfeedConfig::default_level")]
    pub level: f32,
}

impl CrossfeedConfig {
    fn default_level() -> f32 {
        1.0
    }

    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.level.is_nan() || self.level.is_infinite() {
            warnings.push(format!(
                "Crossfeed level ({:.2}) is NaN/inf, resetting to 1.0",
                self.level
            ));
            self.level = 1.0;
        } else {
            self.level = self.level.clamp(0.0, 1.0);
        }
        warnings
    }
}

impl Default for CrossfeedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: CrossfeedProfile::Bauer,
            level: 1.0,
        }
    }
}

/// Multiband Compressor configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultibandCompressorConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl MultibandCompressorConfig {
    pub fn validate(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl Default for MultibandCompressorConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// Time Stretch configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeStretchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "TimeStretchConfig::default_pitch_shift")]
    pub pitch_shift: f32,
}

impl TimeStretchConfig {
    fn default_pitch_shift() -> f32 {
        1.0
    }

    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.pitch_shift.is_nan() || self.pitch_shift.is_infinite() || self.pitch_shift <= 0.0 {
            warnings.push(format!(
                "TimeStretch pitch_shift ({:.2}) is invalid, resetting to 1.0",
                self.pitch_shift
            ));
            self.pitch_shift = 1.0;
        }
        warnings
    }
}

impl Default for TimeStretchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pitch_shift: 1.0,
        }
    }
}

/// Audio engine configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default)]
    pub performance_mode: PerformanceMode,
    #[serde(default)]
    pub eq: EqConfig,
    #[serde(default)]
    pub loudness: LoudnessConfig,
    #[serde(default)]
    pub limiter: LimiterConfig,
    #[serde(default)]
    pub crossfade: CrossfadeConfig,
    #[serde(default)]
    pub convolution: ConvolutionConfig,
    #[serde(default)]
    pub stereo_enhancer: StereoEnhancerConfig,
    #[serde(default)]
    pub crossfeed: CrossfeedConfig,
    #[serde(default)]
    pub multiband_compressor: MultibandCompressorConfig,
    #[serde(default)]
    pub time_stretch: TimeStretchConfig,
    #[serde(default = "EngineConfig::default_gapless_enabled")]
    pub gapless_enabled: bool,
    #[serde(default = "EngineConfig::default_seek_fade_ms")]
    pub seek_fade_ms: u64,
    #[serde(default = "EngineConfig::default_volume_fade_ms")]
    pub volume_fade_ms: u64,
    #[serde(default)]
    pub resampler_quality: ResamplerQuality,
    #[serde(default = "EngineConfig::default_dither_enabled")]
    pub dither_enabled: bool,
    #[serde(default = "EngineConfig::default_denormal_prevention")]
    pub denormal_prevention: bool,
    #[serde(default)]
    pub output_backend: AudioBackend,
}

impl EngineConfig {
    fn default_gapless_enabled() -> bool {
        true
    }
    fn default_seek_fade_ms() -> u64 {
        10
    }
    fn default_volume_fade_ms() -> u64 {
        50
    }
    fn default_dither_enabled() -> bool {
        false
    }
    fn default_denormal_prevention() -> bool {
        true
    }

    /// Apply performance mode suggestions (defaults for new users).
    ///
    /// Unlike the previous implementation, this method only sets suggested
    /// values for fields that have NOT been explicitly customized by the user.
    /// It should only be called during initial config creation, not on live
    /// config updates. For live updates, use `apply_performance_defaults()`
    /// which respects user overrides.
    pub fn apply_performance_mode(&mut self) {
        self.apply_performance_defaults();
    }

    /// Apply performance mode defaults without overriding user settings.
    ///
    /// This sets reasonable defaults based on the performance mode but does
    /// NOT forcibly change settings that the user has explicitly enabled or
    /// configured. The performance mode only provides "suggestions" for
    /// the resampler quality and dither settings.
    pub fn apply_performance_defaults(&mut self) {
        match self.performance_mode {
            PerformanceMode::UltraQuality => {
                self.resampler_quality = ResamplerQuality::HighQuality;
                // dither_enabled is suggested but not forced
            },
            PerformanceMode::Balanced => {
                self.resampler_quality = ResamplerQuality::Balanced;
            },
            PerformanceMode::LowPower => {
                self.resampler_quality = ResamplerQuality::Fast;
                // For LowPower, convolution and stereo enhancer are suggestions
                // rather than mandates. They can be overridden by the user.
                if self.convolution.auto_disable_low_power {
                    self.convolution.enabled = false;
                }
                if self.stereo_enhancer.width > 0.0 && !self.stereo_enhancer.enabled {
                    // Only suggest disabling if not already explicitly enabled
                }
            },
        }
    }

    /// Validate engine config values.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        warnings.extend(self.eq.validate());
        warnings.extend(self.loudness.validate());
        warnings.extend(self.limiter.validate());
        warnings.extend(self.crossfade.validate());
        warnings.extend(self.convolution.validate());
        warnings.extend(self.stereo_enhancer.validate());
        warnings.extend(self.crossfeed.validate());
        warnings.extend(self.multiband_compressor.validate());
        warnings.extend(self.time_stretch.validate());

        if self.seek_fade_ms > 5000 {
            warnings.push(format!(
                "seek_fade_ms ({}) exceeds 5000, clamped",
                self.seek_fade_ms
            ));
            self.seek_fade_ms = 5000;
        }

        if self.volume_fade_ms > 5000 {
            warnings.push(format!(
                "volume_fade_ms ({}) exceeds 5000, clamped",
                self.volume_fade_ms
            ));
            self.volume_fade_ms = 5000;
        }

        warnings
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            performance_mode: PerformanceMode::Balanced,
            eq: EqConfig::default(),
            loudness: LoudnessConfig::default(),
            limiter: LimiterConfig::default(),
            crossfade: CrossfadeConfig::default(),
            convolution: ConvolutionConfig::default(),
            stereo_enhancer: StereoEnhancerConfig::default(),
            crossfeed: CrossfeedConfig::default(),
            multiband_compressor: MultibandCompressorConfig::default(),
            time_stretch: TimeStretchConfig::default(),
            gapless_enabled: true,
            seek_fade_ms: 10,
            volume_fade_ms: 50,
            resampler_quality: ResamplerQuality::Balanced,
            dither_enabled: false,
            denormal_prevention: true,
            output_backend: AudioBackend::Auto,
        }
    }
}
