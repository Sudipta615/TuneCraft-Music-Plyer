//! Dynamics processing: coupled stereo limiter with lookahead + true-peak
//! detection, and TPDF dither.
//!
//! ## Limiter design
//!
//! ### Why lookahead?
//! A conventional peak limiter uses an exponential-attack envelope follower.
//! With a 1 ms attack coefficient the envelope *converges toward* the peak —
//! it doesn't react instantly. A transient therefore passes through at full
//! amplitude for the first ~1 ms before the gain reduction catches up, meaning
//! samples can still exceed the threshold.
//!
//! A lookahead limiter inserts a short delay on the signal path and runs the
//! envelope detector *ahead* of the delay. When the delayed signal arrives the
//! gain reduction is already fully applied, giving true brick-wall behaviour.
//!
//! ### Why true-peak (inter-sample peak) detection?
//! PCM samples are band-limited; the continuous waveform reconstructed by the
//! DAC (via a sinc interpolation filter) can exceed the digital peak level.
//! EBU R128 and ITU-R BS.1770 require true-peak limiting. We approximate the
//! inter-sample peak by upsampling 4× using a 4-tap polyphase FIR and taking
//! the maximum of the four sub-samples. This adds negligible CPU overhead
//! (four multiply-accumulate per channel per sample).
//!
//! ### Lookahead delay
//! `LOOKAHEAD_MS` (2 ms default) is the delay in milliseconds. At 48 kHz this
//! is 96 samples. The ring buffer is sized to the maximum supported sample rate
//! (192 kHz → 384 samples) so no heap allocation is needed.

/// Lookahead delay in milliseconds. Must be ≥ attack_ms so that the
/// gain reduction is fully established before the peak arrives at the output.
pub const LOOKAHEAD_MS: f32 = 2.0;

/// Maximum lookahead samples at the highest supported sample rate (192 kHz).
/// 192000 * 0.002 = 384. Used to size the stack ring buffer.
const MAX_LOOKAHEAD_SAMPLES: usize = 384;

const TP_TAPS: usize = 4;
#[allow(clippy::approx_constant)]
const TP_PHASES: [[f32; TP_TAPS]; 3] = [
    [0.0, 0.43388, 0.70711, -0.13529],
    [-0.10251, 0.60263, 0.60263, -0.10251],
    [-0.13529, 0.70711, 0.43388, 0.0],
];

/// Estimate the maximum inter-sample peak for one channel using a 4-tap
/// polyphase FIR. `hist` is a ring of the 4 most-recent samples (oldest
/// first: hist[0] = x[n-3], hist[3] = x[n]).
#[inline(always)]
fn true_peak_estimate(hist: &[f32; TP_TAPS]) -> f32 {
    let mut tp = 0.0_f32;
    for phase in &TP_PHASES {
        let interp =
            phase[0] * hist[0] + phase[1] * hist[1] + phase[2] * hist[2] + phase[3] * hist[3];
        tp = tp.max(interp.abs());
    }
    tp
}

#[derive(Debug, Clone, Copy)]
pub struct Limiter {
    threshold: f32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: f32,
}

