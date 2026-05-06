use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Valid gain range for EQ bands in dB.
const EQ_BAND_GAIN_MIN: f64 = -24.0;
const EQ_BAND_GAIN_MAX: f64 = 24.0;

/// Valid gain range for bass/treble shelf in dB.
const SHELF_GAIN_MIN: f64 = -12.0;
const SHELF_GAIN_MAX: f64 = 12.0;

/// A single equalizer band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    pub frequency: f64,
    pub gain: f64, // dB, -24.0 .. 24.0
    pub bandwidth: f64,
}

/// A single Mid-Side EQ band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsEqBandState {
    pub frequency: f64,
    pub mid_gain_db: f64,  // dB, -24.0 .. 24.0
    pub side_gain_db: f64, // dB, -24.0 .. 24.0
    pub bandwidth: f64,    // Q value
    pub enabled: bool,
}

impl MsEqBandState {
    pub fn flat() -> Self {
        Self {
            frequency: 1000.0,
            mid_gain_db: 0.0,
            side_gain_db: 0.0,
            bandwidth: 1.0,
            enabled: false,
        }
    }
}

/// Full equalizer state with 10 bands + Mid-Side EQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqualizerState {
    pub bands: Vec<EqBand>,
    pub enabled: bool,
    /// Bass shelf gain dB (separate from EQ bands).
    pub bass_db: f64,
    /// Treble shelf gain dB (separate from EQ bands).
    pub treble_db: f64,
    /// Bass shelf corner frequency in Hz (20–500). Default: 90 Hz.
    #[serde(default = "default_bass_freq")]
    pub bass_freq_hz: f64,
    /// Treble shelf corner frequency in Hz (1000–20 000). Default: 10 000 Hz.
    #[serde(default = "default_treble_freq")]
    pub treble_freq_hz: f64,
    /// Stereo width: 0.0=mono, 1.0=original, >1.0=wider.
    pub stereo_width: f64,
    /// Channel balance: -1.0=left, 0.0=centre, +1.0=right.
    pub balance: f64,
    /// TPDF dither enabled (set true when output is 16-bit integer).
    pub dither_enabled: bool,
    /// Mid-Side EQ bands (up to 5 bands of per-band M/S processing).
    /// This is mastering-grade: independent EQ of the Mid (centre) and
    /// Side (stereo difference) components per frequency band.
    #[serde(default = "default_ms_eq_bands")]
    pub ms_eq_bands: Vec<MsEqBandState>,
    /// Whether the M/S EQ stage is enabled.
    #[serde(default)]
    pub ms_eq_enabled: bool,
}

fn default_ms_eq_bands() -> Vec<MsEqBandState> {
    vec![MsEqBandState::flat(); 5]
}

fn default_bass_freq() -> f64 {
    90.0
}
fn default_treble_freq() -> f64 {
    10_000.0
}

impl Default for EqualizerState {
    fn default() -> Self {
        Self {
            bands: Self::flat_bands(),
            enabled: true,
            bass_db: 0.0,
            treble_db: 0.0,
            bass_freq_hz: 90.0,
            treble_freq_hz: 10_000.0,
            stereo_width: 1.0,
            balance: 0.0,
            dither_enabled: false,
            ms_eq_bands: vec![MsEqBandState::flat(); 5],
            ms_eq_enabled: false,
        }
    }
}

impl EqualizerState {
    /// Validate and clamp all fields to their valid ranges.
    ///
    /// This should be called after deserializing from config/DB to ensure
    /// that out-of-range values (e.g. from corrupted config files or manual
    /// edits) don't cause unexpected DSP behaviour such as extreme gain
    /// boost or filter instability.
    pub fn clamp_to_valid_ranges(&mut self) {
        for band in &mut self.bands {
            band.gain = band.gain.clamp(EQ_BAND_GAIN_MIN, EQ_BAND_GAIN_MAX);
            band.frequency = band.frequency.clamp(20.0, 20_000.0);
            band.bandwidth = band.bandwidth.clamp(0.1, 10.0);
        }
        self.bass_db = self.bass_db.clamp(SHELF_GAIN_MIN, SHELF_GAIN_MAX);
        self.treble_db = self.treble_db.clamp(SHELF_GAIN_MIN, SHELF_GAIN_MAX);
        self.bass_freq_hz = self.bass_freq_hz.clamp(20.0, 500.0);
        self.treble_freq_hz = self.treble_freq_hz.clamp(1000.0, 20_000.0);
        self.stereo_width = self.stereo_width.clamp(0.0, 3.0);
        self.balance = self.balance.clamp(-1.0, 1.0);
        for ms_band in &mut self.ms_eq_bands {
            ms_band.mid_gain_db = ms_band
                .mid_gain_db
                .clamp(EQ_BAND_GAIN_MIN, EQ_BAND_GAIN_MAX);
            ms_band.side_gain_db = ms_band
                .side_gain_db
                .clamp(EQ_BAND_GAIN_MIN, EQ_BAND_GAIN_MAX);
            ms_band.frequency = ms_band.frequency.clamp(20.0, 20_000.0);
            ms_band.bandwidth = ms_band.bandwidth.clamp(0.1, 10.0);
        }
    }
}

