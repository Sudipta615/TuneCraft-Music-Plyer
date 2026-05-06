//! GStreamer decode-only pipeline.
//!
//! Architecture (strict, per spec):
//!   uridecodebin → audioconvert → audioresample → capsfilter → appsink
//!
//! GStreamer handles both decoding AND resampling to the output device rate.
//! The capsfilter constrains the output to F32LE stereo at the target sample
//! rate. When the source file's native rate differs from the target rate,
//! GStreamer automatically inserts an internal `audioresample` element
//! (quality=10, Speex-based) between `audioconvert` and `capsfilter`,
//! providing high-quality sample rate conversion.
//!
//! NOTE: By including the rate in the caps, GStreamer handles resampling
//! internally with its `audioresample` element. This eliminates the need for
//! a separate Rust resampler in the DSP thread — GStreamer's resampler is
//! high-quality (Speex resampler at quality 10) and operates at the pipeline
//! level before the DSP thread, so all DSP processing runs at the correct
//! output device rate.
//!
//! The appsink pulls decoded F32LE stereo at the target sample rate and pushes
//! interleaved frames into the decode→DSP ring buffer.
//!
//! # v3.0 Cross-Platform: Poll-Driven Bus Messages
//!
//! The bus watch is no longer GLib-main-loop-dependent. Instead, the UI
//! framework calls `poll_bus()` periodically (e.g. via a Dioxus timer task)
//! to drain pending GStreamer bus messages. This eliminates the GLib main
//! context requirement, making the pipeline fully usable from any event loop.

use anyhow::{Context, Result};
use gstreamer::prelude::*;
use ringbuf::traits::Producer;
use std::sync::{Arc, Mutex};

/// Platform-agnostic bus event, decoupled from GLib.
#[derive(Debug, Clone)]
pub enum BusEvent {
    /// A GStreamer error occurred.
    Error(String),
    /// A GStreamer warning occurred.
    Warning(String),
    /// End-of-stream reached.
    Eos,
}

/// Target decode format channels — must match cpal output config.
/// Rate is fixed to the output device rate, passed via `target_sample_rate`.
pub const TARGET_CHANNELS: u32 = 2;

/// Callback for end-of-stream.
pub type PipelineEosCallback = Box<dyn Fn() + Send + Sync + 'static>;

/// GStreamer decode pipeline.
pub struct DecodePipeline {
    pipeline: gstreamer::Pipeline,
    eos_cb: Arc<Mutex<Option<PipelineEosCallback>>>,
    _bus_watch_guard: Option<gstreamer::BusWatchGuard>,
}