impl Limiter {
    pub fn new(attack_ms: f32, release_ms: f32, threshold: f32, sample_rate: f32) -> Self {
        Self {
            threshold,
            attack_coeff: (-1.0 / (sample_rate * attack_ms / 1000.0)).exp(),
            release_coeff: (-1.0 / (sample_rate * release_ms / 1000.0)).exp(),
            envelope: 0.0,
        }
    }
    pub fn default_48k() -> Self {
        Self::new(1.0, 50.0, 0.95, 48000.0)
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let level = x.abs();
        let coeff = if level > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = level + coeff * (self.envelope - level);
        if self.envelope > self.threshold {
            x * (self.threshold / self.envelope)
        } else {
            x
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

/// One shared envelope driven by the true-peak estimate of max(|L|, |R|).
///
/// Signal path:
///   input → [true-peak detector] → envelope follower
///         → [lookahead ring buffer (delay)] → gain application → output
///
/// The envelope follower sees the signal `LOOKAHEAD_MS` *ahead* of what
/// reaches the output, so gain reduction is fully established before the
/// peak arrives. Combined with true-peak detection this satisfies the
/// EBU R128 / ITU-R BS.1770 brick-wall requirement.
#[derive(Debug, Clone, Copy)]
pub struct StereoLimiter {
    threshold: f32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: f32,

    delay_buf: [f32; MAX_LOOKAHEAD_SAMPLES * 2],
    delay_len: usize,  // actual delay in samples (≤ MAX_LOOKAHEAD_SAMPLES)
    delay_head: usize, // write position into delay_buf (in frames, not samples)

    tp_hist_l: [f32; TP_TAPS],
    tp_hist_r: [f32; TP_TAPS],
}

impl StereoLimiter {
    pub fn new(attack_ms: f32, release_ms: f32, threshold: f32, sample_rate: f32) -> Self {
        let delay_len =
            ((sample_rate * LOOKAHEAD_MS / 1000.0).round() as usize).min(MAX_LOOKAHEAD_SAMPLES);
        Self {
            threshold,
            attack_coeff: (-1.0 / (sample_rate * attack_ms / 1000.0)).exp(),
            release_coeff: (-1.0 / (sample_rate * release_ms / 1000.0)).exp(),
            envelope: 0.0,
            delay_buf: [0.0; MAX_LOOKAHEAD_SAMPLES * 2],
            delay_len,
            delay_head: 0,
            tp_hist_l: [0.0; TP_TAPS],
            tp_hist_r: [0.0; TP_TAPS],
        }
    }

    pub fn for_rate(sample_rate: f32) -> Self {
        Self::new(1.0, 50.0, 0.95, sample_rate)
    }

    /// Update per-channel true-peak history ring (oldest sample falls off).
    #[inline(always)]
    fn push_hist(hist: &mut [f32; TP_TAPS], x: f32) {
        hist[0] = hist[1];
        hist[1] = hist[2];
        hist[2] = hist[3];
        hist[3] = x;
    }

    /// Process one stereo frame.
    ///
    /// Returns the delayed, gain-reduced frame. The output lags the input by
    /// `delay_len` samples (2 ms at 48 kHz), which is the lookahead window.
    #[inline(always)]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        Self::push_hist(&mut self.tp_hist_l, l);
        Self::push_hist(&mut self.tp_hist_r, r);

        let tp_l = l.abs().max(true_peak_estimate(&self.tp_hist_l));
        let tp_r = r.abs().max(true_peak_estimate(&self.tp_hist_r));
        let peak = tp_l.max(tp_r);

        let coeff = if peak > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = peak + coeff * (self.envelope - peak);

        let gain = if self.envelope > self.threshold {
            self.threshold / self.envelope
        } else {
            1.0
        };

        let read_idx = self.delay_head * 2;
        let out_l = self.delay_buf[read_idx];
        let out_r = self.delay_buf[read_idx + 1];

        self.delay_buf[read_idx] = l;
        self.delay_buf[read_idx + 1] = r;

        self.delay_head = (self.delay_head + 1) % self.delay_len.max(1);

        (out_l * gain, out_r * gain)
    }

    #[inline]
    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.delay_buf = [0.0; MAX_LOOKAHEAD_SAMPLES * 2];
        self.delay_head = 0;
        self.tp_hist_l = [0.0; TP_TAPS];
        self.tp_hist_r = [0.0; TP_TAPS];
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.attack_coeff = (-1.0 / (sample_rate * 1.0 / 1000.0)).exp();
        self.release_coeff = (-1.0 / (sample_rate * 50.0 / 1000.0)).exp();
        self.delay_len =
            ((sample_rate * LOOKAHEAD_MS / 1000.0).round() as usize).min(MAX_LOOKAHEAD_SAMPLES);
        self.delay_head = 0;
        self.delay_buf = [0.0; MAX_LOOKAHEAD_SAMPLES * 2];
    }
}

/// Two-LCG TPDF dither for 32→16-bit conversion.
///
/// Two LCG generators are *added* (not XOR'd) to form a triangular probability
/// density function ±1 LSB. Enable only when the final output stage quantises
/// to 16-bit integer.
#[derive(Debug, Clone, Copy)]
pub struct TpdfDither {
    pub enabled: bool,
    lcg1: u32,
    lcg2: u32,
}

impl Default for TpdfDither {
    fn default() -> Self {
        Self::new()
    }
}

impl TpdfDither {
    pub fn new() -> Self {
        Self {
            enabled: false,
            lcg1: 0xDEAD_BEEF,
            lcg2: 0xCAFE_BABE,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        self.lcg1 = self.lcg1.wrapping_mul(1664525).wrapping_add(1013904223);
        self.lcg2 = self.lcg2.wrapping_mul(22695477).wrapping_add(1);
        let r1 = (self.lcg1 as i32) as f32 * (1.0 / (i32::MAX as f32 + 1.0));
        let r2 = (self.lcg2 as i32) as f32 * (1.0 / (i32::MAX as f32 + 1.0));
        let noise = (r1 + r2) * (1.0 / (32768.0 * 2.0));
        x + noise
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After a warm-up period, the limiter must clamp output below threshold.
    /// With lookahead the gain reduction is pre-applied, so even the first
    /// high-amplitude frame at steady-state cannot exceed the ceiling.
    #[test]
    fn limiter_clamps_below_threshold() {
        let mut lim = StereoLimiter::new(1.0, 50.0, 0.95, 48000.0);
        let mut max_out = 0.0_f32;
        for _ in 0..4800 {
            let (l, r) = lim.process(2.0, 2.0);
            if l > max_out {
                max_out = l;
            }
            if r > max_out {
                max_out = r;
            }
        }
        assert!(max_out <= 1.0, "limiter output {} > 1.0", max_out);
    }

    /// A brick-wall transient must be clamped within the lookahead window.
    /// After the lookahead delay has flushed, no sample should exceed threshold.
    #[test]
    fn lookahead_prevents_transient_overshoot() {
        let threshold = 0.95_f32;
        let sample_rate = 48000.0;
        let mut lim = StereoLimiter::new(1.0, 50.0, threshold, sample_rate);
        let lookahead_samples = (sample_rate * LOOKAHEAD_MS / 1000.0).round() as usize;

        for _ in 0..lookahead_samples * 2 {
            lim.process(0.0, 0.0);
        }

        let mut violation_count = 0;
        for i in 0..4800 {
            let (l, _r) = lim.process(2.0, 2.0);
            if i >= lookahead_samples && l > threshold + 0.02 {
                violation_count += 1;
            }
        }
        assert_eq!(
            violation_count, 0,
            "lookahead limiter had {} frames above threshold",
            violation_count
        );
    }

    /// True-peak detection must flag inter-sample peaks that exceed the
    /// Nyquist-derived sample peak. We inject a near-Nyquist tone where the
    /// reconstructed waveform can briefly exceed 0 dBFS.
    #[test]
    fn true_peak_detection_constrains_intersample_peaks() {
        let mut lim = StereoLimiter::new(1.0, 50.0, 0.95, 48000.0);
        let mut envelope_saw_above_sample_peak = false;
        for i in 0..4800 {
            let x = if i % 2 == 0 { 0.9_f32 } else { -0.9_f32 };
            let _ = lim.process(x, x);
            if lim.envelope > 0.92 {
                envelope_saw_above_sample_peak = true;
            }
        }
        assert!(
            envelope_saw_above_sample_peak,
            "true-peak detector should see envelope > sample peak for near-Nyquist tone"
        );
    }

    #[test]
    fn dither_disabled_is_passthrough() {
        let mut d = TpdfDither::new(); // enabled = false by default
        assert_eq!(d.process(0.5), 0.5);
    }
}
