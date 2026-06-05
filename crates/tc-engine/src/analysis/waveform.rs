//! Waveform generation for visualization
//! Runs off the audio thread

use super::AnalysisError;

/// Waveform data (min/max pairs for display)
#[derive(Debug, Clone)]
pub struct WaveformData {
    /// (min, max) pairs for each pixel
    pub peaks: Vec<(f64, f64)>,
    pub samples_per_pixel: usize,
    pub total_frames: usize,
}

/// Waveform generator (runs off audio thread)
pub struct WaveformGenerator {
    samples_per_pixel: usize,
}

impl WaveformGenerator {
    /// Create a new WaveformGenerator.
    ///
    /// Returns an error instead of panicking if `samples_per_pixel` is 0.
    pub fn new(samples_per_pixel: usize) -> Result<Self, AnalysisError> {
        if samples_per_pixel == 0 {
            return Err(AnalysisError::InvalidSamplesPerPixel(samples_per_pixel));
        }
        Ok(Self { samples_per_pixel })
    }

    /// Generate waveform data from stereo samples
    pub fn generate(&self, samples: &[(f64, f64)]) -> WaveformData {
        let spp = self.samples_per_pixel;
        let num_peaks = (samples.len() + spp - 1) / spp;
        let mut peaks = Vec::with_capacity(num_peaks);

        for chunk in samples.chunks(spp) {
            let mut min = f64::MAX;
            let mut max = f64::MIN;
            for (l, r) in chunk {
                let mono = (l + r) * 0.5;
                min = min.min(mono);
                max = max.max(mono);
            }
            if min == f64::MAX {
                min = 0.0;
                max = 0.0;
            }
            peaks.push((min, max));
        }

        WaveformData {
            peaks,
            samples_per_pixel: spp,
            total_frames: samples.len(),
        }
    }
}