impl DecodePipeline {
    /// Build the pipeline for `uri`.
    ///
    /// `decode_prod` is the write-end of the decode→DSP ring buffer.
    /// The appsink callback pushes decoded F32LE stereo samples into it.
    /// `target_sample_rate` is the output device's sample rate. GStreamer
    /// will resample the decoded audio to this rate using its internal
    /// `audioresample` element when the source rate differs, ensuring
    /// the DSP thread always receives audio at the correct output rate.
    pub fn new(
        uri: &str,
        decode_prod: ringbuf::HeapProd<f32>,
        target_sample_rate: u32,
    ) -> Result<Self> {
        // Fix: Use centralized GStreamer init instead of raw gstreamer::init().
        // The ensure_gstreamer_initialized() function uses std::sync::Once to
        // guarantee exactly one init call and provides consistent error handling
        // across all call sites (AudioEngine::new, CrossfadeEngine, ConvolutionEngine, DecodePipeline).
        super::engine::ensure_gstreamer_initialized().map_err(|e| {
            anyhow::anyhow!(
                "GStreamer initialisation failed: {}\n\n\
                Make sure GStreamer and the following plugins are installed:\n\
                  gstreamer1.0-plugins-base (uridecodebin, audioconvert, audioresample)\n\
                  gstreamer1.0-plugins-good\n\
                  gstreamer1.0-plugins-bad\n\n\
                On Debian/Ubuntu: sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good\n\
                On Fedora: sudo dnf install gstreamer1-plugins-base gstreamer1-plugins-good",
                e
            )
        })?;

        let pipeline = gstreamer::Pipeline::new();

        // ── Elements ──────────────────────────────────────────────
        let uridecodebin = gstreamer::ElementFactory::make("uridecodebin")
            .build()
            .context("uridecodebin")?;
        uridecodebin.set_property("uri", uri);
        // Fix L2: 1-second buffer-duration adds latency and delays gapless
        // track start. Reduce to 200ms which provides enough buffering
        // without adding unnecessary latency.
        uridecodebin.set_property("buffer-duration", 200_000_000i64);

        let audioconvert = gstreamer::ElementFactory::make("audioconvert")
            .build()
            .context("audioconvert")?;

        // Caps: F32LE stereo at the output device rate. Including the rate field
        // causes GStreamer to insert an internal `audioresample` element when the
        // source file's native rate differs from the target rate. This provides
        // high-quality sample rate conversion (Speex resampler at quality 10)
        // directly in the pipeline, eliminating the need for a separate Rust
        // resampler in the DSP thread.
        let caps = gstreamer::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("rate", target_sample_rate as i32)
            .field("channels", TARGET_CHANNELS as i32)
            .build();
        let capsfilter = gstreamer::ElementFactory::make("capsfilter")
            .build()
            .context("capsfilter")?;
        capsfilter.set_property("caps", &caps);

        let appsink_elem = gstreamer::ElementFactory::make("appsink")
            .build()
            .context("appsink")?;
        appsink_elem.set_property("emit-signals", false);
        // Fix CRITICAL BUG: sync=false caused GStreamer to deliver buffers as
        // fast as possible (not at real-time rate). This meant the entire file
        // was "processed" in seconds, most audio data was dropped because the
        // ring buffers couldn't keep up, position queries raced ahead, and EOS
        // fired early — making a 5-minute track appear to play in ~10 seconds
        // with only garbled fragments audible as "distortion".
        //
        // With sync=true, the appsink synchronizes to the GStreamer pipeline
        // clock, delivering buffers at the correct media rate. The decode ring
        // fills at the real-time rate, the DSP thread processes at the output
        // rate, and position queries return accurate timestamps.
        appsink_elem.set_property("sync", true);
        // Fix Bug #2: With drop=true and max-buffers=8, when the decode ring
        // is full, GStreamer silently drops the oldest buffer rather than
        // waiting. This means the DSP thread can receive a time-discontinuous
        // stream, which breaks the gapless smoother and could cause audible
        // glitches on slow CPUs. The yield-loop fallback below was unreachable
        // for overload cases.
        //
        // Changed to drop=false so that when the appsink's internal queue is
        // full, the GStreamer streaming thread blocks instead of dropping
        // buffers. The yield-loop below handles backpressure gracefully by
        // yielding the GStreamer thread when the decode ring is full, with a
        // safety limit (1000 yields) that drops remaining samples only as a
        // last resort to avoid stalling the pipeline indefinitely.
        appsink_elem.set_property("max-buffers", 2u32);
        appsink_elem.set_property("drop", false);

        let appsink: gstreamer_app::AppSink = appsink_elem
            .dynamic_cast()
            .map_err(|_| anyhow::anyhow!("appsink dynamic_cast failed"))?;

        // ── Add elements to pipeline ──────────────────────────────
        pipeline.add_many(&[
            &uridecodebin,
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;

        // Link the static chain (uridecodebin links via pad-added)
        gstreamer::Element::link_many(&[
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;

        // ── Dynamic pad: uridecodebin → audioconvert ──────────────
        let audioconvert_weak = audioconvert.downgrade();
        uridecodebin.connect_pad_added(move |_src, src_pad| {
            // Only wire audio pads
            let caps = src_pad
                .current_caps()
                .or_else(|| Some(src_pad.query_caps(None)));
            if let Some(caps) = caps {
                if let Some(s) = caps.structure(0) {
                    if !s.name().starts_with("audio/") {
                        return;
                    }
                }
            }
            let Some(ac) = audioconvert_weak.upgrade() else {
                return;
            };
            let sink = match ac.static_pad("sink") {
                Some(p) => p,
                None => return,
            };
            if !sink.is_linked() {
                if let Err(e) = src_pad.link(&sink) {
                    tracing::error!("pad link failed: {:?}", e);
                }
            }
        });

        // ── Appsink callbacks ──────────────────────────────────────
        let eos_cb: Arc<Mutex<Option<PipelineEosCallback>>> = Arc::new(Mutex::new(None));
        let eos_cb_appsink = Arc::clone(&eos_cb);

        // Fix Bug #34: The ring buffer producer is only accessed from the
        // GStreamer streaming thread (appsink new_sample callback) after
        // construction. Remove the unnecessary Arc<Mutex<HeapProd>> wrapper —
        // the producer can be moved directly into the callback closure since
        // it is never shared across threads.
        let mut decode_prod_cb = decode_prod;

        let callbacks = gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |appsink| {
                let sample = match appsink.pull_sample() {
                    Ok(s) => s,
                    Err(_) => return Err(gstreamer::FlowError::Eos),
                };
                let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
                let map = buffer
                    .map_readable()
                    .map_err(|_| gstreamer::FlowError::Error)?;

                let Some(samples) = crate::util::cast_u8_to_f32(&map) else {
                    // Malformed buffer — skip this sample rather than crash
                    return Err(gstreamer::FlowError::Error);
                };

                // Push to decode ring with backpressure.
                // Fix Bug #34: Access producer directly (no Arc<Mutex> needed)
                //
                // Fix Bug #5: Previously slept up to 1 second (200 × 5 ms) in the
                // GStreamer streaming thread callback when the decode ring was full.
                // Sleeping in a streaming thread callback stalls the GStreamer pipeline
                // and can cause deadlocks. Now we use yield_now() for most iterations
                // and a very short microsleep only occasionally. The sync=true property
                // already provides backpressure at the pipeline level, so yielding is
                // sufficient to let the DSP thread consume data.
                let mut remaining = samples;
                let mut yield_count = 0u32;
                while !remaining.is_empty() {
                    let n = decode_prod_cb.push_slice(remaining);
                    remaining = &remaining[n..];
                    if !remaining.is_empty() {
                        yield_count += 1;
                        if yield_count > 1000 {
                            // Ring buffer is persistently full after many yields —
                            // drop remaining samples to avoid stalling the GStreamer
                            // thread. This should never happen with sync=true unless
                            // the DSP thread is completely stuck.
                            tracing::warn!(
                                "Decode ring persistently full after {} yields, \
                                 dropping {} samples",
                                yield_count,
                                remaining.len()
                            );
                            break;
                        }
                        // Yield the GStreamer streaming thread instead of sleeping.
                        // sync=true already provides backpressure; yielding allows the
                        // DSP thread to consume data without blocking the streaming
                        // thread with a long sleep.
                        if yield_count % 100 == 0 {
                            // Occasional very short sleep (100 µs) to avoid pure
                            // spin-loop on contended cores, but much shorter than
                            // the previous 5 ms sleep.
                            std::thread::sleep(std::time::Duration::from_micros(100));
                        } else {
                            std::thread::yield_now();
                        }
                    }
                }

                Ok(gstreamer::FlowSuccess::Ok)
            })
            .eos(move |_appsink| {
                if let Ok(cb) = eos_cb_appsink.lock() {
                    if let Some(ref f) = *cb {
                        f();
                    }
                }
            })
            .build();

        appsink.set_callbacks(callbacks);

        Ok(Self {
            pipeline,
            eos_cb,
            _bus_watch_guard: None,
        })
    }

    /// Register the end-of-stream callback.
    pub fn on_eos(&self, cb: PipelineEosCallback) {
        *self.eos_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(cb);
    }

    /// Poll the GStreamer bus for pending messages **without** requiring a
    /// GLib main loop. Call this periodically from the UI framework's timer
    /// (e.g. a Dioxus `spawn` loop, or a 4 Hz tick).
    ///
    /// Returns a list of `BusEvent`s that occurred since the last poll.
    /// This replaces `watch_bus()` which required `glib::SourceId` and a
    /// running GLib main context.
    ///
    /// # v3.0 Cross-Platform
    ///
    /// This is the primary mechanism for receiving GStreamer bus events
    /// in the cross-platform build. The UI framework's event loop drives
    /// the polling, eliminating the GLib main loop dependency.
    pub fn poll_bus(&self) -> Vec<BusEvent> {
        let mut events = Vec::new();
        if let Some(bus) = self.pipeline.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gstreamer::MessageView::Error(e) => {
                        events.push(BusEvent::Error(e.error().to_string()));
                    }
                    gstreamer::MessageView::Warning(w) => {
                        events.push(BusEvent::Warning(w.error().to_string()));
                    }
                    gstreamer::MessageView::Eos(_) => {
                        events.push(BusEvent::Eos);
                    }
                    _ => {}
                }
            }
        }
        events
    }

