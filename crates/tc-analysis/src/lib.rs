//! Audio analysis — BPM detection, chroma/key detection, EBU R128 / ReplayGain 2.0
//! loudness measurement, and waveform generation.
//!
//! ## Modules
//!
//! - `bpm` — onset-strength-based tempo detection (60–200 BPM).
//! - `chroma` — 12-bin chroma + Krumhansl–Schmuckler key-profile matching.
//! - `loudness` — ITU-R BS.1770-4 integrated loudness (LUFS) + derived
//!   ReplayGain 2.0 track gain, with K-weighting duplicated from
//!   `tc_engine::dsp::loudness` so this crate has no runtime dependency
//!   on the audio engine.
//! - `waveform` — min/max peak waveform thumbnails for the UI.
//!
//! All analysis runs on the existing background thread; nothing blocks
//! the audio callback or the UI thread.

mod bpm;
mod chroma;
mod loudness;
mod waveform;

use std::path::Path;

pub use bpm::BpmDetector;
pub use chroma::{ChromaDetector, KeyMode, PitchClass};
use log::{info, warn};
pub use loudness::LoudnessAnalyzer;
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
    /// Detected musical key and mode (None if tonality is ambiguous or
    /// insufficient audio was processed).
    pub key: Option<KeyMode>,
    /// EBU R128 integrated loudness / ReplayGain 2.0 track gain. Populated
    /// since v3.0.0; previously these columns existed in the DB schema but
    /// were never filled, so loudness normalization silently did nothing.
    pub loudness: loudness::LoudnessResult,
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
///
/// ## Threading
///
/// This function is synchronous and CPU-bound.  Always call it from a
/// dedicated background thread (e.g. the library-scan thread pool), never
/// from the audio callback or the UI thread.
pub fn analyze_file(
    path: &Path,
    max_duration_secs: Option<f32>,
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
    let mut chroma_detector = ChromaDetector::new(sample_rate)
        .map_err(|e| AnalysisError::Failed(format!("ChromaDetector init failed: {}", e)))?;
    let mut loudness_analyzer = LoudnessAnalyzer::new(sample_rate);

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
            // Log non-EOF packet errors at debug level before terminating the
            // decode loop. Previously every error was silently swallowed,
            // which made corrupted or truncated files look like a successful
            // analysis with whatever partial data was decoded.
            Err(ref e) => {
                log::debug!("Ending analysis of {}: packet error: {}", path.display(), e);
                break;
            },
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
                        let l = frame[0];
                        let r = frame[1];
                        stereo_chunk.push((l, r));
                        if stereo_chunk.len() >= 512 {
                            bpm_detector.feed(&stereo_chunk);
                            chroma_detector.feed(&stereo_chunk);
                            for &(sl, sr) in &stereo_chunk {
                                loudness_analyzer.process_pair(sl, sr);
                            }
                            stereo_chunk.clear();
                        }
                    }
                    total_samples += (samples.len() / num_channels) as u64;
                } else {
                    for &s in samples.iter() {
                        let v = s;
                        stereo_chunk.push((v, v));
                        if stereo_chunk.len() >= 512 {
                            bpm_detector.feed(&stereo_chunk);
                            chroma_detector.feed(&stereo_chunk);
                            for &(sl, sr) in &stereo_chunk {
                                loudness_analyzer.process_pair(sl, sr);
                            }
                            stereo_chunk.clear();
                        }
                    }
                    total_samples += samples.len() as u64;
                }
                if !stereo_chunk.is_empty() {
                    bpm_detector.feed(&stereo_chunk);
                    chroma_detector.feed(&stereo_chunk);
                    for &(sl, sr) in &stereo_chunk {
                        loudness_analyzer.process_pair(sl, sr);
                    }
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
    let loudness_result = loudness_analyzer.finish();

    let actual_duration = if duration > 0.0 {
        duration
    } else if sample_rate > 0.0 {
        total_samples as f32 / sample_rate
    } else {
        0.0
    };

    info!(
        "Analysis complete for {}: BPM={:.1} (conf={:.2}), key={}, loudness={:?}, duration={:.1}s",
        path.display(),
        bpm_result.bpm,
        bpm_result.confidence,
        key_result.as_ref().map_or("?".to_string(), |k| format!(
            "{} {}",
            k.tonic.name(),
            if k.is_major { "maj" } else { "min" }
        )),
        loudness_result.ebu_r128_loudness,
        actual_duration,
    );

    Ok(TrackAnalysis {
        bpm: bpm_result,
        key: key_result,
        loudness: loudness_result,
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
        let result = analyze_file(std::path::Path::new("/nonexistent/file.mp3"), None);
        assert!(result.is_err());
    }
}
