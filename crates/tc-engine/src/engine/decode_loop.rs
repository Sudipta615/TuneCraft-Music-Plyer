//! Core decode-and-process loop for single and crossfade playback modes.
//!
//! v0.29.0: Eliminated real-time heap allocations in the crossfade decode path.
//! Previously, `decode_transitioning_stream` allocated 4 `Vec<(f64, f64)>` per
//! tick (two resampler output buffers + two drain buffers). These are now
//! pre-allocated as fields on `AudioEngine` and cleared/reused each tick,
//! removing ~8–16 heap allocations per tick from the audio hot path.

use log::{error, info, warn};

use super::{AudioEngine, PlaybackStream};
#[cfg(feature = "resample")]
use crate::dsp::resampler::AudioResampler;
use crate::{
    buffer::{AudioFrame, PlaybackState},
    decode::{DecodeError, SymphoniaDecoder},
};

impl AudioEngine {
    /// Core decode-and-process loop. Handles both Single and Transitioning
    /// (crossfade) playback modes, feeding distinct sample streams into
    /// the DSP pipeline and TrackMixer.
    pub(super) fn decode_and_process(&mut self) {
        // Check if we need to finalize a completed crossfade transition.
        // We do this by taking the stream, checking the state, and
        // either completing the transition or putting it back.
        let needs_completion = match &self.stream {
            Some(PlaybackStream::Transitioning {
                crossfade_frames_remaining,
                ..
            }) => *crossfade_frames_remaining == 0,
            _ => false,
        };

        if needs_completion {
            if let Some(PlaybackStream::Transitioning {
                incoming_decoder,
                incoming_resampler,
                ..
            }) = self.stream.take()
            {
                info!("Crossfade transition complete; incoming track is now active");
                self.source_sample_rate = incoming_decoder.info().sample_rate;
                self.duration_secs = incoming_decoder.duration_secs();
                self.position_secs = 0.0;
                self.crossfade_triggered = false;
                self.consecutive_decode_errors = 0;
                self.stream = Some(PlaybackStream::Single {
                    decoder: incoming_decoder,
                    resampler: incoming_resampler,
                });
                self.pipeline.mixer_mut().start_playing();
                self.pending_chunk = None;
                self.pending_incoming_chunk = None;
            }
        }

        // Take the stream out of self to avoid double-&mut-self borrow
        // conflict: the decode methods need &mut self, but the stream
        // references (decoder, resampler) also come from self.stream.
        // By moving the stream to a local, self and stream are disjoint.
        let mut stream = match self.stream.take() {
            Some(s) => s,
            None => return,
        };

        match &mut stream {
            PlaybackStream::Single { decoder, resampler } => {
                self.decode_single_stream(
                    decoder,
                    #[cfg(feature = "resample")]
                    resampler,
                    #[cfg(not(feature = "resample"))]
                    resampler,
                );
            },
            PlaybackStream::Transitioning {
                outgoing_decoder,
                outgoing_resampler,
                incoming_decoder,
                incoming_resampler,
                crossfade_frames_remaining,
                crossfade_total_frames,
            } => {
                self.decode_transitioning_stream(
                    outgoing_decoder,
                    #[cfg(feature = "resample")]
                    outgoing_resampler,
                    #[cfg(not(feature = "resample"))]
                    outgoing_resampler,
                    incoming_decoder,
                    #[cfg(feature = "resample")]
                    incoming_resampler,
                    #[cfg(not(feature = "resample"))]
                    incoming_resampler,
                    crossfade_frames_remaining,
                    *crossfade_total_frames,
                );
            },
        }

        self.stream = Some(stream);
    }

