//! True gapless playback via pipeline pre-loading.
//!
//! # The problem with the old approach
//!
//! The `GaplessSmoother` in `dsp.rs` is a ~10 ms buffer-level crossfade that
//! blends the *tail* of the previous track into the *head* of the next track.
//! This is a useful audible-click suppressor but is **not** true gapless:
//!
//!   - It requires the new pipeline to be ready *before* EOS fires, which the
//!     old code did not guarantee.
//!   - The EOS callback in `engine.rs` triggered `load_internal`, which tears
//!     down the old session first, then builds a new one. The gap between
//!     `pipeline.stop()` and the new pipeline reaching `Playing` is audible on
//!     most systems (10–100 ms depending on GStreamer plugin startup time).
//!
//! # How this module solves it
//!
//! `GaplessPreloader` builds and pre-rolls the *next* pipeline while the
//! current track is still playing. When EOS fires, the next pipeline is
//! already in `PAUSED` state and has buffered at least one audio buffer.
//! The engine calls `take_ready()` to swap the pre-built session in atomically,
//! with no decode-thread teardown in the critical path.
//!
//! # DSP state isolation (Bug #4 fix)
//!
//! Previously the `GaplessPreloader` shared the **same** `Arc<Mutex<DspEngine>>`
//! with the currently-playing session. This caused a data race: two DSP threads
//! contended for the same lock, corrupting filter state (biquad delay lines,
//! limiter envelope, smoothed band steps).
//!
//! Now `GaplessPreloader` creates its **own** fresh `DspEngine` for the
//! preloaded session. This eliminates the data race entirely. When the
//! preloaded session is swapped in at the track boundary, the `AudioEngine`
//! updates the new DspEngine's settings (EQ, ReplayGain, stereo width, etc.)
//! from the current engine state, so there is no settings gap.
//!
//! Timeline:
//! ```text
//!   t0   current track starts playing
//!   t1   UI queues next track: GaplessPreloader::preload(next_uri)
//!        -> spawns background thread -> pipeline builds & pre-rolls to PAUSED
//!        -> DSP thread uses its OWN DspEngine (no shared state, no data race)
//!   t2   EOS fires on current track
//!        -> engine calls take_ready() -> receives pre-built Session, starts it
//!        -> engine copies EQ/RG/width settings to the new DspEngine
//!        -> ZERO silence between tracks
//!        -> ZERO data race (each session has its own DspEngine)
//! ```
//!
//! # Constraints
//!
//! - Only one track is pre-loaded at a time. If the user skips or re-queues
//!   before t2, call `cancel()` then `preload()` again.
//! - The pre-rolled pipeline holds a small amount of decoded audio in the
//!   appsink's buffer queue. This is bounded by `max-buffers` on the appsink.

