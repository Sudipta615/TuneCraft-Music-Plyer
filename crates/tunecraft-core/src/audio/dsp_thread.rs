use ringbuf::traits::{Consumer, Observer, Producer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::convolution::ConvolutionEngine;
use crate::audio::dsp::DspEngine;
use crate::audio::engine::EngineLoudnessState;

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

    let mut buf = vec![0.0f32; SAMPLE_COUNT];

    let mut last_good_buf = vec![0.0f32; SAMPLE_COUNT];

    let mut last_underrun: u64 = 0;
    let mut effective_underrun: f64 = 0.0;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let current_underrun = underrun_count.load(Ordering::Relaxed);
        let new_underruns = current_underrun.saturating_sub(last_underrun);
        last_underrun = current_underrun;

        effective_underrun = effective_underrun * UNDERRUN_DECAY + new_underruns as f64;

        let min_readahead = if effective_underrun > 0.5 {
            let scale = ((effective_underrun / 100.0).floor() as usize + 1).min(8);
            (MIN_READAHEAD_BASE * scale).min(MIN_READAHEAD_MAX)
        } else {
            MIN_READAHEAD_BASE
        };

        if decode_cons.occupied_len() < min_readahead {
            std::thread::sleep(STARVATION_SLEEP);
            continue;
        }

        buf.resize(SAMPLE_COUNT, 0.0);
        let read = decode_cons.pop_slice(&mut buf[..SAMPLE_COUNT]);
        if read < SAMPLE_COUNT {
            buf[read..SAMPLE_COUNT].fill(0.0);
        }

        let dsp_result = dsp.try_lock();
        match dsp_result {
            Ok(mut engine) => {
                engine.process_buffer(&mut buf);
                engine.tick_seek_fade();
                last_good_buf[..SAMPLE_COUNT].copy_from_slice(&buf[..SAMPLE_COUNT]);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                buf[..SAMPLE_COUNT].copy_from_slice(&last_good_buf[..SAMPLE_COUNT]);
                static SKIP_COUNT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let count = SKIP_COUNT.fetch_add(1, Ordering::Relaxed);
                if count.is_multiple_of(10_000) {
                    tracing::warn!(
                        "DSP lock contended {} times — replayed previous buffer",
                        count
                    );
                }
            }
            Err(std::sync::TryLockError::Poisoned(e)) => {
                let mut engine = e.into_inner();
                engine.process_buffer(&mut buf);
                engine.tick_seek_fade();
                last_good_buf[..SAMPLE_COUNT].copy_from_slice(&buf[..SAMPLE_COUNT]);
            }
        }

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

        {
            let state_guard = loudness_state.try_lock();
            if let Ok(mut state) = state_guard {
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
        }

        let mut remaining = &buf[..SAMPLE_COUNT];
        while !remaining.is_empty() {
            let written = output_prod.push_slice(remaining);
            remaining = &remaining[written..];
            if !remaining.is_empty() {
                std::thread::sleep(STARVATION_SLEEP);
            }
        }

        std::thread::sleep(PROCESS_YIELD_SLEEP);
    }
}
