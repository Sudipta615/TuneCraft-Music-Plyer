//! DSP preset quick-switch per genre.
//!
//! Provides genre-based DSP presets that configure EQ, stereo width,
//! bass/treble, and optional room correction IR paths. The `GenrePresetManager`
//! holds a collection of built-in defaults for common genres and allows
//! users to register custom presets or override built-ins.
//!
//! # Built-in Genres
//!
//! Rock, Pop, Jazz, Classical, Electronic, Hip-Hop, Metal, Acoustic,
//! R&B, Country, Latin, Bollywood.
//!
//! # Usage
//!
//! ```ignore
//! let manager = GenrePresetManager::new();
//! if let Some(preset) = manager.get("rock") {
//!     manager.apply_to_engine("rock", &engine);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::audio::equalizer::EqualizerState;
use crate::audio::AudioEngine;

/// A DSP preset for a specific genre.
///
/// Contains all the parameters needed to configure the audio engine
/// for optimal playback of a given genre: EQ state, stereo width,
/// bass/treble adjustments, and an optional room correction IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenrePreset {
    /// Genre name (e.g. "rock", "jazz", "electronic").
    pub genre: String,
    /// Full equalizer state (bands, MS EQ, dither, etc.).
    pub eq_state: EqualizerState,
    /// Stereo width: 0.0=mono, 1.0=original, >1.0=wider.
    pub stereo_width: f64,
    /// Bass shelf gain in dB.
    pub bass_db: f64,
    /// Treble shelf gain in dB.
    pub treble_db: f64,
    /// Optional path to a room correction impulse response WAV file.
    pub convolution_ir_path: Option<String>,
}

impl GenrePreset {
    /// Create a new genre preset with the given name and EQ state.
    pub fn new(genre: impl Into<String>, eq_state: EqualizerState) -> Self {
        Self {
            genre: genre.into(),
            eq_state,
            stereo_width: 1.0,
            bass_db: 0.0,
            treble_db: 0.0,
            convolution_ir_path: None,
        }
    }

    /// Set stereo width and return self for chaining.
    pub fn with_stereo_width(mut self, width: f64) -> Self {
        self.stereo_width = width;
        self
    }

    /// Set bass gain and return self for chaining.
    pub fn with_bass(mut self, bass_db: f64) -> Self {
        self.bass_db = bass_db;
        self
    }

    /// Set treble gain and return self for chaining.
    pub fn with_treble(mut self, treble_db: f64) -> Self {
        self.treble_db = treble_db;
        self
    }

    /// Set convolution IR path and return self for chaining.
    pub fn with_convolution_ir(mut self, path: impl Into<String>) -> Self {
        self.convolution_ir_path = Some(path.into());
        self
    }
}

/// Manager for genre-based DSP presets.
///
/// Holds a `HashMap<String, GenrePreset>` of genre → preset mappings,
/// plus a set of built-in default presets for common genres. Built-ins
/// can be overridden by registering a custom preset with the same genre
/// name, but cannot be removed from the defaults.
pub struct GenrePresetManager {
    presets: HashMap<String, GenrePreset>,
    /// Tracks which presets are built-in (can be overridden but not removed).
    built_in_genres: Vec<String>,
}

impl GenrePresetManager {
    /// Create a new `GenrePresetManager` with built-in genre presets.
    pub fn new() -> Self {
        let mut manager = Self {
            presets: HashMap::new(),
            built_in_genres: Vec::new(),
        };
        manager.init_built_ins();
        manager
    }

    /// Initialize built-in genre presets with realistic EQ settings.
    fn init_built_ins(&mut self) {
        let built_ins: Vec<GenrePreset> = vec![
            GenrePreset::new("rock", rock_eq())
                .with_stereo_width(1.2)
                .with_bass(3.0)
                .with_treble(2.0),
            GenrePreset::new("pop", pop_eq())
                .with_stereo_width(1.1)
                .with_bass(1.5)
                .with_treble(1.5),
            GenrePreset::new("jazz", jazz_eq())
                .with_stereo_width(1.1)
                .with_bass(1.0)
                .with_treble(0.5),
            GenrePreset::new("classical", classical_eq())
                .with_stereo_width(1.3)
                .with_bass(1.0)
                .with_treble(1.0),
            GenrePreset::new("electronic", electronic_eq())
                .with_stereo_width(1.5)
                .with_bass(5.0)
                .with_treble(3.0),
            GenrePreset::new("hip-hop", hip_hop_eq())
                .with_stereo_width(1.1)
                .with_bass(4.0)
                .with_treble(1.0),
            GenrePreset::new("metal", metal_eq())
                .with_stereo_width(1.2)
                .with_bass(2.5)
                .with_treble(3.0),
            GenrePreset::new("acoustic", acoustic_eq())
                .with_stereo_width(1.0)
                .with_bass(0.5)
                .with_treble(0.5),
            GenrePreset::new("r&b", r_and_b_eq())
                .with_stereo_width(1.15)
                .with_bass(3.0)
                .with_treble(1.5),
            GenrePreset::new("country", country_eq())
                .with_stereo_width(1.05)
                .with_bass(1.0)
                .with_treble(2.0),
            GenrePreset::new("latin", latin_eq())
                .with_stereo_width(1.2)
                .with_bass(2.5)
                .with_treble(2.0),
            GenrePreset::new("bollywood", bollywood_eq())
                .with_stereo_width(1.15)
                .with_bass(2.0)
                .with_treble(2.5),
        ];

        for preset in built_ins {
            let genre_key = preset.genre.to_lowercase();
            self.built_in_genres.push(genre_key.clone());
            self.presets.insert(genre_key, preset);
        }
    }

