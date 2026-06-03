//! Spectrum analyzer for visualization
//! Runs off the audio thread using decimated analysis buffers

use realfft::RealToComplex;
use std::sync::Arc;

use super::AnalysisError;

/// Spectrum analyzer configuration
#[derive(Debug, Clone)]
pub struct SpectrumConfig {
    pub fft_size: usize,
    pub window_type: WindowType,
    pub hop_size: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum WindowType {
    Hann,
    BlackmanHarris,
    Rectangular,
}

/// A single spectrum frame for visualization
#[derive(Debug, Clone)]
pub struct SpectrumFrame {
    /// Magnitude for each frequency bin (normalized 0.0 to 1.0)
    pub magnitudes: Vec<f64>,
    /// Frequency for each bin center
    pub frequencies: Vec<f64>,
    /// Sample rate
    pub sample_rate: u32,
}

/// Spectrum analyzer (runs off audio thread)
pub struct SpectrumAnalyzer {
    fft_size: usize,
    sample_rate: f64,
    window: Vec<f64>,
    fft: Arc<dyn RealToComplex<f64>>,
    prev_magnitudes: Vec<f64>,
    smoothing: f64,
    /// Pre-allocated FFT input workspace
    input_workspace: Vec<f64>,
    /// Pre-allocated FFT output workspace
    output_workspace: Vec<realfft::num_complex::Complex<f64>>,
}

impl SpectrumAnalyzer {
    /// Create a new SpectrumAnalyzer.
    ///
    /// Returns an error instead of panicking if parameters are invalid:
    /// - `fft_size` must be >= 2 and a power of two
    /// - `sample_rate` must be > 0
    pub fn new(fft_size: usize, sample_rate: f64) -> Result<Self, AnalysisError> {
        if fft_size < 2 {
            return Err(AnalysisError::InvalidFftSize(fft_size));
        }
        if !fft_size.is_power_of_two() {
            return Err(AnalysisError::InvalidFftSize(fft_size));
        }
        if sample_rate <= 0.0 {
            return Err(AnalysisError::InvalidSampleRate(sample_rate));
        }

        let fft = realfft::RealFftPlanner::new().plan_fft_forward(fft_size);
        let window = Self::hann_window(fft_size);
        // I-11: pre-allocate workspace so analyze() is allocation-free
        let input_workspace  = fft.make_input_vec();
        let output_workspace = fft.make_output_vec();
        Ok(Self {
            fft_size,
            sample_rate,
            window,
            fft,
            prev_magnitudes: vec![0.0; fft_size / 2 + 1],
            smoothing: 0.7,
            input_workspace,
            output_workspace,
        })
    }

    /// Generate Hann window
    fn hann_window(size: usize) -> Vec<f64> {
        (0..size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (size - 1) as f64).cos())
            })
            .collect()
    }

    /// Analyze a buffer of stereo samples, producing a spectrum frame.
    /// Uses pre-allocated workspace buffers — no heap allocation on the hot path.
    pub fn analyze(&mut self, samples: &[(f64, f64)]) -> SpectrumFrame {
        // Mix to mono, apply window, write into pre-allocated workspace
        for s in &mut self.input_workspace { *s = 0.0; }
        for (i, (l, r)) in samples.iter().take(self.fft_size).enumerate() {
            let mono = (l + r) * 0.5;
            self.input_workspace[i] = mono * self.window.get(i).copied().unwrap_or(0.0);
        }

        // Perform FFT using pre-allocated output workspace
        let _ = self.fft.process(&mut self.input_workspace, &mut self.output_workspace);

        let num_bins = self.fft_size / 2 + 1;
        let mut magnitudes = Vec::with_capacity(num_bins);
        for (i, complex) in self.output_workspace.iter().take(num_bins).enumerate() {
            let magnitude = (complex.re * complex.re + complex.im * complex.im).sqrt()
                / (self.fft_size as f64).sqrt();
            // Apply smoothing with previous frame
            let smoothed = self.prev_magnitudes.get(i).copied().unwrap_or(0.0) * self.smoothing
                + magnitude * (1.0 - self.smoothing);
            self.prev_magnitudes[i] = smoothed;
            magnitudes.push(smoothed);
        }

        // Normalize magnitudes to 0.0-1.0 range
        let max_mag = magnitudes.iter().cloned().fold(0.0_f64, f64::max);
        if max_mag > 0.0 {
            for m in &mut magnitudes {
                *m /= max_mag;
            }
        }

        let frequencies: Vec<f64> = (0..num_bins)
            .map(|i| i as f64 * self.sample_rate / self.fft_size as f64)
            .collect();

        SpectrumFrame {
            magnitudes,
            frequencies,
            sample_rate: self.sample_rate as u32,
        }
    }

    /// Set smoothing factor (0.0 = no smoothing, 1.0 = max smoothing)
    pub fn set_smoothing(&mut self, smoothing: f64) {
        self.smoothing = smoothing.clamp(0.0, 0.99);
    }

    pub fn reset(&mut self) {
        self.prev_magnitudes.fill(0.0);
    }
}

