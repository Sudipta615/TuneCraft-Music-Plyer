//! Audio decoder using Symphonia for format support
//! Supports MP3, FLAC, OGG/Vorbis, WAV, AAC, and more
//! All decoding is off the audio thread and thread-safe

use std::{fs::File, path::Path};

use symphonia::core::{
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
    units::Time,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("Failed to open file: {0}")]
    FileOpen(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("Seek error: {0}")]
    Seek(String),
    #[error("End of stream")]
    EndOfStream,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Decoded audio format information
#[derive(Debug, Clone)]
pub struct DecodeInfo {
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_secs: f32,
    pub codec: String,
    pub bitrate_kbps: Option<u32>,
}

/// A chunk of decoded PCM audio
#[derive(Debug, Clone)]
pub struct DecodedChunk {
    /// Interleaved f32 samples (L, R, L, R, ...)
    pub samples: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
    pub frame_count: usize,
}

/// Symphonia-based audio decoder
pub struct SymphoniaDecoder {
    format_reader: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    info: DecodeInfo,
    /// Reusable sample buffer for decoded output, passed across
    /// decode_next calls instead of allocating a new Vec each time.
    sample_buffer: Vec<f32>,
    /// Symphonia sample buffer for safe format conversion
    conversion_buffer: Option<symphonia::core::audio::SampleBuffer<f32>>,
}

impl SymphoniaDecoder {
    /// Open a file for decoding
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path)
            .map_err(|e| DecodeError::FileOpen(format!("Cannot open {}: {}", path.display(), e)))?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };

        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(|e| DecodeError::UnsupportedFormat(format!("Probe failed: {}", e)))?;

        let format_reader = probed.format;

        let track = format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| DecodeError::UnsupportedFormat("No audio track found".to_string()))?;

        let track_id = track.id;
        let codec_params = &track.codec_params;

        if let Some(delay) = codec_params.delay {
            log::info!("Gapless metadata found: {} samples delay", delay);
        }
        if let Some(padding) = codec_params.padding {
            log::info!("Gapless metadata found: {} samples padding", padding);
        }

        let decoder = symphonia::default::get_codecs()
            .make(codec_params, &decoder_opts)
            .map_err(|e| DecodeError::Decode(format!("Cannot create decoder: {}", e)))?;

        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let src_channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
        // I-07: Warn when loading a file with more than 2 channels.
        // We down-mix to stereo; surround channels beyond L/R are discarded.
        if src_channels > 2 {
            log::warn!(
                "File has {} channels; tc-engine supports up to 2 channels.                  Only the first two channels will be used.",
                src_channels
            );
        }
        let channels = src_channels.min(2);

        // calculation to prevent overflow with extremely large frame counts
        // (e.g. on 32-bit targets where n_frames could be near usize::MAX).
        let duration_secs = codec_params
            .n_frames
            .map(|n| {
                let n_frames = n as f32;
                let rate = sample_rate as f32;
                if rate > 0.0 {
                    n_frames / rate
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let codec = format!("{:?}", codec_params.codec);

        let info = DecodeInfo {
            sample_rate,
            channels,
            duration_secs,
            codec,
            bitrate_kbps: None,
        };

        Ok(Self {
            format_reader,
            decoder,
            track_id,
            info,
            sample_buffer: Vec::with_capacity(4096 * channels),
            conversion_buffer: None,
        })
    }

    /// Decode the next chunk of audio.
    ///
    /// Reuses the internal `sample_buffer` across calls instead of
    /// allocating a new one on every call.
    pub fn decode_next(&mut self, max_frames: usize) -> Result<DecodedChunk, DecodeError> {
        self.sample_buffer.clear();
        let mut frames_decoded = 0;

        while frames_decoded < max_frames {
            let packet = match self.format_reader.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                },
                Err(SymphoniaError::ResetRequired) => continue,
                Err(e) => return Err(DecodeError::Decode(format!("Packet read error: {}", e))),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let decoded_spec = *decoded.spec();
                    let decoded_frames = decoded.frames();

                    if self.conversion_buffer.as_ref().map_or(true, |buf| buf.capacity() < decoded.capacity()) {
                        self.conversion_buffer =
                            Some(symphonia::core::audio::SampleBuffer::<f32>::new(
                                decoded.capacity() as u64,
                                decoded_spec,
                            ));
                    }
                    let conv_buf = if let Some(buf) = self.conversion_buffer.as_mut() {
                        buf
                    } else {
                        return Err(DecodeError::Decode("Conversion buffer failed to initialize".to_string()));
                    };
                    conv_buf.copy_interleaved_ref(decoded);

                    let frames = Self::extract_from_sample_buffer(
                        conv_buf,
                        &mut self.sample_buffer,
                        decoded_spec.channels.count(),
                        self.info.channels,
                        decoded_frames,
                    );
                    frames_decoded += frames;
                },
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Decode(format!("Decode error: {}", e))),
            }
        }

        if self.sample_buffer.is_empty() {
            return Err(DecodeError::EndOfStream);
        }

        Ok(DecodedChunk {
            samples: self.sample_buffer.clone(),
            channels: self.info.channels,
            sample_rate: self.info.sample_rate,
            frame_count: frames_decoded,
        })
    }

    /// Extract f32 samples from a SampleBuffer and handle downmixing/upmixing
    fn extract_from_sample_buffer(
        conv_buf: &symphonia::core::audio::SampleBuffer<f32>,
        output: &mut Vec<f32>,
        src_channels: usize,
        target_channels: usize,
        frames: usize,
    ) -> usize {
        let samples = conv_buf.samples();

        for frame in 0..frames {
            let frame_offset = frame * src_channels;
            for ch in 0..target_channels {
                let sample = if ch < src_channels {
                    samples[frame_offset + ch]
                } else if src_channels > 0 {
                    samples[frame_offset + src_channels - 1]
                } else {
                    0.0
                };
                output.push(sample);
            }
        }
        frames
    }

    /// Seek to a position in seconds
    pub fn seek(&mut self, position_secs: f32) -> Result<(), DecodeError> {
        let seek_to = SeekTo::Time {
            time: Time::from(position_secs),
            track_id: Some(self.track_id),
        };

        self.format_reader
            .seek(SeekMode::Accurate, seek_to)
            .map_err(|e| DecodeError::Seek(format!("Seek failed: {}", e)))?;

        self.decoder.reset();
        Ok(())
    }

    pub fn info(&self) -> &DecodeInfo {
        &self.info
    }

    pub fn duration_secs(&self) -> f32 {
        self.info.duration_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_info() {
        let info = DecodeInfo {
            sample_rate: 44100,
            channels: 2,
            duration_secs: 180.0,
            codec: "mp3".to_string(),
            bitrate_kbps: Some(320),
        };
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
    }
}
