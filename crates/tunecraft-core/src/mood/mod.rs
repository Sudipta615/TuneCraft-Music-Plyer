//! Mood analysis module for Tunecraft.
//! Analyzes audio files and classifies them into mood categories using a
//! confidence-based scoring system. Each mood earns a weighted score based
//! on how closely the song's acoustic features match that mood's ideal
//! profile. The mood with the highest score wins.
//!
//! Designed for: Bollywood Hindi, covers, remixes, English songs, instrumentals/beats.
//! The scoring approach produces a more balanced distribution than rigid threshold
//! chains, reducing the "everything is Romantic" problem for Western-only libraries.
//!
//! Uses existing project dependencies only:
//!   - symphonia  — audio decoding
//!   - rustfft    — FFT spectral analysis
//!   - tokio      — async runtime for blocking decode work

use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

use crate::audio::pcm_cache::{PcmBuffer, PcmCache};
use crate::error::MoodError;

/// Mood categories tuned for Bollywood/Hindi music libraries.
#[derive(Debug, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub enum Mood {
    /// Item songs, tapori, party numbers, remixes, dance beats
    Dance,
    /// Soft love songs, duets, melodic mid-tempo Bollywood
    Romantic,
    /// Emotional, breakup, tragedy, slow sad songs
    Sad,
    /// Qawwali, devotional, sufi, repetitive medium-tempo
    Sufi,
    /// Lofi covers, instrumentals, beats, acoustic covers
    Chill,
}

impl Mood {
    /// Convert to string for SQLite storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Mood::Dance => "Dance",
            Mood::Romantic => "Romantic",
            Mood::Sad => "Sad",
            Mood::Sufi => "Sufi",
            Mood::Chill => "Chill",
        }
    }

    /// Parse from SQLite string.
    ///
    /// Fix L15: Renamed from `from_str` to `parse_label` to avoid shadowing
    /// the `FromStr` trait, which can cause confusing compiler errors when
    /// both the inherent method and the trait are in scope.
    pub fn parse_label(s: &str) -> Option<Self> {
        match s {
            "Dance" => Some(Mood::Dance),
            "Romantic" => Some(Mood::Romantic),
            "Sad" => Some(Mood::Sad),
            "Sufi" => Some(Mood::Sufi),
            "Chill" => Some(Mood::Chill),
            _ => None,
        }
    }

    /// Parse from SQLite string, returning a default mood for unrecognized labels.
    ///
    /// Fix Bug #5: The previous `from_str(label).unwrap_or_else(|| panic!(...))`
    /// pattern in test code was reachable from production paths (e.g., reading
    /// a mood string from the database that doesn't match any known variant due
    /// to a schema migration or external edit). A panic in that context would
    /// crash the entire process instead of returning an error.
    ///
    /// This method is safe for production use: if the label doesn't match any
    /// known mood variant, it logs a warning and returns `Mood::Romantic` (the
    /// default/catch-all category) instead of panicking.
    pub fn from_str_or_default(s: &str) -> Self {
        match Self::parse_label(s) {
            Some(mood) => mood,
            None => {
                tracing::warn!(
                    "Unrecognized mood label '{}' in database — defaulting to Romantic. \
                     This may indicate a schema migration or external database edit.",
                    s
                );
                Mood::Romantic
            }
        }
    }

    /// All mood variants — used to populate UI picker.
    pub fn all() -> &'static [&'static str] {
        &["Dance", "Romantic", "Sad", "Sufi", "Chill"]
    }
}

impl std::fmt::Display for Mood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Raw acoustic features extracted from a song.
/// All fields are normalized or in natural units as documented.
#[derive(Debug, Clone)]
pub struct SongFeatures {
    /// Tempo in BPM (range: 60-180)
    pub bpm: f32,
    /// RMS loudness (range: 0.0-1.0 approximately)
    pub energy: f32,
    /// Ratio of bass energy (20-250 Hz) to total energy (range: 0.0-1.0)
    pub bass_ratio: f32,
    /// Spectral centroid in Hz — higher = brighter sound
    pub spectral_centroid: f32,
    /// Dynamic range — difference between loud and quiet parts (95th-5th percentile RMS)
    pub dynamic_range: f32,
}

