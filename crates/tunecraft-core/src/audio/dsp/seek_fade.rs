//! Seek-fade ramp: click-free volume transition on seek.
//!
//! A linear ramp fades `volume_gain` to 0 (fade-out) then back to the
//! original target (fade-in), driven sample-by-sample inside
//! `DspEngine::process_buffer()`. This eliminates the audible click that
//! a hard mute/unmute produces.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekFadePhase {
    FadeOut,
    FadeIn,
}

pub struct SeekFadeRamp {
    /// Final volume_gain to restore after fade-in.
    pub target: f32,
    /// Per-sample increment/decrement (always positive).
    pub step_per_sample: f32,
    pub phase: SeekFadePhase,
}

impl SeekFadeRamp {
    /// Build a ramp from the current `base_volume` over `fade_ms` milliseconds.
    /// Returns `None` for a zero-duration hard cut or zero base_volume
    /// (which would cause a zero step_per_sample and an infinite ramp).
    pub fn new(base_volume: f32, fade_ms: u32, sample_rate: f32) -> Option<Self> {
        if fade_ms == 0 {
            return None;
        }
        if base_volume == 0.0 {
            return None;
        }
        let fade_samples = (sample_rate * fade_ms as f32 / 1000.0).max(1.0);
        Some(Self {
            target: base_volume,
            step_per_sample: base_volume / fade_samples,
            phase: SeekFadePhase::FadeOut,
        })
    }
}

/// Advance the seek-fade ramp by one stereo sample, mutating `volume_gain`.
///
/// Returns `true` when the ramp is complete and can be cleared.
#[inline]
pub fn advance(ramp: &mut SeekFadeRamp, volume_gain: &mut f32) -> bool {
    match ramp.phase {
        SeekFadePhase::FadeOut => {
            *volume_gain -= ramp.step_per_sample;
            if *volume_gain <= 0.0 {
                *volume_gain = 0.0;
                ramp.phase = SeekFadePhase::FadeIn;
            }
            false
        }
        SeekFadePhase::FadeIn => {
            *volume_gain += ramp.step_per_sample;
            if *volume_gain >= ramp.target {
                *volume_gain = ramp.target;
                return true; // ramp complete
            }
            false
        }
    }
}
