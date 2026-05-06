//! Gapless track-boundary crossfader.
//!
//! Stores the tail of the outgoing track in a stack-allocated buffer and
//! linearly cross-fades it into the head of the incoming track.
//! No heap allocation; the buffer is a fixed-size `[f32; GAPLESS_SAMPLES]`.

/// ~10 ms crossfade at 48 kHz stereo (480 frames × 2 channels).
pub const GAPLESS_SAMPLES: usize = 960;

pub struct GaplessSmoother {
    tail: [f32; GAPLESS_SAMPLES],
    tail_len: usize,
    pub enabled: bool,
}

impl GaplessSmoother {
    pub fn new() -> Self {
        Self {
            tail: [0.0; GAPLESS_SAMPLES],
            tail_len: 0,
            enabled: true,
        }
    }

    /// Capture the tail of the current track for the upcoming transition.
    pub fn capture_tail(&mut self, buf: &[f32]) {
        if !self.enabled {
            return;
        }
        let take = buf.len().min(GAPLESS_SAMPLES);
        let start = buf.len() - take;
        self.tail[..take].copy_from_slice(&buf[start..]);
        self.tail_len = take;
    }

    /// Blend the stored tail into the head of the next track.
    pub fn apply_to_head(&mut self, buf: &mut [f32]) {
        if !self.enabled || self.tail_len == 0 {
            return;
        }
        let n = self.tail_len.min(buf.len());
        for i in 0..n {
            let fade_in = i as f32 / n as f32;
            buf[i] = buf[i] * fade_in + self.tail[i] * (1.0 - fade_in);
        }
        self.tail_len = 0;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gapless_smoother_fades() {
        let mut gs = GaplessSmoother::new();
        let tail = vec![1.0f32; GAPLESS_SAMPLES];
        gs.capture_tail(&tail);
        let mut head = vec![0.0f32; GAPLESS_SAMPLES];
        gs.apply_to_head(&mut head);
        assert!(
            head[0] > 0.9,
            "first sample should be mostly tail: {}",
            head[0]
        );
        assert!(
            head[GAPLESS_SAMPLES - 1] < 0.1,
            "last sample should be mostly new head: {}",
            head[GAPLESS_SAMPLES - 1]
        );
    }

    #[test]
    fn disabled_smoother_is_noop() {
        let mut gs = GaplessSmoother::new();
        gs.enabled = false;
        gs.capture_tail(&vec![1.0f32; GAPLESS_SAMPLES]);
        let mut head = vec![0.0f32; 64];
        gs.apply_to_head(&mut head);
        assert!(head.iter().all(|&x| x == 0.0));
    }
}
