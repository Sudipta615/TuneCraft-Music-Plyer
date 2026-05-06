pub mod audio;
pub mod config;
pub mod database;
pub mod error;
pub mod library;
pub mod mood;
pub mod scrobbler;
pub mod util;

pub use audio::convolution::ConvolutionEngine;
pub use audio::dsp::{
    Biquad, DspEngine, EqBandParams, GaplessSmoother, Limiter, TpdfDither, MAX_EQ_BANDS,
};
pub use audio::equalizer::{load_autoeq_profile, AutoEqFilter, OutputDeviceId, OutputPresetStore};
pub use audio::genre_preset::{GenrePreset, GenrePresetManager};
pub use audio::pcm_cache::{PcmBuffer, PcmCache};
pub use audio::pipeline::BusEvent;
pub use audio::AudioEngine;
pub use database::models::{Playlist, Track};
pub use database::Database;
pub use error::{AudioError, DatabaseError, LyricsError, MoodError, PlaylistError, ScrobblerError};
#[cfg(feature = "lyrics")]
pub use library::lyrics;
pub use mood::{classify_mood, extract_features, extract_features_with_cache, Mood, SongFeatures};
#[cfg(feature = "lastfm")]
pub use scrobbler::lastfm::{LastfmClient, ScrobbleEntry};
#[cfg(feature = "lastfm")]
pub use scrobbler::ScrobbleManager;
pub use util::crypto::CryptoError;
pub use util::validation::{PathValidationError, UrlValidationError};
