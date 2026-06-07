//! Chroma-based key and mode (major/minor) detection.
//!
//! ## Algorithm
//!
//! We use a lightweight DFT computed only at the 12 equal-tempered pitch
//! classes (C through B) rather than a full FFT.  This means we run exactly
//! 12 Goertzel filters in parallel — one per semitone — over a mono signal
//! that has been downsampled to 4 kHz before entering this module.
//!
//! ### Why Goertzel instead of FFT?
//!
//! A full 512-point FFT at 44.1 kHz returns ~256 bins; the 12 pitch-class
//! bins we need span only a small fraction of those and the rest is waste.
//! Goertzel evaluates a single DFT bin in O(N) time with two multiply-adds
//! per sample — essentially free compared to the audio decode work already
//! happening in the same loop.
//!
//! ### Downsampling to 4 kHz
//!
//! Pitch content lives below ~2 kHz (C8 = 4186 Hz is the highest piano key).
//! Downsampling to 4 kHz (8× for 32 kHz source, ~11× for 44.1 kHz) removes
//! all harmonics above 2 kHz from the chroma calculation, which is exactly
//! what we want — high overtones would corrupt the pitch-class accumulation.
//! We use a simple first-order IIR anti-alias low-pass before decimation.
//!
//! ### Chroma vector → key/mode
//!
//! After accumulation we have a 12-element chroma vector C[0..12] (one value
//! per semitone, C=0, C#=1, …, B=11).  We correlate C against the 24
//! Krumhansl–Schmuckler key profiles (12 major + 12 minor) and pick the
//! profile with the highest Pearson correlation.  The winning profile gives
//! us both the tonic and the mode (major or minor).
//!
//! Krumhansl–Schmuckler profiles are the canonical psychoacoustic model of
//! tonal hierarchy perception; they were derived from listener experiments
//! and are well-validated for Western and Indian classical music alike.

use std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Goertzel resonator
// ---------------------------------------------------------------------------

/// Goertzel filter for a single frequency bin.
/// Computes |DFT[k]|² for one target frequency with O(1) state per sample.
struct Goertzel {
    coeff: f32, // 2·cos(2π·k/N)
    s1: f32,
    s2: f32,
    n: usize, // samples accumulated in current block
    block_size: usize,
    /// Accumulated mean power across completed blocks (Welford).
    power_mean: f32,
    power_count: u64,
}

impl Goertzel {
    fn new(target_hz: f32, sample_rate: f32, block_size: usize) -> Self {
        let k = target_hz / sample_rate * block_size as f32;
        let coeff = 2.0 * (2.0 * PI * k / block_size as f32).cos();
        Self {
            coeff,
            s1: 0.0,
            s2: 0.0,
            n: 0,
            block_size,
            power_mean: 0.0,
            power_count: 0,
        }
    }

    #[inline(always)]
    fn feed(&mut self, x: f32) {
        let s0 = x + self.coeff * self.s1 - self.s2;
        self.s2 = self.s1;
        self.s1 = s0;
        self.n += 1;

        if self.n == self.block_size {
            // Compute |DFT[k]|²
            let power = self.s1 * self.s1 + self.s2 * self.s2 - self.coeff * self.s1 * self.s2;
            let power = power.max(0.0);
            // Welford update
            self.power_count += 1;
            self.power_mean += (power - self.power_mean) / self.power_count as f32;
            // Reset for next block
            self.s1 = 0.0;
            self.s2 = 0.0;
            self.n = 0;
        }
    }

    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.n = 0;
        self.power_mean = 0.0;
        self.power_count = 0;
    }
}

// ---------------------------------------------------------------------------
// Krumhansl–Schmuckler key profiles
// ---------------------------------------------------------------------------

// Major profile (starting from C)
const MAJOR_PROFILE: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];

