//! Core audio engine — wires decode → DSP → output pipeline
//!
//! v0.24.0: Added convolution IR reload warning flag propagated to UI via
//! PlaybackInfo (convolution_ir_needs_reload). Batched crossfade drain loop
//! for better efficiency (collects all resampler output before mixing instead
//! of one frame per iteration).
//!
//! v0.23.0: Fixed use-after-move of PoisonError in crossfade decode
//! (double into_inner() call — Bug #1). Re-applied volume after pipeline
//! reset in load_track() (Bug #3). Cleared pending_chunk on crossfade
//! transition start to prevent stale chunk offsets (Bug #9). Fixed
//! crossfade resampler silence gaps by reading all available output frames
//! per feed (Bug #5).
//!
//! v0.22.0: Fixed use-after-move of PoisonError in CPU usage RwLock update
//! (double into_inner() call). Fixed incorrect stall-cache guard condition in
//! decode_transitioning_stream (was checking stall_out_idx > 0 || out_frame_count > 0,
//! now correctly checks stall_out_idx < out_samples.len()).
//!
//! v0.21.0: Fixed critical crossfade resampling bug — both outgoing and
//! incoming resamplers are now applied in the crossfade decode path.
//! Fixed data race in Stop command. Fixed stream recovery to rebuild
//! both resamplers in Transitioning state. Added pending-chunk stalling
//! in crossfade decode. Cached incoming decoder in prepare_next_track.

mod commands;
mod crossfade;
mod decode_loop;
mod helpers;
mod recovery;
mod stream;
#[cfg(test)]
mod tests;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crossbeam::channel::{self, Receiver, Sender};
use log::{error, info, warn};
// Re-export public types from submodules so the public API is unchanged.
pub use stream::EngineError;
use tc_config::EngineConfig;

#[cfg(feature = "resample")]
use crate::dsp::resampler::AudioResampler;
use crate::{
    buffer::{
        EngineCommand, FixedFrameBuffer, PlaybackInfo, PlaybackState, DEFAULT_SAMPLE_RATE,
        OUTPUT_BUFFER_FRAMES,
    },
    decode::{DecodeInfo, SymphoniaDecoder},
    dsp::pipeline::DspPipeline,
    output::CpalOutput,
};

/// Dual-decoder state machine for true gapless playback and crossfading.
///
/// `Single` represents normal single-track playback. `Transitioning` holds
/// both the outgoing (fading) and incoming (rising) decoders simultaneously,
/// allowing the `TrackMixer` to receive genuinely distinct sample streams
/// and perform real overlapping gain scaling.
///
/// Defined in `mod.rs` so that private fields are accessible from all engine
/// submodules (Rust privacy rules: submodules can access parent-module items).
#[allow(clippy::large_enum_variant)]
pub enum PlaybackStream {
    /// Playing a single track with no crossfade in progress.
    Single {
        decoder: SymphoniaDecoder,
        #[cfg(feature = "resample")]
        resampler: Option<AudioResampler>,
        #[cfg(not(feature = "resample"))]
        resampler: Option<()>,
    },
    /// Crossfading between two tracks. The outgoing decoder provides the
    /// tail of the current track while the incoming decoder provides the
    /// head of the next. The mixer's process() method receives distinct
    /// (out_l, out_r) and (in_l, in_r) sample pairs.
    Transitioning {
        outgoing_decoder: SymphoniaDecoder,
        #[cfg(feature = "resample")]
        outgoing_resampler: Option<AudioResampler>,
        #[cfg(not(feature = "resample"))]
        outgoing_resampler: Option<()>,
        incoming_decoder: SymphoniaDecoder,
        #[cfg(feature = "resample")]
        incoming_resampler: Option<AudioResampler>,
        #[cfg(not(feature = "resample"))]
        incoming_resampler: Option<()>,
        /// Frames remaining in the crossfade transition.
        crossfade_frames_remaining: usize,
        /// Total crossfade duration in frames.
        crossfade_total_frames: usize,
    },
}

