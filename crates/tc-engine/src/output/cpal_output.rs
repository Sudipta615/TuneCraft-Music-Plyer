//! Audio output using cpal
//!
//! The output callback is designed to be zero-allocation, zero-blocking.
//!
//! v0.21.0: No changes to this module, but see engine.rs for the fix that
//! ensures the CPAL output stream is paused before buffer reset in the
//! Stop command handler (matching the safety pattern in load_track).

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, Stream, StreamConfig,
};
use tc_config::AudioBackend;
use thiserror::Error;

use crate::buffer::FixedFrameBuffer;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("No audio device available")]
    NoDevice,
    #[error("Failed to open stream: {0}")]
    StreamOpen(String),
    #[error("Unsupported sample format")]
    UnsupportedFormat,
    #[error("Buffer underrun")]
    Underrun,
    #[error("Stream error: {0}")]
    StreamError(String),
}

/// A thread-safe wrapper around CPAL's Stream.
///
/// CPAL's Stream does not implement Send/Sync by default to remain compatible
/// with some platforms (like Web/Emscripten). Since we only target desktop
/// platforms where it is safe to send streams across threads, we wrap it in
/// an unsafe Send + Sync implementation.
pub struct SendSyncStream(pub Stream);
unsafe impl Send for SendSyncStream {}
unsafe impl Sync for SendSyncStream {}

/// Audio output using cpal
pub struct CpalOutput {
    stream: Option<SendSyncStream>,
    device: Device,
    /// Resolved stream config (sample rate, channels, buffer size)
    stream_config: StreamConfig,
    /// Sample format for the output stream
    sample_format: SampleFormat,
    /// Shared buffer between DSP thread and output callback
    buffer: Arc<FixedFrameBuffer>,
    /// Flag to pause output
    paused: Arc<AtomicBool>,
    /// Flag indicating if the audio thread is inside the callback
    in_callback: Arc<AtomicBool>,
    /// Underrun counter
    underruns: Arc<AtomicU32>,
    /// Sample rate of the output device
    actual_sample_rate: u32,
    /// Flag indicating that a stream error has occurred and recovery
    /// may be needed.
    stream_error: Arc<AtomicBool>,
}

