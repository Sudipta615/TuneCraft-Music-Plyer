//! Command processing and dispatch for the audio engine.

use crossbeam::channel::TryRecvError;
use log::{error, info, warn};

use super::{helpers::percent_decode, AudioEngine, PlaybackStream};
use crate::buffer::{EngineCommand, PlaybackState};

impl AudioEngine {
    pub(super) fn process_commands(&mut self) {
        loop {
            match self.cmd_rx.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    warn!("Command channel disconnected");
                    break;
                },
            }
        }
    }

    fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::Play => {
                if self.stream.is_some() {
                    self.update_playback_state(PlaybackState::Playing);
                    info!("Playback started");
                }
            },
            EngineCommand::Pause => {
                // M7: Only pause if there is an active stream. Pausing with
                // no stream loaded sets state to Paused with no audio, which
                // makes the play button appear to do nothing on next press.
                if self.stream.is_some() {
                    self.update_playback_state(PlaybackState::Paused);
                    info!("Playback paused");
                }
            },
            EngineCommand::Stop => {
                // v0.21.0: Pause the CPAL output stream before resetting the
                // ring buffer to prevent the data race / UB that occurs when
                // the audio callback thread is concurrently reading via pop().
                // This matches the safety pattern already used in load_track().
                if let Some(ref output) = self.audio_output {
                    output.reset_buffer();
                } else {
                    unsafe {
                        self.output_buffer.reset();
                    }
                }
                self.position_secs = 0.0;
                self.pipeline.reset();
                self.stream = None;
                self.crossfade_triggered = false;
                self.next_track_path = None;
                self.cached_incoming_decoder = None;
                self.update_playback_state(PlaybackState::Stopped);
                info!("Playback stopped");
            },
            EngineCommand::Seek(pos_secs) => {
                if !pos_secs.is_finite() || pos_secs < 0.0 {
                    warn!("Seek ignored: invalid position {}", pos_secs);
                    return;
                }
                // Seek only works cleanly in Single mode. If crossfading,
                // cancel the crossfade and seek in the incoming track.
                let seek_in_incoming = self.stream.as_ref().is_some_and(|s| s.is_crossfading());
                if seek_in_incoming {
                    // Promote incoming to single, discard outgoing.
                    if let Some(PlaybackStream::Transitioning {
                        incoming_decoder,
                        incoming_resampler,
                        ..
                    }) = self.stream.take()
                    {
                        self.stream = Some(PlaybackStream::Single {
                            decoder: incoming_decoder,
                            resampler: incoming_resampler,
                        });
                        self.pipeline.mixer_mut().start_playing();
                    }
                }

                if let Some(PlaybackStream::Single {
                    ref mut decoder,
                    ref mut resampler,
                }) = self.stream
                {
                    self.pipeline.begin_seek_fadeout();

                    for _ in 0..128 {
                        let (l, r) = self.pipeline.process(0.0, 0.0);
                        self.pending_output_frames.push_back((l, r));
                    }
                    match decoder.seek(pos_secs) {
                        Ok(()) => {
                            self.position_secs = pos_secs;
                            self.source_frames_consumed =
                                (pos_secs * self.source_sample_rate as f32).round() as u64;
                            #[cfg(feature = "resample")]
                            if let Some(ref mut r) = resampler {
                                r.reset();
                            }
                            #[cfg(not(feature = "resample"))]
                            let _ = resampler;
                            self.pipeline.begin_seek_fadein();
                            // Reset crossfade trigger since position changed.
                            self.crossfade_triggered = false;
                            self.pending_chunk = None;
                            self.pending_incoming_chunk = None;
                            info!("Seeked to {:.1}s", pos_secs);
                        },
                        Err(e) => {
                            self.pipeline.begin_seek_fadein();
                            warn!("Seek failed: {}", e);
                        },
                    }
                }
            },
            EngineCommand::SetVolume(vol) => {
                self.pipeline.set_volume(vol);
                self.write_playback_info(|pb| pb.volume = vol);
            },
            EngineCommand::SetSpeed(speed) => {
                if !speed.is_finite() {
                    warn!("SetSpeed ignored: non-finite value {}", speed);
                    return;
                }
                let clamped = speed.clamp(0.25, 4.0);
                self.speed = clamped;
                // Update resampler(s) in the active stream.
                #[cfg(feature = "resample")]
                match &mut self.stream {
                    Some(PlaybackStream::Single {
                        resampler: Some(ref mut r),
                        ..
                    }) => {
                        r.set_speed(clamped);
                    },
                    Some(PlaybackStream::Single { .. }) => {},
                    Some(PlaybackStream::Transitioning {
                        outgoing_resampler,
                        incoming_resampler,
                        ..
                    }) => {
                        if let Some(ref mut r) = outgoing_resampler {
                            r.set_speed(clamped);
                        }
                        if let Some(ref mut r) = incoming_resampler {
                            r.set_speed(clamped);
                        }
                    },
                    None => {},
                }
                self.write_playback_info(|pb| pb.speed = clamped);
                info!("Playback speed set to {:.2}x", clamped);
            },
            // L2: NextTrack, PrevTrack, and LoadTrack are intentional no-ops in
            // the engine. Track navigation and loading is handled by the PlaybackService
            // and TuneCraftApp layers; the engine only manages audio decoding and
            // DSP. These commands exist in the enum for MPRIS/D-Bus compatibility.
            EngineCommand::NextTrack => {
                log::debug!("NextTrack: handled by PlaybackService, not engine");
            },
            EngineCommand::PrevTrack => {
                log::debug!("PrevTrack: handled by PlaybackService, not engine");
            },
            EngineCommand::LoadTrack(_id) => {
                log::debug!("LoadTrack by ID: use load_track() directly on AudioEngine");
            },
            EngineCommand::Shutdown => {
                self.stop();
            },

            EngineCommand::SetEqEnabled(enabled) => {
                self.pipeline.set_eq_enabled(enabled);
            },
            EngineCommand::SetEqBand {
                index,
                frequency,
                gain_db,
                q,
                enabled,
            } => {
                use crate::dsp::equalizer::{EqBandParams, EqFilterType};
                // Graphic EQ bands defaults (first and last are shelves)
                let num_bands = self.pipeline.eq_num_bands();
                let filter_type = if index == 0 {
                    EqFilterType::LowShelf
                } else if num_bands > 1 && index == num_bands - 1 {
                    EqFilterType::HighShelf
                } else {
                    EqFilterType::Peaking
                };
                self.pipeline.set_eq_band(
                    index,
                    EqBandParams {
                        frequency,
                        gain_db,
                        q,
                        filter_type,
                        enabled,
                    },
                );
            },
            EngineCommand::SetBassShelf(gain_db) => {
                self.pipeline.set_bass_shelf(gain_db);
            },
            EngineCommand::SetTrebleShelf(gain_db) => {
                self.pipeline.set_treble_shelf(gain_db);
            },
            EngineCommand::SetPreamp(db) => {
                self.pipeline.set_preamp_db(db);
            },
            EngineCommand::SetStereoWidth(width) => {
                self.pipeline.set_stereo_width(width);
            },
            EngineCommand::SetBalance(balance) => {
                self.pipeline.set_balance(balance);
            },
            EngineCommand::SetDitherEnabled(enabled) => {
                self.pipeline.set_dither_enabled(enabled);
            },
            EngineCommand::SetMidsideEq(enabled) => {
                self.pipeline.set_midside_eq(enabled);
            },
            EngineCommand::SetCrossfeedEnabled(enabled) => {
                self.pipeline.set_crossfeed_enabled(enabled);
            },
            EngineCommand::SetCrossfeedProfile(profile) => {
                self.pipeline.set_crossfeed_profile(profile);
            },
            EngineCommand::SetCrossfeedCustomParams {
                frequency_hz,
                q,
                delay_ms,
                mix_db,
            } => {
                self.pipeline
                    .set_crossfeed_custom_params(frequency_hz, q, delay_ms, mix_db);
            },
            EngineCommand::SetCompressorEnabled(enabled) => {
                self.pipeline.set_compressor_enabled(enabled);
            },
            EngineCommand::SetCompressorBandParams {
                band,
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                makeup_gain_db,
            } => {
                self.pipeline.set_compressor_band_params(
                    band,
                    threshold_db,
                    ratio,
                    attack_ms,
                    release_ms,
                    makeup_gain_db,
                );
            },

            EngineCommand::SetShuffle(_enabled) => {
                info!("Shuffle state change requested via MPRIS (handled by playback layer)");
            },
            EngineCommand::SetLoopStatus(status) => {
                info!(
                    "Loop status set to '{}' via MPRIS (handled by playback layer)",
                    status
                );
            },
            EngineCommand::OpenUri(uri) => {
                if uri.starts_with("file://") {
                    let path_str = uri.trim_start_matches("file://");
                    if let Some(decoded_path_str) = percent_decode(path_str) {
                        let path = std::path::PathBuf::from(decoded_path_str);

                        if let Ok(metadata) = std::fs::metadata(&path) {
                            if !metadata.is_file() {
                                warn!("OpenUri: path is not a regular file: {}", path.display());
                                return;
                            }
                        } else {
                            warn!("OpenUri: cannot access path: {}", path.display());
                            return;
                        }

                        if let Ok(canonical_path) = path.canonicalize() {
                            let home = dirs::home_dir();
                            let audio = dirs::audio_dir();

                            let allowed = [home, audio]
                                .into_iter()
                                .flatten()
                                .filter_map(|p| p.canonicalize().ok())
                                .any(|base| canonical_path.starts_with(base));

                            if allowed {
                                match self.load_track(&canonical_path) {
                                    Ok(info) => {
                                        info!(
                                            "Loaded URI: {} Hz, {} ch, {:.1}s",
                                            info.sample_rate, info.channels, info.duration_secs
                                        );
                                        self.update_playback_state(PlaybackState::Playing);
                                        self.write_playback_info(|pb| {
                                            pb.track_id = self.current_track_id;
                                        });
                                    },
                                    Err(e) => {
                                        warn!("Failed to load URI '{}': {}", uri, e);
                                    },
                                }
                            } else {
                                warn!(
                                    "OpenUri: access denied for path traversal: {:?}",
                                    canonical_path
                                );
                            }
                        } else {
                            warn!("OpenUri: file not found or invalid: {}", path.display());
                        }
                    } else {
                        warn!("OpenUri: failed to percent-decode URI: {}", path_str);
                    }
                } else {
                    warn!("OpenUri: only file:// URIs are supported, got: {}", uri);
                }
            },
            EngineCommand::PrepareNextTrack(path) => match self.prepare_next_track(&path) {
                Ok(info) => {
                    info!(
                        "Prepared next track for crossfade: {} Hz, {:.1}s",
                        info.sample_rate, info.duration_secs
                    );
                },
                Err(e) => {
                    warn!("Failed to prepare next track: {}", e);
                },
            },
            EngineCommand::RecoverStream => match self.recover_output_stream() {
                Ok(()) => info!("Stream recovered via command"),
                Err(e) => error!("Stream recovery failed: {}", e),
            },
        }
    }
}
