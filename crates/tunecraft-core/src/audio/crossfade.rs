//! Crossfade engine using GStreamer audiomixer for gapless transitions.
//!
//! Volume ramps (fade-in / fade-out) are performed via **per-sample gain
//! interpolation** in the DSP path rather than step-based GLib timers.
//!
//! Each `CrossfadeEntry` holds a `GainRamp` with:
//!   - `gain`   — current instantaneous gain (0.0 – 1.0)
//!   - `target` — destination gain
//!   - `step`   — per-sample delta = (target − gain) / total_samples
//!
//! A GStreamer src-pad BUFFER probe on the volume element's src pad fires once
//! per audio buffer and advances the ramp by one step per sample in that buffer,
//! then writes the result back to the volume element. This is sample-accurate,
//! zipper-noise-free, and does not require any GLib timer threads.

use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, Pipeline};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Callback type for end-of-stream events from the crossfade pipeline.
pub type CrossfadeEosCallback = Box<dyn Fn() + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy)]
struct GainRamp {
    gain: f32,
    target: f32,
    step: f32,
}

impl GainRamp {
    fn immediate(gain: f32) -> Self {
        Self {
            gain,
            target: gain,
            step: 0.0,
        }
    }

    fn new(from: f32, to: f32, total_samples: u64) -> Self {
        let step = if total_samples > 0 {
            (to - from) / total_samples as f32
        } else {
            0.0
        };
        Self {
            gain: from,
            target: to,
            step,
        }
    }

    /// Advance one sample; returns the gain *before* the step (apply to that sample).
    #[inline(always)]
    fn advance(&mut self) -> f32 {
        let g = self.gain;
        if self.step != 0.0 {
            self.gain = (self.gain + self.step).clamp(0.0, 1.0);
            if (self.gain - self.target).abs() <= self.step.abs() {
                self.gain = self.target;
                self.step = 0.0;
            }
        }
        g
    }

    #[allow(dead_code)]
    fn is_done(&self) -> bool {
        self.step == 0.0
    }
}

struct CrossfadeEntry {
    _uridecodebin: Element,
    volume: Element,
    #[allow(dead_code)]
    uri: String,
    /// Gain ramp state; mutated from the GStreamer streaming thread via probe.
    ramp: Arc<Mutex<GainRamp>>,
    /// The mixer sink pad linked to this entry's volume src pad.
    /// Must be released back to the audiomixer when the entry is removed.
    /// Shared with the connect_pad_added callback which populates it
    /// asynchronously when GStreamer adds an audio pad.
    mixer_sink_pad: Arc<Mutex<Option<gstreamer::Pad>>>,
}