impl EqualizerState {
    /// Flat 10-band ISO frequencies.
    pub fn flat_bands() -> Vec<EqBand> {
        [
            32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ]
        .iter()
        .map(|&f| EqBand {
            frequency: f,
            gain: 0.0,
            bandwidth: 1.0,
        })
        .collect()
    }
}

// ── Output device ID ─────────────────────────────────────────────────

/// Identifies an output device for per-output preset storage.
/// Use the cpal device name or a stable identifier string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputDeviceId(pub String);

impl OutputDeviceId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
    pub fn default_output() -> Self {
        Self("default".into())
    }
}

// ── Per-output preset store ──────────────────────────────────────────

/// Stores a separate `EqualizerState` per output device.
///
/// Apply via `AudioEngine::set_eq_state` whenever cpal reports a device change.
/// Zero DSP cost — pure config/serialisation layer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputPresetStore {
    presets: HashMap<OutputDeviceId, EqualizerState>,
}

impl OutputPresetStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Save the current EQ state for a given output device.
    pub fn save(&mut self, device: OutputDeviceId, state: EqualizerState) {
        self.presets.insert(device, state);
    }

    /// Retrieve the saved state for a device, or a flat default if none.
    ///
    /// Deserialized values are clamped to valid ranges to guard against
    /// corrupted config data.
    pub fn load(&self, device: &OutputDeviceId) -> EqualizerState {
        let mut state = self.presets.get(device).cloned().unwrap_or_default();
        state.clamp_to_valid_ranges();
        state
    }

    /// Remove a saved preset.
    pub fn remove(&mut self, device: &OutputDeviceId) {
        self.presets.remove(device);
    }

    pub fn devices(&self) -> impl Iterator<Item = &OutputDeviceId> {
        self.presets.keys()
    }
}

// ── AutoEQ integration ───────────────────────────────────────────────

/// One filter entry from an AutoEQ parametric EQ profile.
///
/// AutoEQ exports these as JSON arrays. See https://autoeq.app
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEqFilter {
    /// "PK" = peaking, "LSC" = low-shelf, "HSC" = high-shelf.
    #[serde(rename = "type")]
    pub filter_type: String,
    pub frequency: f64,
    pub gain: f64,
    pub q: f64,
}

/// Deserialise a headphone correction profile exported from AutoEQ (JSON array).
///
/// Returns an `EqualizerState` with up to `MAX_EQ_BANDS` peaking bands populated.
/// Low/high shelf entries are mapped to the bass_db / treble_db fields.
///
/// # Example JSON format (autoeq.app export)
/// ```json
/// [
///   {"type": "LSC", "frequency": 105.0, "gain": 3.5, "q": 0.7},
///   {"type": "PK",  "frequency": 500.0, "gain": -2.1, "q": 1.4},
///   {"type": "HSC", "frequency": 8000.0,"gain": 4.0,  "q": 0.7}
/// ]
/// ```
pub fn load_autoeq_profile(json: &str) -> Result<EqualizerState, serde_json::Error> {
    let filters: Vec<AutoEqFilter> = serde_json::from_str(json)?;

    let mut state = EqualizerState::default();
    let mut band_index = 0;

    for filter in &filters {
        match filter.filter_type.as_str() {
            "LSC" => {
                state.bass_db = filter.gain.clamp(-12.0, 12.0);
            }
            "HSC" => {
                state.treble_db = filter.gain.clamp(-12.0, 12.0);
            }
            "PK" => {
                if band_index < state.bands.len() {
                    state.bands[band_index].frequency = filter.frequency;
                    state.bands[band_index].gain = filter.gain.clamp(-24.0, 24.0);
                    state.bands[band_index].bandwidth = filter.q;
                    band_index += 1;
                }
            }
            _ => {} // unknown filter type — skip
        }
    }

    Ok(state)
}

/// Preset equalizer configurations.
pub mod presets {
    use super::*;

    pub fn rock() -> EqualizerState {
        let mut eq = EqualizerState::default();
        let gains = [-4.0, -2.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0, -3.0];
        for (b, g) in eq.bands.iter_mut().zip(gains.iter()) {
            b.gain = *g;
        }
        eq
    }

    pub fn jazz() -> EqualizerState {
        let mut eq = EqualizerState::default();
        let gains = [3.0, 2.0, 0.0, 1.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0];
        for (b, g) in eq.bands.iter_mut().zip(gains.iter()) {
            b.gain = *g;
        }
        eq
    }

    pub fn classical() -> EqualizerState {
        let mut eq = EqualizerState::default();
        let gains = [4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0];
        for (b, g) in eq.bands.iter_mut().zip(gains.iter()) {
            b.gain = *g;
        }
        eq
    }

    pub fn pop() -> EqualizerState {
        let mut eq = EqualizerState::default();
        let gains = [-1.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0];
        for (b, g) in eq.bands.iter_mut().zip(gains.iter()) {
            b.gain = *g;
        }
        eq
    }