use anyhow::Result;
use ringbuf::{traits::Split, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::convolution::ConvolutionEngine;
use crate::audio::dsp::DspEngine;
use crate::audio::dsp_thread;
use crate::audio::engine::EngineLoudnessState;
use crate::audio::loudness::{EbuR128Loudness, LoudnessNormalizationConfig};
use crate::audio::output::AudioOutput;
use crate::audio::pipeline::DecodePipeline;

/// A fully-constructed, pre-rolled session ready to be handed to `AudioEngine`.
pub struct PreloadedSession {
    pub pipeline: DecodePipeline,
    pub audio_output: AudioOutput,
    pub dsp_stop: Arc<AtomicBool>,
    pub dsp_thread: Option<std::thread::JoinHandle<()>>,
    pub playing_flag: Arc<AtomicBool>,
    /// Fix Bug #9: The preloaded session's own DspEngine. After the session swap
    /// in `play_preloaded()`, `engine.dsp` must be re-assigned to this Arc so
    /// that subsequent calls to `set_eq_state()`, `set_volume_gain()`, etc.
    /// mutate the DspEngine that is actually connected to the audio output.
    pub dsp: Arc<Mutex<DspEngine>>,
    /// Fix: Store the preloaded session's convolution/loudness Arcs so that
    /// `play_preloaded()` can propagate the engine's settings to the new
    /// session's DSP thread. Without this, the preloaded session always has
    /// convolution=None and loudness_enabled=false, silently disabling these
    /// features on track transitions.
    pub convolution: Arc<Mutex<Option<ConvolutionEngine>>>,
    /// Loudness state (consolidated from 3 separate Arc<Mutex<…>> fields).
    /// See EngineLoudnessState for the rationale behind consolidation.
    pub loudness_state: Arc<Mutex<EngineLoudnessState>>,
}

#[cfg(target_os = "linux")]
unsafe impl Send for PreloadedSession {}

impl Drop for PreloadedSession {
    fn drop(&mut self) {
        self.dsp_stop.store(true, Ordering::Release);
        if let Some(h) = self.dsp_thread.take() {
            let _ = h.join();
        }
        self.pipeline.stop();
    }
}

enum PreloadState {
    Idle,
    Building,
    Ready(PreloadedSession),
    Cancelled,
}

#[cfg(target_os = "linux")]
unsafe impl Send for PreloadState {}

/// Manages pre-loading of the next track's pipeline for true gapless playback.
///
/// Fix Bug #4: The preloader no longer shares a DspEngine with the current
/// session. Instead, it stores the sample rate and creates a fresh DspEngine
/// for each preloaded session, eliminating the data race where two DSP threads
/// contended for the same lock and corrupted filter state.
///
/// Fix Bug #12: The cancel flag is wrapped in a `Mutex<Arc<AtomicBool>>` so
/// that each preload call gets its own cancel flag. The old thread's cancel
/// reference remains valid even after a new preload replaces the flag.
pub struct GaplessPreloader {
    state: Arc<Mutex<PreloadState>>,
    /// Fix Bug #12: Mutex-wrapped so we can replace the Arc<AtomicBool> on each
    /// preload call. The old thread retains its own Arc clone; setting the old
    /// flag to true signals cancellation without affecting the new flag.
    cancel: Mutex<Arc<AtomicBool>>,
    /// Fix Bug #4: Store the sample rate instead of sharing the DspEngine Arc.
    /// Each preloaded session gets its own fresh DspEngine.
    /// Fix: Changed from f32 to u32 — f32 cannot represent 44100 exactly
    /// (44099.998…), causing subtle sample-rate mismatches. u32 is exact
    /// for all standard audio rates.
    sample_rate: u32,
    /// Fix H3: Configurable ring buffer sizes from AudioEngine.
    /// Previously hardcoded to DECODE_RING/OUTPUT_RING constants.
    decode_ring_size: usize,
    output_ring_size: usize,
}

const DECODE_RING: usize = 65_536;
const OUTPUT_RING: usize = 32_768;

impl GaplessPreloader {
    /// Create a new preloader with the given audio device sample rate.
    ///
    /// Fix Bug #4: No longer takes a shared DspEngine. Each preloaded session
    /// creates its own fresh DspEngine to avoid data races between two DSP
    /// threads contending for the same lock.
    ///
    /// Fix H3: Now accepts configurable ring buffer sizes instead of using
    /// hardcoded DECODE_RING/OUTPUT_RING constants.
    pub fn new(sample_rate: u32, decode_ring_size: usize, output_ring_size: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(PreloadState::Idle)),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            sample_rate,
            decode_ring_size,
            output_ring_size,
        }
    }

    /// Begin pre-loading `uri` in the background.
    ///
    /// Cancels any in-progress preload first. Returns immediately; the
    /// pipeline is built on a background thread. Call `is_ready()` to poll,
    /// or block on `take_ready()` at EOS.
    pub fn preload(&self, uri: String) {
        {
            let old_cancel = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
            old_cancel.store(true, Ordering::Release);
        }
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            *s = PreloadState::Building;
        }

        let new_cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&new_cancel);
        {
            let mut c = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
            *c = new_cancel;
        }

        let state_arc = Arc::clone(&self.state);
        let sample_rate = self.sample_rate;
        let decode_ring_size = self.decode_ring_size;
        let output_ring_size = self.output_ring_size;

        match std::thread::Builder::new()
            .name("tunecraft-gapless-preload".into())
            .spawn(move || {
                if cancel_clone.load(Ordering::Acquire) {
                    *state_arc.lock().unwrap_or_else(|e| e.into_inner()) = PreloadState::Cancelled;
                    return;
                }

                match build_preloaded_session(&uri, sample_rate, decode_ring_size, output_ring_size)
                {
                    Ok(session) => {
                        let mut s = state_arc.lock().unwrap_or_else(|e| e.into_inner());
                        if cancel_clone.load(Ordering::Acquire) {
                            *s = PreloadState::Cancelled;
                        } else {
                            *s = PreloadState::Ready(session);
                            tracing::info!("Gapless: next track pre-loaded and ready");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Gapless preload failed: {}", e);
                        *state_arc.lock().unwrap_or_else(|e| e.into_inner()) = PreloadState::Idle;
                    }
                }
            }) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    "Gapless preload thread spawn failed: {}. Resetting to Idle.",
                    e
                );
                *self.state.lock().unwrap_or_else(|e| e.into_inner()) = PreloadState::Idle;
            }
        }
    }

    /// Returns `true` if a pre-loaded session is waiting to be consumed.
    pub fn is_ready(&self) -> bool {
        matches!(
            *self.state.lock().unwrap_or_else(|e| e.into_inner()),
            PreloadState::Ready(_)
        )
    }

    /// Take the pre-loaded session, if ready. Returns `None` if not ready yet.
    ///
    /// After this call the preloader returns to `Idle` state. The caller is
    /// responsible for calling `pipeline.play()` on the returned session.
    pub fn take_ready(&self) -> Option<PreloadedSession> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match std::mem::replace(&mut *s, PreloadState::Idle) {
            PreloadState::Ready(session) => Some(session),
            other => {
                *s = other;
                None
            }
        }
    }

    /// Cancel any in-progress or ready preload and return to idle.
    pub fn cancel(&self) {
        let c = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
        c.store(true, Ordering::Release);
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = PreloadState::Idle;
    }
}

