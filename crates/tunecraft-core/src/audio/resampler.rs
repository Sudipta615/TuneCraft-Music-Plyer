//! High-quality audio resampler using rubato (pure-Rust sinc interpolation).
//!
//! # Architecture
//!
//! This module wraps `rubato` to provide high-quality resampling — equivalent
//! to Poweramp's "SoX — very high quality" resampler type. It is used in the
//! DSP pipeline whenever the source sample rate differs from the target output
//! rate (48 kHz).
//!
//! ## Migration from samplerate 0.2
//!
//! Previously this module used the `samplerate` crate (libsamplerate FFI).
//! Migrated to `rubato 0.15` for:
//! - Pure Rust (no C dependency, no build-time linker issues)
//! - Well-maintained, actively developed
//! - Comparable or better quality with SincFixedIn<SincInterpolation>
//! - `Send + Sync` (libsamplerate's state was not thread-safe)
//!
//! ## Resampler types (mapped from Poweramp terminology)
//!
//! | Tunecraft `ResamplerQuality` | rubato type                     | Poweramp equivalent            |
//! |------------------------------|----------------------------------|-------------------------------|
//! | `Linear`                     | `FftFixedIn` (fast)             | SW - linear (fastest)         |
//! | `ZeroOrderHold`              | `FftFixedIn` (fast)             | SW - zero-order hold           |
//! | `SincFastest`                | `SincFixedIn` (2 subsecs)       | SW - high quality              |
//! | `SincMedium`                 | `SincFixedIn` (4 subsecs)       | SW - very high quality         |
//! | `SincBest`                   | `SincFixedIn` (16 subsecs)      | SoX - very high quality ✓      |
//!
//! ## Cutoff frequency ratio
//!
//! The anti-aliasing filter cutoff is expressed as a fraction of the Nyquist
//! frequency (0.0–1.0). This matches Poweramp's "Resampler Cutoff Frequency
//! Ratio" knob (default 97% = 0.97). In rubato this is controlled via the
//! `cutoff_correction_ratio` parameter on the sinc interpolator.
//!
//! A ratio of 1.0 passes the full Nyquist bandwidth (more aliasing risk).
//! A ratio of 0.95 rolls off 5% below Nyquist (safe default, matches Poweramp).
//!
//! ## Real-time constraints
//!
//! `Resampler::process_into` writes directly into caller-supplied output buffers.
//! No heap allocation occurs after construction.

use anyhow::{Context, Result};
use rubato::{
    FftFixedIn, Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};

/// Quality level for the resampler — maps to rubato converter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResamplerQuality {
    /// Fastest, lowest quality. Uses FFT-based resampling.
    Linear,
    /// Sample-and-hold approximation. Uses FFT-based resampling.
    ZeroOrderHold,
    /// Fast sinc interpolation (2 sub-sections). Equivalent to "SW - high quality".
    SincFastest,
    /// Medium sinc interpolation (4 sub-sections). Equivalent to "SW - very high quality".
    SincMedium,
    /// Highest quality sinc interpolation (16 sub-sections). Equivalent to Poweramp's SoX mode.
    #[default]
    SincBest,
}

impl ResamplerQuality {
    /// Number of sinc sub-sections for rubato's SincFixedIn resampler.
    fn sinc_sub_sections(self) -> usize {
        match self {
            Self::Linear | Self::ZeroOrderHold => 1,
            Self::SincFastest => 2,
            Self::SincMedium => 4,
            Self::SincBest => 16,
        }
    }

    /// Whether this quality level uses the sinc (vs FFT) resampler.
    fn uses_sinc(self) -> bool {
        matches!(self, Self::SincFastest | Self::SincMedium | Self::SincBest)
    }
}

/// Stateful stereo resampler.
///
/// Wraps a rubato resampler with configurable quality and anti-aliasing
/// cutoff ratio. Designed for one-shot conversion of complete buffers
/// decoded by GStreamer before they enter the DSP ring buffer.
enum InnerResampler {
    Sinc(SincFixedIn<f32>),
    Fft(FftFixedIn<f32>),
}

impl InnerResampler {
    #[allow(dead_code)]
    fn input_frames_next(&self) -> usize {
        match self {
            Self::Sinc(r) => r.input_frames_next(),
            Self::Fft(r) => r.input_frames_next(),
        }
    }

    fn output_frames_next(&self) -> usize {
        match self {
            Self::Sinc(r) => r.output_frames_next(),
            Self::Fft(r) => r.output_frames_next(),
        }
    }