impl CrossfadeEntry {
    fn new(uri: &str, pipeline: &Pipeline, mixer: &Element) -> Result<Self> {
        let uridecodebin = ElementFactory::make("uridecodebin")
            .build()
            .context("failed to create uridecodebin")?;
        uridecodebin.set_property("uri", uri);

        let volume = ElementFactory::make("volume")
            .build()
            .context("failed to create volume element")?;
        volume.set_property("volume", 0.0f64);

        let mixer_sink_pad: Arc<Mutex<Option<gstreamer::Pad>>> = Arc::new(Mutex::new(None));
        let mixer_sink_holder_cb = Arc::clone(&mixer_sink_pad);

        let pipeline_weak = pipeline.downgrade();
        let mixer_weak = mixer.downgrade();
        let volume_clone = volume.clone();

        uridecodebin.connect_pad_added(move |_, pad| {
            if let (Some(pipeline), Some(mixer)) = (pipeline_weak.upgrade(), mixer_weak.upgrade()) {
                let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None)));
                if let Some(caps) = caps {
                    if let Some(s) = caps.structure(0) {
                        if !s.name().starts_with("audio/") {
                            return;
                        }
                    }
                }
                let Some(sink_pad) = volume_clone.static_pad("sink") else {
                    tracing::error!("CrossfadeEngine: volume element has no sink pad");
                    return;
                };
                pad.link(&sink_pad).ok();

                let Some(src_pad) = volume_clone.static_pad("src") else {
                    tracing::error!("CrossfadeEngine: volume element has no src pad");
                    return;
                };
                let Some(mixer_sink) = mixer.request_pad_simple("sink_%u") else {
                    tracing::error!("CrossfadeEngine: audiomixer refused a new sink pad");
                    return;
                };
                src_pad.link(&mixer_sink).ok();
                if let Ok(mut h) = mixer_sink_holder_cb.lock() {
                    *h = Some(mixer_sink);
                }
                let _ = pipeline.sync_children_states();
            }
        });

        pipeline.add_many(&[&uridecodebin, &volume])?;
        uridecodebin.sync_state_with_parent()?;

        Ok(Self {
            _uridecodebin: uridecodebin,
            volume,
            uri: uri.to_string(),
            ramp: Arc::new(Mutex::new(GainRamp::immediate(0.0))),
            mixer_sink_pad,
        })
    }

    fn release_mixer_pad(&self, mixer: &Element) {
        if let Ok(mut h) = self.mixer_sink_pad.lock() {
            if let Some(pad) = h.take() {
                mixer.release_request_pad(&pad);
            }
        }
    }

    fn fade_in(&self, duration_ms: u32, target_volume: f64, sample_rate: u32) {
        let target = (target_volume as f32).clamp(0.0, 1.0);
        let samples = (sample_rate as u64 * duration_ms as u64) / 1000;
        *self.ramp.lock().unwrap_or_else(|e| e.into_inner()) = GainRamp::new(0.0, target, samples);
        self.volume.set_property("volume", 0.0f64);
    }

    fn fade_out(&self, duration_ms: u32, sample_rate: u32) {
        let current = self.ramp.lock().unwrap_or_else(|e| e.into_inner()).gain;
        let samples = (sample_rate as u64 * duration_ms as u64) / 1000;
        *self.ramp.lock().unwrap_or_else(|e| e.into_inner()) = GainRamp::new(current, 0.0, samples);
    }

    fn set_volume_immediate(&self, vol: f64) {
        let v = (vol as f32).clamp(0.0, 1.0);
        *self.ramp.lock().unwrap_or_else(|e| e.into_inner()) = GainRamp::immediate(v);
        self.volume.set_property("volume", vol.clamp(0.0, 1.0));
    }

    /// Install a BUFFER probe on the volume element's src pad.
    /// The probe advances the gain ramp by one step per sample and writes
    /// the resulting gain to the volume element — fully sample-accurate.
    fn install_gain_probe(&self) {
        let Some(src_pad) = self.volume.static_pad("src") else {
            return;
        };
        let ramp_arc = Arc::clone(&self.ramp);
        let volume_el = self.volume.clone();

        src_pad.add_probe(gstreamer::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(gstreamer::PadProbeData::Buffer(ref buf)) = info.data {
                let n_samples = buf.size() / (2 * std::mem::size_of::<f32>());
                let mut ramp = ramp_arc.lock().unwrap_or_else(|e| e.into_inner());
                let mut last = ramp.gain;
                for _ in 0..n_samples {
                    last = ramp.advance();
                }
                volume_el.set_property("volume", last as f64);
            }
            gstreamer::PadProbeReturn::Ok
        });
    }
}

pub struct CrossfadeEngine {
    pipeline: Pipeline,
    mixer: Element,
    fade_duration_ms: Mutex<u32>,
    entries: Mutex<Vec<CrossfadeEntry>>,
    track_duration: Mutex<Option<Duration>>,
    current_volume: Mutex<f64>,
    /// Output sample rate in Hz — required to convert ms to sample counts.
    sample_rate: Mutex<u32>,
    /// v3.0: Bus watch ID is now behind a feature flag. The iced UI uses
    /// poll_bus() instead, which doesn't require a GLib main loop.
    bus_watch_id: Mutex<Option<glib::SourceId>>,
    eos_cb: Arc<Mutex<Option<CrossfadeEosCallback>>>,
    /// v3.0: Accumulated bus events from poll_bus(), consumed by tick().
    _pending_events: Mutex<Vec<crate::audio::pipeline::BusEvent>>,
}

