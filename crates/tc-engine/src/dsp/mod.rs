//! Digital Signal Processing module — EQ, limiter, loudness, resampler, and the full pipeline.

pub mod biquad;
pub mod convolution;
pub mod crossfade;
pub mod crossfeed;
pub mod dither;
pub mod equalizer;
pub mod gain;
pub mod limiter;
pub mod loudness;
pub mod multiband_compressor;
pub mod pipeline;
#[cfg(feature = "resample")]
pub mod resampler;
pub mod stereo;
pub mod time_stretch;

pub use biquad::{BiquadCoeffs, BiquadState, FilterType, SmoothedBiquad};
pub use convolution::ConvolutionEngine;
pub use crossfade::{CrossfadeConfig, CrossfadeCurve, MixerState, TrackMixer};
pub use dither::{Dither, DitherType};
pub use equalizer::{EqBandParams, EqFilterType, ParametricEq, MAX_EQ_BANDS};
pub use gain::{FadeProcessor, FadeState, GainProcessor};
pub use limiter::LookaheadLimiter;
pub use loudness::{LoudnessMetadata, LoudnessMode, LoudnessNormalizer};
pub use pipeline::DspPipeline;
#[cfg(feature = "resample")]
pub use resampler::AudioResampler;
#[cfg(feature = "resample")]
pub use resampler::ResamplerError;
pub use stereo::StereoEnhancer;

pub use crossfeed::Crossfeed;
pub use multiband_compressor::MultibandCompressor;
pub use time_stretch::TimeStretcher;
