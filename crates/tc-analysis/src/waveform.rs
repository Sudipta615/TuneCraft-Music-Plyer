//! Waveform generation for audio visualization.

/// Waveform generator
pub struct WaveformGenerator {
    samples_per_pixel: usize,
}

impl WaveformGenerator {
    /// Create a new WaveformGenerator.
    ///
    /// Returns an error if `samples_per_pixel` is 0, which would cause
    /// an infinite loop or panic during generation.
    pub fn new(samples_per_pixel: usize) -> Result<Self, super::AnalysisError> {
        if samples_per_pixel == 0 {
            return Err(super::AnalysisError::InvalidSamplesPerPixel(samples_per_pixel));
        }
        Ok(Self { samples_per_pixel })
    }

    /// Generate waveform from stereo samples
    pub fn generate(&self, samples: &[(f64, f64)]) -> super::WaveformResult {
        let num_peaks = samples.len().div_ceil(self.samples_per_pixel);
        let mut peaks = Vec::with_capacity(num_peaks);

        for chunk in samples.chunks(self.samples_per_pixel) {
            let mut min = f64::MAX;
            let mut max = f64::MIN;
            for (l, r) in chunk {
                let mono = (l + r) * 0.5;
                min = min.min(mono);
                max = max.max(mono);
            }
            if min == f64::MAX { min = 0.0; max = 0.0; }
            peaks.push((min, max));
        }

        super::WaveformResult {
            peaks,
            samples_per_pixel: self.samples_per_pixel,
            total_frames: samples.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_generator() {
        let gen = WaveformGenerator::new(100).unwrap();
        let samples: Vec<(f64, f64)> = (0..1000).map(|i| {
            let v = (i as f64 / 100.0).sin();
            (v, v)
        }).collect();
        let result = gen.generate(&samples);
        assert!(!result.peaks.is_empty());
        assert_eq!(result.total_frames, 1000);
    }

    #[test]
    fn test_waveform_generator_zero_samples_per_pixel() {
        assert!(WaveformGenerator::new(0).is_err());
        assert!(WaveformGenerator::new(1).is_ok());
    }
}

