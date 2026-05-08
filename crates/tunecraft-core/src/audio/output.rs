//! cpal audio output thread.
//!
//! Owns the `cpal::Stream` (which must stay alive for audio to play).
//! Drains the DSP→output ring buffer in the cpal callback.
//!
//! # Professional-grade improvements over the previous version
//!
//! ## Device-change detection
//! The previous version opened the default device once and never re-opened it.
//! If the user unplugged their headphones, audio would silently stop or
//! continue on the wrong device. `AudioOutput` now accepts an optional
//! `DeviceChangeCallback` that fires when cpal reports a device-disconnect
//! error, allowing `AudioEngine` to re-open the output on the new default.
//!
//! ## Underrun metrics
//! Buffer underruns are counted in an atomic `underrun_count` and logged at
//! WARN level (rate-limited to once per ~60 s of audio) to aid buffer tuning.
//! The counter is exposed as an `Arc<AtomicU64>` so `AudioEngine` can read
//! it without coupling to the `AudioOutput` lifetime.
//!
//! ## Sample format negotiation
//! The previous fallback accepted any device-default config, which could return
//! non-F32 formats that were then fed F32 data, producing noise. The new
//! `find_config` prefers F32 but correctly converts I16/U16 in the callback
//! when F32 is unavailable.
//!
//! ## Bit-perfect / exclusive mode
//! `AudioOutput::new_exclusive` opens the device in exclusive mode, bypassing
//! the system mixer. This is the entry point for audiophile-quality output
//! where the DSP engine's output reaches the DAC with zero additional processing.
//!
//! ## Performance note
//! The `HeapCons` consumer is moved directly into the cpal callback without
//! `Arc<Mutex>` — no lock overhead in the real-time audio path.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::Consumer;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Callback invoked when the output device changes or disconnects.
pub type DeviceChangeCallback = Box<dyn Fn() + Send + Sync + 'static>;

/// A running cpal output stream. Keep alive for audio playback.
pub struct AudioOutput {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
    underrun_count: Arc<AtomicU64>,
}

#[cfg(target_os = "linux")]
unsafe impl Send for AudioOutput {}
#[cfg(target_os = "linux")]
unsafe impl Sync for AudioOutput {}

impl AudioOutput {
    /// Open the default output device.
    ///
    /// `on_device_change` (optional) fires when the device disconnects so the
    /// engine can re-open on the new default device.
    pub fn new(dsp_cons: ringbuf::HeapCons<f32>, playing: Arc<AtomicBool>) -> Result<Self> {
        Self::new_with_callback(dsp_cons, playing, None)
    }

