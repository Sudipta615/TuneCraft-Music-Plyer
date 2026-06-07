//! Audio analysis — BPM detection, mood classification, chroma/key detection,
//! lyrics sentiment, and waveform generation.
//!
//! ## What's new in v0.25.0
//!
//! The mood classifier now uses **two additional signal layers** to sharpen the
//! Romantic vs Sad distinction, which is the hardest classification problem for
//! slow Bollywood tracks:
//!
//! - **Chroma / key-mode detection** (`chroma` module): 12 Goertzel filters + Krumhansl–Schmuckler
//!   key-profile matching → major/minor mode. Minor key biases toward Sad; major key biases toward
//!   Romantic.
//!
//! - **Lyrics sentiment** (`lyrics_sentiment` module): a 400+ entry bilingual lexicon (Hindi/Urdu
//!   Devanagari, romanised Hindi, English) that scores lyrics text as Romantic or Sad.  When lyrics
//!   are available in the DB (fetched by the LRCLIB integration), they are passed to `analyze_file`
//!   and the result overrides the audio-only classification when confidence is sufficient (≥ 0.15).
//!
//! All new analysis runs on the existing background thread; nothing blocks
//! the audio callback or the UI thread.

mod bpm;
mod chroma;
mod lyrics_sentiment;
mod mood;
mod waveform;

use std::path::Path;

pub use bpm::BpmDetector;
pub use chroma::{ChromaDetector, KeyMode, PitchClass};
use log::{info, warn};
pub use lyrics_sentiment::{LyricsSentiment, Sentiment, SentimentResult};
pub use mood::MoodClassifier;
use thiserror::Error;
pub use waveform::WaveformGenerator;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("Analysis failed: {0}")]
    Failed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("Invalid sample rate: {0}")]
    InvalidSampleRate(f32),
    #[error("FFT size must be >= 2 and a power of two, got {0}")]
    InvalidFftSize(usize),
    #[error("WaveformGenerator samples_per_pixel must be > 0, got {0}")]
    InvalidSamplesPerPixel(usize),
}

/// BPM detection result
#[derive(Debug, Clone)]
pub struct BpmResult {
    pub bpm: f32,
    pub confidence: f32,
}

/// Mood classification result
#[derive(Debug, Clone)]
pub struct MoodResult {
    pub mood: String,
    pub energy: f32,
    pub valence: f32,
}

/// Waveform data for visualization
#[derive(Debug, Clone)]
pub struct WaveformResult {
    pub peaks: Vec<(f32, f32)>,
    pub samples_per_pixel: usize,
    /// Total number of audio frames analyzed
    pub total_frames: usize,
}

/// Complete analysis result for a single track
#[derive(Debug, Clone)]
pub struct TrackAnalysis {
    pub bpm: BpmResult,
    pub mood: MoodResult,
    /// Detected musical key and mode (None if tonality is ambiguous or
    /// insufficient audio was processed).
    pub key: Option<KeyMode>,
    pub duration_secs: f32,
    pub sample_rate: u32,
    pub channels: usize,
}