    /// Look up a genre preset by name (case-insensitive).
    pub fn get(&self, genre: &str) -> Option<&GenrePreset> {
        self.presets.get(&genre.to_lowercase())
    }

    /// Register a genre preset (add or update).
    ///
    /// If a preset with the same genre name already exists, it will be
    /// replaced. Built-in presets can be overridden but not removed.
    pub fn register(&mut self, preset: GenrePreset) {
        let genre_key = preset.genre.to_lowercase();
        self.presets.insert(genre_key, preset);
    }

    /// Remove a custom genre preset.
    ///
    /// Built-in presets cannot be removed — they will be restored to their
    /// default values. If the genre is a built-in, this resets it to the
    /// default rather than removing it.
    pub fn remove(&mut self, genre: &str) {
        let genre_key = genre.to_lowercase();
        if self.built_in_genres.contains(&genre_key) {
            self.reset_built_in(&genre_key);
        } else {
            self.presets.remove(&genre_key);
        }
    }

    /// Reset a built-in preset to its default values.
    fn reset_built_in(&mut self, genre_key: &str) {
        let defaults: Vec<GenrePreset> = vec![
            GenrePreset::new("rock", rock_eq())
                .with_stereo_width(1.2)
                .with_bass(3.0)
                .with_treble(2.0),
            GenrePreset::new("pop", pop_eq())
                .with_stereo_width(1.1)
                .with_bass(1.5)
                .with_treble(1.5),
            GenrePreset::new("jazz", jazz_eq())
                .with_stereo_width(1.1)
                .with_bass(1.0)
                .with_treble(0.5),
            GenrePreset::new("classical", classical_eq())
                .with_stereo_width(1.3)
                .with_bass(1.0)
                .with_treble(1.0),
            GenrePreset::new("electronic", electronic_eq())
                .with_stereo_width(1.5)
                .with_bass(5.0)
                .with_treble(3.0),
            GenrePreset::new("hip-hop", hip_hop_eq())
                .with_stereo_width(1.1)
                .with_bass(4.0)
                .with_treble(1.0),
            GenrePreset::new("metal", metal_eq())
                .with_stereo_width(1.2)
                .with_bass(2.5)
                .with_treble(3.0),
            GenrePreset::new("acoustic", acoustic_eq())
                .with_stereo_width(1.0)
                .with_bass(0.5)
                .with_treble(0.5),
            GenrePreset::new("r&b", r_and_b_eq())
                .with_stereo_width(1.15)
                .with_bass(3.0)
                .with_treble(1.5),
            GenrePreset::new("country", country_eq())
                .with_stereo_width(1.05)
                .with_bass(1.0)
                .with_treble(2.0),
            GenrePreset::new("latin", latin_eq())
                .with_stereo_width(1.2)
                .with_bass(2.5)
                .with_treble(2.0),
            GenrePreset::new("bollywood", bollywood_eq())
                .with_stereo_width(1.15)
                .with_bass(2.0)
                .with_treble(2.5),
        ];

        for preset in defaults {
            if preset.genre == genre_key {
                self.presets.insert(genre_key.to_string(), preset);
                return;
            }
        }
    }

    /// Returns all available genre names sorted alphabetically.
    pub fn list_genres(&self) -> Vec<&str> {
        let mut genres: Vec<&str> = self.presets.keys().map(|s| s.as_str()).collect();
        genres.sort();
        genres
    }

