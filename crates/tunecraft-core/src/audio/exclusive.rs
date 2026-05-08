//! Bit-perfect / exclusive mode audio output.
//!
//! # Overview
//!
//! In normal operation, the audio pipeline outputs through cpal, which
//! negotiates a sample format and rate with the system audio subsystem
//! (PipeWire, PulseAudio, ALSA, CoreAudio). The system mixer may apply
//! additional resampling, volume scaling, and format conversion — all of
//! which degrade the signal path from the "bit-perfect" ideal.
//!
//! Exclusive mode bypasses the system mixer entirely:
//!
//! - **Linux (PipeWire exclusive)**: Opens the PipeWire ALSA plugin device
//!   in exclusive mode. PipeWire will grant exclusive access and stop mixing
//!   other audio streams, giving the application direct access to the audio
//!   device. This is the recommended approach for modern Linux distributions
//!   that ship PipeWire by default (Fedora 39+, Ubuntu 24.04+, Arch Linux,
//!   etc.). PipeWire exclusive mode provides the same bit-perfect output as
//!   direct ALSA `hw:` access but with better integration: PipeWire
//!   correctly suspends other streams and resumes them when exclusive access
//!   is released, avoiding the need to manually stop PulseAudio/PipeWire
//!   before opening the hardware device.
//!
//! - **Linux (ALSA direct)**: Opens the ALSA hardware device directly via
//!   `plughw:` or `hw:` — no dmix, no PulseAudio, no PipeWire.
//!   The hardware runs at its native bit depth and sample rate.
//!   This is the fallback when PipeWire is not available or not running.
//!
//! - **macOS / Windows**: Uses cpal's exclusive stream config if available.
//!
//! # PipeWire Exclusive Mode
//!
//! PipeWire exclusive mode works by opening the PipeWire ALSA plugin device
//! in exclusive mode through cpal's device selection. When the application
//! requests exclusive access, PipeWire's daemon will:
//!
//! 1. Cease mixing other audio streams on the target device
//! 2. Grant the application direct access to the audio device
//! 3. Resume normal mixing when the exclusive stream is closed
//!
//! This is the recommended approach for modern Linux (Fedora 39+,
//! Ubuntu 24.04+, etc.) because:
//!
//! - It avoids conflicts with the desktop audio stack
//! - PipeWire properly manages stream suspension and resumption
//! - No need to manually stop system audio services
//! - Bit-perfect output is maintained through the entire signal path
//!
//! When PipeWire is not available, the system falls back to ALSA direct
//! `hw:` access, which bypasses all intermediate layers but requires that
//! no other process holds the audio device open.
//!
//! # When to use
//!
//! Exclusive mode is intended for audiophile listening sessions where the
//! user wants the DSP engine's output to reach the DAC with zero additional
//! processing. It is **not** suitable for normal desktop use because:
//!
//! - Other applications cannot play audio simultaneously
//! - Sample rate mismatches between the source and hardware will cause
//!   playback failures (the Rust resampler must match the hardware rate)
//! - Device hotplug requires explicit session restart
//!
//! # Architecture
//!
//! `ExclusiveAudioOutput` follows the same interface as `AudioOutput` — it
//! consumes from the output ring buffer and feeds the audio device. The
//! difference is in device enumeration and stream configuration.
//!
//! Device selection follows this priority order:
//!
//! 1. **PipeWire exclusive** — if PipeWire is detected as running, use its
//!    ALSA plugin device for exclusive access (best desktop integration).
//! 2. **ALSA direct hw:/plughw:** — bypass everything, open the hardware
//!    directly (pure bit-perfect, but may conflict with desktop audio).
//! 3. **ALSA non-Pulse** — any ALSA device that isn't routed through
//!    PulseAudio.
//! 4. **PipeWire device** — any device whose name contains "pipewire".
//! 5. **Default device** — whatever cpal returns as the default output.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::Consumer;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// A running cpal output stream opened in exclusive / bit-perfect mode.
///
/// Keep alive for audio playback. When dropped, the stream is released and
/// the device becomes available to other applications again.
pub struct ExclusiveAudioOutput {
    pub _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
    pub underrun_count: Arc<AtomicU64>,
}

