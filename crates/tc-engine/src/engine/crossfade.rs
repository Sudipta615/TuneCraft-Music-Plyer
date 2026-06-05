//! Crossfade trigger detection and transition management.

use log::{info, warn};

#[cfg(feature = "resample")]
use super::recovery;
use super::{AudioEngine, EngineError, PlaybackStream};
use crate::decode::{DecodeInfo, SymphoniaDecoder};
use std::path::Path;

impl AudioEngine {
    /// Prepare the next track for crossfading by pre-opening its decoder.
    /// The incoming decoder is created ahead of time so that when the
    /// current track reaches its final N seconds, the crossfade can begin
    /// immediately without any I/O delay.
    ///
    /// v0.21.0: The opened decoder is now cached in `cached_incoming_decoder`
    /// instead of being dropped and re-opened later. This eliminates a
    /// redundant file open + probe + decoder creation when the crossfade
    /// trigger fires.
    pub fn prepare_next_track(
        &mut self,
        path: &Path,
    ) -> Result<DecodeInfo, EngineError> {
        // If crossfade is disabled or there is no current stream, just
        // remember the path for a regular track transition later.
        if !self.config.crossfade.enabled {
            self.next_track_path = Some(path.clone());
            self.cached_incoming_decoder = None;
            // Open decoder just to return info, but we won't cache it for
            // crossfade — load_track will be called later instead.
            let decoder = SymphoniaDecoder::open(path)?;
            return Ok(decoder.info().clone());
        }
        self.next_track_path = Some(path.clone());
        // Open and cache the decoder so begin_crossfade_transition can
        // use it immediately without re-opening the file.
        let decoder = SymphoniaDecoder::open(path)?;
        let info = decoder.info().clone();
        self.cached_incoming_decoder = Some(decoder);
        info!("Next track prepared for crossfade: {}", path.display());
        Ok(info)
    }

    /// Check if the active track has entered its final N seconds and
    /// trigger the crossfade transition if so. The threshold is computed
    /// from the crossfade duration in the config, converted to sample
    /// counts for sample-accurate timing (not wall-clock time).
    pub(super) fn check_crossfade_trigger(&mut self) {
        if self.crossfade_triggered || !self.config.crossfade.enabled {
            return;
        }
        if self.next_track_path.is_none() {
            return;
        }

        // Determine the remaining time in the current track.
        // M1: Clamp to 0.0 to prevent negative values when position slightly
        // overshoots duration due to floating-point drift or seek rounding.
        let remaining_secs = (self.duration_secs - self.position_secs).max(0.0);
        let crossfade_duration_secs = self.config.crossfade.duration_ms as f64 / 1000.0;

        // Add a small lead time (0.5s) so the incoming decoder has time
        // to start producing samples before the crossfade begins.
        let trigger_threshold = crossfade_duration_secs + 0.5;

        if remaining_secs <= trigger_threshold && remaining_secs > 0.0 {
            self.crossfade_triggered = true;
            self.begin_crossfade_transition();
        }
    }

    /// Transition from Single to Transitioning state by spawning the
    /// incoming decoder and initializing the crossfade parameters.
    fn begin_crossfade_transition(&mut self) {
        let next_path = match self.next_track_path.take() {
            Some(p) => p,
            None => return,
        };

        // v0.21.0: Use the cached decoder from prepare_next_track() if
        // available, avoiding a redundant file open. Fall back to opening
        // the file if the cache was cleared or never populated.
        let incoming_decoder = match self.cached_incoming_decoder.take() {
            Some(d) => {
                info!("Using cached incoming decoder for crossfade");
                d
            },
            None => match SymphoniaDecoder::open(&next_path) {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to open incoming track for crossfade: {}", e);
                    self.crossfade_triggered = false;
                    return;
                },
            },
        };

        let incoming_info = incoming_decoder.info().clone();
        let incoming_sample_rate = incoming_info.sample_rate;

        // Create resampler for the incoming track.
        #[cfg(feature = "resample")]
        let incoming_resampler = recovery::build_resampler(
            self.config.resampler_quality,
            incoming_sample_rate as f64,
            self.output_sample_rate as f64,
            self.speed,
        );

        #[cfg(not(feature = "resample"))]
        let incoming_resampler: Option<()> = None;

        // Calculate crossfade frame count based on output sample rate.
        let crossfade_total_frames = (self.config.crossfade.duration_ms as f64
            * 0.001
            * self.output_sample_rate as f64) as usize;

        // Extract the current decoder and resampler from the stream.
        let current_stream = self.stream.take();
        match current_stream {
            Some(PlaybackStream::Single { decoder, resampler }) => {
                info!(
                    "Crossfade transition starting: {} frames ({:.1}s), incoming: {} Hz",
                    crossfade_total_frames,
                    self.config.crossfade.duration_ms as f64 / 1000.0,
                    incoming_sample_rate
                );

                // Tell the pipeline mixer to start crossfading.
                self.pipeline.mixer_mut().start_crossfade();

                self.stream = Some(PlaybackStream::Transitioning {
                    outgoing_decoder: decoder,
                    outgoing_resampler: resampler,
                    incoming_decoder,
                    incoming_resampler,
                    crossfade_frames_remaining: crossfade_total_frames,
                    crossfade_total_frames,
                });

                // Clear any pending chunks from the old single-stream state.
                // Bug #9 fix: Also clear pending_chunk (the outgoing stall cache).
                // If a crossfade fires while the ring buffer was stalled with the
                // old track's data, the stale chunk index offsets may not match
                // the now-Transitioning stream layout, potentially producing an
                // out-of-bounds read from the samples slice.
                self.pending_chunk = None;
                self.pending_incoming_chunk = None;
            },
            Some(PlaybackStream::Transitioning { .. }) => {
                // Already transitioning — shouldn't happen since crossfade_triggered
                // prevents re-entry, but handle gracefully.
                warn!("Crossfade triggered while already transitioning; ignoring");
            },
            None => {
                warn!("Crossfade triggered but no active stream");
            },
        }
    }
}
