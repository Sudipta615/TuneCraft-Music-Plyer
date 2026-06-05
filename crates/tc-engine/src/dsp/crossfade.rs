//! Crossfade and gapless playback mixer
//!
//! Handles sample-accurate transitions between tracks with configurable
//! crossfade curves. Supports three curve types:
//! - **Linear**: simple linear interpolation (causes volume dip at center)
//! - **Equal-power**: cosine/sine crossfade that preserves perceived loudness
//! - **S-curve**: smoothstep interpolation for the smoothest transition
//!
//! Gapless playback is handled separately via a simpler mechanism that
//! simply switches to the next track's samples at the boundary.

/// Crossfade curve shape
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrossfadeCurve {
    /// Linear interpolation (causes ~3 dB dip at center)
    Linear,
    /// Equal-power: cos/sin crossfade (preserves perceived loudness)
    EqualPower,
    /// S-curve: smoothstep for the most natural transition
    SCurve,
}

/// State of the crossfade mixer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixerState {
    /// Playing current track only
    PlayingCurrent,
    /// Crossfading from current to next
    Crossfading,
    /// Playing next track only (after crossfade completes)
    PlayingNext,
    /// No audio (stopped)
    Silent,
}

/// Crossfade configuration
#[derive(Debug, Clone, Copy)]
pub struct CrossfadeConfig {
    /// Duration of the crossfade in frames
    pub duration_frames: usize,
    /// Crossfade curve shape
    pub curve: CrossfadeCurve,
    /// Whether to detect silence boundaries for smarter transitions
    pub smart_boundaries: bool,
}

impl Default for CrossfadeConfig {
    fn default() -> Self {
        Self {
            duration_frames: 88200, // 2 seconds at 44100 Hz
            curve: CrossfadeCurve::EqualPower,
            smart_boundaries: true,
        }
    }
}

/// The crossfade/gapless mixer
///
/// Manages transitions between tracks by crossfading the outgoing track's
/// tail with the incoming track's head. The outgoing tail must be provided
/// in advance (pre-read) for gapless/crossfade operation.
#[derive(Debug, Clone)]
pub struct TrackMixer {
    state: MixerState,
    /// Current position in the crossfade (0 to duration_frames)
    crossfade_pos: usize,
    config: CrossfadeConfig,
    /// Whether crossfade is enabled (when disabled, transitions are gapless only)
    enabled: bool,
    /// Buffer for outgoing track tail (pre-read for gapless/crossfade)
    outgoing_buffer_left: Vec<f64>,
    outgoing_buffer_right: Vec<f64>,
    /// Read position in outgoing buffer
    outgoing_pos: usize,
}

impl TrackMixer {
    /// Create a new mixer with a default 2-second crossfade
    pub fn new(sample_rate: f64) -> Self {
        let default_duration = (2.0 * sample_rate) as usize;
        Self {
            state: MixerState::Silent,
            crossfade_pos: 0,
            config: CrossfadeConfig {
                duration_frames: default_duration,
                curve: CrossfadeCurve::EqualPower,
                smart_boundaries: true,
            },
            enabled: true,
            outgoing_buffer_left: Vec::new(),
            outgoing_buffer_right: Vec::new(),
            outgoing_pos: 0,
        }
    }

    /// Get the crossfade duration in frames
    pub fn duration_frames(&self) -> usize {
        self.config.duration_frames
    }

    /// Get the crossfade duration in milliseconds (rounded)
    pub fn duration_ms(&self, sample_rate: f64) -> u64 {
        if sample_rate > 0.0 {
            (self.config.duration_frames as f64 / sample_rate * 1000.0).round() as u64
        } else {
            0
        }
    }

    /// Set crossfade duration in milliseconds
    pub fn set_duration_ms(&mut self, duration_ms: u64, sample_rate: f64) {
        self.config.duration_frames = (duration_ms as f64 * 0.001 * sample_rate) as usize;
    }