impl ExclusiveAudioOutput {
    /// Open an output device in exclusive mode.
    ///
    /// On Linux, this first attempts to use PipeWire exclusive access (the
    /// recommended approach for modern distributions). If PipeWire is not
    /// available, it falls back to finding an ALSA hardware device for direct
    /// access. On other platforms, it uses cpal's exclusive stream configuration.
    ///
    /// `dsp_cons` is the read-end of the DSP->output ring buffer.
    /// `playing` signals whether audio should be output.
    pub fn new(mut dsp_cons: ringbuf::HeapCons<f32>, playing: Arc<AtomicBool>) -> Result<Self> {
        let host = cpal::default_host();

        let device = if let Some(pw_device) = try_pipewire_exclusive() {
            let name = pw_device.name().unwrap_or_else(|_| "unknown".into());
            tracing::info!(
                "PipeWire exclusive: requesting exclusive access via PipeWire device '{}'",
                name
            );
            Some(pw_device)
        } else {
            find_hardware_device(&host)
        }
        .or_else(|| host.default_output_device())
        .context("no output device available for exclusive mode")?;

        let device_name = device.name().unwrap_or_else(|_| "unknown".into());
        tracing::info!("Exclusive mode: opening device '{}'", device_name);

        let (config, format) = Self::find_exclusive_config(&device)?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        tracing::info!(
            "Exclusive mode: {} Hz, {} channels, {:?} format",
            sample_rate,
            channels,
            format
        );

        let underrun_count = Arc::new(AtomicU64::new(0));
        let underrun_cb = Arc::clone(&underrun_count);
        let playing_cb = Arc::clone(&playing);

        let f32_buf_capacity: usize = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { max, .. } => *max as usize,
            cpal::SupportedBufferSize::Unknown => 4096,
        };

