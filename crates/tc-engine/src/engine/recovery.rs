//! Audio output stream recovery and health monitoring.
//!
//! v0.29.0: Extracted `build_resampler` helper to eliminate duplicated
//! resampler-creation code across `recovery.rs`, `crossfade.rs`, and
//! `mod.rs` (load_track). All three sites now share the same logic for
//! creating a resampler with the correct quality, speed, and error handling.

use std::sync::Arc;
use std::time::Duration;

use log::{error, info, warn};

use crate::buffer::{FixedFrameBuffer, DEFAULT_SAMPLE_RATE, OUTPUT_BUFFER_FRAMES};
#[cfg(feature = "resample")]
use crate::dsp::resampler::AudioResampler;
use crate::output::CpalOutput;

#[cfg(feature = "resample")]
use super::PlaybackStream;
use super::{AudioEngine, EngineError};

impl AudioEngine {
    /// Attempt to recover the audio output stream after a device change
    /// or error. This pauses decoding, re-detects the output device,
    /// rebuilds the stream at the new sample rate, and hot-swaps the
    /// output without requiring an application restart.
    pub fn recover_output_stream(&mut self) -> Result<(), EngineError> {
        const MAX_RECOVERY_ATTEMPTS: u32 = 5;
        if self.stream_recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
            return Err(EngineError::StreamRecovery(format!(
                "Exceeded maximum stream recovery attempts ({})",
                MAX_RECOVERY_ATTEMPTS
            )));
        }

        self.stream_recovery_attempts += 1;
        info!(
            "Attempting stream recovery (attempt {}/{})",
            self.stream_recovery_attempts, MAX_RECOVERY_ATTEMPTS
        );

        // Stop the current output.
        if let Some(mut output) = self.audio_output.take() {
            output.stop();
        }

        // Brief pause to allow the OS to settle after device change.
        std::thread::sleep(Duration::from_millis(100));

        // Re-detect the output device and sample rate.
        let new_output_sample_rate =
            Self::detect_output_sample_rate().unwrap_or(DEFAULT_SAMPLE_RATE);

        let old_rate = self.output_sample_rate;
        let sample_rate_changed = new_output_sample_rate != old_rate;

        // Create a new output buffer and CpalOutput.
        let new_buffer = Arc::new(
            FixedFrameBuffer::new(OUTPUT_BUFFER_FRAMES)
                .map_err(|e| EngineError::Config(format!("Output buffer: {}", e)))?,
        );

        let mut new_output = CpalOutput::new(Arc::clone(&new_buffer))?;
        let actual_rate = new_output.sample_rate();
        new_output.start()?;

        // Swap the output buffer. We need to pause the output first to
        // avoid data races on the buffer indices.
        self.audio_output = Some(new_output);
        self.output_buffer = new_buffer;
        self.output_sample_rate = actual_rate;

        // If the sample rate changed, rebuild the pipeline and resampler.
        if sample_rate_changed {
            info!(
                "Sample rate changed during recovery: {} Hz -> {} Hz",
                old_rate, actual_rate
            );
            self.pipeline.set_sample_rate(actual_rate as f64);

            // Rebuild resampler(s) if we have an active stream.
            // v0.21.0: When in Transitioning state, we now rebuild BOTH
            // the outgoing and incoming resamplers, not just the incoming
            // one. If the sample rate changed during recovery and the
            // outgoing resampler is left at the old rate, the remainder
            // of the crossfade will produce audio at the wrong pitch.
            #[cfg(feature = "resample")]
            if let Some(ref mut stream) = self.stream {
                match stream {
                    PlaybackStream::Single { decoder, resampler } => {
                        let source_rate = decoder.info().sample_rate;
                        *resampler = build_resampler(
                            self.config.resampler_quality,
                            source_rate as f64,
                            actual_rate as f64,
                            self.speed,
                        );
                    },
                    PlaybackStream::Transitioning {
                        outgoing_decoder,
                        outgoing_resampler,
                        incoming_decoder,
                        incoming_resampler,
                        ..
                    } => {
                        // Rebuild outgoing resampler
                        let out_rate = outgoing_decoder.info().sample_rate;
                        *outgoing_resampler = build_resampler(
                            self.config.resampler_quality,
                            out_rate as f64,
                            actual_rate as f64,
                            self.speed,
                        );
                        // Rebuild incoming resampler
                        let in_rate = incoming_decoder.info().sample_rate;
                        *incoming_resampler = build_resampler(
                            self.config.resampler_quality,
                            in_rate as f64,
                            actual_rate as f64,
                            self.speed,
                        );
                    },
                }
            }
        }

        self.successful_playback_ticks = 0; // Reset the stability timer on recovery
        info!(
            "Stream recovery successful (output rate: {} Hz)",
            actual_rate
        );
        Ok(())
    }

    /// Check if the audio output has encountered an error that requires
    /// stream recovery (e.g., device disconnection). Also checks for
    /// device changes by comparing the current device against the default.
    pub(super) fn check_stream_health(&mut self) {
        if let Some(ref output) = self.audio_output {
            // Check for stream errors reported by CPAL's error callback.
            if output.take_stream_error() {
                warn!("Audio stream error detected — attempting recovery");
                match self.recover_output_stream() {
                    Ok(()) => info!("Stream recovered after error detection"),
                    Err(e) => error!("Stream recovery failed: {}", e),
                }
                return;
            }

            // High underrun count can indicate stream issues.
            let underruns = output.take_underruns();
            if underruns > 1000 {
                warn!(
                    "High underrun count ({}) detected; may indicate device issue",
                    underruns
                );
            }
        }
    }
}

/// Shared helper for creating a resampler with the engine's current config
/// and speed settings. Eliminates duplicated match/Ok/Err blocks across
/// `load_track`, `begin_crossfade_transition`, and `recover_output_stream`.

/// Returns `None` if the resampler feature is disabled or if creation fails
/// (a warning is logged on failure).
#[cfg(feature = "resample")]
pub(super) fn build_resampler(
    quality: tc_config::ResamplerQuality,
    source_rate: f64,
    output_rate: f64,
    speed: f64,
) -> Option<AudioResampler> {
    match AudioResampler::new(quality, source_rate, output_rate) {
        Ok(mut r) => {
            if (speed - 1.0).abs() > 0.001 {
                r.set_speed(speed);
            }
            Some(r)
        },
        Err(e) => {
            warn!("Failed to create resampler: {}", e);
            None
        },
    }
}