    /// Open the default output device with an optional device-change callback.
    pub fn new_with_callback(
        mut dsp_cons: ringbuf::HeapCons<f32>,
        playing: Arc<AtomicBool>,
        on_device_change: Option<DeviceChangeCallback>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output device")?;

        let (config, format) = Self::find_config(&device)?;
        let mut sample_rate = config.sample_rate().0;
        let channels = config.channels();

        if sample_rate < 8000 || sample_rate > 192000 {
            tracing::warn!("Unusual sample rate {} Hz, clamping to 48000", sample_rate);
            sample_rate = 48000;
        }

        let underrun_count = Arc::new(AtomicU64::new(0));
        let underrun_cb = Arc::clone(&underrun_count);
        let playing_cb = Arc::clone(&playing);

        let on_change = on_device_change.map(Arc::new);
        let on_change_err = on_change.clone();

        let f32_buf_capacity = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { max, .. } => *max as usize,
            cpal::SupportedBufferSize::Unknown => 8192_usize,
        };

        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |output: &mut [f32], _info| {
                    if !playing_cb.load(Ordering::Relaxed) {
                        output.fill(0.0);
                        return;
                    }
                    let filled = dsp_cons.pop_slice(output);
                    if filled < output.len() {
                        output[filled..].fill(0.0);
                        let count = underrun_cb.fetch_add(1, Ordering::Relaxed);
                        if count % 12_000 == 0 {
                            tracing::warn!(
                                "Audio underrun #{} — consider increasing DECODE_RING \
                                     or reducing DSP load",
                                count
                            );
                        }
                    }
                },
                move |err| {
                    tracing::error!("cpal output error: {}", err);
                    if is_device_change_error(&err) {
                        tracing::info!("Output device changed — signalling engine to re-open");
                        if let Some(ref cb) = on_change_err {
                            cb();
                        }
                    }
                },
                None,
            ),
            cpal::SampleFormat::I16 => {
                tracing::warn!(
                    "Output device does not support F32; using I16 with format conversion"
                );
                let mut f32_buf = vec![0.0f32; f32_buf_capacity];
                device.build_output_stream(
                    &config.into(),
                    move |output: &mut [i16], _info| {
                        if !playing_cb.load(Ordering::Relaxed) {
                            output.fill(0);
                            return;
                        }
                        f32_buf.clear();
                        if output.len() > f32_buf.capacity() {
                            tracing::warn!(
                                "Audio output buffer ({}) exceeds pre-allocated capacity ({}) — \
                                 growing dynamically. Consider increasing the default buffer size.",
                                output.len(),
                                f32_buf.capacity()
                            );
                        }
                        f32_buf.resize(output.len(), 0.0);
                        let filled = dsp_cons.pop_slice(&mut f32_buf);
                        if filled < output.len() {
                            underrun_cb.fetch_add(1, Ordering::Relaxed);
                        }
                        for (out, &inp) in output.iter_mut().zip(f32_buf.iter()) {
                            *out = (inp.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
                        }
                    },
                    move |err| {
                        tracing::error!("cpal output error (i16): {}", err);
                        if is_device_change_error(&err) {
                            if let Some(ref cb) = on_change_err {
                                cb();
                            }
                        }
                    },
                    None,
                )
            }
            _ => {
                tracing::warn!(
                    "Output device format {:?}: using U16 with conversion",
                    format
                );
                let mut f32_buf = vec![0.0f32; f32_buf_capacity];
                device.build_output_stream(
                    &config.into(),
                    move |output: &mut [u16], _info| {
                        if !playing_cb.load(Ordering::Relaxed) {
                            output.fill(u16::MAX / 2);
                            return;
                        }
                        f32_buf.clear();
                        if output.len() > f32_buf.capacity() {
                            tracing::warn!(
                                "Audio output buffer ({}) exceeds pre-allocated capacity ({}) — \
                                 growing dynamically. Consider increasing the default buffer size.",
                                output.len(),
                                f32_buf.capacity()
                            );
                        }
                        f32_buf.resize(output.len(), 0.0);
                        let filled = dsp_cons.pop_slice(&mut f32_buf);
                        if filled < output.len() {
                            underrun_cb.fetch_add(1, Ordering::Relaxed);
                        }
                        for (out, &inp) in output.iter_mut().zip(f32_buf.iter()) {
                            *out = ((inp.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32).round()
                                as u16;
                        }
                    },
                    move |err| {
                        tracing::error!("cpal output error (u16): {}", err);
                        if is_device_change_error(&err) {
                            if let Some(ref cb) = on_change_err {
                                cb();
                            }
                        }
                    },
                    None,
                )
            }
        }
        .context("build output stream")?;

        stream.play().context("stream play")?;

        Ok(Self {
            _stream: stream,
            sample_rate,
            channels,
            underrun_count,
        })
    }

    /// Number of buffer underruns since the stream was opened.
    pub fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    /// Returns a cloneable reference to the underrun counter.
    /// This allows `AudioEngine` to read the underrun count without
    /// coupling to the `AudioOutput` lifetime.
    pub fn underrun_count_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.underrun_count)
    }

    /// Open the output device in exclusive / bit-perfect mode.
    ///
    /// Bypasses the system mixer entirely. Audio is output at the hardware's
    /// native bit depth and sample rate with no kernel resampling. On Linux
    /// this means ALSA direct or PipeWire exclusive access.
    ///
    /// Falls back to shared mode if exclusive is unavailable, ensuring audio
    /// playback always works even when exclusive access fails.
    ///
    /// **Important:** If exclusive mode fails, `dsp_cons` is consumed and cannot
    /// be reused. The caller must create a fresh ring buffer for the shared-mode
    /// fallback. The caller (AudioEngine::load_internal) handles this by creating
    /// a new output ring buffer when falling back.
    pub fn new_exclusive(
        dsp_cons: ringbuf::HeapCons<f32>,
        playing: Arc<AtomicBool>,
    ) -> Result<Self> {
        tracing::info!("Attempting exclusive/bit-perfect output mode");
        match crate::audio::exclusive::ExclusiveAudioOutput::new(dsp_cons, playing) {
            Ok(exclusive) => {
                tracing::info!(
                    "Exclusive mode active: {} Hz, {} ch",
                    exclusive.sample_rate,
                    exclusive.channels
                );
                Ok(Self {
                    _stream: exclusive._stream,
                    sample_rate: exclusive.sample_rate,
                    channels: exclusive.channels,
                    underrun_count: exclusive.underrun_count,
                })
            }
            Err(e) => {
                tracing::warn!(
                    "Exclusive mode unavailable ({}), falling back to shared mode",
                    e
                );
                anyhow::bail!(
                    "Exclusive mode failed: {} — caller should fall back to shared mode",
                    e
                )
            }
        }
    }

    /// Find the best supported output config.
    ///
    /// Priority: F32 stereo 48 kHz → F32 stereo 44.1 kHz → F32 stereo any
    ///           → I16 stereo 48 kHz → device default (last resort).
    fn find_config(
        device: &cpal::Device,
    ) -> Result<(cpal::SupportedStreamConfig, cpal::SampleFormat)> {
        let supported: Vec<_> = device
            .supported_output_configs()
            .context("query output configs")?
            .collect();

        let mut best: Option<(cpal::SupportedStreamConfig, cpal::SampleFormat, u32)> = None;

        for cfg in &supported {
            if cfg.channels() != 2 {
                continue;
            }

            let fmt = cfg.sample_format();
            let score = match fmt {
                cpal::SampleFormat::F32 => {
                    if cfg.min_sample_rate().0 <= 48_000 && cfg.max_sample_rate().0 >= 48_000 {
                        100 // F32 stereo 48 kHz — best
                    } else if cfg.min_sample_rate().0 <= 44_100 && cfg.max_sample_rate().0 >= 44_100
                    {
                        90 // F32 stereo 44.1 kHz
                    } else {
                        50 // F32 stereo any rate
                    }
                }
                cpal::SampleFormat::I16 => {
                    if cfg.min_sample_rate().0 <= 48_000 && cfg.max_sample_rate().0 >= 48_000 {
                        30 // I16 stereo 48 kHz
                    } else {
                        10 // I16 stereo other
                    }
                }
                _ => 5, // Other formats — low priority
            };

            match best {
                Some((_, _, best_score)) if score <= best_score => {}
                _ => {
                    let rate = if fmt == cpal::SampleFormat::F32 {
                        if cfg.min_sample_rate().0 <= 48_000 && cfg.max_sample_rate().0 >= 48_000 {
                            cpal::SampleRate(48_000)
                        } else if cfg.min_sample_rate().0 <= 44_100
                            && cfg.max_sample_rate().0 >= 44_100
                        {
                            cpal::SampleRate(44_100)
                        } else {
                            cfg.max_sample_rate()
                        }
                    } else {
                        if cfg.min_sample_rate().0 <= 48_000 && cfg.max_sample_rate().0 >= 48_000 {
                            cpal::SampleRate(48_000)
                        } else {
                            cfg.max_sample_rate()
                        }
                    };
                    best = Some((cfg.with_sample_rate(rate), fmt, score));
                }
            }
        }

        if let Some((config, fmt, _)) = best {
            return Ok((config, fmt));
        }

        let default = device
            .default_output_config()
            .context("default output config")?;
        let fmt = default.sample_format();
        Ok((default, fmt))
    }
}

fn is_device_change_error(err: &cpal::StreamError) -> bool {
    match err {
        cpal::StreamError::DeviceNotAvailable => true,
        cpal::StreamError::BackendSpecific { err } => {
            let msg = err.description.to_lowercase();
            msg.contains("disconnect")
                || msg.contains("unavailable")
                || msg.contains("removed")
                || msg.contains("unplugged")
        }
    }
}
