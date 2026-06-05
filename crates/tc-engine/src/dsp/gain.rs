//! Gain and fade processing — smooth volume control and track fade-in/fade-out

use crate::buffer::AudioFrame;

/// Simple gain/volume control with smooth ramping to avoid zipper noise
#[derive(Debug, Clone, Copy)]
pub struct GainProcessor {
    pub gain: f64,
    pub target_gain: f64,
    /// Linear interpolation speed per sample (0.0–1.0)
    pub slew_rate: f64,
}

impl GainProcessor {
    pub fn new() -> Self {
        Self {
            gain: 1.0,
            target_gain: 1.0,
            slew_rate: 0.001,
        }
    }

    /// Create with specific initial gain and ramp time
    pub fn with_ramp(initial_gain: f64, ramp_time_ms: f64, sample_rate: f64) -> Self {
        let slew_rate = if ramp_time_ms > 0.0 && sample_rate > 0.0 {
            1.0 / (ramp_time_ms * 0.001 * sample_rate)
        } else {
            1.0
        };
        Self {
            gain: initial_gain,
            target_gain: initial_gain,
            slew_rate,
        }
    }

    /// Set target gain (smooth transition)
    pub fn set_gain(&mut self, gain: f64) {
        self.target_gain = gain.clamp(0.0, 1.5);
    }

    /// Get current gain value
    pub fn current_gain(&self) -> f64 {
        self.gain
    }

    /// Set the slew rate — higher values mean faster transitions
    pub fn set_slew_rate(&mut self, rate: f64) {
        self.slew_rate = rate.clamp(0.0001, 1.0);
    }

    /// Process a frame with smooth gain ramping
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        self.gain += (self.target_gain - self.gain) * self.slew_rate;
        // L9: Flush denormals when gain has nearly converged.
        // Previously only process_stereo() flushed denormals; process_frame()
        // and process_sample() could hover near target causing FPU denormal stalls.
        if (self.gain - self.target_gain).abs() < 1e-9 {
            self.gain = self.target_gain;
        }
        for i in 0..frame.num_channels as usize {
            frame.channels[i] *= self.gain;
        }
    }

    /// Process a stereo sample pair with smooth gain ramping
    #[inline]
    pub fn process_stereo(&mut self, left: f64, right: f64) -> (f64, f64) {
        self.gain += (self.target_gain - self.gain) * self.slew_rate;
        // I-15: Flush denormals when gain has nearly converged.
        if (self.gain - self.target_gain).abs() < 1e-9 {
            self.gain = self.target_gain;
        }
        (left * self.gain, right * self.gain)
    }

    /// Process a single sample
    #[inline]
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        self.gain += (self.target_gain - self.gain) * self.slew_rate;
        // L9: Flush denormals (consistent with process_stereo).
        if (self.gain - self.target_gain).abs() < 1e-9 {
            self.gain = self.target_gain;
        }
        sample * self.gain
    }

    /// Immediately snap gain to the target (no ramp)
    pub fn snap(&mut self) {
        self.gain = self.target_gain;
    }

    /// Check if the gain has converged to the target
    pub fn is_settled(&self) -> bool {
        (self.gain - self.target_gain).abs() < 1e-8
    }

    /// Reset gain to unity
    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.target_gain = 1.0;
    }
}

impl Default for GainProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Current state of a fade
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FadeState {
    #[default]
    Idle,
    FadingIn,
    FadingOut,
    FadedOut,
}

/// Smooth fade-in/fade-out processor for track transitions and pause/resume
pub struct FadeProcessor {
    gain: f64,
    increment_per_sample: f64,
    pub state: FadeState,
    total_samples: u64,
    samples_processed: u64,
    sample_rate: f64,
    /// Default fade duration in seconds
    default_duration_secs: f64,
}

impl FadeProcessor {
    /// Create a new fade processor with a default fade duration
    pub fn new(fade_time_ms: f64, sample_rate: f64) -> Self {
        Self {
            gain: 1.0,
            increment_per_sample: 0.0,
            state: FadeState::Idle,
            total_samples: 0,
            samples_processed: 0,
            sample_rate,
            default_duration_secs: fade_time_ms / 1000.0,
        }
    }

    /// Begin a fade-in using the default duration
    pub fn fade_in(&mut self) {
        self.fade_in_duration(self.default_duration_secs);
    }