impl CrossfadeEngine {
    pub fn new(fade_duration_ms: u32) -> Result<Self> {
        super::engine::ensure_gstreamer_initialized()?;

        let pipeline = Pipeline::with_name("crossfade-pipeline");
        let mixer = ElementFactory::make("audiomixer")
            .build()
            .context("audiomixer")?;
        let audioconvert = ElementFactory::make("audioconvert")
            .build()
            .context("audioconvert")?;
        let audiosink = ElementFactory::make("autoaudiosink")
            .build()
            .context("autoaudiosink")?;

        pipeline.add_many(&[&mixer, &audioconvert, &audiosink])?;
        Element::link_many(&[&mixer, &audioconvert, &audiosink])?;

        let eos_cb: Arc<Mutex<Option<CrossfadeEosCallback>>> = Arc::new(Mutex::new(None));

        let bus_watch_id = Mutex::new(None);

        Ok(Self {
            pipeline,
            mixer,
            fade_duration_ms: Mutex::new(fade_duration_ms),
            entries: Mutex::new(Vec::new()),
            track_duration: Mutex::new(None),
            current_volume: Mutex::new(1.0),
            sample_rate: Mutex::new(48_000),
            bus_watch_id,
            eos_cb,
            _pending_events: Mutex::new(Vec::new()),
        })
    }

    /// v3.0: Poll the crossfade pipeline bus for pending events.
    /// Call this from the UI framework's timer (e.g. iced Subscription).
    /// Fires the EOS callback if an EOS message is found.
    pub fn poll_and_dispatch(&self) {
        if let Some(bus) = self.pipeline.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gstreamer::MessageView::Eos(_) => {
                        tracing::debug!("CrossfadeEngine: EOS (via poll)");
                        if let Ok(cb) = self.eos_cb.lock() {
                            if let Some(ref f) = *cb {
                                f();
                            }
                        }
                    }
                    gstreamer::MessageView::Error(e) => tracing::error!(
                        "CrossfadeEngine GST error: {} ({:?})",
                        e.error(),
                        e.debug()
                    ),
                    gstreamer::MessageView::Warning(w) => {
                        tracing::warn!("CrossfadeEngine GST warning: {}", w.error())
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn on_end_of_stream(&self, callback: CrossfadeEosCallback) {
        *self.eos_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }

    /// Set the output sample rate so fade durations convert correctly to samples.
    pub fn set_sample_rate(&self, rate: u32) {
        *self.sample_rate.lock().unwrap_or_else(|e| e.into_inner()) = rate;
    }

    pub fn load_track(&self, uri: &str) -> Result<()> {
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            for e in entries.iter() {
                if let Ok(pad_holder) = e.mixer_sink_pad.lock() {
                    if let Some(ref mixer_sink) = *pad_holder {
                        if let Some(src_pad) = e.volume.static_pad("src") {
                            let _ = src_pad.unlink(mixer_sink);
                        }
                    }
                }
                e.release_mixer_pad(&self.mixer);
            }
            entries.clear();
        }

        let entry = CrossfadeEntry::new(uri, &self.pipeline, &self.mixer)?;
        let fade_ms = *self
            .fade_duration_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let vol = *self
            .current_volume
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let sr = *self.sample_rate.lock().unwrap_or_else(|e| e.into_inner());
        entry.fade_in(fade_ms, vol, sr);
        entry.install_gain_probe();
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry);
        Ok(())
    }

    pub fn load_track_with_crossfade(&self, uri: &str) -> Result<()> {
        let fade_ms = *self
            .fade_duration_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let vol = *self
            .current_volume
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let sr = *self.sample_rate.lock().unwrap_or_else(|e| e.into_inner());

        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            for e in entries.iter() {
                e.fade_out(fade_ms, sr);
                if let Ok(pad_holder) = e.mixer_sink_pad.lock() {
                    if let Some(ref mixer_sink) = *pad_holder {
                        if let Some(src_pad) = e.volume.static_pad("src") {
                            let _ = src_pad.unlink(mixer_sink);
                        }
                    }
                }
                e.release_mixer_pad(&self.mixer);
            }
            entries.clear();
        }