    pub fn bass_boost() -> EqualizerState {
        let mut eq = EqualizerState::default();
        let gains = [8.0, 6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        for (b, g) in eq.bands.iter_mut().zip(gains.iter()) {
            b.gain = *g;
        }
        eq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_bands() {
        let bands = EqualizerState::flat_bands();
        assert_eq!(bands.len(), 10);
        for band in &bands {
            assert_eq!(band.gain, 0.0);
        }
        assert_eq!(bands[0].frequency, 32.0);
        assert_eq!(bands[9].frequency, 16000.0);
    }

    #[test]
    fn test_per_output_preset_roundtrip() {
        let mut store = OutputPresetStore::new();
        let id = OutputDeviceId::new("Headphones (Realtek)");
        let mut state = EqualizerState::default();
        state.bass_db = 3.0;
        state.stereo_width = 1.5;
        store.save(id.clone(), state.clone());
        let loaded = store.load(&id);
        assert!((loaded.bass_db - 3.0).abs() < 1e-9);
        assert!((loaded.stereo_width - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_per_output_preset_missing_returns_default() {
        let store = OutputPresetStore::new();
        let id = OutputDeviceId::new("nonexistent");
        let loaded = store.load(&id);
        assert_eq!(loaded.bass_db, 0.0);
        assert!(loaded.enabled);
    }

    #[test]
    fn test_autoeq_load() {
        let json = r#"[
            {"type": "LSC", "frequency": 105.0, "gain": 3.5, "q": 0.7},
            {"type": "PK",  "frequency": 500.0, "gain": -2.1, "q": 1.4},
            {"type": "PK",  "frequency": 2000.0,"gain": 1.8,  "q": 2.0},
            {"type": "HSC", "frequency": 8000.0,"gain": 4.0,  "q": 0.7}
        ]"#;
        let state = load_autoeq_profile(json).expect("parse failed");
        assert!((state.bass_db - 3.5).abs() < 1e-9, "LSC → bass_db");
        assert!((state.treble_db - 4.0).abs() < 1e-9, "HSC → treble_db");
        assert!(
            (state.bands[0].gain - (-2.1)).abs() < 1e-9,
            "first PK band gain"
        );
        assert!(
            (state.bands[0].frequency - 500.0).abs() < 1e-9,
            "first PK freq"
        );
        assert!(
            (state.bands[1].gain - 1.8).abs() < 1e-9,
            "second PK band gain"
        );
    }

    #[test]
    fn test_rock_preset() {
        let eq = presets::rock();
        assert!(eq.bands[3].gain > 0.0);
        assert!(eq.bands[0].gain < 0.0);
    }

    #[test]
    fn test_clamp_to_valid_ranges_clamps_out_of_range() {
        let mut state = EqualizerState::default();
        // Set out-of-range values
        state.bands[0].gain = 50.0;
        state.bands[0].frequency = 50000.0;
        state.bands[0].bandwidth = 0.01;
        state.bass_db = 100.0;
        state.treble_db = -50.0;
        state.bass_freq_hz = 5.0;
        state.treble_freq_hz = 500.0;
        state.stereo_width = 5.0;
        state.balance = -2.0;
        state.ms_eq_bands[0].mid_gain_db = 100.0;
        state.ms_eq_bands[0].side_gain_db = -100.0;
        state.ms_eq_bands[0].frequency = 50000.0;
        state.ms_eq_bands[0].bandwidth = 0.0;

        state.clamp_to_valid_ranges();

        assert!(
            (state.bands[0].gain - 24.0).abs() < 1e-9,
            "band gain should be clamped to 24"
        );
        assert!(
            (state.bands[0].frequency - 20000.0).abs() < 1e-9,
            "frequency should be clamped to 20k"
        );
        assert!(
            (state.bands[0].bandwidth - 0.1).abs() < 1e-9,
            "bandwidth should be clamped to 0.1"
        );
        assert!(
            (state.bass_db - 12.0).abs() < 1e-9,
            "bass_db should be clamped to 12"
        );
        assert!(
            (state.treble_db - (-12.0)).abs() < 1e-9,
            "treble_db should be clamped to -12"
        );
        assert!(
            (state.bass_freq_hz - 20.0).abs() < 1e-9,
            "bass_freq should be clamped to 20"
        );
        assert!(
            (state.treble_freq_hz - 1000.0).abs() < 1e-9,
            "treble_freq should be clamped to 1k"
        );
        assert!(
            (state.stereo_width - 3.0).abs() < 1e-9,
            "stereo_width should be clamped to 3"
        );
        assert!(
            (state.balance - (-1.0)).abs() < 1e-9,
            "balance should be clamped to -1"
        );
        assert!(
            (state.ms_eq_bands[0].mid_gain_db - 24.0).abs() < 1e-9,
            "MS mid gain clamped"
        );
        assert!(
            (state.ms_eq_bands[0].side_gain_db - (-24.0)).abs() < 1e-9,
            "MS side gain clamped"
        );
    }
}
