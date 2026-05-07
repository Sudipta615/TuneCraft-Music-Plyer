//! Track loading logic extracted from the AudioEngine coordinator module.
//!
//! Architecture #18: `load_internal()` was previously inline in engine/mod.rs
//! alongside tick(), callbacks, and genre preset logic. Extracting it into
//! its own module completes the decomposition of the engine module into
//! domain-specific submodules: transport, seek, replaygain, volume, crossfade,
//! gapless, presets, eq_control, and now loader.

use anyhow::Result;
use ringbuf::{traits::Split, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::audio::dsp_thread;
use crate::audio::output::AudioOutput;
use crate::audio::pipeline::DecodePipeline;

use super::{AudioEngine, Session};

/// Internal track loading implementation.
///
/// Tears down any existing session, creates new ring buffers, initializes
/// the DSP engine, spawns the DSP thread, and creates a new Session.
pub fn load_internal(
    engine: &AudioEngine,
    uri: String,
    path: Option<std::path::PathBuf>,
) -> Result<()> {
    {
        let mut s = engine.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut old) = s.take() {
            old.stop_and_join();
        }
    }

    let (decode_prod, decode_cons) = HeapRb::<f32>::new(engine.decode_ring_size).split();

    engine
        .loudness_state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .loudness
        .reset();

    {
        let dsp_arc = engine.dsp_arc();
        let mut dsp = dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
        dsp.reset_state();
        dsp.mark_new_track();
    }

    let playing_flag = Arc::new(AtomicBool::new(false));
    let exclusive = engine
        .volume_state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .exclusive_mode;

    let (output_prod, audio_output) = if exclusive {
        let (prod, cons) = HeapRb::<f32>::new(engine.output_ring_size).split();
        match AudioOutput::new_exclusive(cons, Arc::clone(&playing_flag)) {
            Ok(out) => {
                tracing::info!("Exclusive mode active: {} Hz", out.sample_rate);
                (prod, out)
            }
            Err(e) => {
                tracing::warn!("Exclusive mode failed ({}), falling back to shared mode", e);
                let (prod, cons) = HeapRb::<f32>::new(engine.output_ring_size).split();
                (prod, AudioOutput::new(cons, Arc::clone(&playing_flag))?)
            }
        }
    } else {
        let (prod, cons) = HeapRb::<f32>::new(engine.output_ring_size).split();
        (prod, AudioOutput::new(cons, Arc::clone(&playing_flag))?)
    };

    let device_rate = audio_output.sample_rate;
    let underrun_count = audio_output.underrun_count_arc();

    let pipeline = DecodePipeline::new(&uri, decode_prod, device_rate)?;

    {
        let dsp_arc = engine.dsp_arc();
        let mut dsp = dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
        dsp.set_sample_rate(device_rate as f32);
    }

    let dsp_stop = Arc::new(AtomicBool::new(false));
    let dsp_handle = dsp_thread::spawn_dsp_thread(dsp_thread::DspThreadConfig {
        decode_cons,
        output_prod,
        dsp: engine.dsp_arc(),
        stop: Arc::clone(&dsp_stop),
        convolution: Arc::clone(&engine.convolution),
        loudness_state: Arc::clone(&engine.loudness_state),
        underrun_count: Arc::clone(&underrun_count),
    });

    let session = Session {
        pipeline,
        _audio_output: audio_output,
        dsp_stop,
        dsp_thread: dsp_handle,
        playing: playing_flag,
        is_playing: false,
        underrun_count: underrun_count.clone(),
    };

    underrun_count.store(0, Ordering::Relaxed);

    *engine.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);

    *engine
        .last_reported_position
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *engine
        .last_reported_duration
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;

    let speed = engine
        .volume_state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .playback_speed;
    if (speed - 1.0).abs() > 0.001 {
        let s = engine.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref sess) = *s {
            sess.pipeline.set_rate(speed);
        }
    }

    let rg = engine
        .rg_state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .enabled;
    if rg {
        if let Some(ref p) = path {
            if let Err(e) = engine.apply_replaygain_for(&p) {
                tracing::warn!("ReplayGain: {}", e);
            }
        }
    }

    *engine
        .current_track_path
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = path;

    Ok(())
}
