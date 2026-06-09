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

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.reset();
            }
        }
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
        if !self.enabled || (self.speed - 1.0).abs() < 0.001 {
            return (left, right);
        }

        self.input_buffer_l.push_back(left);
        self.input_buffer_r.push_back(right);

        // We need at least window_size samples to do anything meaningful
        if self.input_buffer_l.len() < self.window_size {
            return (0.0, 0.0);
        }

        // Basic linear interpolation for the read pointer
        let pos_floor = self.read_pos.floor() as usize;
        let pos_frac = self.read_pos - pos_floor as f32;

        let out_l = if pos_floor + 1 < self.input_buffer_l.len() {
            let s1 = self.input_buffer_l[pos_floor];
            let s2 = self.input_buffer_l[pos_floor + 1];
            s1 + pos_frac * (s2 - s1)
        } else {
            self.input_buffer_l[pos_floor]
        };

        let out_r = if pos_floor + 1 < self.input_buffer_r.len() {
            let s1 = self.input_buffer_r[pos_floor];
            let s2 = self.input_buffer_r[pos_floor + 1];
            s1 + pos_frac * (s2 - s1)
        } else {
            self.input_buffer_r[pos_floor]
        };

        // Advance read pointer by speed
        self.read_pos += self.speed;

        // When read pointer gets too far, wrap it back (creates repeating grains or skipping)
        if self.read_pos >= self.window_size as f32 {
            self.read_pos -= self.window_size as f32;

            // Discard old samples
            let discard = self.window_size;
            if self.input_buffer_l.len() > discard {
                self.input_buffer_l.drain(0..discard);
                self.input_buffer_r.drain(0..discard);
            } else {
                self.input_buffer_l.clear();
                self.input_buffer_r.clear();
            }
        }

        // Apply a basic fade (Hanning window shape approximation) to reduce clicks at grain boundaries
        let window_phase = (self.read_pos / self.window_size as f32) * std::f32::consts::PI;
        let window = window_phase.sin();

        (out_l * window, out_r * window)
    }

    pub fn reset(&mut self) {
        self.input_buffer_l.clear();
        self.input_buffer_r.clear();
        self.read_pos = 0.0;
    }
}