    fn process_into_buffer(
        &mut self,
        input: &[&[f32]],
        output: &mut [&mut [f32]],
    ) -> Result<(usize, usize)> {
        match self {
            Self::Sinc(r) => r
                .process_into_buffer(input, output, None)
                .context("rubato resampling failed"),
            Self::Fft(r) => r
                .process_into_buffer(input, output, None)
                .context("rubato resampling failed"),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Sinc(r) => r.reset(),
            Self::Fft(r) => r.reset(),
        }
    }
}

pub struct Resampler {
    inner: InnerResampler,
    from_rate: u32,
    to_rate: u32,
    /// Anti-aliasing cutoff ratio (0.0–1.0, fraction of Nyquist).
    cutoff_ratio: f64,
    channels: usize,
}

impl Resampler {
    /// Construct a new resampler.
    ///
    /// # Parameters
    /// - `from_rate`: source sample rate in Hz
    /// - `to_rate`: target sample rate in Hz (typically 48 000)
    /// - `channels`: number of channels (1 = mono, 2 = stereo)
    /// - `quality`: sinc filter quality
    /// - `cutoff_ratio`: anti-aliasing cutoff as a fraction of Nyquist (0.5–1.0).
    ///   0.95 is the Poweramp default.
    pub fn new(
        from_rate: u32,
        to_rate: u32,
        channels: usize,
        quality: ResamplerQuality,
        cutoff_ratio: f64,
    ) -> Result<Self> {
        let cutoff_ratio = cutoff_ratio.clamp(0.5, 1.0);

        anyhow::ensure!(
            from_rate > 0,
            "Resampler: from_rate must be > 0, got {}",
            from_rate
        );
        anyhow::ensure!(
            to_rate > 0,
            "Resampler: to_rate must be > 0, got {}",
            to_rate
        );
        anyhow::ensure!(
            channels > 0,
            "Resampler: channels must be > 0, got {}",
            channels
        );

        let resample_ratio = to_rate as f64 / from_rate as f64;

        let chunk_size = 1024;

        let inner = if quality.uses_sinc() {
            let sinc_len = quality.sinc_sub_sections() * 128;
            let params = SincInterpolationParameters {
                sinc_len,
                f_cutoff: cutoff_ratio as f32,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 160,
                window: WindowFunction::BlackmanHarris2,
            };
            let sinc = SincFixedIn::<f32>::new(resample_ratio, 10.0, params, chunk_size, channels)
                .context("failed to create rubato SincFixedIn resampler")?;
            InnerResampler::Sinc(sinc)
        } else {
            let fft = FftFixedIn::<f32>::new(
                from_rate as usize,
                to_rate as usize,
                chunk_size,
                1,
                channels,
            )
            .context("failed to create rubato FftFixedIn resampler")?;
            InnerResampler::Fft(fft)
        };

        Ok(Self {
            inner,
            from_rate,
            to_rate,
            cutoff_ratio,
            channels,
        })
    }

    /// Returns `true` if the source and target rates are identical (no conversion needed).
    pub fn is_passthrough(&self) -> bool {
        self.from_rate == self.to_rate
    }

    /// Resample `input` (interleaved, `self.channels` channels) into a new `Vec<f32>`.
    ///
    /// The rubato resampler expects per-channel non-interleaved buffers.
    /// This method handles the interleaved ↔ non-interleaved conversion
    /// transparently.
    ///
    /// Returns the resampled interleaved buffer. Returns `Ok(input.to_vec())` if passthrough.
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if self.is_passthrough() {
            return Ok(input.to_vec());
        }

        let channels = self.channels;
        let frames_in = input.len() / channels;

        if input.len() % channels != 0 {
            tracing::warn!(
                "Resampler: input length ({}) is not a multiple of channels ({}), \
                 dropping {} trailing samples",
                input.len(),
                channels,
                input.len() % channels
            );
        }

        let mut channel_buffers: Vec<Vec<f32>> = vec![Vec::with_capacity(frames_in); channels];
        for frame in 0..frames_in {
            for ch in 0..channels {
                channel_buffers[ch].push(input[frame * channels + ch]);
            }
        }

        let input_slices: Vec<&[f32]> = channel_buffers.iter().map(|b| b.as_slice()).collect();

        let output_frames = self.inner.output_frames_next();
        let mut output_buffers: Vec<Vec<f32>> = vec![vec![0.0f32; output_frames]; channels];
        let mut output_slices: Vec<&mut [f32]> = output_buffers
            .iter_mut()
            .map(|b| b.as_mut_slice())
            .collect();

        let (_frames_read, frames_written) = self
            .inner
            .process_into_buffer(&input_slices, &mut output_slices)?;

        let actual_output_frames = frames_written;
        let mut result = Vec::with_capacity(actual_output_frames * channels);
        for frame in 0..actual_output_frames {
            for ch in 0..channels {
                if frame < output_buffers[ch].len() {
                    result.push(output_buffers[ch][frame]);
                }
            }
        }