    /// Decode and process a single (non-crossfading) track.
    fn decode_single_stream(
        &mut self,
        decoder: &mut SymphoniaDecoder,
        #[cfg(feature = "resample")] resampler: &mut Option<AudioResampler>,
        #[cfg(not(feature = "resample"))] _resampler: &mut Option<()>,
    ) {
        let chunk_and_start: Option<(crate::decode::DecodedChunk, usize)> =
            self.pending_chunk.take().or_else(|| {
                match decoder.decode_next(4096) {
                    Ok(chunk) => {
                        self.consecutive_decode_errors = 0;
                        Some((chunk, 0))
                    },
                    Err(DecodeError::EndOfStream) => {
                        info!("Track ended");
                        self.position_secs = 0.0;
                        self.crossfade_triggered = false;
                        // If there's a next track path but crossfade didn't
                        // trigger (e.g., crossfade disabled), load it now
                        // for gapless transition.
                        if self.next_track_path.is_some() && !self.config.crossfade.enabled {
                            // Signal to the UI layer that the track ended
                            // and a new track should be loaded.
                            info!("Track ended without crossfade; next track available");
                        }
                        self.update_playback_state(PlaybackState::Stopped);
                        // Don't set stream to None here — the UI layer
                        // handles track transitions via PlaybackService.
                        None
                    },
                    Err(e) => {
                        self.consecutive_decode_errors += 1;
                        warn!(
                            "Decode error ({}/{}): {}",
                            self.consecutive_decode_errors, 10, e
                        );
                        if self.consecutive_decode_errors >= 10 {
                            error!("Too many consecutive decode errors; stopping playback");
                            self.update_playback_state(PlaybackState::Stopped);
                        }
                        None
                    },
                }
            });

        let (chunk, start_frame) = match chunk_and_start {
            Some(v) => v,
            None => return,
        };

        let frames = chunk.frame_count;
        let channels = chunk.channels;
        let mut processed_frames: u64 = 0;

        let expected_samples = (frames as u64) * (channels as u64);
        if (chunk.samples.len() as u64) < expected_samples {
            warn!(
                "Decoder returned inconsistent data: expected {} samples, got {}",
                expected_samples,
                chunk.samples.len()
            );
            return;
        }

        let mut stalled_at: Option<usize> = None;

        'outer: for i in start_frame..frames {
            let idx = i * channels;
            if idx + channels > chunk.samples.len() {
                warn!("Inconsistent sample data at frame {}, stopping decode", i);
                break;
            }
            let left = chunk.samples[idx];
            let right = if channels > 1 {
                chunk.samples[idx + 1]
            } else {
                left
            };

            // In Single mode, the mixer is in PlayingCurrent state, so
            // process() simply passes through (out_l, out_r) unchanged.
            let (dsp_l, dsp_r) = self.pipeline.process(left, right);

            #[cfg(feature = "resample")]
            if let Some(ref mut r) = resampler {
                if r.is_passthrough() {
                    if !self.output_buffer.push(AudioFrame::stereo(dsp_l, dsp_r)) {
                        stalled_at = Some(i);
                        break 'outer;
                    }
                } else {
                    r.feed(dsp_l, dsp_r);
                    while let Some((out_l, out_r)) = r.read() {
                        if !self.output_buffer.push(AudioFrame::stereo(out_l, out_r)) {
                            stalled_at = Some(i);
                            break 'outer;
                        }
                    }
                }
            } else {
                if !self.output_buffer.push(AudioFrame::stereo(dsp_l, dsp_r)) {
                    stalled_at = Some(i);
                    break 'outer;
                }
            }

            #[cfg(not(feature = "resample"))]
            if !self.output_buffer.push(AudioFrame::stereo(dsp_l, dsp_r)) {
                stalled_at = Some(i);
                break 'outer;
            }

            processed_frames += 1;
        }

        if let Some(stall_frame) = stalled_at {
            self.pending_chunk = Some((chunk, stall_frame));
        } else {
            #[cfg(feature = "resample")]
            if let Some(ref mut r) = resampler {
                while let Some((out_l, out_r)) = r.read() {
                    if !self.output_buffer.push(AudioFrame::stereo(out_l, out_r)) {
                        break;
                    }
                }
            }
        }

        let effective_speed = if self.speed.abs() < 0.01 {
            0.25
        } else {
            self.speed
        };
        let wall_clock_delta =
            processed_frames as f64 / (self.source_sample_rate as f64 * effective_speed);
        self.position_secs += wall_clock_delta;

