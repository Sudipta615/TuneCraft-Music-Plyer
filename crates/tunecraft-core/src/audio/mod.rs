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

pub mod engine;
pub mod equalizer;
pub mod crossfade;
pub mod replaygain;
pub mod resampler;
pub mod dsp;
pub mod pipeline;
pub mod output;
pub mod dsp_thread;
pub mod gapless;
pub mod loudness;
pub mod exclusive;
pub mod convolution;
pub mod genre_preset;
pub mod pcm_cache;

pub use engine::{AudioEngine, PlayerState, PositionCallback, StateCallback, EndOfStreamCallback, DurationCallback};
pub use equalizer::{EqBand, EqualizerState, OutputDeviceId, OutputPresetStore, AutoEqFilter, load_autoeq_profile};
pub use crossfade::CrossfadeEngine;
pub use replaygain::{ReplayGainInfo, ReplayGainMode, ReplayGainConfig, ReplayGainApplyMode};
pub use resampler::{Resampler, ResamplerQuality, resample_once};
pub use dsp::{DspEngine, Biquad, Limiter, EqBandParams, TpdfDither, GaplessSmoother, MAX_EQ_BANDS, MsEqBand, MAX_MS_EQ_BANDS};
pub use dsp_thread::DspThreadConfig;
pub use gapless::GaplessPreloader;
pub use loudness::{EbuR128Loudness, LoudnessNormalizationConfig};
pub use exclusive::ExclusiveAudioOutput;
pub use convolution::ConvolutionEngine;
pub use genre_preset::{GenrePresetManager, GenrePreset};
pub use pcm_cache::{PcmCache, PcmBuffer};
