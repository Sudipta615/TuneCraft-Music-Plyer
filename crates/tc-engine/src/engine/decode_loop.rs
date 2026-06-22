//! Core decode-and-process loop for single and crossfade playback modes.
//!
//! v0.29.0: Eliminated real-time heap allocations in the crossfade decode path.
//! Previously, `decode_transitioning_stream` allocated 4 `Vec<(f32, f32)>` per
//! tick (two resampler output buffers + two drain buffers). These are now
//! pre-allocated as fields on `AudioEngine` and cleared/reused each tick,
//! removing ~8–16 heap allocations per tick from the audio hot path.

use log::{error, info, warn};

use std::sync::Arc;

use super::{AudioEngine, PlaybackStream};
#[cfg(feature = "resample")]
use crate::dsp::resampler::AudioResampler;
use crate::{
    buffer::{AudioFrame, PlaybackInfo, PlaybackState},
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
        // Always drain pending output frames before attempting to process new frames.
        while let Some(&(l, r)) = self.pending_output_frames.front() {
            if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                self.pending_output_frames.pop_front();
            } else {
                return;
            }
        }

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

        // Loop unswitching to avoid checking `channels > 1` per sample in the hot path.
        macro_rules! process_frames {
            ($is_stereo:expr) => {
                'outer: for i in start_frame..frames {
                    let idx = i * channels;
                    if idx + channels > chunk.samples.len() {
                        warn!("Inconsistent sample data at frame {}, stopping decode", i);
                        break;
                    }
                    let left = chunk.samples[idx];
                    let right = if $is_stereo {
                        chunk.samples[idx + 1]
                    } else {
                        left
                    };

                    // In Single mode, the mixer is in PlayingCurrent state, so
                    // process() simply passes through (out_l, out_r) unchanged.
                    let (dsp_l, dsp_r) = self.pipeline.process(left, right);

                    #[cfg(feature = "resample")]
                    let bypass = resampler.is_none();
                    #[cfg(not(feature = "resample"))]
                    let bypass = true;

                    if bypass {
                        if self.output_buffer.push(AudioFrame::stereo(dsp_l, dsp_r)) {
                            processed_frames += 1;
                            continue;
                        } else {
                            self.pending_output_frames.push_back((dsp_l, dsp_r));
                            processed_frames += 1;
                            stalled_at = Some(i + 1);
                            break 'outer;
                        }
                    }

                    #[cfg(feature = "resample")]
                    if let Some(ref mut r) = resampler {
                        if r.is_passthrough() {
                            self.pending_output_frames.push_back((dsp_l, dsp_r));
                        } else {
                            r.feed(dsp_l, dsp_r);
                            while let Some((out_l, out_r)) = r.read() {
                                self.pending_output_frames.push_back((out_l, out_r));
                            }
                        }
                    }

                    processed_frames += 1;

                    // Drain newly generated frames to output buffer
                    while let Some(&(l, r)) = self.pending_output_frames.front() {
                        if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                            self.pending_output_frames.pop_front();
                        } else {
                            // We successfully processed frame `i`, so next time start at `i + 1`
                            stalled_at = Some(i + 1);
                            break 'outer;
                        }
                    }
                }
            };
        }

        if channels > 1 {
            process_frames!(true);
        } else {
            process_frames!(false);
        }

        if let Some(stall_frame) = stalled_at {
            if stall_frame < frames {
                self.pending_chunk = Some((chunk, stall_frame));
            }
        } else {
            #[cfg(feature = "resample")]
            if let Some(ref mut r) = resampler {
                while let Some((out_l, out_r)) = r.read() {
                    self.pending_output_frames.push_back((out_l, out_r));
                }
                while let Some(&(l, r)) = self.pending_output_frames.front() {
                    if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                        self.pending_output_frames.pop_front();
                    } else {
                        break;
                    }
                }
            }
        }

        self.source_frames_consumed += processed_frames;
        self.position_secs = self.source_frames_consumed as f32 / self.source_sample_rate as f32;

        let pos = self.position_secs;
        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                position_secs: pos,
                ..old.as_ref().clone()
            })
        });
    }

    /// Decode and process during a crossfade transition, pulling frames
    /// from both the outgoing and incoming decoders simultaneously and
    /// feeding them as distinct sample pairs into the TrackMixer.
    ///
    /// v0.29.0: Uses pre-allocated scratch buffers (`rs_out_buf`, `rs_in_buf`,
    /// `drain_out_buf`, `drain_in_buf`) on `AudioEngine` instead of creating
    /// new `Vec<(f32, f32)>` on every tick. This eliminates ~8–16 heap
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
        // Always drain pending output frames before attempting to process new frames.
        while let Some(&(l, r)) = self.pending_output_frames.front() {
            if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                self.pending_output_frames.pop_front();
            } else {
                return;
            }
        }

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

        // Loop unswitching to avoid checking `out_channels > 1` and `in_channels > 1` per sample.
        macro_rules! process_frames_transition {
            ($out_is_stereo:expr, $in_is_stereo:expr) => {
                for _ in 0..max_frames {
                    if *crossfade_frames_remaining == 0 {
                        // Crossfade complete — will be handled on next tick.
                        break;
                    }

                    // Get outgoing samples (or silence if the outgoing stream ended).
                    let (out_l, out_r) = if out_idx + out_channels <= out_samples.len() {
                        let l = out_samples[out_idx];
                        let r = if $out_is_stereo {
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
                        let r = if $in_is_stereo {
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

                        let (ors_l, ors_r) =
                            self.rs_out_buf.get(rs_idx).copied().unwrap_or((0.0, 0.0));
                        let (irs_l, irs_r) =
                            self.rs_in_buf.get(rs_idx).copied().unwrap_or((0.0, 0.0));

                        // Feed both RESAMPLED streams into the mixer with distinct inputs.
                        let (mixed_l, mixed_r) = self
                            .pipeline
                            .mixer_mut()
                            .process(ors_l, ors_r, irs_l, irs_r);

                        // Apply the remaining DSP stages (limiter, volume, dither) to
                        // the mixed output.
                        let (final_l, final_r) = self.pipeline.process_post_mix(mixed_l, mixed_r);
                        self.pending_output_frames.push_back((final_l, final_r));
                        *crossfade_frames_remaining = crossfade_frames_remaining.saturating_sub(1);
                        processed_frames += 1;

                        while let Some(&(l, r)) = self.pending_output_frames.front() {
                            if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                                self.pending_output_frames.pop_front();
                            } else {
                                // Buffer full — cache partial chunk positions for both
                                // streams so we can resume on the next tick without
                                // dropping audio data. Note: out_idx and in_idx are already
                                // incremented past the current frame, so we won't process it twice.
                                stalled_at = Some((out_idx, in_idx));
                                break;
                            }
                        }

                        if stalled_at.is_some() {
                            break;
                        }
                    }

                    if stalled_at.is_some() {
                        break;
                    }
                }
            };
        }

        match (out_channels > 1, in_channels > 1) {
            (true, true) => process_frames_transition!(true, true),
            (true, false) => process_frames_transition!(true, false),
            (false, true) => process_frames_transition!(false, true),
            (false, false) => process_frames_transition!(false, false),
        }

        // If we stalled during the resampled-frame sub-loop, break outer loop too.
        if stalled_at.is_some() {
            // (Break handled implicitly by macro termination)
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
                self.pending_output_frames.push_back((final_l, final_r));
                *crossfade_frames_remaining = crossfade_frames_remaining.saturating_sub(1);
                processed_frames += 1;

                let mut output_full = false;
                while let Some(&(l, r)) = self.pending_output_frames.front() {
                    if self.output_buffer.push(AudioFrame::stereo(l, r)) {
                        self.pending_output_frames.pop_front();
                    } else {
                        output_full = true;
                        break;
                    }
                }

                if output_full {
                    break;
                }
            }
        }

        // Update position based on processed output frames.
        let time_delta = processed_frames as f32 / self.output_sample_rate as f32;
        let incoming_rate = incoming_decoder.info().sample_rate;

        // During crossfade, track position advances based on the incoming
        // track since that's what the user will hear after the transition.
        // Only update once crossfade is well underway (> 50%).
        //
        // v3.1.2: `time_delta` is *wall-clock* elapsed time (output frames /
        // output rate). At `speed` != 1.0 the decoder consumes track
        // content faster (speed > 1) or slower (speed < 1) than wall-clock
        // time, so the actual *track-content* time elapsed this tick is
        // `time_delta * speed`, not `time_delta`. The previous code
        // advanced `position_secs` by raw `time_delta` (no speed factor)
        // and then separately multiplied by `speed` again when deriving
        // `source_frames_consumed` — leaving `position_secs` itself
        // speed-inconsistent with the frame count, so the on-screen
        // position during a crossfade drifted from the actual track
        // position at any speed other than 1.0x, and jumped once the
        // crossfade completed and the normal (correct) position calc in
        // `decode_single_stream` took back over. Now `position_secs`
        // itself is advanced by content time, and `source_frames_consumed`
        // is derived from it without re-applying `speed` — matching the
        // same `frames = position * rate` relationship used by the
        // non-transitioning path.
        let content_time_delta = time_delta * self.speed;
        if *crossfade_frames_remaining < crossfade_total_frames / 2 {
            self.position_secs += content_time_delta;
            self.source_sample_rate = incoming_rate;
            self.duration_secs = incoming_decoder.duration_secs();
            self.source_frames_consumed =
                (self.position_secs * incoming_rate as f32).round() as u64;
        } else {
            self.position_secs += content_time_delta;
            self.source_frames_consumed =
                (self.position_secs * self.source_sample_rate as f32).round() as u64;
        }

        let pos = self.position_secs;
        let dur = self.duration_secs;
        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                position_secs: pos,
                duration_secs: dur,
                ..old.as_ref().clone()
            })
        });
    }
}