    /// Apply a genre preset to the audio engine.
    ///
    /// Sets the EQ state, stereo width, bass, and treble on the engine.
    /// Returns `true` if the preset was found and applied, `false` otherwise.
    pub fn apply_to_engine(&self, genre: &str, engine: &AudioEngine) -> bool {
        let Some(preset) = self.get(genre) else {
            return false;
        };

        let mut eq_state = preset.eq_state.clone();
        eq_state.stereo_width = preset.stereo_width;
        eq_state.bass_db = preset.bass_db;
        eq_state.treble_db = preset.treble_db;

        engine.set_eq_state(eq_state);
        engine.set_stereo_width(preset.stereo_width);
        engine.set_bass(preset.bass_db);
        engine.set_treble(preset.treble_db);

        tracing::info!(
            "Genre preset applied: '{}' (bass={:.1}dB, treble={:.1}dB, width={:.2})",
            preset.genre,
            preset.bass_db,
            preset.treble_db,
            preset.stereo_width,
        );

        true
    }

    /// Returns the number of registered presets.
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    /// Returns true if there are no presets.
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    /// Returns whether a genre is a built-in preset.
    pub fn is_built_in(&self, genre: &str) -> bool {
        self.built_in_genres.contains(&genre.to_lowercase())
    }
}

impl Default for GenrePresetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: create an EqualizerState with custom band gains.
fn make_eq(gains: [f64; 10]) -> EqualizerState {
    let mut eq = EqualizerState::default();
    for (band, gain) in eq.bands.iter_mut().zip(gains.iter()) {
        band.gain = *gain;
    }
    eq
}

/// Rock: V-shape — scooped mids, strong low-mid and upper-mid presence.
fn rock_eq() -> EqualizerState {
    make_eq([-4.0, -2.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0, -3.0])
}

/// Pop: Vocal-forward with bright midrange and gentle bass/treble lift.
fn pop_eq() -> EqualizerState {
    make_eq([-1.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0])
}

/// Jazz: Warm, mid-scoop with gentle high and low presence.
fn jazz_eq() -> EqualizerState {
    make_eq([3.0, 2.0, 0.0, 1.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0])
}

/// Classical: Nearly flat with subtle bass/treble enhancement.
fn classical_eq() -> EqualizerState {
    make_eq([4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0])
}

/// Electronic: Heavy bass emphasis with bright highs and wide presence.
fn electronic_eq() -> EqualizerState {
    make_eq([6.0, 5.0, 2.0, 0.0, -1.0, 0.0, 1.0, 3.0, 4.0, 5.0])
}

/// Hip-Hop: Deep sub-bass and low-mid punch with vocal clarity.
fn hip_hop_eq() -> EqualizerState {
    make_eq([5.0, 4.0, 3.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0, -1.0])
}

/// Metal: Scooped mids (the classic "smile curve"), aggressive highs.
fn metal_eq() -> EqualizerState {
    make_eq([3.0, 1.0, -2.0, -3.0, -2.0, -1.0, 0.0, 2.0, 4.0, 5.0])
}

/// Acoustic: Natural and flat with a touch of warmth and presence.
fn acoustic_eq() -> EqualizerState {
    make_eq([1.0, 1.0, 0.0, 1.0, 2.0, 2.0, 1.0, 0.0, 1.0, 1.0])
}

/// R&B: Smooth bass, warm mids, silky highs.
fn r_and_b_eq() -> EqualizerState {
    make_eq([3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0])
}

/// Country: Bright and present with midrange emphasis for vocals/instruments.
fn country_eq() -> EqualizerState {
    make_eq([1.0, 0.0, 1.0, 2.0, 3.0, 2.0, 1.0, 0.0, 2.0, 3.0])
}

/// Latin: Punchy bass with rhythmic midrange presence and bright top.
fn latin_eq() -> EqualizerState {
    make_eq([3.0, 2.0, 0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 3.0, 4.0])
}

