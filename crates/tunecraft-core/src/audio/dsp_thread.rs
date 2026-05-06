use ringbuf::traits::{Consumer, Observer, Producer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::convolution::ConvolutionEngine;
use crate::audio::dsp::DspEngine;
use crate::audio::engine::EngineLoudnessState;

// ── DspThreadConfig (Optimization #12) ──────────────────────────────────
// Refactored from 9 separate parameters into a single struct. Easier to
// extend, reduces chance of argument-order bugs, and documents the
// contract between the spawner and the DSP thread.

/// Configuration bundle for the DSP thread.
/// All fields are Arc-wrapped shared state between the audio engine
/// and the DSP thread.
pub struct DspThreadConfig {
    pub decode_cons: ringbuf::HeapCons<f32>,
    pub output_prod: ringbuf::HeapProd<f32>,
    pub dsp: Arc<Mutex<DspEngine>>,
    pub stop: Arc<AtomicBool>,
    pub convolution: Arc<Mutex<Option<ConvolutionEngine>>>,
    /// Consolidated loudness state (enabled, config, loudness measurement).
    /// Fix Issue #15: Replaces 3 separate Arc<Mutex<…>> fields with a single
    /// lock to eliminate the deadlock surface where two threads could acquire
    /// loudness / loudness_enabled / loudness_config in different orders.
    pub loudness_state: Arc<Mutex<EngineLoudnessState>>,
    pub underrun_count: Arc<AtomicU64>,
}

/// Spawn the DSP thread. Returns `None` and logs an error if the OS refuses to
/// create the thread (e.g. resource limits), so callers can degrade gracefully
/// instead of crashing.
///
/// `underrun_count` is a shared reference to the output stream's underrun
/// counter. The DSP thread reads it to adapt its read-ahead buffer level:
/// when underruns increase, the thread requires more buffered data before
/// processing, building headroom to prevent further underruns.
///
/// Fix Bug #1: The adaptive underrun logic now uses exponential decay instead
/// of an absolute counter. Previously, even a single underrun permanently
/// increased latency until the next track load. Now the read-ahead scale
/// decays over time, returning to baseline when underruns stop occurring.
pub fn spawn_dsp_thread(config: DspThreadConfig) -> Option<std::thread::JoinHandle<()>> {
    match std::thread::Builder::new()
        .name("tunecraft-dsp".into())
        .spawn(move || {
            dsp_thread_main(config);
        }) {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::error!(
                "Failed to spawn DSP thread: {} — audio processing will be unavailable",
                e
            );
            None
        }
    }
}

/// Buffer size in stereo frames. Keep in 256–512 range per spec.
const FRAME_COUNT: usize = 256;
const SAMPLE_COUNT: usize = FRAME_COUNT * 2; // interleaved stereo

/// Minimum number of samples that must be available before we process.
/// Increases adaptively when underruns are detected to build more headroom.
const MIN_READAHEAD_BASE: usize = SAMPLE_COUNT;
const MIN_READAHEAD_MAX: usize = SAMPLE_COUNT * 8; // 8x headroom cap

/// How long to sleep when the decode ring is starved.
///
/// At 48 kHz / 256 frames a buffer lasts ~5.3 ms. Sleeping 1 ms on starvation
/// yields the CPU without spinning, while still waking up well before the
/// output ring runs dry (output ring is sized for multiple buffers of headroom).
/// On old hardware this is the difference between ~0% and ~100% idle CPU.
const STARVATION_SLEEP: Duration = Duration::from_micros(1000);

/// How long to sleep after processing one buffer when the output ring has space.
///
/// Without this, the DSP thread spins at 100% CPU when the decode ring is
/// consistently full — it pulls a buffer, pushes it, and immediately loops
/// back to pull another. This 500 µs sleep (≈ 10% of one buffer duration at
/// 48 kHz) is inaudible but prevents the thread from consuming an entire core
/// when idle or during sustained playback.
const PROCESS_YIELD_SLEEP: Duration = Duration::from_micros(500);

/// Exponential decay factor for adaptive read-ahead.
/// Each iteration multiplies the underrun-based scale by this factor.
/// At 48 kHz with 256-frame buffers, one iteration ≈ 5.3 ms.
/// After ~200 iterations (~1 s), scale decays to 0.13× its peak.
/// After ~400 iterations (~2 s), scale decays to 0.02× its peak.
/// This ensures that temporary underrun spikes increase latency briefly
/// but the read-ahead returns to baseline once conditions improve.
const UNDERRUN_DECAY: f64 = 0.99;