pub struct AudioEngine {
    output_buffer: Arc<FixedFrameBuffer>,
    cmd_tx: Sender<EngineCommand>,
    cmd_rx: Receiver<EngineCommand>,
    /// Playback info stored in an RwLock.  All accesses use explicit
    /// error handling instead of silently recovering from poisoning.
    playback_info: Arc<std::sync::RwLock<PlaybackInfo>>,
    running: Arc<AtomicBool>,
    audio_output: Option<CpalOutput>,
    pipeline: DspPipeline,
    /// The dual-decoder state machine — replaces the single `decoder` field.
    stream: Option<PlaybackStream>,
    config: EngineConfig,
    /// Wall-clock position in seconds (updated per tick using
    /// `frames / (source_rate * speed)` rather than `frames / source_rate`).
    position_secs: f32,
    duration_secs: f32,
    source_sample_rate: u32,
    output_sample_rate: u32,
    speed: f32,
    current_track_id: Option<u64>,
    dsp_time: Duration,
    total_time: Duration,
    tick_start: Option<Instant>,
    last_cpu_reset: Instant,
    /// Consecutive decode-error counter for circuit-breaker (robustness).
    consecutive_decode_errors: u32,
    /// Count of fade-out frames dropped due to a full output buffer during seek.
    dropped_fadeout_frames: u64,
    /// Cached partial decoded chunk when the output ring-buffer was full.
    /// On the next tick we resume from where we left off rather than
    /// discarding the remaining frames (fixes audio dropout under CPU load).
    pending_chunk: Option<(crate::decode::DecodedChunk, usize)>,
    /// Cached partial decoded chunk for the incoming decoder during crossfade.
    pending_incoming_chunk: Option<(crate::decode::DecodedChunk, usize)>,
    /// Whether we have already triggered the crossfade for the current track
    /// (prevents re-triggering when position wobbles near the threshold).
    crossfade_triggered: bool,
    /// Path of the next track to crossfade into, if provided.
    next_track_path: Option<std::path::PathBuf>,
    /// v0.21.0: Pre-opened incoming decoder cached by prepare_next_track().
    /// Previously, prepare_next_track opened the decoder just to return
    /// DecodeInfo and then dropped it, forcing begin_crossfade_transition
    /// to open the same file a second time. Caching eliminates this
    /// redundant I/O and ensures the crossfade can start immediately
    /// when the trigger fires.
    cached_incoming_decoder: Option<SymphoniaDecoder>,
    /// Number of consecutive stream recovery attempts (capped to avoid
    /// infinite retry loops when no audio device is available).
    stream_recovery_attempts: u32,
    /// Number of consecutive successful playing ticks since last recovery or start.
    successful_playback_ticks: u32,
    // ── Pre-allocated buffers for zero-allocation hot paths ──
    /// Scratch buffer for collecting resampler output frames during crossfade
    /// decode. Reused across ticks to avoid per-frame Vec allocation.
    /// v0.29.0: Eliminates ~8–16 heap allocations per tick in the crossfade path
    /// (two per resampler per tick: one for outgoing, one for incoming, plus
    /// two for drain). Each Vec<(f32,f32)> was previously allocated with
    /// `Vec::new()` and `push()` inside decode_transitioning_stream.
    rs_out_buf: Vec<(f32, f32)>,
    rs_in_buf: Vec<(f32, f32)>,
    drain_out_buf: Vec<(f32, f32)>,
    drain_in_buf: Vec<(f32, f32)>,
    /// FIFO buffer to hold fully processed, resampled frames that are waiting
    /// to be written to the output ring buffer.
    pending_output_frames: std::collections::VecDeque<(f32, f32)>,
}

impl AudioEngine {
    /// Create a new audio engine.
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let output_buffer = Arc::new(
            FixedFrameBuffer::new(OUTPUT_BUFFER_FRAMES)
                .map_err(|e| EngineError::Config(format!("Output buffer: {}", e)))?,
        );
        let (cmd_tx, cmd_rx) = channel::bounded(256);
        let output_sample_rate = Self::detect_output_sample_rate().unwrap_or(DEFAULT_SAMPLE_RATE);
        let pipeline = DspPipeline::from_config(&config, output_sample_rate as f32);
        let info = PlaybackInfo {
            sample_rate: output_sample_rate,
            ..Default::default()
        };