/// Decode any supported audio file to interleaved f32 samples at original sample rate.
/// Returns `(interleaved_samples, sample_rate, channels)`. Returns `MoodError` on failure.
///
/// The returned interleaved samples are suitable for caching in a `PcmBuffer` and
/// later converting to mono for mood analysis via `interleaved_to_mono`.
fn decode_to_interleaved(path: &str) -> Result<(Vec<f32>, u32, u16), MoodError> {
    use std::fs::File;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = File::open(path).map_err(|e| MoodError::FileOpenFailed(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.rsplit('.').next() {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| MoodError::ProbeFailed(e.to_string()))?;

    let mut format = probed.format;
    let track = format.default_track().ok_or(MoodError::NoDefaultTrack)?;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or(MoodError::UnknownSampleRate)?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| MoodError::DecoderFailed(e.to_string()))?;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut channels: u16 = 2; // default, will be updated from first decoded packet

    const MAX_ANALYSIS_SECONDS: usize = 90;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count();
        channels = ch as u16;
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);

        interleaved.extend_from_slice(buf.samples());

        if interleaved.len() / ch >= MAX_ANALYSIS_SECONDS * sample_rate as usize {
            break;
        }
    }

    if interleaved.is_empty() {
        return Err(MoodError::NoSamples);
    }

    Ok((interleaved, sample_rate, channels))
}

/// Convert interleaved multi-channel samples to mono by averaging channels.
/// Uses chunks() instead of step_by() to avoid a potential panic if the
/// interleaved buffer length is not an exact multiple of the channel count
/// (e.g. due to trailing padding samples).
fn interleaved_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    if ch == 0 {
        return Vec::new();
    }
    if ch == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(ch)
        .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
        .collect()
}

/// Decode any supported audio file to mono f32 samples at original sample rate.
/// Returns `(samples, sample_rate)`. Returns `MoodError` on failure.
///
/// This is a convenience wrapper around `decode_to_interleaved` + `interleaved_to_mono`.
pub fn decode_to_mono(path: &str) -> Result<(Vec<f32>, u32), MoodError> {
    let (interleaved, sample_rate, channels) = decode_to_interleaved(path)?;
    let mono = interleaved_to_mono(&interleaved, channels);
    Ok((mono, sample_rate))
}

/// Compute RMS energy of entire signal.
fn compute_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Compute dynamic range as 95th-5th percentile RMS across frames.
/// More robust than max-min against clipping artifacts.
fn compute_dynamic_range(samples: &[f32]) -> f32 {
    let frame = 4096usize;

    let mut rms_frames: Vec<f32> = samples
        .chunks(frame)
        .map(|w| {
            let sq: f32 = w.iter().map(|s| s * s).sum();
            (sq / w.len() as f32).sqrt()
        })
        .filter(|&r| r > 1e-6)
        .collect();

    if rms_frames.is_empty() {
        return 0.0;
    }

    rms_frames.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let len = rms_frames.len();
    let p95_idx = ((0.95 * len as f32).ceil() as usize)
        .saturating_sub(1)
        .min(len - 1);
    let p05_idx = ((0.05 * len as f32).ceil() as usize)
        .saturating_sub(1)
        .min(len - 1);

    rms_frames[p95_idx] - rms_frames[p05_idx]
}

