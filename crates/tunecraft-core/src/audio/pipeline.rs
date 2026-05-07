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
    _bus_watch_guard: Option<gstreamer::bus::BusWatchGuard>,
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

        let uridecodebin = gstreamer::ElementFactory::make("uridecodebin")
            .build()
            .context("uridecodebin")?;
        uridecodebin.set_property("uri", uri);
        uridecodebin.set_property("buffer-duration", 200_000_000i64);

        let audioconvert = gstreamer::ElementFactory::make("audioconvert")
            .build()
            .context("audioconvert")?;

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
        appsink_elem.set_property("sync", true);
        appsink_elem.set_property("max-buffers", 2u32);
        appsink_elem.set_property("drop", false);

        let appsink: gstreamer_app::AppSink = appsink_elem
            .dynamic_cast()
            .map_err(|_| anyhow::anyhow!("appsink dynamic_cast failed"))?;

        pipeline.add_many(&[
            &uridecodebin,
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;

        gstreamer::Element::link_many(&[
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;

        let audioconvert_weak = audioconvert.downgrade();
        uridecodebin.connect_pad_added(move |_src, src_pad| {
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

        let eos_cb: Arc<Mutex<Option<PipelineEosCallback>>> = Arc::new(Mutex::new(None));
        let eos_cb_appsink = Arc::clone(&eos_cb);

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
                    return Err(gstreamer::FlowError::Error);
                };

                let mut remaining = samples;
                let mut yield_count = 0u32;
                while !remaining.is_empty() {
                    let n = decode_prod_cb.push_slice(remaining);
                    remaining = &remaining[n..];
                    if !remaining.is_empty() {
                        yield_count += 1;
                        if yield_count > 1000 {
                            tracing::warn!(
                                "Decode ring persistently full after {} yields, \
                                 dropping {} samples",
                                yield_count,
                                remaining.len()
                            );
                            break;
                        }
                        if yield_count % 100 == 0 {
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
}

impl Drop for DecodePipeline {
    fn drop(&mut self) {
        self.stop();
    }
}