        Ok(Self {
            output_buffer,
            cmd_tx,
            cmd_rx,
            playback_info: Arc::new(std::sync::RwLock::new(info)),
            running: Arc::new(AtomicBool::new(false)),
            audio_output: None,
            pipeline,
            stream: None,
            config,
            position_secs: 0.0,
            duration_secs: 0.0,
            source_sample_rate: DEFAULT_SAMPLE_RATE,
            output_sample_rate,
            speed: 1.0,
            current_track_id: None,
            dsp_time: Duration::ZERO,
            total_time: Duration::ZERO,
            tick_start: None,
            last_cpu_reset: Instant::now(),
            consecutive_decode_errors: 0,
            dropped_fadeout_frames: 0,
            pending_chunk: None,
            pending_incoming_chunk: None,
            crossfade_triggered: false,
            next_track_path: None,
            cached_incoming_decoder: None,
            stream_recovery_attempts: 0,
            successful_playback_ticks: 0,
            // Pre-allocate scratch buffers for the crossfade decode hot path.
            // A single resampler feed typically produces 0–4 output frames for
            // each input frame, so 64 entries is generous. The buffers grow
            // automatically if ever exceeded (rare), then stay at the high-water
            // mark for subsequent ticks — no shrinking needed.
            rs_out_buf: Vec::with_capacity(64),
            rs_in_buf: Vec::with_capacity(64),
            drain_out_buf: Vec::with_capacity(128),
            drain_in_buf: Vec::with_capacity(128),
            pending_output_frames: std::collections::VecDeque::with_capacity(16384),
        })
    }

    /// Convenience constructor using the default configuration.
    pub fn new_default() -> Result<Self, EngineError> {
        Self::new(EngineConfig::default())
    }

    pub(super) fn detect_output_sample_rate() -> Option<u32> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let default_config = device.default_output_config().ok()?;
        Some(default_config.sample_rate().0)
    }

    pub fn start(&mut self) -> Result<(), EngineError> {
        if self.running.load(Ordering::Acquire) {
            return Err(EngineError::AlreadyRunning);
        }
        let audio_backend = self.config.output_backend;
        let mut output = CpalOutput::new(Arc::clone(&self.output_buffer), audio_backend)?;
        self.output_sample_rate = output.sample_rate();
        output.start()?;
        self.audio_output = Some(output);

        self.running.store(true, Ordering::Release);
        self.pipeline
            .set_sample_rate(self.output_sample_rate as f32);
        self.update_playback_state(PlaybackState::Stopped);
        self.stream_recovery_attempts = 0;
        info!(
            "Audio engine started (output rate: {} Hz)",
            self.output_sample_rate
        );
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(mut output) = self.audio_output.take() {
            output.stop();
        }
        self.stream = None;
        self.crossfade_triggered = false;
        self.next_track_path = None;
        self.cached_incoming_decoder = None;
        self.update_playback_state(PlaybackState::Stopped);
        info!("Audio engine stopped");
    }

    pub fn send_command(&self, cmd: EngineCommand) {
        if let Err(e) = self.cmd_tx.send(cmd) {
            warn!("Failed to send engine command: {}", e);
        }
    }

    pub fn send_command_channel(&mut self) -> crossbeam::channel::Sender<EngineCommand> {
        self.cmd_tx.clone()
    }

    pub fn load_track(&mut self, path: &std::path::Path) -> Result<DecodeInfo, EngineError> {
        let decoder = SymphoniaDecoder::open(path)?;
        let info = decoder.info().clone();

        self.source_sample_rate = info.sample_rate;
        self.duration_secs = info.duration_secs;
        self.position_secs = 0.0;
        self.consecutive_decode_errors = 0;
        self.crossfade_triggered = false;

        #[cfg(feature = "resample")]
        let resampler = recovery::build_resampler(
            self.config.resampler_quality,
            self.source_sample_rate as f32,
            self.output_sample_rate as f32,
            self.speed,
        );

        #[cfg(not(feature = "resample"))]
        let resampler: Option<()> = None;

        self.stream = Some(PlaybackStream::Single { decoder, resampler });

        // SAFETY: We pause the CPAL output stream before resetting the ring
        // buffer indices, ensuring the consumer thread is draining silence
        // and not touching read_pos/write_pos concurrently.
        if let Some(ref output) = self.audio_output {
            output.pause();
        }
        unsafe {
            self.output_buffer.reset();
        }
        self.pipeline.reset();
        // Re-apply the current volume after reset, which resets GainProcessor to 1.0.
        // Without this, each new track plays at full volume until the next SetVolume command.
        if let Ok(pb) = self.playback_info.read() {
            self.pipeline.set_volume(pb.volume);
        }
        // Start the mixer in PlayingCurrent state for the new track.
        self.pipeline.mixer_mut().start_playing();
        if let Some(ref output) = self.audio_output {
            output.resume();
        }

        match self.playback_info.write() {
            Ok(mut pb) => {
                pb.duration_secs = info.duration_secs;
                pb.sample_rate = info.sample_rate;
                pb.track_id = self.current_track_id;
                pb.speed = self.speed;
            },
            Err(e) => {
                error!(
                    "PlaybackInfo RwLock poisoned during load_track; restarting engine state: {}",
                    e
                );
                *e.into_inner() = PlaybackInfo {
                    duration_secs: info.duration_secs,
                    sample_rate: info.sample_rate,
                    track_id: self.current_track_id,
                    speed: self.speed,
                    ..Default::default()
                };
            },
        }

        info!(
            "Loaded track: {} Hz, {} ch, {:.1}s",
            info.sample_rate, info.channels, info.duration_secs
        );
        Ok(info)
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        if let Some(prev) = self.tick_start {
            self.total_time += now.duration_since(prev);
        }
        self.tick_start = Some(now);

        self.process_commands();

        let state = self.current_state();
        if state == PlaybackState::Playing {
            let dsp_start = Instant::now();

            // Check for crossfade trigger before decoding.
            self.check_crossfade_trigger();

            self.decode_and_process();
            self.dsp_time += dsp_start.elapsed();

            // Periodic stream health check.
            self.check_stream_health();

            self.successful_playback_ticks += 1;
            if self.successful_playback_ticks >= 1000 {
                if self.stream_recovery_attempts > 0 {
                    info!("Playback stable for 5 seconds; resetting stream recovery attempts");
                    self.stream_recovery_attempts = 0;
                }
                self.successful_playback_ticks = 0;
            }
        }

        if now.duration_since(self.last_cpu_reset) >= Duration::from_secs(2) {
            let cpu_pct = if self.total_time.as_nanos() > 0 {
                (self.dsp_time.as_nanos() as f32 / self.total_time.as_nanos() as f32) * 100.0
            } else {
                0.0
            };

            let resampler_disabled = self.is_resampler_disabled();
            let convolution_ir_needs_reload = self.pipeline.convolution_ir_needs_reload();

            match self.playback_info.write() {
                Ok(mut pb) => {
                    pb.cpu_usage_pct = cpu_pct;
                    pb.resampler_disabled = resampler_disabled;
                    pb.convolution_ir_needs_reload = convolution_ir_needs_reload;
                },
                Err(e) => {
                    error!(
                        "PlaybackInfo RwLock poisoned during CPU update; resetting: {}",
                        e
                    );
                    let mut pb = e.into_inner();
                    pb.cpu_usage_pct = cpu_pct;
                    pb.resampler_disabled = resampler_disabled;
                    pb.convolution_ir_needs_reload = convolution_ir_needs_reload;
                },
            }
            self.dsp_time = Duration::ZERO;
            self.total_time = Duration::ZERO;
            self.last_cpu_reset = now;
        }
    }

    pub fn playback_info(&self) -> PlaybackInfo {
        match self.playback_info.read() {
            Ok(pb) => pb.clone(),
            Err(e) => {
                error!("PlaybackInfo RwLock poisoned in read");
                e.into_inner().clone()
            },
        }
    }

    pub fn playback_info_arc(&self) -> Arc<std::sync::RwLock<PlaybackInfo>> {
        Arc::clone(&self.playback_info)
    }

    pub fn pipeline_mut(&mut self) -> &mut DspPipeline {
        &mut self.pipeline
    }
    pub fn pipeline(&self) -> &DspPipeline {
        &self.pipeline
    }

    pub fn set_config(&mut self, config: EngineConfig) {
        let p = &mut self.pipeline;
        p.set_eq_enabled(config.eq.enabled);
        p.set_loudness_mode(match config.loudness.mode {
            tc_config::LoudnessMode::Off => crate::dsp::loudness::LoudnessMode::Off,
            tc_config::LoudnessMode::TrackReplayGain => {
                crate::dsp::loudness::LoudnessMode::TrackReplayGain
            },
            tc_config::LoudnessMode::AlbumReplayGain => {
                crate::dsp::loudness::LoudnessMode::AlbumReplayGain
            },
            tc_config::LoudnessMode::EbuR128 => crate::dsp::loudness::LoudnessMode::EbuR128,
        });
        p.set_stereo_width(config.stereo_enhancer.width);
        p.set_stereo_enhancer_enabled(config.stereo_enhancer.enabled);
        p.set_dither_enabled(config.dither_enabled);
        p.set_limiter_enabled(config.limiter.enabled);
        if config.crossfade.enabled != self.config.crossfade.enabled
            || config.crossfade.duration_ms != self.config.crossfade.duration_ms
        {
            p.mixer_mut().set_enabled(config.crossfade.enabled);
            p.mixer_mut()
                .set_duration_ms(config.crossfade.duration_ms, self.output_sample_rate as f32);
        }
        if config.performance_mode != self.config.performance_mode {
            self.pipeline = DspPipeline::from_config(&config, self.output_sample_rate as f32);
        }
        self.config = config;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn set_track_id(&mut self, id: u64) {
        self.current_track_id = Some(id);
        self.write_playback_info(|pb| pb.track_id = Some(id));
    }

    #[cfg(feature = "resample")]
    pub fn is_resampler_disabled(&self) -> bool {
        match &self.stream {
            Some(PlaybackStream::Single { resampler, .. }) => {
                resampler.as_ref().is_some_and(|r| r.is_disabled())
            },
            Some(PlaybackStream::Transitioning {
                incoming_resampler, ..
            }) => incoming_resampler.as_ref().is_some_and(|r| r.is_disabled()),
            None => false,
        }
    }

    #[cfg(not(feature = "resample"))]
    pub fn is_resampler_disabled(&self) -> bool {
        false
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}