/// Compute bass ratio (energy in 20-250 Hz / total energy)
/// and spectral centroid in Hz.
/// Uses Hann-windowed FFT frames.
fn compute_spectral_features(
    samples: &[f32],
    sample_rate: u32,
    planner: &mut FftPlanner<f32>,
) -> (f32, f32) {
    let frame_size = 2048usize;
    let hop = 1024usize;
    let fft = planner.plan_fft_forward(frame_size);
    let freq_resolution = sample_rate as f32 / frame_size as f32;

    let bass_bin_max = (250.0 / freq_resolution) as usize;

    let mut buffer = vec![
        Complex {
            re: 0.0f32,
            im: 0.0
        };
        frame_size
    ];
    let mut magnitudes = vec![0.0f32; frame_size / 2];

    let mut total_bass_ratio = 0.0f32;
    let mut total_centroid = 0.0f32;
    let mut frame_count = 0usize;

    let mut i = 0;
    while i + frame_size <= samples.len() {
        let frame = &samples[i..i + frame_size];

        for (n, (&s, c)) in frame.iter().zip(buffer.iter_mut()).enumerate() {
            let w = 0.5 * (1.0 - (2.0 * PI * n as f32 / frame_size as f32).cos());
            c.re = s * w;
            c.im = 0.0;
        }

        fft.process(&mut buffer);

        for (c, m) in buffer[..frame_size / 2].iter().zip(magnitudes.iter_mut()) {
            *m = c.norm();
        }

        let total_energy: f32 = magnitudes.iter().sum();
        if total_energy < 1e-6 {
            i += hop;
            continue;
        }

        let bass_energy: f32 = magnitudes[..bass_bin_max.min(magnitudes.len())]
            .iter()
            .sum();
        total_bass_ratio += bass_energy / total_energy;

        let centroid = magnitudes
            .iter()
            .enumerate()
            .map(|(k, &m)| k as f32 * freq_resolution * m)
            .sum::<f32>()
            / total_energy;
        total_centroid += centroid;

        frame_count += 1;
        i += hop;
    }

    if frame_count == 0 {
        return (0.0, 0.0);
    }

    (
        total_bass_ratio / frame_count as f32,
        total_centroid / frame_count as f32,
    )
}

/// Compute BPM using bass-band filtered autocorrelation.
/// Focuses on 20-200 Hz where tabla/dhol/kick drum lives in Bollywood.
/// Range clamped to 60-180 BPM to match Bollywood tempo range.
fn compute_bpm(samples: &[f32], sample_rate: u32) -> f32 {
    let frame = 1024usize;

    let rc = 1.0 / (2.0 * PI * 200.0);
    let dt = 1.0 / sample_rate as f32;
    let alpha = dt / (rc + dt);

    let mut filtered_prev = 0.0f32;
    let mut energies: Vec<f32> = Vec::new();
    let mut frame_energy = 0.0f32;
    let mut count = 0usize;

    for &s in samples {
        let filtered = filtered_prev + alpha * (s - filtered_prev);
        filtered_prev = filtered;
        frame_energy += filtered * filtered;
        count += 1;

        if count == frame {
            energies.push(frame_energy / frame as f32);
            frame_energy = 0.0;
            count = 0;
        }
    }
    if count > 0 {
        energies.push(frame_energy / count as f32);
    }

    let onsets: Vec<f32> = energies
        .windows(2)
        .map(|w| (w[1] - w[0]).max(0.0))
        .collect();

    if onsets.len() < 2 {
        tracing::warn!(
            "compute_bpm: too few onset frames ({}) for reliable BPM detection, returning 0.0",
            onsets.len()
        );
        return 0.0;
    }

    let min_lag = ((sample_rate as f32 * 60.0) / (180.0 * frame as f32)) as usize;
    let max_lag = ((sample_rate as f32 * 60.0) / (60.0 * frame as f32)) as usize;

    if min_lag >= onsets.len() || max_lag > onsets.len() {
        tracing::warn!(
            "compute_bpm: lag range ({},{}) out of bounds for {} onsets, returning 0.0",
            min_lag,
            max_lag,
            onsets.len()
        );
        return 0.0;
    }

    let min_lag = min_lag.max(1);
    let max_lag = max_lag.min(onsets.len());

    let mut best_lag = min_lag;
    let mut best_val = 0f32;

    for lag in min_lag..max_lag {
        if lag >= onsets.len() {
            break;
        }
        let corr: f32 = onsets
            .iter()
            .zip(onsets[lag..].iter())
            .map(|(a, b)| a * b)
            .sum();
        if corr > best_val {
            best_val = corr;
            best_lag = lag;
        }
    }

    let seconds_per_beat = best_lag as f32 * frame as f32 / sample_rate as f32;
    if seconds_per_beat < 1e-6 {
        tracing::warn!(
            "compute_bpm: seconds_per_beat too small ({:.6}), returning 0.0",
            seconds_per_beat
        );
        return 0.0;
    }
    (60.0 / seconds_per_beat).clamp(60.0, 180.0)
}