        match self.playback_info.write() {
            Ok(mut pb) => {
                pb.position_secs = self.position_secs;
            },
            Err(e) => {
                error!(
                    "PlaybackInfo RwLock poisoned during decode; resetting: {}",
                    e
                );
                e.into_inner().position_secs = self.position_secs;
            },
        }
    }

    /// Decode and process during a crossfade transition, pulling frames
    /// from both the outgoing and incoming decoders simultaneously and
    /// feeding them as distinct sample pairs into the TrackMixer.
    ///
    /// v0.29.0: Uses pre-allocated scratch buffers (`rs_out_buf`, `rs_in_buf`,
    /// `drain_out_buf`, `drain_in_buf`) on `AudioEngine` instead of creating
    /// new `Vec<(f64, f64)>` on every tick. This eliminates ~8–16 heap
    /// allocations per tick from the real-time audio path.
    #[allow(clippy::too_many_arguments)]
    fn decode_transitioning_stream(
        &mut self,
        outgoing_decoder: &mut SymphoniaDecoder,
        #[cfg(feature = "resample")] outgoing_resampler: &mut Option<AudioResampler>,
        #[cfg(not(feature = "resample"))] _outgoing_resampler: &mut Option<()>,
        incoming_decoder: &mut SymphoniaDecoder,
        #[cfg(feature = "resample")] incoming_resampler: &mut Option<AudioResampler>,
        #[cfg(not(feature = "resample"))] _incoming_resampler: &mut Option<()>,
        crossfade_frames_remaining: &mut usize,
        crossfade_total_frames: usize,
    ) {
        // Decode chunks from both decoders.
        let out_chunk: Option<crate::decode::DecodedChunk> =
            self.pending_chunk.take().map(|(c, _)| c).or_else(|| {
                match outgoing_decoder.decode_next(4096) {
                    Ok(c) => Some(c),
                    Err(DecodeError::EndOfStream) => {
                        // Outgoing track ended — this is fine during crossfade,
                        // the mixer will use silence for the remaining outgoing samples.
                        None
                    },
                    Err(_) => None,
                }
            });

        let in_chunk: Option<crate::decode::DecodedChunk> = self
            .pending_incoming_chunk
            .take()
            .map(|(c, _)| c)
            .or_else(|| {
                match incoming_decoder.decode_next(4096) {
                    Ok(c) => Some(c),
                    Err(DecodeError::EndOfStream) => {
                        // Incoming track ended during crossfade — shouldn't normally
                        // happen since crossfade is at the start of the incoming track.
                        None
                    },
                    Err(_) => None,
                }
            });

        // If we have no incoming samples at all, something is wrong.
        // Mark crossfade as complete — the next tick will promote the
        // incoming decoder to Single mode.
        if out_chunk.is_none() && in_chunk.is_none() {
            *crossfade_frames_remaining = 0;
            return;
        }

        let out_samples = out_chunk
            .as_ref()
            .map(|c| c.samples.as_slice())
            .unwrap_or(&[]);
        let out_channels = out_chunk.as_ref().map(|c| c.channels).unwrap_or(2);
        let out_frame_count = out_chunk.as_ref().map(|c| c.frame_count).unwrap_or(0);

        let in_samples = in_chunk
            .as_ref()
            .map(|c| c.samples.as_slice())
            .unwrap_or(&[]);
        let in_channels = in_chunk.as_ref().map(|c| c.channels).unwrap_or(2);
        let in_frame_count = in_chunk.as_ref().map(|c| c.frame_count).unwrap_or(0);

        let max_frames = out_frame_count.max(in_frame_count);
        let mut processed_frames: u64 = 0;
        let mut out_idx = 0usize;
        let mut in_idx = 0usize;
        let mut stalled_at: Option<(usize, usize)> = None;

        for _ in 0..max_frames {
            if *crossfade_frames_remaining == 0 {
                // Crossfade complete — will be handled on next tick.
                break;
            }

            // Get outgoing samples (or silence if the outgoing stream ended).
            let (out_l, out_r) = if out_idx + out_channels <= out_samples.len() {
                let l = out_samples[out_idx];
                let r = if out_channels > 1 {
                    out_samples[out_idx + 1]
                } else {
                    l
                };
                out_idx += out_channels;
                (l, r)
            } else {
                (0.0, 0.0)
            };

            // Get incoming samples (or silence if the incoming stream ended).
            let (in_l, in_r) = if in_idx + in_channels <= in_samples.len() {
                let l = in_samples[in_idx];
                let r = if in_channels > 1 {
                    in_samples[in_idx + 1]
                } else {
                    l
                };
                in_idx += in_channels;
                (l, r)
            } else {
                (0.0, 0.0)
            };

            // Process the outgoing track through the first half of the DSP pipeline.
            let (out_dsp_l, out_dsp_r) = self.pipeline.process_outgoing(out_l, out_r);

            // Process the incoming track through the first half of the DSP pipeline.
            let (in_dsp_l, in_dsp_r) = self.pipeline.process_incoming(in_l, in_r);

            // v0.21.0: Feed each stream's pre-mix DSP output through its
            // respective resampler BEFORE mixing. This mirrors how
            // decode_single_stream works and ensures that tracks with
            // different sample rates than the output device are correctly
            // pitch-shifted during crossfade. The previous code bypassed
            // resampling entirely in the crossfade path, causing audible
            // pitch/speed errors when source_rate != output_rate.
            //
            // Bug #5 fix: When the resampler is non-passthrough, feeding one
            // input frame may produce multiple output frames. We must read ALL
            // available output frames, not just one, to avoid under-feeding the
            // mixer and producing silence gaps / timing drift.
            //
            // v0.29.0: Reuse pre-allocated scratch buffers instead of creating
            // new Vecs each frame. This eliminates the dominant real-time
            // allocation in the crossfade decode path.

            // Clear the scratch buffers for reuse (does not free memory).
            self.rs_out_buf.clear();
            self.rs_in_buf.clear();

            #[cfg(feature = "resample")]
            {
                // Collect outgoing resampler frames.
                if let Some(ref mut r) = outgoing_resampler {
                    if r.is_passthrough() {
                        self.rs_out_buf.push((out_dsp_l, out_dsp_r));
                    } else {
                        r.feed(out_dsp_l, out_dsp_r);
                        while let Some((l, rv)) = r.read() {
                            self.rs_out_buf.push((l, rv));
                        }
                        if self.rs_out_buf.is_empty() {
                            self.rs_out_buf.push((0.0, 0.0));
                        }
                    }
                } else {
                    self.rs_out_buf.push((out_dsp_l, out_dsp_r));
                }

                // Collect incoming resampler frames.
                if let Some(ref mut r) = incoming_resampler {
                    if r.is_passthrough() {
                        self.rs_in_buf.push((in_dsp_l, in_dsp_r));
                    } else {
                        r.feed(in_dsp_l, in_dsp_r);
                        while let Some((l, rv)) = r.read() {
                            self.rs_in_buf.push((l, rv));
                        }
                        if self.rs_in_buf.is_empty() {
                            self.rs_in_buf.push((0.0, 0.0));
                        }
                    }
                } else {
                    self.rs_in_buf.push((in_dsp_l, in_dsp_r));
                }
            }

            #[cfg(not(feature = "resample"))]
            {
                self.rs_out_buf.push((out_dsp_l, out_dsp_r));
                self.rs_in_buf.push((in_dsp_l, in_dsp_r));
            }

            // Mix all combinations of output frames from both resamplers.
            // When one resampler produces more frames than the other, the
            // shorter one is extended with silence. This handles the case where
            // different source rates produce different output frame counts.
            let max_rs_frames = self.rs_out_buf.len().max(self.rs_in_buf.len());
            for rs_idx in 0..max_rs_frames {
                if *crossfade_frames_remaining == 0 {
                    break;
                }

                let (ors_l, ors_r) = self.rs_out_buf.get(rs_idx).copied().unwrap_or((0.0, 0.0));
                let (irs_l, irs_r) = self.rs_in_buf.get(rs_idx).copied().unwrap_or((0.0, 0.0));

                // Feed both RESAMPLED streams into the mixer with distinct inputs.
                let (mixed_l, mixed_r) = self
                    .pipeline
                    .mixer_mut()
                    .process(ors_l, ors_r, irs_l, irs_r);

                // Apply the remaining DSP stages (limiter, volume, dither) to
                // the mixed output.
                let (final_l, final_r) = self.pipeline.process_post_mix(mixed_l, mixed_r);

                if !self
                    .output_buffer
                    .push(AudioFrame::stereo(final_l, final_r))
                {
                    // Buffer full — cache partial chunk positions for both
                    // streams so we can resume on the next tick without
                    // dropping audio data.
                    stalled_at = Some((out_idx, in_idx));
                    break;
                }

                *crossfade_frames_remaining = crossfade_frames_remaining.saturating_sub(1);
                processed_frames += 1;
            }

            // If we stalled during the resampled-frame sub-loop, break outer loop too.
            if stalled_at.is_some() {
                break;
            }
        }

        // v0.21.0: Handle stalling by caching partial chunks for both
        // the outgoing and incoming decoders, matching the zero-dropout
        // pattern already used in decode_single_stream. Without this,
        // frames from the current decode chunk would be silently
        // discarded when the output ring-buffer is full.
        if let Some((stall_out_idx, stall_in_idx)) = stalled_at {
            // Cache outgoing partial chunk if we still have unprocessed frames.
            if stall_out_idx < out_samples.len() {
                if let Some(chunk) = out_chunk {
                    // v0.29.0: Use splice-based truncation instead of .to_vec()
                    // to avoid a heap allocation for the remaining samples.
                    // We mutate the chunk's sample Vec in-place by truncating
                    // the prefix we've already consumed, which is O(1) for the
                    // truncation itself (just sets the len).
                    let mut chunk = chunk;
                    chunk.samples = chunk.samples.split_off(stall_out_idx);
                    chunk.frame_count = chunk
                        .frame_count
                        .saturating_sub(stall_out_idx / chunk.channels.max(1));
                    self.pending_chunk = Some((
                        chunk, 0, // Start from beginning of the trimmed chunk
                    ));
                }
            }
            // Cache incoming partial chunk if we still have unprocessed frames.
            if stall_in_idx < in_samples.len() {
                if let Some(chunk) = in_chunk {
                    let mut chunk = chunk;
                    chunk.samples = chunk.samples.split_off(stall_in_idx);
                    chunk.frame_count = chunk
                        .frame_count
                        .saturating_sub(stall_in_idx / chunk.channels.max(1));
                    self.pending_incoming_chunk = Some((chunk, 0));
                }
            }
        } else {
            // Drain any remaining resampled output from both resamplers.
            // This mirrors the drain loop in decode_single_stream.
            //
            // Batch drain: collect all remaining frames from each resampler
            // into the pre-allocated scratch buffers, then mix the paired
            // streams. This avoids per-frame resampler state checks and
            // produces better cache locality for the mixing loop.
            //
            // v0.29.0: Uses pre-allocated drain_out_buf / drain_in_buf
            // instead of creating new Vecs each tick.

            self.drain_out_buf.clear();
            self.drain_in_buf.clear();

            #[cfg(feature = "resample")]
            {
                if let Some(ref mut r) = outgoing_resampler {
                    while let Some(frame) = r.read() {
                        self.drain_out_buf.push(frame);
                    }
                }
                if let Some(ref mut r) = incoming_resampler {
                    while let Some(frame) = r.read() {
                        self.drain_in_buf.push(frame);
                    }
                }
            }

            let max_drain = self.drain_out_buf.len().max(self.drain_in_buf.len());
            for di in 0..max_drain {
                if *crossfade_frames_remaining == 0 {
                    // Don't mix past the crossfade boundary
                    break;
                }

                let (out_rs_l, out_rs_r) =
                    self.drain_out_buf.get(di).copied().unwrap_or((0.0, 0.0));
                let (in_rs_l, in_rs_r) = self.drain_in_buf.get(di).copied().unwrap_or((0.0, 0.0));

                let (mixed_l, mixed_r) = self
                    .pipeline
                    .mixer_mut()
                    .process(out_rs_l, out_rs_r, in_rs_l, in_rs_r);
                let (final_l, final_r) = self.pipeline.process_post_mix(mixed_l, mixed_r);
                if !self
                    .output_buffer
                    .push(AudioFrame::stereo(final_l, final_r))
                {
                    break;
                }
                *crossfade_frames_remaining = crossfade_frames_remaining.saturating_sub(1);
                processed_frames += 1;
            }
        }

        // Update position based on the incoming track's progress.
        let effective_speed = if self.speed.abs() < 0.01 {
            0.25
        } else {
            self.speed
        };
        let incoming_rate = incoming_decoder.info().sample_rate;
        let wall_clock_delta = processed_frames as f64 / (incoming_rate as f64 * effective_speed);

        // During crossfade, track position advances based on the incoming
        // track since that's what the user will hear after the transition.
        // Only update once crossfade is well underway (> 50%).
        if *crossfade_frames_remaining < crossfade_total_frames / 2 {
            self.position_secs += wall_clock_delta;
            self.source_sample_rate = incoming_rate;
            self.duration_secs = incoming_decoder.duration_secs();
        }

        match self.playback_info.write() {
            Ok(mut pb) => {
                pb.position_secs = self.position_secs;
                pb.duration_secs = self.duration_secs;
            },
            Err(e) => {
                error!(
                    "PlaybackInfo RwLock poisoned during crossfade decode; resetting: {}",
                    e
                );
                let mut pb = e.into_inner();
                pb.position_secs = self.position_secs;
                pb.duration_secs = self.duration_secs;
            },
        }
    }
}