/// Bollywood: Rich mids and bright production with strong presence.
fn bollywood_eq() -> EqualizerState {
    make_eq([2.0, 1.0, 1.0, 2.0, 3.0, 3.0, 2.0, 1.0, 3.0, 4.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_genres_exist() {
        let manager = GenrePresetManager::new();
        let genres = manager.list_genres();
        assert!(genres.contains(&"acoustic"));
        assert!(genres.contains(&"bollywood"));
        assert!(genres.contains(&"classical"));
        assert!(genres.contains(&"country"));
        assert!(genres.contains(&"electronic"));
        assert!(genres.contains(&"hip-hop"));
        assert!(genres.contains(&"jazz"));
        assert!(genres.contains(&"latin"));
        assert!(genres.contains(&"metal"));
        assert!(genres.contains(&"pop"));
        assert!(genres.contains(&"r&b"));
        assert!(genres.contains(&"rock"));
        assert_eq!(genres.len(), 12);
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let manager = GenrePresetManager::new();
        assert!(manager.get("Rock").is_some());
        assert!(manager.get("ROCK").is_some());
        assert!(manager.get("rock").is_some());
    }

    #[test]
    fn test_unknown_genre() {
        let manager = GenrePresetManager::new();
        assert!(manager.get("polka").is_none());
    }

    #[test]
    fn test_register_custom_preset() {
        let mut manager = GenrePresetManager::new();
        let preset = GenrePreset::new("ambient", EqualizerState::default())
            .with_stereo_width(1.8)
            .with_bass(-1.0)
            .with_treble(2.0);
        manager.register(preset);
        assert!(manager.get("ambient").is_some());
        assert!(!manager.is_built_in("ambient"));
    }

    #[test]
    fn test_remove_custom_preset() {
        let mut manager = GenrePresetManager::new();
        manager.register(GenrePreset::new("custom", EqualizerState::default()));
        assert!(manager.get("custom").is_some());
        manager.remove("custom");
        assert!(manager.get("custom").is_none());
    }

    #[test]
    fn test_remove_built_in_resets_to_default() {
        let mut manager = GenrePresetManager::new();
        let custom = GenrePreset::new("rock", EqualizerState::default()).with_bass(10.0);
        manager.register(custom);
        assert!((manager.get("rock").unwrap().bass_db - 10.0).abs() < 1e-9);
        manager.remove("rock");
        assert!(manager.get("rock").is_some());
        assert!((manager.get("rock").unwrap().bass_db - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_rock_preset_values() {
        let manager = GenrePresetManager::new();
        let rock = manager.get("rock").unwrap();
        assert!((rock.bass_db - 3.0).abs() < 1e-9);
        assert!((rock.treble_db - 2.0).abs() < 1e-9);
        assert!((rock.stereo_width - 1.2).abs() < 1e-9);
        assert!(
            rock.eq_state.bands[3].gain > 0.0,
            "250 Hz band should be boosted"
        );
        assert!(
            rock.eq_state.bands[0].gain < 0.0,
            "32 Hz band should be cut"
        );
    }

    #[test]
    fn test_electronic_preset_values() {
        let manager = GenrePresetManager::new();
        let elec = manager.get("electronic").unwrap();
        assert!((elec.bass_db - 5.0).abs() < 1e-9);
        assert!((elec.treble_db - 3.0).abs() < 1e-9);
        assert!((elec.stereo_width - 1.5).abs() < 1e-9);
        assert!(
            elec.eq_state.bands[0].gain > 4.0,
            "32 Hz should be heavily boosted"
        );
    }

    #[test]
    fn test_classical_preset_values() {
        let manager = GenrePresetManager::new();
        let classical = manager.get("classical").unwrap();
        assert!((classical.stereo_width - 1.3).abs() < 1e-9);
        assert!(
            (classical.eq_state.bands[4].gain - 0.0).abs() < 1e-9,
            "500 Hz should be flat"
        );
    }

    #[test]
    fn test_list_genres_sorted() {
        let manager = GenrePresetManager::new();
        let genres = manager.list_genres();
        let mut sorted = genres.clone();
        sorted.sort();
        assert_eq!(genres, sorted, "genres should be sorted alphabetically");
    }

    #[test]
    fn test_override_built_in() {
        let mut manager = GenrePresetManager::new();
        let custom = GenrePreset::new("jazz", EqualizerState::default())
            .with_bass(8.0)
            .with_treble(8.0);
        manager.register(custom);
        let jazz = manager.get("jazz").unwrap();
        assert!((jazz.bass_db - 8.0).abs() < 1e-9);
        assert!(manager.is_built_in("jazz"));
    }

    #[test]
    fn test_genre_preset_builder() {
        let preset = GenrePreset::new("test", EqualizerState::default())
            .with_stereo_width(2.0)
            .with_bass(5.0)
            .with_treble(-3.0)
            .with_convolution_ir("/path/to/ir.wav");
        assert_eq!(preset.genre, "test");
        assert!((preset.stereo_width - 2.0).abs() < 1e-9);
        assert!((preset.bass_db - 5.0).abs() < 1e-9);
        assert!((preset.treble_db - (-3.0)).abs() < 1e-9);
        assert_eq!(
            preset.convolution_ir_path,
            Some("/path/to/ir.wav".to_string())
        );
    }

    #[test]
    fn test_is_built_in() {
        let manager = GenrePresetManager::new();
        assert!(manager.is_built_in("rock"));
        assert!(manager.is_built_in("Rock")); // case-insensitive
        assert!(!manager.is_built_in("custom"));
    }
}
