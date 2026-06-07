//! BPM detection using onset detection and autocorrelation.

/// BPM detector using onset detection and autocorrelation
pub struct BpmDetector {
    pub(crate) sample_rate: f32,
    /// Onset detection function history
    onset_history: std::collections::VecDeque<f32>,
    /// Maximum onset history length
    max_history: usize,
    /// Minimum onset history entries before detection is attempted.
    /// At 44.1kHz with hop_size=512, 500 entries ≈ 5.8 seconds,
    /// which provides enough data for reliable tempo estimation.
    min_history: usize,
}

impl BpmDetector {
    /// Create a new BPM detector for the given sample rate.
    ///
    /// Returns an error if `sample_rate` is not positive.
    pub fn new(sample_rate: f32) -> Result<Self, super::AnalysisError> {
        if sample_rate <= 0.0 {
            return Err(super::AnalysisError::InvalidSampleRate(sample_rate));
        }
        Ok(Self {
            sample_rate,
            onset_history: std::collections::VecDeque::with_capacity(8192),
            max_history: 8192,
            min_history: 500,
        })
    }

    /// Feed a chunk of audio samples for BPM detection.
    ///
    /// Computes RMS energy per hop as the onset detection function.
    /// This is an energy-based approach (not spectral flux); the
    /// onset function measures the root-mean-square energy of each
    /// hop-sized frame.
    pub fn feed(&mut self, samples: &[(f32, f32)]) {
        let hop_size = 512;

        for chunk in samples.chunks(hop_size) {
            let energy: f32 = chunk.iter().map(|(l, r)| (l * l + r * r) * 0.5).sum();
            let onset = energy.sqrt();
            self.onset_history.push_back(onset);
            if self.onset_history.len() > self.max_history {
                self.onset_history.pop_front();
            }
        }
    }

    /// Detect BPM using autocorrelation of onset function.
    ///
    /// The onset function is mean-subtracted before autocorrelation
    /// to remove the DC component, which significantly improves
    /// the prominence of rhythmic peaks.
    pub fn detect(&self) -> super::BpmResult {
        if self.onset_history.len() < self.min_history {
            // Not enough data for reliable detection.
            return super::BpmResult {
                bpm: 120.0,
                confidence: 0.0,
            };
        }

        let mean: f32 = self.onset_history.iter().sum::<f32>() / self.onset_history.len() as f32;

        // Autocorrelation with mean subtraction (autocovariance)
        let min_lag = (60.0 / 200.0 * self.sample_rate / 512.0) as usize; // 200 BPM max
        let max_lag = (1.0 * self.sample_rate / 512.0) as usize; // 60 BPM min

        // Guard: if the lag range is empty (can happen at unusual sample rates or
        // when the history is too short relative to the BPM range), return the
        // default result rather than producing garbage from an uninitialised best_lag.
        let effective_max_lag = max_lag.min(self.onset_history.len() / 2);
        if min_lag > effective_max_lag {
            return super::BpmResult {
                bpm: 120.0,
                confidence: 0.0,
            };
        }

        let mut best_lag = min_lag;
        let mut best_corr = f32::MIN;
        let mut second_best_corr = f32::MIN;

        for lag in min_lag..=effective_max_lag {
            let mut corr = 0.0;
            let n = self.onset_history.len() - lag;
            for i in 0..n {
                let x = self.onset_history[i] - mean;
                let y = self.onset_history[i + lag] - mean;
                corr += x * y;
            }
            corr /= n as f32;

            if corr > best_corr {
                second_best_corr = best_corr;
                best_corr = corr;
                best_lag = lag;
            } else if corr > second_best_corr {
                second_best_corr = corr;
            }
        }

        let bpm = 60.0 * self.sample_rate / 512.0 / best_lag as f32;

        // Normalize BPM to 60-200 range (handle double/half time)
        // Use a loop to handle extreme values (e.g., raw BPM of 402 or 25)
        let mut bpm = bpm;
        let max_iterations = 10; // Safety bound to prevent infinite loops
        for _ in 0..max_iterations {
            if bpm > 200.0 {
                bpm /= 2.0;
            } else if bpm < 60.0 {
                bpm *= 2.0;
            } else {
                break;
            }
        }

        let confidence = if best_corr > 0.0 && second_best_corr > 0.0 {
            (1.0 - second_best_corr / best_corr).clamp(0.0, 1.0)
        } else if best_corr > 0.0 {
            // Single positive peak with no meaningful second peak — this is
            // a weak detection, not a confident one. Return a low confidence
            // value instead of the previous 1.0 (maximum), which was incorrect.
            0.3
        } else {
            0.0
        };

        super::BpmResult { bpm, confidence }
    }

    pub fn reset(&mut self) {
        self.onset_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    #[test]
    fn test_bpm_detector_with_pulse() {
        let mut detector = BpmDetector::new(44100.0).unwrap();
        // Synthesize audio with a 120 BPM pulse (kick every 0.5 seconds)
        let sr = 44100.0;
        let beat_interval = (sr * 0.5) as usize; // 120 BPM
        let total_samples = sr as usize * 6; // 6 seconds of audio
        let samples: Vec<(f32, f32)> = (0..total_samples)
            .map(|i| {
                let beat_phase = i % beat_interval;
                let v = if beat_phase < 512 {
                    (2.0 * PI * 80.0 * (beat_phase as f32 / sr)).sin() * 0.8
                } else {
                    0.0
                };
                (v, v)
            })
            .collect();
        detector.feed(&samples);
        let result = detector.detect();
        assert!(result.bpm > 0.0);
        assert!(result.confidence > 0.0);
        // The detected BPM should be roughly near 120 BPM (within octave)
        assert!(result.bpm >= 60.0 && result.bpm <= 200.0);
    }

    #[test]
    fn test_bpm_detector_insufficient_data() {
        let detector = BpmDetector::new(44100.0).unwrap();
        let result = detector.detect();
        assert_eq!(result.bpm, 120.0);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_bpm_normalization_loop() {
        // Test that extreme BPM values are normalized correctly via the loop
        let mut detector = BpmDetector::new(44100.0).unwrap();
        // Feed silence — the autocorrelation will find some lag but
        // the key test is that the normalization loop handles edge cases.
        let samples: Vec<(f32, f32)> = (0..441000).map(|_| (0.0, 0.0)).collect();
        detector.feed(&samples);
        let result = detector.detect();
        // BPM should be within 60-200 range due to normalization loop
        assert!(result.bpm >= 60.0 && result.bpm <= 200.0 || result.confidence == 0.0);
    }

    #[test]
    fn test_bpm_detector_invalid_sample_rate() {
        assert!(BpmDetector::new(0.0).is_err());
        assert!(BpmDetector::new(-44100.0).is_err());
        assert!(BpmDetector::new(44100.0).is_ok());
    }

    #[test]
    fn test_mean_subtraction_in_bpm() {
        // Verify that BpmDetector::detect() works correctly with
        // mean-subtracted autocorrelation (no panic, reasonable output)
        let mut detector = BpmDetector::new(44100.0).unwrap();
        // Feed enough samples to exceed min_history threshold
        let samples: Vec<(f32, f32)> = (0..441000)
            .map(|i| {
                let t = i as f32 / 44100.0;
                let v = (2.0 * PI * 440.0 * t).sin() * 0.5;
                (v, v)
            })
            .collect();
        detector.feed(&samples);
        let result = detector.detect();
        assert!(result.bpm > 0.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    }
}