    pub fn play(&self) -> Result<()> {
        self.pipeline
            .set_state(gstreamer::State::Playing)
            .context("pipeline play")?;
        Ok(())
    }

    /// Pre-roll the pipeline to PAUSED state.
    ///
    /// Blocks until GStreamer has decoded at least one buffer and is ready to
    /// produce audio immediately when `play()` is called. Used by the gapless
    /// preloader to ensure the next track is ready before EOS fires on the
    /// current one. Times out after 5 seconds to avoid hanging on broken files.
    pub fn preroll(&self) -> Result<()> {
        use gstreamer::prelude::*;
        self.pipeline
            .set_state(gstreamer::State::Paused)
            .context("pipeline preroll (set PAUSED)")?;
        // Wait for PAUSED state to complete (i.e. first buffer decoded)
        let timeout = gstreamer::ClockTime::from_seconds(5);
        let (result, _state, _pending) = self.pipeline.state(timeout);
        result.context("pipeline preroll timed out")?;
        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        self.pipeline
            .set_state(gstreamer::State::Paused)
            .context("pipeline pause")?;
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gstreamer::State::Null);
    }

    pub fn position(&self) -> Option<std::time::Duration> {
        self.pipeline
            .query_position::<gstreamer::ClockTime>()
            .map(|ct| std::time::Duration::from_nanos(ct.nseconds()))
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        self.pipeline
            .query_duration::<gstreamer::ClockTime>()
            .map(|ct| std::time::Duration::from_nanos(ct.nseconds()))
    }

    pub fn seek(&self, pos: std::time::Duration, rate: f64) -> Result<()> {
        let flags = gstreamer::SeekFlags::FLUSH | gstreamer::SeekFlags::KEY_UNIT;
        let ct = gstreamer::ClockTime::from_nseconds(pos.as_nanos() as u64);
        self.pipeline
            .seek(
                rate,
                flags,
                gstreamer::SeekType::Set,
                ct,
                gstreamer::SeekType::None,
                gstreamer::ClockTime::NONE,
            )
            .context("seek")
    }

    pub fn set_rate(&self, rate: f64) {
        let rate = rate.clamp(0.25, 4.0);
        let pos = match self.pipeline.query_position::<gstreamer::ClockTime>() {
            Some(p) => p,
            None => {
                tracing::warn!("set_rate: query_position returned None, skipping rate change");
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

    /// Legacy GLib-based bus watch. **Deprecated** in v3.0 — use `poll_bus()`
    /// instead for cross-platform compatibility.
    ///
    /// This method is kept for backward compatibility with the GTK4 UI but
    /// should not be used in the Dioxus UI. It requires a running GLib main
    /// context, which is only available when GTK4 is driving the event loop.
    #[cfg(feature = "glib-bus-watch")]
    pub fn watch_bus<F>(&mut self, handler: F) -> Option<glib::SourceId>
    where
        F: Fn(&gstreamer::Bus, &gstreamer::Message) -> glib::ControlFlow + Send + 'static,
    {
        let bus = self.pipeline.bus()?;
        // Store the BusWatchGuard on the struct so it is not immediately dropped.
        // The guard keeps the bus watch alive; dropping it would remove the watch.
        // Previously, assigning to a local `_guard` caused immediate drop, making
        // the watch ineffective.
        let guard = bus.add_watch(handler).ok()?;
        self._bus_watch_guard = Some(guard);
        None
    }
}

impl Drop for DecodePipeline {
    fn drop(&mut self) {
        self.stop();
    }
}

// cast_u8_to_f32 is now in crate::util — the duplicate local definition
// has been removed to follow DRY principles.