/// Analyze a complete audio file and return full analysis results.
///
/// ## Parameters
///
/// - `path` — path to the audio file (any format Symphonia supports).
/// - `max_duration_secs` — cap on how much audio to decode.  `None` uses 120 s. The full 120 s
///   limit is still used for signal analysis; chroma detection may finish earlier once sufficient
///   data is accumulated.
/// - `lyrics` — optional lyrics text already in the database (synced LRC preferred, plain text
///   fallback).  When `Some`, the lyrics sentiment analyser runs and can override the Romantic/Sad
///   classification.
///
/// ## Threading
///
/// This function is synchronous and CPU-bound.  Always call it from a
/// dedicated background thread (e.g. the library-scan thread pool), never
/// from the audio callback or the UI thread.
pub fn analyze_file(
    path: &Path,
    max_duration_secs: Option<f32>,
    lyrics: Option<&str>,
) -> Result<TrackAnalysis, AnalysisError> {
    use symphonia::core::{
        audio::SampleBuffer,
        codecs::{DecoderOptions, CODEC_TYPE_NULL},
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    };

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    let decoder_opts = DecoderOptions::default();

    info!("Analyzing file: {}", path.display());

    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| AnalysisError::Decode(format!("Probe failed: {}", e)))?;

    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AnalysisError::Decode("No audio track found".to_string()))?;

    let codec_params = &track.codec_params;
    let sample_rate = codec_params.sample_rate.unwrap_or(44100) as f32;
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let duration = codec_params
        .n_frames
        .map(|n| n as f32 / sample_rate)
        .unwrap_or(0.0);

    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &decoder_opts)
        .map_err(|e| AnalysisError::Decode(format!("Decoder creation failed: {}", e)))?;

    let max_duration = max_duration_secs.unwrap_or(120.0);
    let mut bpm_detector = BpmDetector::new(sample_rate)
        .map_err(|e| AnalysisError::Failed(format!("BpmDetector init failed: {}", e)))?;
    let mut mood_classifier = MoodClassifier::new(sample_rate)
        .map_err(|e| AnalysisError::Failed(format!("MoodClassifier init failed: {}", e)))?;
    let mut chroma_detector = ChromaDetector::new(sample_rate)
        .map_err(|e| AnalysisError::Failed(format!("ChromaDetector init failed: {}", e)))?;

    let mut total_samples: u64 = 0;
    let max_samples = (max_duration * sample_rate) as u64;
    let mut stereo_chunk: Vec<(f32, f32)> = Vec::with_capacity(512);
    let mut decode_errors: u32 = 0;

    loop {
        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            },
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                let spec = *audio_buf.spec();
                let num_channels = spec.channels.count();
                let frames = audio_buf.frames();

                let mut sample_buf: SampleBuffer<f32> = SampleBuffer::new(frames as u64, spec);
                sample_buf.copy_interleaved_ref(audio_buf);
                let samples = sample_buf.samples();

                stereo_chunk.clear();
                if num_channels > 1 {
                    for frame in samples.chunks_exact(num_channels) {
                        let l = frame[0] as f32;
                        let r = frame[1] as f32;
                        stereo_chunk.push((l, r));
                        if stereo_chunk.len() >= 512 {
                            bpm_detector.feed(&stereo_chunk);
                            mood_classifier.feed(&stereo_chunk);
                            chroma_detector.feed(&stereo_chunk);
                            stereo_chunk.clear();
                        }
                    }
                    total_samples += (samples.len() / num_channels) as u64;
                } else {
                    for &s in samples.iter() {
                        let v = s as f32;
                        stereo_chunk.push((v, v));
                        if stereo_chunk.len() >= 512 {
                            bpm_detector.feed(&stereo_chunk);
                            mood_classifier.feed(&stereo_chunk);
                            chroma_detector.feed(&stereo_chunk);
                            stereo_chunk.clear();
                        }
                    }
                    total_samples += samples.len() as u64;
                }
                if !stereo_chunk.is_empty() {
                    bpm_detector.feed(&stereo_chunk);
                    mood_classifier.feed(&stereo_chunk);
                    chroma_detector.feed(&stereo_chunk);
                }

                if total_samples >= max_samples {
                    info!(
                        "Analysis duration limit reached ({:.0}s), stopping decode",
                        max_duration
                    );
                    break;
                }
            },
            Err(e) => {
                decode_errors += 1;
                if decode_errors <= 5 {
                    warn!(
                        "Decode error in {}: {}, continuing with partial data",
                        path.display(),
                        e
                    );
                } else if decode_errors == 6 {
                    warn!("Too many decode errors, suppressing further warnings");
                }
                if decode_errors >= 50 {
                    warn!(
                        "Aborting analysis of {} after {} decode errors — file is too corrupt",
                        path.display(),
                        decode_errors
                    );
                    return Err(AnalysisError::Decode(format!(
                        "Too many decode errors ({}) in {}",
                        decode_errors,
                        path.display()
                    )));
                }
                continue;
            },
        }
    }

    if decode_errors > 0 {
        warn!(
            "File {} had {} decode errors; analysis may be incomplete",
            path.display(),
            decode_errors
        );
    }

    let bpm_result = bpm_detector.detect();
    let key_result = chroma_detector.detect();

    // --- Stage 1: audio-only classification ---
    let mut mood_result = mood_classifier.classify_with_bpm(bpm_result.bpm, bpm_result.confidence);

    // --- Stage 2: refine with chroma key/mode ---
    // Only apply when the audio classifier landed in the ambiguous Romantic/Sad
    // zone (low energy, low flux) AND chroma confidence is reasonable (≥ 0.55).
    // We deliberately use a high chroma confidence threshold so a noisy or
    // percussion-heavy signal doesn't incorrectly override the mood label.
    if let Some(ref km) = key_result {
        let in_grey_zone = mood_result.mood == "Romantic" || mood_result.mood == "Sad";
        if in_grey_zone && km.confidence >= 0.55 {
            if km.is_major && mood_result.mood == "Sad" {
                // Major key + Sad signal → more likely Romantic; flip.
                mood_result.mood = "Romantic".to_string();
                mood_result.valence = (mood_result.valence * 1.3).min(1.0);
                info!(
                    "Chroma override: {} major → Romantic (conf={:.2})",
                    km.tonic.name(),
                    km.confidence
                );
            } else if !km.is_major && mood_result.mood == "Romantic" {
                // Minor key + Romantic signal → more likely Sad; flip.
                mood_result.mood = "Sad".to_string();
                mood_result.valence = (mood_result.valence * 0.7).min(1.0);
                info!(
                    "Chroma override: {} minor → Sad (conf={:.2})",
                    km.tonic.name(),
                    km.confidence
                );
            }
        }
    }

    // --- Stage 3: refine with lyrics sentiment (highest priority) ---
    // Lyrics are the strongest single signal for Romantic/Sad disambiguation.
    // We only override when:
    //   (a) lyrics are available,
    //   (b) the track is already in the Romantic/Sad zone after stages 1+2,
    //   (c) sentiment confidence ≥ 0.15 (not ambiguous).
    if let Some(text) = lyrics {
        if !text.trim().is_empty() {
            let analyser = LyricsSentiment::new();
            let sentiment = analyser.analyse(text);

            let in_grey_zone = mood_result.mood == "Romantic" || mood_result.mood == "Sad";

            if in_grey_zone && sentiment.sentiment != Sentiment::Ambiguous {
                let new_mood = match sentiment.sentiment {
                    Sentiment::Romantic => "Romantic",
                    Sentiment::Sad => "Sad",
                    Sentiment::Ambiguous => unreachable!(),
                };
                if new_mood != mood_result.mood {
                    info!(
                        "Lyrics override: {} → {} (R={} S={} conf={:.2})",
                        mood_result.mood,
                        new_mood,
                        sentiment.romantic_hits,
                        sentiment.sad_hits,
                        sentiment.confidence
                    );
                    mood_result.mood = new_mood.to_string();
                    if new_mood == "Sad" {
                        mood_result.valence = (mood_result.valence * 0.65).min(1.0);
                    } else {
                        mood_result.valence = (mood_result.valence * 1.25).min(1.0);
                    }
                }
            }
        }
    }

    let actual_duration = if duration > 0.0 {
        duration
    } else if sample_rate > 0.0 {
        total_samples as f32 / sample_rate
    } else {
        0.0
    };

    info!(
        "Analysis complete for {}: BPM={:.1} (conf={:.2}), mood={}, key={}, duration={:.1}s",
        path.display(),
        bpm_result.bpm,
        bpm_result.confidence,
        mood_result.mood,
        key_result.as_ref().map_or("?".to_string(), |k| format!(
            "{} {}",
            k.tonic.name(),
            if k.is_major { "maj" } else { "min" }
        )),
        actual_duration,
    );

    Ok(TrackAnalysis {
        bpm: bpm_result,
        mood: mood_result,
        key: key_result,
        duration_secs: actual_duration,
        sample_rate: sample_rate as u32,
        channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_file_nonexistent() {
        let result = analyze_file(std::path::Path::new("/nonexistent/file.mp3"), None, None);
        assert!(result.is_err());
    }
}