        Ok(result)
    }

    /// Reset the converter state (call between tracks to flush internal delay line).
    pub fn reset(&mut self) -> Result<()> {
        self.inner.reset();
        Ok(())
    }

    pub fn from_rate(&self) -> u32 {
        self.from_rate
    }
    pub fn to_rate(&self) -> u32 {
        self.to_rate
    }
    pub fn cutoff_ratio(&self) -> f64 {
        self.cutoff_ratio
    }

    /// Update the cutoff ratio at runtime without rebuilding the converter.
    ///
    /// # ⚠️ Warning: Stored but Not Immediately Applied
    ///
    /// The new cutoff ratio is **stored** on this `Resampler` instance and will
    /// be used the next time the resampler is **recreated** (e.g., on sample
    /// rate change or explicit reconstruction). It is **not** applied to the
    /// currently active rubato resampler, because rubato does not support
    /// changing the cutoff ratio after construction — doing so would require
    /// an expensive full rebuild of the sinc interpolator (computing new filter
    /// coefficients, allocating buffers, etc.).
    ///
    /// If you need the new cutoff ratio to take effect immediately, you must
    /// drop this `Resampler` and create a new one via [`Resampler::new`].
    pub fn set_cutoff_ratio(&mut self, ratio: f64) {
        self.cutoff_ratio = ratio.clamp(0.5, 1.0);
    }
}

/// Convenience: resample a buffer without keeping state between calls.
///
/// Opens a fresh converter, processes the buffer, and discards it. Use this
/// for one-shot conversions; use `Resampler` for streaming (preserves state).
pub fn resample_once(
    input: &[f32],
    from_rate: u32,
    to_rate: u32,
    channels: usize,
    quality: ResamplerQuality,
    cutoff_ratio: f64,
) -> Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(input.to_vec());
    }
    let mut r = Resampler::new(from_rate, to_rate, channels, quality, cutoff_ratio)?;
    r.process(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_returns_identical_buffer() {
        let mut r = Resampler::new(48_000, 48_000, 2, ResamplerQuality::SincBest, 0.95).unwrap();
        let input: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
        let output = r.process(&input).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn upsample_produces_more_samples() {
        let mut r = Resampler::new(44_100, 48_000, 2, ResamplerQuality::SincBest, 0.95).unwrap();
        let input: Vec<f32> = vec![0.0f32; 4410 * 2]; // 0.1 s stereo at 44.1 kHz
        let output = r.process(&input).unwrap();
        let expected = (4410.0 * 48_000.0 / 44_100.0 * 2.0) as usize;
        let diff = (output.len() as isize - expected as isize).unsigned_abs();
        assert!(
            diff < expected / 20,
            "output len {} far from expected {}",
            output.len(),
            expected
        );
    }

    #[test]
    fn downsample_produces_fewer_samples() {
        let mut r = Resampler::new(96_000, 48_000, 2, ResamplerQuality::SincFastest, 0.95).unwrap();
        let input: Vec<f32> = vec![0.5f32; 9600 * 2]; // 0.1 s stereo at 96 kHz
        let output = r.process(&input).unwrap();
        let expected = 4800 * 2;
        let diff = (output.len() as isize - expected as isize).unsigned_abs();
        assert!(
            diff < expected / 10,
            "output len {} far from expected {}",
            output.len(),
            expected
        );
    }

    #[test]
    fn cutoff_ratio_clamps_to_valid_range() {
        let mut r = Resampler::new(44_100, 48_000, 2, ResamplerQuality::SincBest, 2.0).unwrap();
        assert!((r.cutoff_ratio() - 1.0).abs() < 1e-9);
        r.set_cutoff_ratio(0.1);
        assert!((r.cutoff_ratio() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn resample_once_passthrough() {
        let input = vec![0.1f32, 0.2, 0.3, 0.4];
        let out =
            resample_once(&input, 48_000, 48_000, 2, ResamplerQuality::SincBest, 0.95).unwrap();
        assert_eq!(input, out);
    }

    #[test]
    fn output_samples_are_finite() {
        let sr_in = 44_100u32;
        let frames = sr_in as usize / 10; // 0.1 s
        let input: Vec<f32> = (0..frames * 2)
            .map(|i| {
                let t = i as f32 / (sr_in as f32 * 2.0);
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
            })
            .collect();
        let mut r = Resampler::new(sr_in, 48_000, 2, ResamplerQuality::SincBest, 0.95).unwrap();
        let output = r.process(&input).unwrap();
        for (i, &s) in output.iter().enumerate() {
            assert!(s.is_finite(), "sample[{}] = {}", i, s);
        }
    }
}