fn dsp_thread_main(config: DspThreadConfig) {
    let DspThreadConfig {
        mut decode_cons,
        mut output_prod,
        dsp,
        stop,
        convolution,
        loudness_state,
        underrun_count,
    } = config;

    // Fix M4: Use a Vec instead of a fixed array so that DspEngine::process_buffer
    // can pad odd-length buffers. The Vec is pre-allocated to SAMPLE_COUNT and
    // resized each iteration, which is allocation-free after the first iteration
    // since the capacity is already sufficient.
    let mut buf = vec![0.0f32; SAMPLE_COUNT];

    // Fix Bug #3: Store the last successfully processed buffer. When the DSP
    // lock is contended, we output the previous buffer rather than passing
    // through unprocessed audio. This avoids sudden volume jumps or clipping
    // artifacts when the UI thread holds the lock briefly (e.g. changing EQ).
    // On the very first iteration, `last_good_buf` is silence.
    let mut last_good_buf = vec![0.0f32; SAMPLE_COUNT];

    // Fix Bug #1: Track the last-seen underrun count and a decaying
    // "effective underrun" value. When the underrun counter increases
    // (new underruns happening), the effective value jumps up. When the
    // counter is stable (no new underruns), the effective value decays
    // exponentially, returning the read-ahead to baseline.
    let mut last_underrun: u64 = 0;
    let mut effective_underrun: f64 = 0.0;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // Fix Bug #1: Adaptive read-ahead with exponential decay.
        //
        // Previously, the DSP thread checked `underrun_count > 0` using
        // the absolute counter. Since the caller only resets the counter
        // on track load, even a single underrun permanently increased
        // latency until the next track load. There was no decay/cooldown:
        // once read-ahead was scaled up 8×, it stayed at max for the rest
        // of the session.
        //
        // Now we track the *delta* in underrun count (new underruns since
        // last check) and maintain an exponentially-decaying "effective
        // underrun" score. When new underruns occur, the score jumps up.
        // When no new underruns occur, the score decays toward zero,
        // reducing the read-ahead back to baseline over ~2 seconds.
        let current_underrun = underrun_count.load(Ordering::Relaxed);
        let new_underruns = current_underrun.saturating_sub(last_underrun);
        last_underrun = current_underrun;

        // Add new underruns to the effective score, then apply decay.
        effective_underrun = effective_underrun * UNDERRUN_DECAY + new_underruns as f64;

        let min_readahead = if effective_underrun > 0.5 {
            // Scale up read-ahead proportionally to effective underrun score (capped)
            let scale = ((effective_underrun / 100.0).floor() as usize + 1).min(8);
            (MIN_READAHEAD_BASE * scale).min(MIN_READAHEAD_MAX)
        } else {
            MIN_READAHEAD_BASE
        };

        if decode_cons.occupied_len() < min_readahead {
            std::thread::sleep(STARVATION_SLEEP);
            continue;
        }

        // Pull exactly SAMPLE_COUNT samples from the decode ring.
        // Fix M4: Resize buf to SAMPLE_COUNT each iteration in case process_buffer
        // padded it with an extra zero sample on the previous iteration.
        buf.resize(SAMPLE_COUNT, 0.0);
        let read = decode_cons.pop_slice(&mut buf[..SAMPLE_COUNT]);
        if read < SAMPLE_COUNT {
            // Partial read (shouldn't happen but guard anyway)
            buf[read..SAMPLE_COUNT].fill(0.0);
        }

        // Apply DSP — take the lock once per buffer, not per sample.
        // The DspEngine processes the buffer in-place with no allocation.
        // Fix H6: When the DSP lock is contended, we must NOT skip processing
        // entirely — that bypasses the limiter and causes clipping on loud
        // material. Instead, we use try_lock and fall back to processing without
        // EQ if the lock is briefly unavailable. The limiter must always run.
        let dsp_result = dsp.try_lock();
        match dsp_result {
            Ok(mut engine) => {
                engine.process_buffer(&mut buf);
                // Fix H1/H2: Check for pending seek fade restoration
                engine.tick_seek_fade();
                // Fix Bug #3: Save the successfully processed buffer so we can
                // reuse it on the next lock contention instead of outputting
                // unprocessed audio.
                last_good_buf[..SAMPLE_COUNT].copy_from_slice(&buf[..SAMPLE_COUNT]);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                // Fix Bug #3: Lock is contended — output the previous buffer
                // (already processed with EQ/limiter) rather than the raw
                // unprocessed audio. This avoids volume jumps, clipping, and
                // bypassing the limiter. A repeated buffer is less audible
                // than a sudden loud burst of unprocessed audio.
                buf[..SAMPLE_COUNT].copy_from_slice(&last_good_buf[..SAMPLE_COUNT]);
                static SKIP_COUNT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let count = SKIP_COUNT.fetch_add(1, Ordering::Relaxed);
                if count % 10_000 == 0 {
                    tracing::warn!(
                        "DSP lock contended {} times — replayed previous buffer",
                        count
                    );
                }
            }
            Err(std::sync::TryLockError::Poisoned(e)) => {
                // Recover from poisoned lock and process anyway
                let mut engine = e.into_inner();
                engine.process_buffer(&mut buf);
                engine.tick_seek_fade();
                last_good_buf[..SAMPLE_COUNT].copy_from_slice(&buf[..SAMPLE_COUNT]);
            }
        }

        // Fix #6 + Optimization #11: Apply room correction convolution.
        //
        // Previously the convolution lock was held for the entire 512-sample
        // buffer (~5 ms at 48 kHz), blocking the UI thread if it tried to
        // load/unload an IR simultaneously. Now we use try_lock and skip
        // convolution for this buffer if the lock is contended (the UI thread
        // is swapping the IR). The IR data itself is managed via Arc::swap
        // in the engine's convolution module, so contention only lasts during
        // the actual swap — typically a single buffer period at most.
        //
        // This is a "good enough" approach: skipping one buffer of convolution
        // is inaudible (5 ms of bypass) compared to blocking the UI thread
        // or causing priority inversion. A full double-buffer / Arc::swap
        // pattern for the ConvolutionEngine could be implemented in a future
        // iteration for zero-contention convolution.
        if let Ok(mut conv_guard) = convolution.try_lock() {
            if let Some(ref mut conv) = *conv_guard {
                if conv.enabled {
                    for frame in buf[..SAMPLE_COUNT].chunks_exact_mut(2) {
                        let (l, r) = conv.process_advance(frame[0], frame[1]);
                        frame[0] = l;
                        frame[1] = r;
                    }
                }
            }
        }

        // Fix #7: Accumulate EBU R128 loudness measurement and apply gain.
        // Called on every processed buffer so the running loudness estimate
        // stays current and the normalization gain is applied next buffer.
        // Fix M3: Always apply loudness gain, even when config lock is contended.
        // Fix Issue #15: Consolidated loudness_state lock replaces the previous
        // 2-lock pattern (loudness_enabled + loudness_config), eliminating a
        // potential deadlock when two threads acquired them in different orders.
        {
            let state_guard = loudness_state.try_lock();
            match state_guard {
                Ok(mut state) => {
                    if state.enabled {
                        let cfg = state.config.clone();
                        state.loudness.process_buffer(&buf[..SAMPLE_COUNT], &cfg);
                        let gain = state.loudness.current_gain();
                        drop(state);
                        if let Ok(mut dsp_guard) = dsp.try_lock() {
                            dsp_guard.set_loudness_gain(gain);
                        }
                    }
                }
                Err(_) => {
                    // Loudness state lock contended — can't read current gain.
                    // Previous gain remains applied (safe but potentially stale).
                }
            }
        }

        // Push to output ring. If full, sleep briefly rather than spinning.
        let mut remaining = &buf[..SAMPLE_COUNT];
        while !remaining.is_empty() {
            let written = output_prod.push_slice(remaining);
            remaining = &remaining[written..];
            if !remaining.is_empty() {
                std::thread::sleep(STARVATION_SLEEP);
            }
        }

        // Fix Bug #7: Yield the CPU after processing a buffer to prevent the
        // DSP thread from spinning at 100% when the decode ring is consistently
        // full. Without this, the tight loop (check → read → process → push →
        // repeat) never yields, consuming an entire core even during normal
        // playback. The 500 µs sleep is ~10% of one buffer period at 48 kHz
        // and is inaudible, but reduces CPU from ~100% to ~10%.
        std::thread::sleep(PROCESS_YIELD_SLEEP);
    }
}