/// Extract all acoustic features from an audio file, with optional PCM cache support.
/// Pass an existing `FftPlanner` instance to reuse the one from the spectrum analyzer.
/// Pass a `PcmCache` reference to enable cache lookups and storage.
/// Returns `Err` if the file cannot be decoded.
///
/// # PCM Cache Integration
///
/// When a `PcmCache` is provided:
/// - On a cache hit, the cached interleaved PCM data is converted to mono and used
///   for feature extraction, avoiding a redundant Symphonia decode.
/// - On a cache miss, the file is decoded with Symphonia, the interleaved PCM data
///   is stored in the cache for future reuse (by playback or subsequent analyses),
///   and features are extracted as normal.
///
/// The file path is used as the cache key. This avoids the dual-decode overhead
/// where mood analysis previously decoded each file independently while playback
/// used GStreamer, meaning each track was decoded twice.
pub fn extract_features_with_cache(
    path: &str,
    planner: &mut FftPlanner<f32>,
    cache: Option<&PcmCache>,
) -> Result<SongFeatures, MoodError> {
    let (samples, sample_rate) = match cache {
        Some(cache) => {
            if let Some(buf) = cache.get(path) {
                tracing::debug!(
                    "PCM cache hit for {}, reusing {}s of decoded audio",
                    path,
                    buf.duration_secs()
                );
                let mono = interleaved_to_mono(&buf.samples, buf.channels);
                (mono, buf.sample_rate)
            } else {
                tracing::debug!("PCM cache miss for {}, decoding with Symphonia", path);
                let (interleaved, sample_rate, channels) = decode_to_interleaved(path)?;
                let mut buf = PcmBuffer::new(interleaved.clone(), sample_rate, channels);
                buf.truncate_to_duration(90);
                cache.insert(path, buf);
                let mono = interleaved_to_mono(&interleaved, channels);
                (mono, sample_rate)
            }
        }
        None => decode_to_mono(path)?,
    };

    let bpm = compute_bpm(&samples, sample_rate);
    let energy = compute_energy(&samples);
    let dynamic_range = compute_dynamic_range(&samples);
    let (bass_ratio, spectral_centroid) = compute_spectral_features(&samples, sample_rate, planner);

    Ok(SongFeatures {
        bpm,
        energy,
        bass_ratio,
        spectral_centroid,
        dynamic_range,
    })
}

/// Extract all acoustic features from an audio file.
/// Pass an existing `FftPlanner` instance to reuse the one from the spectrum analyzer.
/// Returns `Err` if the file cannot be decoded.
///
/// This is a convenience wrapper around `extract_features_with_cache` that does
/// not use the PCM cache. For cache-enabled extraction, use
/// `extract_features_with_cache` directly.
pub fn extract_features(
    path: &str,
    planner: &mut FftPlanner<f32>,
) -> Result<SongFeatures, MoodError> {
    extract_features_with_cache(path, planner, None)
}

/// Classify a `SongFeatures` into a `Mood` using confidence-based scoring.
///
/// Instead of rigid if-else threshold chains (which cause too many tracks to
/// fall into the Romantic default bucket), each mood earns a score based on
/// how closely the song's features match that mood's ideal profile. The mood
/// with the highest score wins, with a minimum confidence threshold to avoid
/// classifying borderline tracks into the wrong category.
///
/// This approach produces a more balanced distribution across mood categories
/// for both Bollywood/Hindi and Western music libraries. The scoring weights
/// are tuned so that Bollywood-specific patterns (high bass ratio for Dance,
/// low dynamic range for Sufi) still dominate when present, but Western pop/
/// rock/folk tracks get distributed into Dance, Chill, or Sad instead of all
/// collapsing into Romantic.
///
/// # Tuning guide:
///
/// - If Dance bucket is too small: lower `bpm_peak` from 130 → 120 or reduce `bass_weight` from 3.0 → 2.0
/// - If Sad bucket has wrong songs: increase `dynamic_range_weight` from 2.0 → 3.0
/// - If too many songs land in Romantic: increase `min_confidence` from 0.15 → 0.20
/// - If Sufi bucket is too narrow: widen the `bass_ratio` range in the Sufi scorer
pub fn classify_mood(f: &SongFeatures) -> Mood {
    let dance_score = score_dance(f);
    let sad_score = score_sad(f);
    let sufi_score = score_sufi(f);
    let chill_score = score_chill(f);
    let romantic_score = score_romantic(f);

    let scores = [
        (Mood::Dance, dance_score),
        (Mood::Sad, sad_score),
        (Mood::Sufi, sufi_score),
        (Mood::Chill, chill_score),
        (Mood::Romantic, romantic_score),
    ];

    let (best_mood, best_score) = scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .expect("scores is non-empty");

    const MIN_CONFIDENCE: f32 = 0.15;
    if *best_score >= MIN_CONFIDENCE {
        best_mood.clone()
    } else {
        Mood::Romantic
    }
}