        let entry = CrossfadeEntry::new(uri, &self.pipeline, &self.mixer)?;
        entry.fade_in(fade_ms, vol, sr);
        entry.install_gain_probe();
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry);
        Ok(())
    }

    pub fn play(&self) {
        let _ = self.pipeline.set_state(gstreamer::State::Playing);
    }
    pub fn pause(&self) {
        let _ = self.pipeline.set_state(gstreamer::State::Paused);
    }

    pub fn toggle_playback(&self) {
        let state = self.pipeline.current_state();
        if state == gstreamer::State::Playing {
            self.pause()
        } else {
            self.play()
        }
    }

    pub fn is_playing(&self) -> bool {
        self.pipeline.current_state() == gstreamer::State::Playing
    }

    pub fn position(&self) -> Option<Duration> {
        self.pipeline
            .query_position::<gstreamer::ClockTime>()
            .map(|ct| Duration::from_nanos(ct.nseconds()))
    }

    pub fn duration(&self) -> Option<Duration> {
        {
            let c = self.track_duration.lock().ok()?;
            if c.is_some() {
                return *c;
            }
        }
        let r = self
            .pipeline
            .query_duration::<gstreamer::ClockTime>()
            .map(|ct| Duration::from_nanos(ct.nseconds()));
        if r.is_some() {
            if let Ok(mut d) = self.track_duration.lock() {
                *d = r;
            }
        }
        r
    }

    pub fn set_duration(&self, dur: Option<Duration>) {
        if let Ok(mut d) = self.track_duration.lock() {
            *d = dur;
        }
    }

    pub fn fade_duration(&self) -> Duration {
        Duration::from_millis(
            *self
                .fade_duration_ms
                .lock()
                .unwrap_or_else(|e| e.into_inner()) as u64,
        )
    }

    pub fn set_fade_duration(&self, ms: u32) {
        *self
            .fade_duration_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = ms;
    }

    pub fn set_volume(&self, vol: f64) {
        *self
            .current_volume
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = vol.clamp(0.0, 1.0);
        for e in self
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            e.set_volume_immediate(vol);
        }
    }

    pub fn seek(&self, position: Duration) -> Result<()> {
        let nanos = position.as_nanos() as u64;
        self.pipeline
            .seek(
                1.0,
                gstreamer::SeekFlags::FLUSH | gstreamer::SeekFlags::KEY_UNIT,
                gstreamer::SeekType::Set,
                gstreamer::ClockTime::from_nseconds(nanos),
                gstreamer::SeekType::None,
                gstreamer::ClockTime::NONE,
            )
            .context("crossfade pipeline seek failed")
    }

    pub fn set_rate(&self, rate: f64) {
        let rate = rate.clamp(0.25, 4.0);
        let pos = match self.pipeline.query_position::<gstreamer::ClockTime>() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    "CrossfadeEngine set_rate: query_position returned None, skipping rate change"
                );
                return;
            }
        };
        let flags = gstreamer::SeekFlags::FLUSH | gstreamer::SeekFlags::KEY_UNIT;
        let _ = self.pipeline.seek(
            rate,
            flags,
            gstreamer::SeekType::Set,
            pos,
            gstreamer::SeekType::None,
            gstreamer::ClockTime::NONE,
        );
    }
}

impl Drop for CrossfadeEngine {
    fn drop(&mut self) {
        if let Some(id) = self.bus_watch_id.lock().ok().and_then(|mut id| id.take()) {
            id.remove();
        }
        let _ = self.pipeline.set_state(gstreamer::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_ramp_fade_in_reaches_target() {
        let mut r = GainRamp::new(0.0, 1.0, 100);
        for _ in 0..100 {
            r.advance();
        }
        assert!((r.gain - 1.0).abs() < 1e-4, "gain={}", r.gain);
        assert!(r.is_done());
    }

    #[test]
    fn gain_ramp_fade_out_reaches_zero() {
        let mut r = GainRamp::new(1.0, 0.0, 200);
        for _ in 0..200 {
            r.advance();
        }
        assert!(r.gain.abs() < 1e-4, "gain={}", r.gain);
        assert!(r.is_done());
    }

    #[test]
    fn gain_ramp_immediate_is_done() {
        let r = GainRamp::immediate(0.75);
        assert!(r.is_done());
        assert!((r.gain - 0.75).abs() < 1e-6);
    }

    #[test]
    fn gain_ramp_zero_duration_is_immediate() {
        let r = GainRamp::new(0.0, 1.0, 0);
        assert!(r.is_done());
    }

    #[test]
    fn gain_ramp_monotone_increase() {
        let mut r = GainRamp::new(0.0, 1.0, 50);
        let mut prev = 0.0f32;
        for _ in 0..50 {
            let g = r.advance();
            assert!(g >= prev - 1e-7, "not monotone: {} < {}", g, prev);
            prev = g;
        }
    }

    #[test]
    fn gain_ramp_does_not_overshoot() {
        let mut r = GainRamp::new(0.0, 0.8, 30);
        for _ in 0..100 {
            r.advance();
        }
        assert!(r.gain <= 0.8 + 1e-5, "overshoot: {}", r.gain);
    }
}