    /// Enable or disable the crossfade mixer.
    /// When disabled, the mixer bypasses crossfade and transitions are gapless.
    /// When enabled, the mixer will crossfade between tracks using the
    /// configured curve and duration.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // If currently crossfading, immediately jump to next track
            if self.state == MixerState::Crossfading {
                self.state = MixerState::PlayingNext;
                self.outgoing_buffer_left.clear();
                self.outgoing_buffer_right.clear();
                self.outgoing_pos = 0;
            }
        }
    }

    /// Whether the crossfade mixer is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Whether the mixer is currently in the Crossfading state (High #H5).
    pub fn is_crossfading(&self) -> bool {
        self.state == MixerState::Crossfading
    }

    /// Set crossfade curve type
    pub fn set_curve(&mut self, curve: CrossfadeCurve) {
        self.config.curve = curve;
    }

    /// Start a crossfade to the next track.
    /// If crossfade is disabled, this immediately switches to PlayingNext
    /// for a gapless transition instead of a crossfade.
    pub fn start_crossfade(&mut self) {
        if self.enabled {
            self.state = MixerState::Crossfading;
            self.crossfade_pos = 0;
        } else {
            // Crossfade disabled: gapless transition
            self.state = MixerState::PlayingNext;
            self.outgoing_buffer_left.clear();
            self.outgoing_buffer_right.clear();
            self.outgoing_pos = 0;
        }
    }

    /// Signal that the current track has started playing
    pub fn start_playing(&mut self) {
        self.state = MixerState::PlayingCurrent;
    }

    /// Set the outgoing track's tail samples (for gapless/crossfade)
    pub fn set_outgoing_tail(&mut self, left: Vec<f64>, right: Vec<f64>) {
        self.outgoing_buffer_left = left;
        self.outgoing_buffer_right = right;
        self.outgoing_pos = 0;
    }

    /// Compute crossfade gains for a given normalized position and curve type.
    ///
    /// Returns `(outgoing_gain, incoming_gain)`.
    /// At `t=0.0`, outgoing is full and incoming is silent.
    /// At `t=1.0`, outgoing is silent and incoming is full.
    #[inline]
    pub fn compute_gains_for_curve(t: f64, curve: CrossfadeCurve) -> (f64, f64) {
        let t = t.clamp(0.0, 1.0);
        match curve {
            CrossfadeCurve::Linear => (1.0 - t, t),
            CrossfadeCurve::EqualPower => {
                // Equal-power crossfade: preserves perceived loudness
                // because cos²(θ) + sin²(θ) = 1
                let cos_t = (std::f64::consts::FRAC_PI_2 * t).cos();
                let sin_t = (std::f64::consts::FRAC_PI_2 * t).sin();
                (cos_t, sin_t)
            },
            CrossfadeCurve::SCurve => {
                // S-curve: smoothstep for the most natural transition
                let s = t * t * (3.0 - 2.0 * t);
                (1.0 - s, s)
            },
        }
    }

    /// Compute crossfade gains using the currently configured curve.
    #[inline]
    fn compute_gains(&self, t: f64) -> (f64, f64) {
        Self::compute_gains_for_curve(t, self.config.curve)
    }

    /// Mix a stereo sample from current and next tracks during crossfade.
    ///
    /// # Arguments
    /// * `current_left` / `current_right` - Samples from the current (outgoing) track
    /// * `next_left` / `next_right` - Samples from the next (incoming) track
    ///
    /// # Returns
    /// The mixed stereo sample
    #[inline]
    pub fn process(
        &mut self,
        current_left: f64,
        current_right: f64,
        next_left: f64,
        next_right: f64,
    ) -> (f64, f64) {
        match self.state {
            MixerState::PlayingCurrent => (current_left, current_right),
            MixerState::PlayingNext => (next_left, next_right),
            MixerState::Silent => (0.0, 0.0),
            MixerState::Crossfading => {
                let t = if self.config.duration_frames > 0 {
                    self.crossfade_pos as f64 / self.config.duration_frames as f64
                } else {
                    1.0
                };

                let (out_gain, in_gain) = self.compute_gains(t);

                // Use outgoing buffer if available, otherwise use current track samples
                let (out_l, out_r) = if self.outgoing_pos < self.outgoing_buffer_left.len() {
                    let l = self.outgoing_buffer_left[self.outgoing_pos];
                    let r = self.outgoing_buffer_right[self.outgoing_pos];
                    self.outgoing_pos += 1;
                    (l, r)
                } else {
                    (current_left, current_right)
                };

                let mixed_l = out_l * out_gain + next_left * in_gain;
                let mixed_r = out_r * out_gain + next_right * in_gain;

                self.crossfade_pos += 1;
                if self.crossfade_pos >= self.config.duration_frames {
                    self.state = MixerState::PlayingNext;
                    self.outgoing_buffer_left.clear();
                    self.outgoing_buffer_right.clear();
                    self.outgoing_pos = 0;
                }

                (mixed_l, mixed_r)
            },
        }
    }

    /// Process for gapless playback (no crossfade, seamless transition).
    ///
    /// When `next_available` is true, the next track's samples are used
    /// directly, enabling sample-accurate gapless transitions.
    #[inline]
    pub fn process_gapless(
        &mut self,
        current_left: f64,
        current_right: f64,
        next_available: bool,
        next_left: f64,
        next_right: f64,
    ) -> (f64, f64) {
        if next_available {
            (next_left, next_right)
        } else {
            (current_left, current_right)
        }
    }

    /// Get the current mixer state
    pub fn state(&self) -> MixerState {
        self.state
    }

    /// Get the crossfade progress (0.0 to 1.0) if crossfading
    pub fn crossfade_progress(&self) -> Option<f64> {
        if self.state == MixerState::Crossfading && self.config.duration_frames > 0 {
            Some(self.crossfade_pos as f64 / self.config.duration_frames as f64)
        } else {
            None
        }
    }

    /// Reset all mixer state
    pub fn reset(&mut self) {
        self.state = MixerState::Silent;
        self.crossfade_pos = 0;
        self.outgoing_buffer_left.clear();
        self.outgoing_buffer_right.clear();
        self.outgoing_pos = 0;
        // Note: `enabled` and config are NOT reset — they are persistent settings
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_power_crossfade_preserves_energy() {
        // At midpoint (t=0.5), equal power should preserve energy
        let (out_gain, in_gain) =
            TrackMixer::compute_gains_for_curve(0.5, CrossfadeCurve::EqualPower);
        let energy = out_gain * out_gain + in_gain * in_gain;
        assert!(
            (energy - 1.0).abs() < 0.01,
            "Equal power should preserve energy at midpoint, got {}",
            energy
        );
    }

    #[test]
    fn test_linear_crossfade_dips_at_center() {
        let (out_gain, in_gain) = TrackMixer::compute_gains_for_curve(0.5, CrossfadeCurve::Linear);
        // Linear at center: both 0.5
        assert!((out_gain - 0.5).abs() < 0.01);
        assert!((in_gain - 0.5).abs() < 0.01);
        // Perceived energy dips: 0.5² + 0.5² = 0.5 < 1.0
        let energy = out_gain * out_gain + in_gain * in_gain;
        assert!(energy < 0.7, "Linear crossfade should dip at center");
    }

    #[test]
    fn test_s_curve_monotonic() {
        let mut prev_in = 0.0;
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let (_, in_gain) = TrackMixer::compute_gains_for_curve(t, CrossfadeCurve::SCurve);
            assert!(
                in_gain >= prev_in - 1e-10,
                "S-curve should be monotonically increasing"
            );
            prev_in = in_gain;
        }
    }

    #[test]
    fn test_crossfade_completion() {
        let mut mixer = TrackMixer::new(44100.0);
        mixer.set_duration_ms(100, 44100.0);
        mixer.start_crossfade();
        let duration = mixer.config.duration_frames;
        for _ in 0..duration + 10 {
            mixer.process(0.5, 0.5, 0.5, 0.5);
        }
        assert_eq!(mixer.state(), MixerState::PlayingNext);
    }

    #[test]
    fn test_crossfade_start_and_end_gains() {
        // At t=0: outgoing should be full, incoming should be silent
        let (out0, in0) = TrackMixer::compute_gains_for_curve(0.0, CrossfadeCurve::EqualPower);
        assert!(
            (out0 - 1.0).abs() < 1e-10,
            "Outgoing at start should be 1.0"
        );
        assert!((in0 - 0.0).abs() < 1e-10, "Incoming at start should be 0.0");

        // At t=1: outgoing should be silent, incoming should be full
        let (out1, in1) = TrackMixer::compute_gains_for_curve(1.0, CrossfadeCurve::EqualPower);
        assert!((out1 - 0.0).abs() < 1e-10, "Outgoing at end should be 0.0");
        assert!((in1 - 1.0).abs() < 1e-10, "Incoming at end should be 1.0");
    }

    #[test]
    fn test_gapless_transition() {
        let mut mixer = TrackMixer::new(44100.0);
        // Before next track is available
        let (l, r) = mixer.process_gapless(0.5, 0.5, false, 0.0, 0.0);
        assert!((l - 0.5).abs() < 1e-10);
        assert!((r - 0.5).abs() < 1e-10);

        // After next track is available
        let (l, r) = mixer.process_gapless(0.5, 0.5, true, 0.8, 0.8);
        assert!((l - 0.8).abs() < 1e-10);
        assert!((r - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_outgoing_buffer_crossfade() {
        let mut mixer = TrackMixer::new(44100.0);
        mixer.set_duration_ms(10, 44100.0); // Very short crossfade for testing

        let tail_len = mixer.config.duration_frames;
        let tail_left: Vec<f64> = (0..tail_len).map(|i| 0.5 - i as f64 * 0.001).collect();
        let tail_right: Vec<f64> = (0..tail_len).map(|i| 0.5 - i as f64 * 0.001).collect();
        mixer.set_outgoing_tail(tail_left, tail_right);
        mixer.start_crossfade();

        // Process through crossfade
        for _ in 0..tail_len + 10 {
            mixer.process(0.0, 0.0, 0.5, 0.5);
        }
        assert_eq!(mixer.state(), MixerState::PlayingNext);
    }
}