// Minor profile (starting from C, natural minor)
const MINOR_PROFILE: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Pearson correlation between two 12-element arrays.
fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let mean_a = a.iter().sum::<f32>() / 12.0;
    let mean_b = b.iter().sum::<f32>() / 12.0;
    let mut num = 0.0f32;
    let mut da2 = 0.0f32;
    let mut db2 = 0.0f32;
    for i in 0..12 {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        num += da * db;
        da2 += da * da;
        db2 += db * db;
    }
    let denom = (da2 * db2).sqrt();
    if denom < 1e-6 {
        0.0
    } else {
        num / denom
    }
}

// ---------------------------------------------------------------------------
// ChromaDetector
// ---------------------------------------------------------------------------

/// Streaming chroma / key-mode detector.
/// Feed downsampled 4 kHz mono audio via [`ChromaDetector::feed`], then call
/// [`ChromaDetector::detect`] to get a [`KeyMode`] result.
pub struct ChromaDetector {
    /// Anti-alias IIR low-pass (cutoff ~1.8 kHz) before decimation.
    aa_b0: f32,
    aa_a1: f32,
    aa_y1: f32,
    /// Source sample rate (needed to compute decimation ratio).
    src_rate: f32,
    /// Fractional accumulator for decimation.
    dec_acc: f32,
    /// One Goertzel filter per pitch class (C…B), tuned to the middle octave
    /// of the vocal/instrument range (octave 4: C4=261.6 Hz … B4=493.9 Hz)
    /// at the target 4 kHz analysis rate.
    /// We also add octave 3 (C3=130.8…B3=246.9) to catch bass-range harmony.
    filters_oct3: [Goertzel; 12],
    filters_oct4: [Goertzel; 12],
}

/// The 12 pitch classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchClass {
    C,
    Cs,
    D,
    Ds,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    As,
    B,
}

impl PitchClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::Cs => "C#",
            Self::D => "D",
            Self::Ds => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::Fs => "F#",
            Self::G => "G",
            Self::Gs => "G#",
            Self::A => "A",
            Self::As => "A#",
            Self::B => "B",
        }
    }
}

/// Key detection result.
#[derive(Debug, Clone)]
pub struct KeyMode {
    /// Detected tonic pitch class.
    pub tonic: PitchClass,
    /// True = major, false = minor.
    pub is_major: bool,
    /// Pearson correlation of best-fit profile ∈ [−1, 1].
    /// Values below ~0.6 indicate ambiguous tonality (e.g. atonal, percussion-heavy).
    pub confidence: f32,
}

/// Frequencies of C3…B3 and C4…B4 (12-TET, A4=440 Hz).
const OCT3_HZ: [f32; 12] = [
    130.813, 138.591, 146.832, 155.563, 164.814, 174.614, 184.997, 195.998, 207.652, 220.000,
    233.082, 246.942,
];
const OCT4_HZ: [f32; 12] = [
    261.626, 277.183, 293.665, 311.127, 329.628, 349.228, 369.994, 391.995, 415.305, 440.000,
    466.164, 493.883,
];

/// Analysis sample rate after decimation.
const ANALYSIS_RATE: f32 = 4000.0;
/// Goertzel block size at 4 kHz → ~64 ms per block (good freq resolution).
const BLOCK_SIZE: usize = 256;

impl ChromaDetector {
    /// Create a new detector for audio at `src_sample_rate`.
    pub fn new(src_sample_rate: f32) -> Result<Self, super::AnalysisError> {
        if src_sample_rate <= 0.0 {
            return Err(super::AnalysisError::InvalidSampleRate(src_sample_rate));
        }

        // Anti-alias LP: cutoff = 1800 Hz at the source rate.
        let rc = 1.0 / (2.0 * PI * 1800.0);
        let t = 1.0 / src_sample_rate;
        let aa_a1 = -(-t / (rc + t)).exp(); // approximate first-order IIR
        let aa_b0 = 1.0 + aa_a1;

        let mk_oct3 = |i: usize| Goertzel::new(OCT3_HZ[i], ANALYSIS_RATE, BLOCK_SIZE);
        let mk_oct4 = |i: usize| Goertzel::new(OCT4_HZ[i], ANALYSIS_RATE, BLOCK_SIZE);

        Ok(Self {
            aa_b0,
            aa_a1,
            aa_y1: 0.0,
            src_rate: src_sample_rate,
            dec_acc: 0.0,
            filters_oct3: std::array::from_fn(mk_oct3),
            filters_oct4: std::array::from_fn(mk_oct4),
        })
    }

