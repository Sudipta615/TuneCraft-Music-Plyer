//! Track loading logic extracted from the AudioEngine coordinator module.
//!
//! Architecture #18: `load_internal()` was previously inline in engine/mod.rs
//! alongside tick(), callbacks, and genre preset logic. Extracting it into
//! its own module completes the decomposition of the engine module into
//! domain-specific submodules: transport, seek, replaygain, volume, crossfade,
//! gapless, presets, eq_control, and now loader.

use anyhow::Result;
use ringbuf::{HeapRb, traits::Split};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::audio::dsp_thread;
use crate::audio::output::AudioOutput;
use crate::audio::pipeline::DecodePipeline;

use super::{Session, AudioEngine};

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
        if let Some(mut old) = s.take() { old.stop_and_join(); }
    }

    let (decode_prod, decode_cons) = HeapRb::<f32>::new(engine.decode_ring_size).split();

    // Reset EBU R128 loudness measurement at track boundaries.
    engine.loudness_state.lock().unwrap_or_else(|e| e.into_inner()).loudness.reset();

    // Reset DSP state first, THEN signal the gapless smoother that a new
    // track is starting. The previous order (mark_new_track then reset_state)
    // cleared the seek_fade_ramp via reset_state() after it had been set up
    // by the gapless logic, and also reset filter state that the gapless
    // smoother's apply_to_head() depended on.
    {
        let mut dsp = engine.dsp_arc().lock().unwrap_or_else(|e| e.into_inner());
        dsp.reset_state();
        dsp.mark_new_track();
    }

    // Create audio output FIRST so we know the device sample rate.
    let playing_flag = Arc::new(AtomicBool::new(false));
    let exclusive = engine.volume_state.lock().unwrap_or_else(|e| e.into_inner()).exclusive_mode;

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

    // Create pipeline with the device rate so GStreamer outputs at the correct rate.
    let pipeline = DecodePipeline::new(&uri, decode_prod, device_rate)?;

    {
        let mut dsp = engine.dsp_arc().lock().unwrap_or_else(|e| e.into_inner());
        dsp.set_sample_rate(device_rate as f32);
    }

    let dsp_stop  = Arc::new(AtomicBool::new(false));
    let dsp_handle = dsp_thread::spawn_dsp_thread(
        dsp_thread::DspThreadConfig {
            decode_cons, output_prod,
            dsp: engine.dsp_arc(),
            stop: Arc::clone(&dsp_stop),
            convolution: Arc::clone(&engine.convolution),
            loudness_state: Arc::clone(&engine.loudness_state),
            underrun_count: Arc::clone(&underrun_count),
        },
    );

    let session = Session {
        pipeline, _audio_output: audio_output, dsp_stop,
        dsp_thread: dsp_handle, playing: playing_flag, is_playing: false,
        underrun_count: underrun_count.clone(),
    };

    // Reset the underrun counter when starting a new session.
    underrun_count.store(0, Ordering::Relaxed);

    *engine.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);

    // Reset last-reported state for the new track
    *engine.last_reported_position.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *engine.last_reported_duration.lock().unwrap_or_else(|e| e.into_inner()) = None;

    // Apply playback speed if non-default
    let speed = engine.volume_state.lock().unwrap_or_else(|e| e.into_inner()).playback_speed;
    if (speed - 1.0).abs() > 0.001 {
        let s = engine.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref sess) = *s { sess.pipeline.set_rate(speed); }
    }

    // Apply ReplayGain if enabled
    let rg = engine.rg_state.lock().unwrap_or_else(|e| e.into_inner()).enabled;
    if rg {
        if let Some(p) = path {
            if let Err(e) = engine.apply_replaygain_for(&p) { tracing::warn!("ReplayGain: {}", e); }
        }
    }

    // Fix Bug #15: Store the current track path so that enabling ReplayGain
    // on a playing track can immediately compute and apply the RG factor.
    *engine.current_track_path.lock().unwrap_or_else(|e| e.into_inner()) = path;

    Ok(())
}
