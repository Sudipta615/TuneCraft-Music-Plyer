//! Pure Rust Time Stretching (Granular Overlap-Add)
//! A basic implementation of pitch-independent time stretching.

use std::collections::VecDeque;

pub struct TimeStretcher {
    enabled: bool,
    speed: f32,
    sample_rate: f32,

    // Simplified granular synthesis buffer
    input_buffer_l: VecDeque<f32>,
    input_buffer_r: VecDeque<f32>,

    // Fractional read pointer
    read_pos: f32,

    window_size: usize,
}

impl TimeStretcher {
    pub fn new(sample_rate: f32) -> Self {
        // ~40ms window
        let window_size = (sample_rate * 0.04) as usize;
        Self {
            enabled: false,
            speed: 1.0,
            sample_rate,
            input_buffer_l: VecDeque::with_capacity(window_size * 4),
            input_buffer_r: VecDeque::with_capacity(window_size * 4),
            read_pos: 0.0,
            window_size,
        }
    }

    pub fn set_enabled(&mut self, _enabled: bool) {
        // TimeStretcher is currently broken (Hanning phase, allocations).
        // Force it to remain disabled.
        self.enabled = false;
        self.reset();
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.25, 4.0);
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.window_size = (sample_rate * 0.04) as usize;
        self.reset();
    }

    /// Feed a sample into the stretcher.
    /// Note: A true time stretcher changes the number of output samples relative to input samples.
    /// Since the current pipeline is 1:1 `process(l, r) -> (l, r)`, a true stretcher requires a buffer
    /// and a pull-based API. For this basic DSP node, we will do a granular read that returns exactly
    /// one sample out for one sample in, but advances the internal read pointer at `speed`.
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Bypassed: granular stretcher causes artifacts and allocations.
        (left, right)
    }

    pub fn reset(&mut self) {
        self.input_buffer_l.clear();
        self.input_buffer_r.clear();
        self.read_pos = 0.0;
    }
}