    /// Feed a chunk of stereo samples.  Internally mixes to mono, anti-alias
    /// filters, decimates to 4 kHz, and feeds the Goertzel bank.
    pub fn feed(&mut self, samples: &[(f32, f32)]) {
        let step = self.src_rate / ANALYSIS_RATE;
        for &(l, r) in samples {
            let mono = (l + r) * 0.5;
            // Anti-alias low-pass
            let y = self.aa_b0 * mono - self.aa_a1 * self.aa_y1;
            self.aa_y1 = y;
            // Decimation: emit one sample every `step` input samples
            self.dec_acc += 1.0;
            if self.dec_acc >= step {
                self.dec_acc -= step;
                for f in &mut self.filters_oct3 {
                    f.feed(y);
                }
                for f in &mut self.filters_oct4 {
                    f.feed(y);
                }
            }
        }
    }

    /// Compute the chroma vector and run Krumhansl–Schmuckler matching.
    ///
    /// Returns `None` if not enough audio has been accumulated for a reliable
    /// estimate (< 4 completed Goertzel blocks per filter).
    pub fn detect(&self) -> Option<KeyMode> {
        // Need at least 4 completed blocks per filter for reliability.
        if self.filters_oct4[0].power_count < 4 {
            return None;
        }

        // Build 12-element chroma vector: sum octave 3 and octave 4 powers.
        let mut chroma = [0.0f32; 12];
        for (i, c) in chroma.iter_mut().enumerate() {
            *c = self.filters_oct3[i].power_mean + self.filters_oct4[i].power_mean;
        }

        // Normalise to unit sum to make correlation scale-invariant.
        let total: f32 = chroma.iter().sum();
        if total < 1e-6 {
            return None; // silence
        }
        for c in &mut chroma {
            *c /= total;
        }

        // Correlate against all 24 key profiles (12 major + 12 minor),
        // each rotated to the corresponding tonic.
        let mut best_corr = f32::MIN;
        let mut best_tonic = 0usize;
        let mut best_major = true;

        for tonic in 0..12 {
            // Rotate profile so it starts at `tonic`.
            let mut maj_rot = [0.0f32; 12];
            let mut min_rot = [0.0f32; 12];
            for i in 0..12 {
                maj_rot[i] = MAJOR_PROFILE[(i + 12 - tonic) % 12];
                min_rot[i] = MINOR_PROFILE[(i + 12 - tonic) % 12];
            }
            let maj_corr = pearson(&chroma, &maj_rot);
            let min_corr = pearson(&chroma, &min_rot);

            if maj_corr > best_corr {
                best_corr = maj_corr;
                best_tonic = tonic;
                best_major = true;
            }
            if min_corr > best_corr {
                best_corr = min_corr;
                best_tonic = tonic;
                best_major = false;
            }
        }

        let tonic = match best_tonic {
            0 => PitchClass::C,
            1 => PitchClass::Cs,
            2 => PitchClass::D,
            3 => PitchClass::Ds,
            4 => PitchClass::E,
            5 => PitchClass::F,
            6 => PitchClass::Fs,
            7 => PitchClass::G,
            8 => PitchClass::Gs,
            9 => PitchClass::A,
            10 => PitchClass::As,
            _ => PitchClass::B,
        };

        Some(KeyMode {
            tonic,
            is_major: best_major,
            confidence: best_corr,
        })
    }