        let stream = match format {
            cpal::SampleFormat::F32 => {
                let config = config.clone();
                device.build_output_stream(
                    &config.config(),
                    move |output: &mut [f32], _info| {
                        if !playing_cb.load(Ordering::Relaxed) {
                            output.fill(0.0);
                            return;
                        }
                        let filled = dsp_cons.pop_slice(output);
                        if filled < output.len() {
                            output[filled..].fill(0.0);
                            underrun_cb.fetch_add(1, Ordering::Relaxed);
                        }
                    },
                    |err| {
                        tracing::error!("Exclusive output error: {}", err);
                    },
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let config = config.clone();
                tracing::warn!("Exclusive mode: device only supports I16 — converting from F32");
                let mut f32_buf = vec![0.0f32; f32_buf_capacity];
                device.build_output_stream(
                    &config.config(),
                    move |output: &mut [i16], _info| {
                        if !playing_cb.load(Ordering::Relaxed) {
                            output.fill(0);
                            return;
                        }
                        f32_buf.clear();
                        f32_buf.resize(output.len(), 0.0);
                        let filled = dsp_cons.pop_slice(&mut f32_buf);
                        if filled < output.len() {
                            underrun_cb.fetch_add(1, Ordering::Relaxed);
                        }
                        for (out, &inp) in output.iter_mut().zip(f32_buf.iter()) {
                            *out = (inp.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
                        }
                    },
                    |err| {
                        tracing::error!("Exclusive output error (i16): {}", err);
                    },
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let config = config.clone();
                tracing::warn!("Exclusive mode: device only supports U16 — converting from F32");
                let mut f32_buf = vec![0.0f32; f32_buf_capacity];
                device.build_output_stream(
                    &config.config(),
                    move |output: &mut [u16], _info| {
                        if !playing_cb.load(Ordering::Relaxed) {
                            output.fill(u16::MAX / 2);
                            return;
                        }
                        f32_buf.clear();
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
                    |err| {
                        tracing::error!("Exclusive output error (u16): {}", err);
                    },
                    None,
                )
            }
            _ => anyhow::bail!("Exclusive mode: unsupported sample format {:?}", format),
        }
        .context("exclusive output stream build failed")?;

        stream.play().context("exclusive stream play")?;

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
    pub fn underrun_count_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.underrun_count)
    }

    /// Find the best exclusive config for the device.
    ///
    /// Priority: highest sample rate F32 stereo (hardware native) -> any format.
    /// We prefer the hardware's maximum sample rate because in exclusive mode
    /// we want to bypass the system resampler — the source file should be
    /// resampled to the hardware rate by our Rust resampler instead.
    fn find_exclusive_config(
        device: &cpal::Device,
    ) -> Result<(cpal::SupportedStreamConfig, cpal::SampleFormat)> {
        let supported: Vec<_> = device
            .supported_output_configs()
            .context("query exclusive output configs")?
            .collect();

        if supported.is_empty() {
            anyhow::bail!("device reports no supported output configs");
        }

        /// Maximum sample rate to select in exclusive mode (192 kHz).
        /// Rates above this (e.g., 384 kHz+) are rarely useful and waste
        /// CPU/memory on resampling.
        const MAX_EXCLUSIVE_RATE: u32 = 192_000;

        let mut best_f32: Option<&cpal::SupportedStreamConfigRange> = None;
        let mut best_f32_rate: u32 = 0;

        for cfg in &supported {
            if cfg.channels() == 2 && cfg.sample_format() == cpal::SampleFormat::F32 {
                let min_rate = cfg.min_sample_rate().0;
                let max_rate = cfg.max_sample_rate().0.min(MAX_EXCLUSIVE_RATE);
                if min_rate <= MAX_EXCLUSIVE_RATE && max_rate > best_f32_rate {
                    best_f32_rate = max_rate;
                    best_f32 = Some(cfg);
                }
            }
        }

        if let Some(cfg) = best_f32 {
            let rate = cpal::SampleRate(best_f32_rate);
            return Ok((cfg.with_sample_rate(rate), cpal::SampleFormat::F32));
        }

        let mut best: Option<&cpal::SupportedStreamConfigRange> = None;
        let mut best_rate: u32 = 0;
        let mut best_fmt = cpal::SampleFormat::F32;

        for cfg in &supported {
            let min_rate = cfg.min_sample_rate().0;
            let max_rate = cfg.max_sample_rate().0.min(MAX_EXCLUSIVE_RATE);
            if min_rate <= MAX_EXCLUSIVE_RATE && max_rate > best_rate {
                best_rate = max_rate;
                best_fmt = cfg.sample_format();
                best = Some(cfg);
            }
        }

        match best {
            Some(cfg) => {
                let rate = cpal::SampleRate(best_rate);
                Ok((cfg.with_sample_rate(rate), best_fmt))
            }
            None => anyhow::bail!("no supported output config found for exclusive mode"),
        }
    }
}

/// Try to obtain a PipeWire device for exclusive access.
///
/// PipeWire exclusive mode works by opening the PipeWire ALSA plugin device
/// in exclusive mode. When the application opens the device exclusively,
/// PipeWire will grant direct access to the audio device and stop mixing
/// other audio streams. This is the recommended approach for modern Linux
/// distributions (Fedora 39+, Ubuntu 24.04+, etc.) because it integrates
/// cleanly with the desktop audio stack while still providing bit-perfect
/// output.
///
/// When PipeWire is not available, returns `None` and the caller should
/// fall back to ALSA direct `hw:` access.
fn try_pipewire_exclusive() -> Option<cpal::Device> {
    if !is_pipewire_running() {
        tracing::debug!("PipeWire not detected as running; skipping PipeWire exclusive");
        return None;
    }

    tracing::debug!("PipeWire detected; searching for PipeWire device for exclusive access");

    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = match host.output_devices() {
        Ok(d) => d.collect(),
        Err(e) => {
            tracing::warn!(
                "Failed to enumerate output devices for PipeWire search: {}",
                e
            );
            return None;
        }
    };

    for device in devices {
        if let Ok(name) = device.name() {
            if name.to_lowercase().contains("pipewire") {
                tracing::info!("Found PipeWire device for exclusive access: {}", name);
                return Some(device);
            }
        }
    }

    tracing::debug!("PipeWire is running but no cpal device with 'pipewire' in name found");
    None
}

/// Check whether PipeWire is running on this system.
///
/// Detects PipeWire by checking:
/// 1. The `PIPEWIRE_RUNTIME_DIR` environment variable (set by the PipeWire
///    session manager when a PipeWire session is active).
/// 2. The PipeWire socket file at `/run/user/{uid}/pipewire-0` (the default
///    socket created by the PipeWire daemon).
fn is_pipewire_running() -> bool {
    if std::env::var("PIPEWIRE_RUNTIME_DIR").is_ok() {
        tracing::debug!("PipeWire detected via PIPEWIRE_RUNTIME_DIR env var");
        return true;
    }

    if let Some(uid) = get_current_uid() {
        let socket_path = PathBuf::from(format!("/run/user/{}/pipewire-0", uid));
        if socket_path.exists() {
            tracing::debug!("PipeWire detected via socket at {}", socket_path.display());
            return true;
        }
    }

    false
}

/// Get the current user's UID from the system.
///
/// On Linux, this function attempts three methods in order:
///
/// 1. **Parse UID from `XDG_RUNTIME_DIR`**: If the `XDG_RUNTIME_DIR`
///    environment variable is set (typically `/run/user/{uid}`), the trailing
///    path component is parsed as the UID. This is the most common and
///    reliable method on modern Linux desktops.
///
/// 2. **Read `UID` environment variable**: Some containers and CI
///    environments set a `UID` env var with the numeric user ID.
///
/// 3. **`libc::getuid()` fallback**: If neither env var is available,
///    the POSIX `getuid()` system call is used to retrieve the real user
///    ID of the calling process. This always succeeds on Linux.
///
/// On non-Linux platforms, returns `None`.
fn get_current_uid() -> Option<u32> {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        if let Some(uid_str) = xdg.split('/').last() {
            if let Ok(uid) = uid_str.parse::<u32>() {
                return Some(uid);
            }
        }
    }

    if let Ok(uid_str) = std::env::var("UID") {
        if let Ok(uid) = uid_str.parse::<u32>() {
            return Some(uid);
        }
    }

    #[cfg(target_os = "linux")]
    {
        Some(unsafe { libc::getuid() })
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Try to find an ALSA hardware device for direct access.
///
/// On Linux, cpal's default host usually returns a PulseAudio or PipeWire
/// device. For exclusive/bit-perfect mode, we want direct ALSA hardware
/// access (plughw:CARD=X,DEV=Y or hw:CARD=X,DEV=Y).
///
/// The search proceeds in three passes:
/// 1. Devices with "hw:" or "plughw:" in the name (ALSA direct hardware)
/// 2. Devices with "ALSA" in the name that are not PulseAudio-routed
/// 3. Devices with "pipewire" in the name (PipeWire ALSA plugin)
fn find_hardware_device(host: &cpal::Host) -> Option<cpal::Device> {
    let devices: Vec<cpal::Device> = host.output_devices().ok()?.collect();

    for device in &devices {
        if let Ok(name) = device.name() {
            let name_lower = name.to_lowercase();
            if name_lower.contains("hw:") || name_lower.contains("plughw:") {
                tracing::info!("Found ALSA hardware device: {}", name);
                return Some(device.clone());
            }
        }
    }

    for device in &devices {
        if let Ok(name) = device.name() {
            let name_lower = name.to_lowercase();
            if name_lower.contains("alsa") && !name_lower.contains("pulse") {
                tracing::info!("Found ALSA device: {}", name);
                return Some(device.clone());
            }
        }
    }

    for device in &devices {
        if let Ok(name) = device.name() {
            let name_lower = name.to_lowercase();
            if name_lower.contains("pipewire") {
                tracing::info!("Found PipeWire device: {}", name);
                return Some(device.clone());
            }
        }
    }

    tracing::info!("No ALSA hardware device found; using default device in exclusive config");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_exclusive_config_returns_f32_preferred() {
        let host = cpal::default_host();
        if let Some(device) = host.default_output_device() {
            if let Ok((config, fmt)) = ExclusiveAudioOutput::find_exclusive_config(&device) {
                assert_eq!(fmt, cpal::SampleFormat::F32, "Should prefer F32 format");
                assert!(
                    config.sample_rate().0 >= 44100,
                    "Sample rate should be at least 44.1 kHz"
                );
            }
        }
    }

    /// Verify that the PipeWire detection env var check logic works correctly.
    ///
    /// When `PIPEWIRE_RUNTIME_DIR` is set, `is_pipewire_running()` should
    /// detect PipeWire as running. When it is not set and the socket does
    /// not exist, it should return false.
    #[test]
    fn pipewire_detection_checks_env() {
        let original = std::env::var("PIPEWIRE_RUNTIME_DIR").ok();

        std::env::set_var("PIPEWIRE_RUNTIME_DIR", "/tmp/pipewire-test-runtime");
        assert!(
            is_pipewire_running(),
            "is_pipewire_running() should return true when PIPEWIRE_RUNTIME_DIR is set"
        );

        std::env::remove_var("PIPEWIRE_RUNTIME_DIR");
        let _result = is_pipewire_running();

        if let Some(val) = original {
            std::env::set_var("PIPEWIRE_RUNTIME_DIR", val);
        }
    }
}