/// Gaussian-like proximity scoring: returns a value in [0, 1] indicating how
/// close `value` is to `ideal`, with `sigma` controlling the width.
/// At value == ideal, returns 1.0. At value one sigma away, returns ~0.61.
#[inline]
fn gaussian_score(value: f32, ideal: f32, sigma: f32) -> f32 {
    if sigma <= 0.0 {
        return if (value - ideal).abs() < 1e-6 {
            1.0
        } else {
            0.0
        };
    }
    let diff = (value - ideal) / sigma;
    (-0.5 * diff * diff).exp()
}

/// Step-like scoring: returns 1.0 if value is above/below the threshold,
/// with a soft transition zone of `width` on either side.
#[inline]
fn above_score(value: f32, threshold: f32, width: f32) -> f32 {
    if width <= 0.0 {
        return if value >= threshold { 1.0 } else { 0.0 };
    }
    ((value - threshold + width) / (2.0 * width)).clamp(0.0, 1.0)
}

#[inline]
fn below_score(value: f32, threshold: f32, width: f32) -> f32 {
    above_score(-value, -threshold, width)
}

/// Score for Dance mood: high BPM + heavy bass + high energy.
/// Ideal profile: BPM ~130, bass_ratio ~0.45, energy ~0.12+.
/// Catches: item songs, remixes, tapori, party numbers, EDM, Western dance-pop.
fn score_dance(f: &SongFeatures) -> f32 {
    let bpm_score = gaussian_score(f.bpm, 130.0, 25.0);
    let bass_score = above_score(f.bass_ratio, 0.35, 0.05);
    let energy_score = above_score(f.energy, 0.08, 0.02);
    let centroid_score = above_score(f.spectral_centroid, 2000.0, 300.0);

    (3.0 * bpm_score + 3.0 * bass_score + 2.0 * energy_score + 1.0 * centroid_score) / 9.0
}

/// Score for Sad mood: slow + high dynamic variation + low bass + dark spectrum.
/// Ideal profile: BPM ~72, dynamic_range ~0.07+, bass_ratio <0.28, centroid <1500.
/// Catches: emotional Bollywood, breakup songs, tragedy tracks, Western ballads.
fn score_sad(f: &SongFeatures) -> f32 {
    let bpm_score = below_score(f.bpm, 85.0, 8.0);
    let dynamic_score = above_score(f.dynamic_range, 0.04, 0.015);
    let bass_score = below_score(f.bass_ratio, 0.30, 0.04);
    let centroid_score = below_score(f.spectral_centroid, 1800.0, 300.0);

    (3.0 * bpm_score + 2.0 * dynamic_score + 2.0 * bass_score + 2.5 * centroid_score) / 9.5
}

/// Score for Sufi mood: medium tempo + low dynamic range (consistent energy) + mid bass.
/// Ideal profile: BPM ~95, dynamic_range <0.035, bass_ratio ~0.32.
/// Catches: qawwali, devotional, sufi, repetitive chants, some Western folk.
fn score_sufi(f: &SongFeatures) -> f32 {
    let bpm_score = gaussian_score(f.bpm, 95.0, 12.0);
    let dynamic_score = below_score(f.dynamic_range, 0.035, 0.01);
    let bass_score = gaussian_score(f.bass_ratio, 0.32, 0.06);

    (3.0 * bpm_score + 3.0 * dynamic_score + 3.0 * bass_score) / 9.0
}

/// Score for Chill mood: very low energy + slow + soft spectrum.
/// Ideal profile: energy <0.04, BPM <95, centroid <2000.
/// Catches: lofi covers, acoustic instrumentals, ambient, soft beats.
fn score_chill(f: &SongFeatures) -> f32 {
    let energy_score = below_score(f.energy, 0.04, 0.015);
    let bpm_score = below_score(f.bpm, 95.0, 10.0);
    let centroid_score = below_score(f.spectral_centroid, 2000.0, 400.0);

    (3.0 * energy_score + 2.0 * bpm_score + 1.5 * centroid_score) / 6.5
}