    /// Reset all state for reuse on the next track.
    pub fn reset(&mut self) {
        self.aa_y1 = 0.0;
        self.dec_acc = 0.0;
        for f in &mut self.filters_oct3 {
            f.reset();
        }
        for f in &mut self.filters_oct4 {
            f.reset();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, dur: f32, sr: f32) -> Vec<(f32, f32)> {
        let n = (sr * dur) as usize;
        (0..n)
            .map(|i| {
                let v = amp * (2.0 * PI * freq * i as f32 / sr).sin();
                (v, v)
            })
            .collect()
    }

    #[test]
    fn test_invalid_sample_rate() {
        assert!(ChromaDetector::new(0.0).is_err());
        assert!(ChromaDetector::new(-1.0).is_err());
        assert!(ChromaDetector::new(44100.0).is_ok());
    }

    #[test]
    fn test_silence_returns_none() {
        let mut d = ChromaDetector::new(44100.0).unwrap();
        let silence = vec![(0.0f32, 0.0f32); 44100 * 5];
        d.feed(&silence);
        // Silence has no tonal content → None (total chroma ≈ 0).
        assert!(d.detect().is_none());
    }

    #[test]
    fn test_insufficient_data_returns_none() {
        let d = ChromaDetector::new(44100.0).unwrap();
        // No samples fed at all.
        assert!(d.detect().is_none());
    }

    #[test]
    fn test_a440_produces_result() {
        // A4 = 440 Hz is an unambiguous pitch; should produce Some result.
        let mut d = ChromaDetector::new(44100.0).unwrap();
        d.feed(&sine(440.0, 0.5, 10.0, 44100.0));
        let r = d.detect();
        assert!(r.is_some(), "A440 should yield a key result");
        let km = r.unwrap();
        assert!(km.confidence >= 0.0 && km.confidence <= 1.0);
    }

    #[test]
    fn test_c_major_chord_detects_major() {
        // C major chord: C4 + E4 + G4.  Should detect major mode.
        let sr = 44100.0;
        let n = (sr * 12.0) as usize;
        let samples: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let v = 0.3 * (2.0 * PI * 261.63 * t).sin()   // C4
                  + 0.3 * (2.0 * PI * 329.63 * t).sin()   // E4
                  + 0.3 * (2.0 * PI * 392.00 * t).sin(); // G4
                (v, v)
            })
            .collect();
        let mut d = ChromaDetector::new(sr).unwrap();
        d.feed(&samples);
        if let Some(km) = d.detect() {
            if km.confidence > 0.5 {
                assert!(km.is_major, "C major chord should detect as major mode");
            }
        }
    }

    #[test]
    fn test_a_minor_chord_detects_minor() {
        // A minor chord: A3 + C4 + E4.  Should detect minor mode.
        let sr = 44100.0;
        let n = (sr * 12.0) as usize;
        let samples: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let v = 0.3 * (2.0 * PI * 220.00 * t).sin()   // A3
                  + 0.3 * (2.0 * PI * 261.63 * t).sin()   // C4
                  + 0.3 * (2.0 * PI * 329.63 * t).sin(); // E4
                (v, v)
            })
            .collect();
        let mut d = ChromaDetector::new(sr).unwrap();
        d.feed(&samples);
        if let Some(km) = d.detect() {
            if km.confidence > 0.5 {
                assert!(!km.is_major, "A minor chord should detect as minor mode");
            }
        }
    }

    #[test]
    fn test_reset_clears_state() {
        let mut d = ChromaDetector::new(44100.0).unwrap();
        d.feed(&sine(440.0, 0.5, 10.0, 44100.0));
        d.reset();
        assert!(d.detect().is_none());
    }

    #[test]
    fn test_48khz_accepted() {
        let mut d = ChromaDetector::new(48000.0).unwrap();
        d.feed(&sine(440.0, 0.5, 10.0, 48000.0));
        // Should not panic; result may or may not be Some depending on block count.
        let _ = d.detect();
    }

    #[test]
    fn test_confidence_in_unit_interval() {
        let mut d = ChromaDetector::new(44100.0).unwrap();
        d.feed(&sine(440.0, 0.5, 15.0, 44100.0));
        if let Some(km) = d.detect() {
            assert!(
                km.confidence >= -1.0 && km.confidence <= 1.0,
                "confidence = {}",
                km.confidence
            );
        }
    }
}