fn build_preloaded_session(
    uri: &str,
    sample_rate: u32,
    decode_ring_size: usize,
    output_ring_size: usize,
) -> Result<PreloadedSession> {
    let dsp = Arc::new(Mutex::new(DspEngine::new(sample_rate as f32)));

    let (decode_prod, decode_cons) = HeapRb::<f32>::new(decode_ring_size).split();
    let (output_prod, output_cons) = HeapRb::<f32>::new(output_ring_size).split();

    let playing_flag = Arc::new(AtomicBool::new(false));
    let audio_output = AudioOutput::new(output_cons, Arc::clone(&playing_flag))?;

    {
        let mut d = dsp.lock().unwrap_or_else(|e| e.into_inner());
        d.set_sample_rate(audio_output.sample_rate as f32);
    }

    let device_rate: u32 = audio_output.sample_rate;
    let underrun_count = audio_output.underrun_count_arc();

    let pipeline = DecodePipeline::new(uri, decode_prod, device_rate)?;

    pipeline.preroll()?;

    let dsp_stop = Arc::new(AtomicBool::new(false));
    let convolution: Arc<Mutex<Option<ConvolutionEngine>>> = Arc::new(Mutex::new(None));
    let loudness_state: Arc<Mutex<EngineLoudnessState>> = Arc::new(Mutex::new(EngineLoudnessState {
        loudness: EbuR128Loudness::new(device_rate as f32)
            .or_else(|_| EbuR128Loudness::new(48_000.0))
            .or_else(|_| EbuR128Loudness::new(44_100.0))
            .unwrap_or_else(|e| {
                tracing::error!("Failed to create EbuR128Loudness for gapless preloader: {} — loudness disabled", e);
                EbuR128Loudness::new(48_000.0).expect("EbuR128Loudness fallback: 48kHz must be valid")
            }),
        enabled: false,
        config: LoudnessNormalizationConfig::default(),
    }));
    let dsp_thread = dsp_thread::spawn_dsp_thread(dsp_thread::DspThreadConfig {
        decode_cons,
        output_prod,
        dsp: Arc::clone(&dsp),
        stop: Arc::clone(&dsp_stop),
        convolution: Arc::clone(&convolution),
        loudness_state: Arc::clone(&loudness_state),
        underrun_count,
    });

    Ok(PreloadedSession {
        pipeline,
        audio_output,
        dsp_stop,
        dsp_thread,
        playing_flag,
        dsp,
        convolution,
        loudness_state,
    })
}