    /// Begin a fade-in over the given duration in seconds
    pub fn fade_in_duration(&mut self, duration_secs: f64) {
        self.total_samples = (duration_secs * self.sample_rate) as u64;
        self.samples_processed = 0;
        self.gain = 0.0;
        self.increment_per_sample = if self.total_samples > 0 {
            1.0 / self.total_samples as f64
        } else {
            1.0
        };
        self.state = FadeState::FadingIn;
    }

    /// Begin a fade-out using the default duration
    pub fn fade_out(&mut self) {
        self.fade_out_duration(self.default_duration_secs);
    }

    /// Begin a fade-out over the given duration in seconds
    pub fn fade_out_duration(&mut self, duration_secs: f64) {
        self.total_samples = (duration_secs * self.sample_rate) as u64;
        self.samples_processed = 0;
        self.gain = 1.0;
        self.increment_per_sample = if self.total_samples > 0 {
            -1.0 / self.total_samples as f64
        } else {
            -1.0
        };
        self.state = FadeState::FadingOut;
    }

    /// Whether the fade-out has completed (output is silent)
    pub fn is_faded_out(&self) -> bool {
        self.state == FadeState::FadedOut
    }

    /// Process a stereo sample pair
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        if self.state == FadeState::Idle {
            return (left, right);
        }
        let out_l = left * self.gain;
        let out_r = right * self.gain;
        self.advance(1);
        (out_l, out_r)
    }

    /// Process an audio frame
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        if self.state == FadeState::Idle {
            return;
        }
        for i in 0..frame.num_channels as usize {
            frame.channels[i] *= self.gain;
        }
        self.advance(1);
    }

    /// Get the current gain value
    pub fn gain(&self) -> f64 {
        self.gain
    }

    fn advance(&mut self, n: u64) {
        self.samples_processed += n;
        self.gain += self.increment_per_sample * n as f64;

        match self.state {
            FadeState::FadingIn
                if self.gain >= 1.0 || self.samples_processed >= self.total_samples =>
            {
                self.gain = 1.0;
                self.state = FadeState::Idle;
            },
            FadeState::FadingOut
                if self.gain <= 0.0 || self.samples_processed >= self.total_samples =>
            {
                self.gain = 0.0;
                self.state = FadeState::FadedOut;
            },
            _ => {},
        }
    }

    pub fn cancel(&mut self, gain: f64) {
        self.gain = gain.clamp(0.0, 1.0);
        self.state = if self.gain >= 1.0 {
            FadeState::Idle
        } else if self.gain <= 0.0 {
            FadeState::FadedOut
        } else {
            // snapped to 1.0 on the next process() call. The caller can
            // explicitly fade_in() or fade_out() after cancelling if desired.
            FadeState::FadingIn
        };
        self.increment_per_sample = 0.0;
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.increment_per_sample = 0.0;
        self.state = FadeState::Idle;
        self.total_samples = 0;
        self.samples_processed = 0;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gain_processor_smooth() {
        let mut gp = GainProcessor::new();
        gp.set_slew_rate(0.5);
        gp.set_gain(0.5);
        let (l, r) = gp.process_stereo(1.0, 1.0);
        assert!(l < 1.0 && l > 0.5);
    }

    #[test]
    fn test_gain_processor_snap() {
        let mut gp = GainProcessor::new();
        gp.set_gain(0.0);
        gp.snap();
        assert!(gp.is_settled());
    }

    #[test]
    fn test_gain_with_ramp() {
        let gp = GainProcessor::with_ramp(0.5, 10.0, 44100.0);
        assert!((gp.gain - 0.5).abs() < 1e-10);
        assert!((gp.current_gain() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_fade_in_completes() {
        let mut fade = FadeProcessor::new(1.0, 44100.0);
        fade.fade_in();
        assert_eq!(fade.state, FadeState::FadingIn);
        for _ in 0..50000 {
            fade.process(1.0, 1.0);
        }
        assert_eq!(fade.state, FadeState::Idle);
    }

    #[test]
    fn test_fade_out_completes() {
        let mut fade = FadeProcessor::new(1.0, 44100.0);
        fade.fade_out();
        for _ in 0..50000 {
            fade.process(1.0, 1.0);
        }
        assert!(fade.is_faded_out());
    }

    #[test]
    fn test_fade_no_click() {
        let mut fade = FadeProcessor::new(10.0, 44100.0);
        fade.fade_out();
        let mut prev = 1.0;
        for _ in 0..5000 {
            let (l, _r) = fade.process(1.0, 1.0);
            let delta = (l - prev).abs();
            assert!(delta < 0.05, "Fade should be smooth, delta={}", delta);
            prev = l;
        }
    }
}
