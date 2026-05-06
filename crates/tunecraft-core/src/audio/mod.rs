//! Audio subsystem for TuneCraft.
//!
//! # Issue #24: Duplicate filenames between `audio/` and `audio/engine/`
//!
//! Several modules exist at both the top level (`audio/crossfade.rs`,
//! `audio/convolution.rs`, `audio/loudness.rs`, `audio/replaygain.rs`,
//! `audio/gapless.rs`) and as submodules of `engine/` (`engine/crossfade.rs`,
//! `engine/convolution.rs`, etc.). The top-level files are thin wrappers /
//! public API re-exports that delegate to the engine submodules which contain
//! the actual implementations. This is intentional: the top-level modules
//! provide the public API surface, while the engine/ submodules contain the
//! AudioEngine-specific methods that operate on the engine's internal state.
//! Future refactoring could merge these, but the current split keeps the
//! public API separate from the engine's private methods.

pub mod convolution;
pub mod crossfade;
pub mod dsp;
pub mod dsp_thread;
pub mod engine;
pub mod equalizer;
pub mod exclusive;
pub mod gapless;
pub mod genre_preset;
pub mod loudness;
pub mod output;
pub mod pcm_cache;
pub mod pipeline;
pub mod replaygain;
pub mod resampler;

pub use convolution::ConvolutionEngine;
pub use crossfade::CrossfadeEngine;
pub use dsp::{
    Biquad, DspEngine, EqBandParams, GaplessSmoother, Limiter, MsEqBand, TpdfDither, MAX_EQ_BANDS,
    MAX_MS_EQ_BANDS,
};
pub use dsp_thread::DspThreadConfig;
pub use engine::{
    AudioEngine, DurationCallback, EndOfStreamCallback, PlayerState, PositionCallback,
    StateCallback,
};
pub use equalizer::{
    load_autoeq_profile, AutoEqFilter, EqBand, EqualizerState, OutputDeviceId, OutputPresetStore,
};
pub use exclusive::ExclusiveAudioOutput;
pub use gapless::GaplessPreloader;
pub use genre_preset::{GenrePreset, GenrePresetManager};
pub use loudness::{EbuR128Loudness, LoudnessNormalizationConfig};
pub use pcm_cache::{PcmBuffer, PcmCache};
pub use replaygain::{ReplayGainApplyMode, ReplayGainConfig, ReplayGainInfo, ReplayGainMode};
pub use resampler::{resample_once, Resampler, ResamplerQuality};