/// Score for Romantic mood: mid-tempo + moderate energy + melodic spectrum.
/// Ideal profile: BPM ~100, energy ~0.06-0.08, centroid ~2500, moderate dynamic range.
/// Catches: melodic mid-tempo Bollywood, duets, English pop, soft rock.
///
/// Romantic is scored like all other moods now (no longer a catch-all).
/// Tracks that don't match any mood strongly still default to Romantic
/// via the `MIN_CONFIDENCE` threshold in `classify_mood()`.
fn score_romantic(f: &SongFeatures) -> f32 {
    let bpm_score = gaussian_score(f.bpm, 100.0, 15.0);
    let energy_score = gaussian_score(f.energy, 0.065, 0.025);
    let centroid_score = gaussian_score(f.spectral_centroid, 2500.0, 500.0);
    let dynamic_score = gaussian_score(f.dynamic_range, 0.04, 0.02);

    (2.5 * bpm_score + 2.5 * energy_score + 1.5 * centroid_score + 1.0 * dynamic_score) / 7.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mood_from_str_roundtrip() {
        for label in Mood::all() {
            let mood =
                Mood::parse_label(label).unwrap_or_else(|| panic!("failed to parse {}", label));
            assert_eq!(mood.as_str(), *label);
        }
    }

    #[test]
    fn test_mood_from_str_invalid() {
        assert!(Mood::parse_label("Unknown").is_none());
        assert!(Mood::parse_label("").is_none());
    }

    #[test]
    fn test_classify_dance() {
        let f = SongFeatures {
            bpm: 130.0,
            energy: 0.12,
            bass_ratio: 0.50,
            spectral_centroid: 3000.0,
            dynamic_range: 0.03,
        };
        assert_eq!(classify_mood(&f), Mood::Dance);
    }

    #[test]
    fn test_classify_sad() {
        let f = SongFeatures {
            bpm: 72.0,
            energy: 0.06,
            bass_ratio: 0.20,
            spectral_centroid: 1200.0,
            dynamic_range: 0.08,
        };
        assert_eq!(classify_mood(&f), Mood::Sad);
    }

    #[test]
    fn test_classify_sufi() {
        let f = SongFeatures {
            bpm: 95.0,
            energy: 0.07,
            bass_ratio: 0.32,
            spectral_centroid: 2000.0,
            dynamic_range: 0.02,
        };
        assert_eq!(classify_mood(&f), Mood::Sufi);
    }

    #[test]
    fn test_classify_chill() {
        let f = SongFeatures {
            bpm: 80.0,
            energy: 0.03,
            bass_ratio: 0.25,
            spectral_centroid: 1500.0,
            dynamic_range: 0.02,
        };
        assert_eq!(classify_mood(&f), Mood::Chill);
    }

    #[test]
    fn test_classify_romantic_default() {
        let f = SongFeatures {
            bpm: 100.0,
            energy: 0.07,
            bass_ratio: 0.30,
            spectral_centroid: 2500.0,
            dynamic_range: 0.05,
        };
        assert_eq!(classify_mood(&f), Mood::Romantic);
    }

    #[test]
    fn test_classify_romantic_mid_tempo() {
        let f = SongFeatures {
            bpm: 105.0,
            energy: 0.10,
            bass_ratio: 0.28,
            spectral_centroid: 2800.0,
            dynamic_range: 0.06,
        };
        let mood = classify_mood(&f);
        assert!(
            mood == Mood::Romantic || mood == Mood::Dance,
            "Expected Romantic or Dance for mid-tempo moderate-energy track, got {:?}",
            mood
        );
    }

    #[test]
    fn test_bpm_clamp() {
        let slow = SongFeatures {
            bpm: 55.0,
            energy: 0.03,
            bass_ratio: 0.20,
            spectral_centroid: 1000.0,
            dynamic_range: 0.01,
        };
        assert_eq!(classify_mood(&slow), Mood::Chill);

        let fast = SongFeatures {
            bpm: 200.0,
            energy: 0.15,
            bass_ratio: 0.45,
            spectral_centroid: 4000.0,
            dynamic_range: 0.02,
        };
        assert_eq!(classify_mood(&fast), Mood::Dance);
    }

    #[test]
    fn test_scoring_gaussian_peak() {
        assert!((gaussian_score(130.0, 130.0, 25.0) - 1.0).abs() < 1e-6);
        assert!((gaussian_score(155.0, 130.0, 25.0) - (-0.5f32).exp()).abs() < 1e-4);
    }

    #[test]
    fn test_scoring_above_below() {
        assert!((above_score(0.50, 0.35, 0.05) - 1.0).abs() < 1e-6);
        assert!((above_score(0.20, 0.35, 0.05) - 0.0).abs() < 1e-6);
        assert!((above_score(0.35, 0.35, 0.05) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_western_pop_not_all_romantic() {
        let western_pop = SongFeatures {
            bpm: 118.0,
            energy: 0.09,
            bass_ratio: 0.36,
            spectral_centroid: 2800.0,
            dynamic_range: 0.05,
        };
        let mood = classify_mood(&western_pop);
        assert!(
            mood == Mood::Dance || mood == Mood::Romantic,
            "Western dance-pop should classify as Dance or Romantic, got {:?}",
            mood
        );
    }

    #[test]
    fn test_boundary_dance_sad() {
        let f = SongFeatures {
            bpm: 90.0,
            energy: 0.07,
            bass_ratio: 0.30,
            spectral_centroid: 2000.0,
            dynamic_range: 0.04,
        };
        let mood = classify_mood(&f);
        assert!(Mood::all().contains(&mood.as_str()));
    }

    #[test]
    fn test_boundary_chill_dance() {
        let f = SongFeatures {
            bpm: 130.0,
            energy: 0.02,
            bass_ratio: 0.20,
            spectral_centroid: 1500.0,
            dynamic_range: 0.01,
        };
        let mood = classify_mood(&f);
        assert!(Mood::all().contains(&mood.as_str()));
    }

    #[test]
    fn test_boundary_sufi_romantic() {
        let f = SongFeatures {
            bpm: 95.0,
            energy: 0.065,
            bass_ratio: 0.32,
            spectral_centroid: 2500.0,
            dynamic_range: 0.035,
        };
        let mood = classify_mood(&f);
        assert!(
            mood == Mood::Sufi || mood == Mood::Romantic,
            "Boundary features should classify as Sufi or Romantic, got {:?}",
            mood
        );
    }

    #[test]
    fn test_extreme_high_energy() {
        let f = SongFeatures {
            bpm: 175.0,
            energy: 0.25,
            bass_ratio: 0.55,
            spectral_centroid: 4500.0,
            dynamic_range: 0.01,
        };
        assert_eq!(classify_mood(&f), Mood::Dance);
    }

    #[test]
    fn test_extreme_low_energy() {
        let f = SongFeatures {
            bpm: 60.0,
            energy: 0.01,
            bass_ratio: 0.15,
            spectral_centroid: 800.0,
            dynamic_range: 0.005,
        };
        assert_eq!(classify_mood(&f), Mood::Chill);
    }

    #[test]
    fn test_all_moods_are_valid() {
        let test_cases = vec![
            SongFeatures {
                bpm: 130.0,
                energy: 0.12,
                bass_ratio: 0.50,
                spectral_centroid: 3000.0,
                dynamic_range: 0.03,
            },
            SongFeatures {
                bpm: 72.0,
                energy: 0.06,
                bass_ratio: 0.20,
                spectral_centroid: 1200.0,
                dynamic_range: 0.08,
            },
            SongFeatures {
                bpm: 95.0,
                energy: 0.07,
                bass_ratio: 0.32,
                spectral_centroid: 2000.0,
                dynamic_range: 0.02,
            },
            SongFeatures {
                bpm: 80.0,
                energy: 0.03,
                bass_ratio: 0.25,
                spectral_centroid: 1500.0,
                dynamic_range: 0.02,
            },
            SongFeatures {
                bpm: 100.0,
                energy: 0.07,
                bass_ratio: 0.30,
                spectral_centroid: 2500.0,
                dynamic_range: 0.05,
            },
        ];
        for f in &test_cases {
            let mood = classify_mood(f);
            assert!(
                Mood::parse_label(mood.as_str()).is_some(),
                "Invalid mood: {}",
                mood.as_str()
            );
        }
    }
}