impl CpalOutput {
    /// Create a new cpal output
    pub fn new(buffer: Arc<FixedFrameBuffer>, backend: AudioBackend) -> Result<Self, OutputError> {
        let host = match backend {
            #[cfg(target_os = "linux")]
            AudioBackend::ExclusiveAlsa => {
                log::info!("Audio output: Requesting exclusive ALSA host");
                cpal::host_from_id(cpal::HostId::Alsa).unwrap_or_else(|_| cpal::default_host())
            },
            #[cfg(target_os = "windows")]
            AudioBackend::ExclusiveAsio => {
                log::info!("Audio output: Requesting exclusive ASIO host");
                cpal::host_from_id(cpal::HostId::Asio).unwrap_or_else(|_| cpal::default_host())
            },
            #[cfg(target_os = "macos")]
            AudioBackend::ExclusiveCoreAudioHog => {
                log::info!("Audio output: Requesting CoreAudio Hog Mode");
                cpal::default_host() // CoreAudio is the default on macOS
            },
            _ => cpal::default_host(),
        };

        // If ALSA was requested, try to find a hardware device rather than 'default'
        let mut device = None;
        if backend == AudioBackend::ExclusiveAlsa {
            #[cfg(target_os = "linux")]
            if let Ok(devices) = host.output_devices() {
                let mut valid_devices: Vec<_> = devices
                    .filter(|d| {
                        let name = d.name().unwrap_or_default().to_lowercase();
                        name != "default"
                            && !name.starts_with("sysdefault")
                            && !name.contains("pulse")
                            && !name.contains("pipewire")
                            && !name.contains("dmix")
                    })
                    .collect();

                valid_devices.sort_by_key(|d| {
                    if d.name()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains("analog")
                    {
                        0
                    } else {
                        1
                    }
                });

                if let Some(hw_dev) = valid_devices.into_iter().next() {
                    log::info!(
                        "Audio output: Selected exclusive hardware device: {}",
                        hw_dev.name().unwrap_or_default()
                    );
                    device = Some(hw_dev);
                }
            }
        }

        let device = device
            .or_else(|| host.default_output_device())
            .ok_or(OutputError::NoDevice)?;

        // Use the device's default config instead of max-sample-rate.
        let default_config = device
            .default_output_config()
            .map_err(|e| OutputError::StreamOpen(format!("Cannot get default config: {}", e)))?;

        let target_sample_rate = default_config.sample_rate().0;

        let supported = device
            .supported_output_configs()
            .map_err(|e| OutputError::StreamOpen(format!("Cannot query configs: {}", e)))?;
        let supported_configs: Vec<_> = supported.collect();

        let config = supported_configs
            .iter()
            .find(|c| {
                c.sample_format() == SampleFormat::F32
                    && c.min_sample_rate().0 <= target_sample_rate
                    && c.max_sample_rate().0 >= target_sample_rate
            })
            .map(|c| c.with_sample_rate(cpal::SampleRate(target_sample_rate)))
            .or_else(|| {
                supported_configs
                    .iter()
                    .find(|c| c.sample_format() == SampleFormat::F32)
                    .map(|c| {
                        let rate =
                            target_sample_rate.clamp(c.min_sample_rate().0, c.max_sample_rate().0);
                        c.with_sample_rate(cpal::SampleRate(rate))
                    })
            })
            .or_else(|| {
                supported_configs
                    .iter()
                    .find(|c| {
                        (c.sample_format() == SampleFormat::I16
                            || c.sample_format() == SampleFormat::U16)
                            && c.min_sample_rate().0 <= target_sample_rate
                            && c.max_sample_rate().0 >= target_sample_rate
                    })
                    .map(|c| c.with_sample_rate(cpal::SampleRate(target_sample_rate)))
            })
            .or_else(|| {
                supported_configs
                    .iter()
                    .find(|c| {
                        c.sample_format() == SampleFormat::I16
                            || c.sample_format() == SampleFormat::U16
                    })
                    .map(|c| {
                        let rate =
                            target_sample_rate.clamp(c.min_sample_rate().0, c.max_sample_rate().0);
                        c.with_sample_rate(cpal::SampleRate(rate))
                    })
            })
            .ok_or(OutputError::UnsupportedFormat)?;

        let actual_sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let sample_format = config.sample_format();

        let buffer_size = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                cpal::BufferSize::Fixed(2048.clamp(*min, *max))
            }
            cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Fixed(2048),
        };

        let stream_config = StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(actual_sample_rate),
            buffer_size,
        };

        log::info!(
            "Audio output: {} Hz, {} ch, {:?}, buffer size: {:?}",
            actual_sample_rate,
            channels,
            sample_format,
            buffer_size
        );

        Ok(Self {
            stream: None,
            device,
            stream_config,
            sample_format,
            buffer,
            paused: Arc::new(AtomicBool::new(false)),
            in_callback: Arc::new(AtomicBool::new(false)),
            underruns: Arc::new(AtomicU32::new(0)),
            actual_sample_rate,
            stream_error: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the audio output stream
    pub fn start(&mut self) -> Result<(), OutputError> {
        let buffer = Arc::clone(&self.buffer);
        let paused = Arc::clone(&self.paused);
        let in_callback = Arc::clone(&self.in_callback);
        let underruns = Arc::clone(&self.underruns);
        let stream_error = Arc::clone(&self.stream_error);
        let channels = self.stream_config.channels as usize;

        // Error callback: instead of just logging, set the stream_error
        // flag so the engine can detect device disconnections and attempt
        // recovery. Common errors include device removal (USB unplug),
        // Bluetooth disconnection, and sample rate changes when the OS
        // switches the default audio device.
        let error_callback = move |err: cpal::StreamError| {
            log::error!("Audio output error: {}", err);
            stream_error.store(true, Ordering::Release);
        };

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                let in_callback = Arc::clone(&in_callback);
                self.device
                    .build_output_stream(
                        &self.stream_config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            Self::audio_callback(
                                data,
                                &buffer,
                                &paused,
                                &in_callback,
                                &underruns,
                                channels,
                            );
                        },
                        error_callback,
                        None,
                    )
                    .map_err(|e| OutputError::StreamOpen(format!("{}", e)))?
            },
            SampleFormat::I16 => {
                let in_callback = Arc::clone(&in_callback);
                self.device
                    .build_output_stream(
                        &self.stream_config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            Self::audio_callback_i16(
                                data,
                                &buffer,
                                &paused,
                                &in_callback,
                                &underruns,
                                channels,
                            );
                        },
                        error_callback,
                        None,
                    )
                    .map_err(|e| OutputError::StreamOpen(format!("{}", e)))?
            },
            SampleFormat::U16 => {
                let in_callback = Arc::clone(&in_callback);
                self.device
                    .build_output_stream(
                        &self.stream_config,
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            Self::audio_callback_u16(
                                data,
                                &buffer,
                                &paused,
                                &in_callback,
                                &underruns,
                                channels,
                            );
                        },
                        error_callback,
                        None,
                    )
                    .map_err(|e| OutputError::StreamOpen(format!("{}", e)))?
            },
            _ => {
                return Err(OutputError::UnsupportedFormat);
            },
        };

        stream
            .play()
            .map_err(|e| OutputError::StreamOpen(format!("Play failed: {}", e)))?;
        self.stream = Some(SendSyncStream(stream));

        // v0.20.0: Escalate the audio callback thread to real-time priority.
        // This prevents audio dropouts under heavy CPU contention (e.g., during
        // library scans with FTS indexing, or other I/O-intensive operations).
        //
        // On Linux, this requires either:
        //   - rtkit permissions (typically granted via PAM configuration)
        //   - `ulimit -r` adjustments (e.g., `ulimit -r 9` for 90% of CPU time)
        //   - Running with CAP_SYS_NICE capability
        // These requirements should be documented in packaging specs (.deb / .spec).
        //
        // If priority escalation fails, we log a warning but continue — the
        // audio will still work, just with default scheduling priority.
        #[cfg(feature = "thread-priority")]
        {
            use thread_priority::*;
            match set_current_thread_priority(ThreadPriority::Max) {
                Ok(()) => {
                    log::info!(
                        "Audio thread escalated to real-time priority (ThreadPriority::Max)"
                    );
                },
                Err(e) => {
                    log::warn!(
                        "Failed to set real-time priority for audio thread: {}. \
                         Audio will continue with default scheduling. \
                         On Linux, ensure rtkit permissions or ulimit -r is configured.",
                        e
                    );
                },
            }
        }

        log::info!("Audio output stream started successfully");
        Ok(())
    }

    /// Audio callback for F32 output - ZERO ALLOCATION, ZERO BLOCKING
    #[inline]
    fn audio_callback(
        data: &mut [f32],
        buffer: &FixedFrameBuffer,
        paused: &AtomicBool,
        in_callback: &AtomicBool,
        underruns: &AtomicU32,
        channels: usize,
    ) {
        let _guard = CallbackGuard::new(in_callback);
        if paused.load(Ordering::SeqCst) {
            data.fill(0.0);
            return;
        }

        for frame in data.chunks_mut(channels) {
            match buffer.pop() {
                Some(audio_frame) => {
                    for (ch, sample) in frame.iter_mut().enumerate() {
                        *sample = if ch < audio_frame.num_channels as usize {
                            audio_frame.channels[ch]
                        } else {
                            0.0
                        };
                    }
                },
                None => {
                    frame.fill(0.0);
                    underruns.fetch_add(1, Ordering::SeqCst);
                },
            }
        }
    }

    /// Audio callback for I16 output
    #[inline]
    fn audio_callback_i16(
        data: &mut [i16],
        buffer: &FixedFrameBuffer,
        paused: &AtomicBool,
        in_callback: &AtomicBool,
        underruns: &AtomicU32,
        channels: usize,
    ) {
        let _guard = CallbackGuard::new(in_callback);
        if paused.load(Ordering::SeqCst) {
            data.fill(0);
            return;
        }

        for frame in data.chunks_mut(channels) {
            match buffer.pop() {
                Some(audio_frame) => {
                    for (ch, sample) in frame.iter_mut().enumerate() {
                        let val = if ch < audio_frame.num_channels as usize {
                            audio_frame.channels[ch]
                        } else {
                            0.0
                        };
                        *sample = (val * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    }
                },
                None => {
                    frame.fill(0);
                    underruns.fetch_add(1, Ordering::SeqCst);
                },
            }
        }
    }

    /// Audio callback for U16 output
    #[inline]
    fn audio_callback_u16(
        data: &mut [u16],
        buffer: &FixedFrameBuffer,
        paused: &AtomicBool,
        in_callback: &AtomicBool,
        underruns: &AtomicU32,
        channels: usize,
    ) {
        let _guard = CallbackGuard::new(in_callback);
        if paused.load(Ordering::SeqCst) {
            data.fill(32768);
            return;
        }

        for frame in data.chunks_mut(channels) {
            match buffer.pop() {
                Some(audio_frame) => {
                    for (ch, sample) in frame.iter_mut().enumerate() {
                        let val = if ch < audio_frame.num_channels as usize {
                            audio_frame.channels[ch]
                        } else {
                            0.0
                        };
                        *sample = (val * 32767.0 + 32768.0).clamp(0.0, 65535.0) as u16;
                    }
                },
                None => {
                    frame.fill(32768);
                    underruns.fetch_add(1, Ordering::SeqCst);
                },
            }
        }
    }

    /// Pause the output
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        while self.in_callback.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
    }

    /// Resume the output
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Reset the output buffer safely by pausing playback first
    pub fn reset_buffer(&self) {
        self.pause();
        unsafe {
            self.buffer.reset();
        }
        self.resume();
    }

    /// Get the number of underruns since last check
    pub fn take_underruns(&self) -> u32 {
        self.underruns.swap(0, Ordering::Relaxed)
    }

    /// Get the actual sample rate
    pub fn sample_rate(&self) -> u32 {
        self.actual_sample_rate
    }

    /// Check if a stream error has been reported (e.g., device disconnection).
    /// The error flag is cleared after reading.
    pub fn take_stream_error(&self) -> bool {
        self.stream_error.swap(false, Ordering::AcqRel)
    }

    /// Stop the output stream
    pub fn stop(&mut self) {
        self.stream = None;
    }

    /// Get the current device name for diagnostic purposes.
    pub fn device_name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "unknown".to_string())
    }
}

struct CallbackGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> CallbackGuard<'a> {
    fn new(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self { flag }
    }
}

impl<'a> Drop for CallbackGuard<'a> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}
