pub mod audio;
pub mod library;
pub mod database;
pub mod config;
pub mod scrobbler;
pub mod mood;
pub mod util;
pub mod error;

pub use audio::AudioEngine;
pub use audio::dsp::{DspEngine, Biquad, Limiter, EqBandParams, TpdfDither, GaplessSmoother, MAX_EQ_BANDS};
pub use audio::equalizer::{OutputDeviceId, OutputPresetStore, AutoEqFilter, load_autoeq_profile};
pub use audio::convolution::ConvolutionEngine;
pub use audio::genre_preset::{GenrePresetManager, GenrePreset};
pub use audio::pcm_cache::{PcmCache, PcmBuffer};
pub use audio::pipeline::BusEvent;
pub use database::Database;
pub use database::models::{Track, Playlist};
pub use mood::{Mood, SongFeatures, classify_mood, extract_features, extract_features_with_cache};
pub use util::validation::{PathValidationError, UrlValidationError};
pub use util::crypto::CryptoError;
pub use error::{AudioError, DatabaseError, ScrobblerError, LyricsError, PlaylistError, MoodError};
#[cfg(feature = "lastfm")]
pub use scrobbler::ScrobbleManager;
#[cfg(feature = "lastfm")]
pub use scrobbler::lastfm::{LastfmClient, ScrobbleEntry};
#[cfg(feature = "lyrics")]
pub use library::lyrics;
