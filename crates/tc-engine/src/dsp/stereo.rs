//! Phase-safe stereo width enhancer
//!
//! Uses mid/side (M/S) processing to adjust stereo width without
//! introducing phase problems. Mono signals remain mono regardless
//! of width setting (phase-safe guarantee).
//!
//! Disabled by default — the user must opt in.

/// Stereo width enhancer
///
/// Operates on the mid/side representation:
/// ```text
/// mid  = (L + R) / 2    (mono component)
/// side = (L - R) / 2    (stereo component)
/// L' = mid + side * width
/// R' = mid - side * width
/// ```
///
/// - `width = 0.0`: mono (side eliminated)
/// - `width = 1.0`: passthrough (no change)
/// - `width > 1.0`: enhanced stereo (side boosted)
/// - `width = 2.0`: maximum safe widening
#[derive(Debug, Clone)]
pub struct StereoEnhancer {
    width: f64,
    current_width: f64,
    slew_rate: f64,
    enabled: bool,
}

impl StereoEnhancer {
    /// Create a new stereo enhancer (disabled, width = 1.0)
    pub fn new() -> Self {
        Self {
            width: 1.0,
            current_width: 1.0,
            slew_rate: 0.001,
            enabled: false,
        }
    }

    /// Set the stereo width factor.
    ///
    /// Clamped to [0.0, 2.0]:
    /// - 0.0 = mono collapse
    /// - 1.0 = passthrough
    /// - 2.0 = maximum widening
    pub fn set_width(&mut self, width: f64) {
        self.width = width.clamp(0.0, 2.0);
    }

    /// Get the current width setting
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Enable or disable the stereo enhancer
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the enhancer is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Process a stereo sample with width adjustment.
    ///
    /// This is phase-safe: if the input is mono (L == R), the output
    /// will also be mono regardless of the width setting.
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled || (self.width - 1.0).abs() < 0.001 {
            return (left, right);
        }

        // Dezippering: smoothly approach target width
        self.current_width += (self.width - self.current_width) * self.slew_rate;

        // Mid/side decomposition
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        // Apply width to side channel only (phase-safe)
        let adjusted_side = side * self.current_width;

        // Reconstruct stereo
        (mid + adjusted_side, mid - adjusted_side)
    }

    /// Reset runtime state variables only (not user-configured settings).
    ///
    /// Preserves `width` so that after a seek or stop — which call
    /// `pipeline.reset()` — the user's stereo width setting is not lost.
    /// Only resets `current_width` (the dezippered runtime value) back to
    /// passthrough, allowing it to smoothly ramp up to `width` again.
    pub fn reset(&mut self) {
        self.current_width = 1.0;
        // Do NOT reset self.width — that's the user's configured setting.
        self.slew_rate = 0.001;
    }
}

impl Default for StereoEnhancer {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_width_1_is_passthrough() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.0);
        let (l, r) = enhancer.process(0.5, 0.3);
        assert!((l - 0.5).abs() < 1e-10);
        assert!((r - 0.3).abs() < 1e-10);
    }

    #[test]
    fn test_mono_collapse() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(0.0);
        for _ in 0..10000 {
            enhancer.process(0.8, 0.2);
        }
        let (l, r) = enhancer.process(0.8, 0.2);
        // Width 0 = mono, both channels should be the average
        assert!((l - r).abs() < 1e-4, "Width 0 should produce mono");
        assert!((l - 0.5).abs() < 1e-4, "Mono should be average of L and R");
    }

    #[test]
    fn test_phase_safe() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.5);
        // If input is mono, output should remain mono (no artificial stereo)
        let (l, r) = enhancer.process(0.5, 0.5);
        assert!((l - r).abs() < 1e-10, "Mono input should stay mono");
    }

    #[test]
    fn test_widening_increases_stereo_separation() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);

        // Input with some stereo content
        let input_l = 0.7;
        let input_r = 0.3;

        enhancer.set_width(1.0);
        let (norm_l, norm_r) = enhancer.process(input_l, input_r);

        enhancer.set_width(1.5);
        let (wide_l, wide_r) = enhancer.process(input_l, input_r);

        // Widening should increase the L-R difference
        let normal_diff = (norm_l - norm_r).abs();
        let wide_diff = (wide_l - wide_r).abs();
        assert!(
            wide_diff > normal_diff,
            "Widening should increase stereo separation"
        );
    }

    #[test]
    fn test_width_clamped() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_width(5.0); // Should be clamped to 2.0
        assert!(
            (enhancer.width() - 2.0).abs() < 1e-10,
            "Width should be clamped to 2.0"
        );

        enhancer.set_width(-1.0); // Should be clamped to 0.0
        assert!(
            (enhancer.width() - 0.0).abs() < 1e-10,
            "Width should be clamped to 0.0"
        );
    }

    #[test]
    fn test_disabled_is_passthrough() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(false);
        enhancer.set_width(0.0); // Even with width=0
        let (l, r) = enhancer.process(0.8, 0.2);
        assert!(
            (l - 0.8).abs() < 1e-10,
            "Disabled enhancer should pass through"
        );
        assert!(
            (r - 0.2).abs() < 1e-10,
            "Disabled enhancer should pass through"
        );
    }

    #[test]
    fn test_mid_preserved() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.5);

        let input_l = 0.6;
        let input_r = 0.4;
        let (out_l, out_r) = enhancer.process(input_l, input_r);

        // The mid component (average) should always be preserved
        let input_mid = (input_l + input_r) * 0.5;
        let output_mid = (out_l + out_r) * 0.5;
        assert!(
            (input_mid - output_mid).abs() < 1e-10,
            "Mid component should be preserved"
        );
    }
}
