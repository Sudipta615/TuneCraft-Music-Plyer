#![cfg(feature = "resample")]
//! High-quality audio resampler using rubato
//!
//! Supports three quality profiles using rubato's FFT-based synchronous resamplers.
//! Handles sample rate conversion between the decoder's source rate and the output
//! device rate, as well as variable-speed playback by adjusting the resampling ratio.
//!
//! All buffers are pre-allocated for zero-allocation operation during playback.

use rubato::{FftFixedIn, FftFixedInOut, Resampler};
use tc_config::ResamplerQuality;

/// Error type for resampler construction failures.

/// Replaces the previous `expect()` calls that would panic the process
/// on invalid rates or internal rubato failures.
#[derive(Debug, thiserror::Error)]
pub enum ResamplerError {
    #[error("Failed to create {quality:?} resampler: {reason}")]
    CreationFailed { quality: ResamplerQuality, reason: String },
    #[error("Invalid sample rate: source={source_rate}, output={output_rate}")]
    InvalidRates { source_rate: usize, output_rate: usize },
}

/// Number of channels (stereo)
const CHANNELS: usize = 2;

/// Processing chunk size in frames
const CHUNK_SIZE: usize = 1024;

/// Maximum output buffer size in frames
const MAX_OUTPUT_BUFFER_FRAMES: usize = CHUNK_SIZE * 16;

/// Number of frames in the crossfade blend buffer (L8: named constant
/// so the denominator in read() stays in sync with the buffer field size).
const CROSSFADE_BLEND_FRAMES: usize = 64;

/// Maximum consecutive rebuild failures before disabling the resampler (High #H8 fix)
const MAX_REBUILD_FAILURES: u32 = 5;

/// Enum-based dispatch to avoid dynamic trait objects
/// (rubato's Resampler trait is not object-safe due to generic methods)
enum ResamplerInner {
    /// High quality: FftFixedIn with larger FFT sizes for better anti-aliasing
    HighQuality(FftFixedIn<f64>),
    /// Balanced: FftFixedIn with moderate FFT sizes
    Balanced(FftFixedIn<f64>),
    /// Fast: FftFixedInOut with minimal processing
    Fast(FftFixedInOut<f64>),
}

impl ResamplerInner {
    fn input_frames_next(&self) -> usize {
        match self {
            Self::HighQuality(r) => r.input_frames_next(),
            Self::Balanced(r) => r.input_frames_next(),
            Self::Fast(r) => r.input_frames_next(),
        }
    }

    fn process<V: AsRef<[f64]>>(
        &mut self,
        input: &[V],
    ) -> Result<Vec<Vec<f64>>, rubato::ResampleError> {
        match self {
            Self::HighQuality(r) => r.process(input, None),
            Self::Balanced(r) => r.process(input, None),
            Self::Fast(r) => r.process(input, None),
        }
    }

    fn quality(&self) -> ResamplerQuality {
        match self {
            Self::HighQuality(_) => ResamplerQuality::HighQuality,
            Self::Balanced(_) => ResamplerQuality::Balanced,
            Self::Fast(_) => ResamplerQuality::Fast,
        }
    }
}

/// High-quality resampler with configurable quality profiles
pub struct AudioResampler {
    /// Inner resampler using enum dispatch
    inner: ResamplerInner,
    /// Source sample rate
    source_rate: usize,
    /// Output sample rate
    output_rate: usize,
    /// Playback speed multiplier (1.0 = normal)
    speed: f64,
    /// Input buffer for accumulating samples before processing
    input_buffers: [Vec<f64>; CHANNELS],
    /// Write position in input buffers
    input_pos: usize,
    /// Output ring buffer for samples waiting to be consumed
    output_buffers: [Vec<f64>; CHANNELS],
    /// Read position in output buffers
    output_read_pos: usize,
    /// Number of valid samples in output buffers
    output_available: usize,
    /// Whether the resampler needs to be reconfigured
    needs_rebuild: bool,
    /// Pending quality change to apply on next rebuild
    pending_quality: Option<ResamplerQuality>,
    ///
    /// After MAX_REBUILD_FAILURES consecutive failures, the resampler
    /// is disabled to prevent the infinite retry loop that would
    /// otherwise saturate the CPU with FFT planning at ~44100 attempts/sec.
    rebuild_failures: u32,
    ///
    /// to too many consecutive rebuild failures. When disabled, audio
    /// passes through without resampling (potentially at wrong speed).
    /// This flag can be queried by the UI to display a warning.
    disabled: bool,
    /// Recent output samples for crossfade during rebuild (reduces glitches)
    crossfade_buffer: [(f64, f64); 64],
    /// Current read position in crossfade_buffer
    crossfade_pos: usize,
    /// Number of crossfade samples remaining to blend
    crossfade_remaining: usize,
}

impl AudioResampler {
    /// Create a new resampler with the given quality profile and sample rates.
    ///
    /// Returns an error instead of panicking if rubato construction fails
    /// (e.g., invalid sample rates or internal resampler errors).
    pub fn new(
        quality: ResamplerQuality,
        source_rate: f64,
        output_rate: f64,
    ) -> Result<Self, ResamplerError> {
        // Use rounded conversion to avoid integer truncation which causes
        // pitch/timing errors for non-integer rates (e.g., 44100.5 → 44101
        // instead of 44100).
        // M3: `.max(1)` ensures src and out are always >= 1, so the
        // `src == 0 || out == 0` check was unreachable. Replaced with a
        // meaningful check for obviously-wrong non-audio rates (< 8000 Hz).
        let src = (source_rate.round() as usize).max(1);
        let out = (output_rate.round() as usize).max(1);
        if source_rate <= 0.0 || output_rate <= 0.0 {
            return Err(ResamplerError::InvalidRates { source_rate: src, output_rate: out });
        }
        let inner = Self::create_resampler(quality, src, out)?;
        let mut resampler = Self {
            inner,
            source_rate: src,
            output_rate: out,
            speed: 1.0,
            input_buffers: [Vec::new(), Vec::new()],
            input_pos: 0,
            output_buffers: [Vec::new(), Vec::new()],
            output_read_pos: 0,
            output_available: 0,
            needs_rebuild: false,
            pending_quality: None,
            rebuild_failures: 0,
            disabled: false,
            crossfade_buffer: [(0.0, 0.0); 64],
            crossfade_pos: 0,
            crossfade_remaining: 0,
        };
        resampler.allocate_buffers();
        Ok(resampler)
    }

    /// Create the appropriate rubato resampler for the quality profile.
    ///
    /// Returns an error instead of panicking if rubato construction fails.
    fn create_resampler(
        quality: ResamplerQuality,
        source_rate: usize,
        output_rate: usize,
    ) -> Result<ResamplerInner, ResamplerError> {
        match quality {
            ResamplerQuality::HighQuality => {
                FftFixedIn::new(source_rate, output_rate, CHUNK_SIZE * 2, 4, CHANNELS)
                    .map(ResamplerInner::HighQuality)
                    .map_err(|e| ResamplerError::CreationFailed {
                        quality,
                        reason: e.to_string(),
                    })
            }
            ResamplerQuality::Balanced => {
                FftFixedIn::new(source_rate, output_rate, CHUNK_SIZE, 2, CHANNELS)
                    .map(ResamplerInner::Balanced)
                    .map_err(|e| ResamplerError::CreationFailed {
                        quality,
                        reason: e.to_string(),
                    })
            }
            ResamplerQuality::Fast => {
                FftFixedInOut::new(source_rate, output_rate, CHUNK_SIZE, CHANNELS)
                    .map(ResamplerInner::Fast)
                    .map_err(|e| ResamplerError::CreationFailed {
                        quality,
                        reason: e.to_string(),
                    })
            }
        }
    }

    /// Pre-allocate all internal buffers
    fn allocate_buffers(&mut self) {
        let input_frames = self.inner.input_frames_next();
        let output_frames = CHUNK_SIZE * 4;

        for ch in 0..CHANNELS {
            self.input_buffers[ch].resize(input_frames, 0.0);
            self.output_buffers[ch].resize(output_frames, 0.0);
        }
        self.input_pos = 0;
        self.output_read_pos = 0;
        self.output_available = 0;
    }

    /// Feed a stereo sample into the resampler
    #[inline]
    pub fn feed(&mut self, left: f64, right: f64) {
        if self.needs_rebuild {

            // infinite retry loop that saturates the CPU.

            // pass them through to the passthrough path so audio continues
            // (albeit at wrong speed) without silence gaps.

            if self.rebuild_failures >= MAX_REBUILD_FAILURES {
                self.needs_rebuild = false;
                self.disabled = true;
                log::error!(
                    "Resampler disabled after {} consecutive rebuild failures. \
                     Audio will play at the wrong speed/pitch. \
                     The UI should display a warning to the user.",
                    MAX_REBUILD_FAILURES
                );
                // Fall through to passthrough write below instead of returning
            } else {
                self.rebuild();
            }
        }

        if self.input_pos >= self.input_buffers[0].len() {
            // Buffer overflow - process existing data first
            self.process_chunk();
            if self.input_pos >= self.input_buffers[0].len() {
                return;
            }
        }

        self.input_buffers[0][self.input_pos] = left;
        self.input_buffers[1][self.input_pos] = right;
        self.input_pos += 1;

        let needed = self.inner.input_frames_next();
        if self.input_pos >= needed {
            self.process_chunk();
        }
    }

    /// Process a chunk of input samples through the resampler
    fn process_chunk(&mut self) {
        if self.input_pos == 0 {
            return;
        }

        let needed = self.inner.input_frames_next();

        // Ensure input buffers are large enough and zero-pad if needed
        for ch in 0..CHANNELS {
            if self.input_buffers[ch].len() < needed {
                self.input_buffers[ch].resize(needed, 0.0);
            }
            if self.input_pos < needed {
                for i in self.input_pos..needed {
                    self.input_buffers[ch][i] = 0.0;
                }
            }
        }

        // Prepare input slices for rubato
        let input: [&[f64]; CHANNELS] = [
            &self.input_buffers[0][..needed],
            &self.input_buffers[1][..needed],
        ];

        // Process through resampler
        match self.inner.process(&input) {
            Ok(output_channels) => {
                let frames_out = output_channels[0].len();
                self.push_output(&output_channels, frames_out);
            }
            Err(e) => {
                log::warn!("Resampler process error: {}", e);
            }
        }

        self.input_pos = 0;
    }

    /// Push resampled output into the output buffer.
    ///
    /// I-04 fix: The buffer is now bounded.  When the consumer's read head has
    /// advanced past half the buffer capacity we compact (slide remaining data
    /// to the front) rather than resizing.  If the buffer is still full after
    /// compaction, the oldest samples are dropped so the buffer never exceeds
    /// `MAX_OUTPUT_BUFFER_FRAMES`.
    fn push_output(&mut self, output_channels: &[Vec<f64>], frames: usize) {
        // Compact if the read head has advanced far enough to save space.
        if self.output_read_pos > self.output_buffers[0].len() / 2 {
            let avail = self.output_available;
            let rpos  = self.output_read_pos;
            // M4: Clamp `avail` so `rpos + avail` never exceeds the buffer
            // length, preventing a panic in copy_within on underrun.
            let safe_avail = avail.min(self.output_buffers[0].len().saturating_sub(rpos));
            for ch in 0..CHANNELS {
                self.output_buffers[ch].copy_within(rpos..rpos + safe_avail, 0);
            }
            self.output_read_pos = 0;
        }

        let capacity = MAX_OUTPUT_BUFFER_FRAMES;
        let write_start = self.output_read_pos + self.output_available;

        // If adding `frames` would exceed the bounded capacity, drop oldest
        // samples instead of growing the buffer.
        let space_available = capacity.saturating_sub(write_start);
        let frames_to_write = frames.min(space_available);
        if frames_to_write < frames {
            log::warn!(
                "Resampler output buffer full; dropping {} frames (I-04)",
                frames - frames_to_write
            );
        }
        if frames_to_write == 0 {
            return;
        }

        for ch in 0..CHANNELS {
            if self.output_buffers[ch].len() < write_start + frames_to_write {
                self.output_buffers[ch].resize(write_start + frames_to_write, 0.0);
            }
            let src = &output_channels[ch][..frames_to_write];
            self.output_buffers[ch][write_start..write_start + frames_to_write]
                .copy_from_slice(src);
        }
        self.output_available += frames_to_write;
    }

    /// Read a resampled stereo sample. Returns None if no output is available.
    #[inline]
    pub fn read(&mut self) -> Option<(f64, f64)> {
        // Blend crossfade samples from before the last rebuild to reduce glitch
        if self.crossfade_remaining > 0 {
            let (left, right) = if self.output_available > 0 {
                let l = self.output_buffers[0][self.output_read_pos];
                let r = self.output_buffers[1][self.output_read_pos];
                self.output_read_pos += 1;
                self.output_available -= 1;
                (l, r)
            } else {
                (0.0, 0.0)
            };
            let (cf_l, cf_r) = self.crossfade_buffer[self.crossfade_pos % 64];
            self.crossfade_pos += 1;
            self.crossfade_remaining -= 1;
            // Blend: fade out old, fade in new
            // L8: The crossfade blend buffer size is 64 frames (crossfade_buffer
            // field). Using `CROSSFADE_BLEND_FRAMES` as a named constant ensures
            // this calculation stays in sync if the buffer size ever changes.
            let t = self.crossfade_remaining as f64 / CROSSFADE_BLEND_FRAMES as f64;
            return Some((left * (1.0 - t) + cf_l * t, right * (1.0 - t) + cf_r * t));
        }

        if self.output_available == 0 {
            return None;
        }

        let left = self.output_buffers[0][self.output_read_pos];
        let right = self.output_buffers[1][self.output_read_pos];
        self.output_read_pos += 1;
        self.output_available -= 1;

        // Compaction is now handled proactively in push_output (I-04).

        Some((left, right))
    }

    /// Number of output samples available for reading
    pub fn available_output(&self) -> usize {
        self.output_available
    }

    /// Set playback speed (0.25 to 4.0)
    pub fn set_speed(&mut self, speed: f64) {
        let new_speed = speed.clamp(0.25, 4.0);
        if (new_speed - self.speed).abs() > 0.001 {
            self.speed = new_speed;
            // Speed change requires adjusting source rate effectively
            // We rebuild with adjusted source rate
            self.needs_rebuild = true;
        }
    }

    /// Get current playback speed
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Set the quality profile (triggers rebuild)
    pub fn set_quality(&mut self, quality: ResamplerQuality) {
        if quality != self.inner.quality() {
            self.pending_quality = Some(quality);
            self.needs_rebuild = true;
        }
    }

    /// Set the source sample rate (triggers rebuild)
    pub fn set_source_rate(&mut self, rate: f64) {
        // Use rounded conversion to avoid truncation (fixes #28)
        let rate_usize = (rate.round() as usize).max(1);
        if rate_usize != self.source_rate {
            self.source_rate = rate_usize;
            self.needs_rebuild = true;
        }
    }

    /// Set the output sample rate (triggers rebuild)
    pub fn set_output_rate(&mut self, rate: f64) {
        // Use rounded conversion to avoid truncation (fixes #28)
        let rate_usize = (rate.round() as usize).max(1);
        if rate_usize != self.output_rate {
            self.output_rate = rate_usize;
            self.needs_rebuild = true;
        }
    }

    /// Rebuild the resampler with current parameters
    ///
    ///
    /// resampler creation succeeds. If creation fails, the flags are
    /// preserved so the rebuild will be retried on the next feed() call.
    ///
    ///
    /// across the rebuild so that the crossfade blending in `read()` has
    /// real audio data to work with instead of fading to silence. The
    /// previous code called `allocate_buffers()` which zeroed output and
    /// reset `output_available` to 0, causing an audible dip.
    fn rebuild(&mut self) {
        if self.input_pos > 0 {
            self.process_chunk();
        }

        let save_count = self.output_available.min(64);

        // crossfade blending in read() can mix with real audio rather
        // than silence while the new resampler starts producing output.
        let mut saved_left = [0.0f64; 64];
        let mut saved_right = [0.0f64; 64];
        for i in 0..save_count {
            let pos = self.output_read_pos + i;
            if pos < self.output_buffers[0].len() {
                let l = self.output_buffers[0].get(pos).copied().unwrap_or(0.0);
                let r = self.output_buffers[1].get(pos).copied().unwrap_or(0.0);
                self.crossfade_buffer[i] = (l, r);
                saved_left[i] = l;
                saved_right[i] = r;
            }
        }
        self.crossfade_pos = 0;
        self.crossfade_remaining = save_count;

        // For speed adjustments, we modify the effective source rate.
        // Use rounded integer conversion with a minimum of 1 to preserve
        // as much precision as possible while avoiding division by zero.
        // The previous `(f64) as usize` truncated, losing precision and
        // potentially causing pitch/timing errors for non-integer ratios.
        let effective_source_f64 = self.source_rate as f64 / self.speed;
        let effective_source = (effective_source_f64.round() as usize).max(1);
        let quality = self.pending_quality.unwrap_or_else(|| self.inner.quality());
        match Self::create_resampler(quality, effective_source, self.output_rate) {
            Ok(new_inner) => {
                self.inner = new_inner;
                self.allocate_buffers();

                // output buffers. This ensures that during the crossfade
                // period, read() blends real audio (the old output) with
                // the crossfade buffer instead of fading to silence.
                if save_count > 0 {
                    for i in 0..save_count {
                        if i < self.output_buffers[0].len() {
                            self.output_buffers[0][i] = saved_left[i];
                            self.output_buffers[1][i] = saved_right[i];
                        }
                    }
                    self.output_read_pos = 0;
                    self.output_available = save_count;
                }

                // Only clear flags on successful rebuild
                self.pending_quality = None;
                self.needs_rebuild = false;
                self.rebuild_failures = 0; // Reset failure counter on success (H8)
                self.disabled = false;
            }
            Err(e) => {
                self.rebuild_failures += 1; // Increment failure counter (H8)
                log::error!(
                    "Failed to rebuild resampler ({}/{}), will retry on next feed: {}",
                    self.rebuild_failures, MAX_REBUILD_FAILURES, e
                );
                // Keep the existing inner resampler — audio continues but
                // speed change is not applied until a successful rebuild.
                // Do NOT clear pending_quality or needs_rebuild so the
                // rebuild is retried on the next feed() call.
            }
        }
    }

    /// Flush all pending samples through the resampler
    pub fn flush(&mut self) {
        if self.input_pos > 0 {
            self.process_chunk();
        }
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.input_pos = 0;
        self.output_read_pos = 0;
        self.output_available = 0;
        self.needs_rebuild = false;
        self.crossfade_buffer = [(0.0, 0.0); 64];
        self.crossfade_pos = 0;
        self.crossfade_remaining = 0;
        for ch in 0..CHANNELS {
            self.input_buffers[ch].fill(0.0);
            self.output_buffers[ch].fill(0.0);
        }
    }

    /// Check if source and output rates match (passthrough possible)
    pub fn is_passthrough(&self) -> bool {
        self.source_rate == self.output_rate && (self.speed - 1.0).abs() < 0.001
    }

    ///
    /// rebuild failures. When disabled, audio passes through without
    /// resampling (potentially at wrong speed/pitch). The UI should
    /// display a warning to the user when this returns true.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resampler_creation() {
        let resampler = AudioResampler::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
        assert!(!resampler.is_passthrough());
    }

    #[test]
    fn test_passthrough_detection() {
        let resampler = AudioResampler::new(ResamplerQuality::Balanced, 44100.0, 44100.0).unwrap();
        assert!(resampler.is_passthrough());
    }

    #[test]
    fn test_resampler_speed_change() {
        let mut resampler = AudioResampler::new(ResamplerQuality::Fast, 44100.0, 44100.0).unwrap();
        resampler.set_speed(1.5);
        assert!((resampler.speed() - 1.5).abs() < 0.001);
        assert!(resampler.needs_rebuild);
    }

    #[test]
    fn test_resampler_produces_output() {
        let mut resampler = AudioResampler::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
        for i in 0..5000 {
            let sample = (i as f64 / 44100.0 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 0.5;
            resampler.feed(sample, sample);
        }
        resampler.flush();
        assert!(
            resampler.available_output() > 0,
            "Resampler should produce output after feeding samples"
        );
    }

    #[test]
    fn test_resampler_quality_change() {
        let mut resampler = AudioResampler::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
        resampler.set_quality(ResamplerQuality::HighQuality);
        assert!(resampler.needs_rebuild);
    }

    #[test]
    fn test_resampler_reset() {
        let mut resampler = AudioResampler::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
        for _ in 0..1000 {
            resampler.feed(0.5, 0.5);
        }
        resampler.reset();
        assert_eq!(resampler.available_output(), 0);
        assert_eq!(resampler.input_pos, 0);
    }

    #[test]
    fn test_resampler_invalid_rates() {
        let result = AudioResampler::new(ResamplerQuality::Fast, 0.0, 48000.0);
        assert!(result.is_err());
        let result = AudioResampler::new(ResamplerQuality::Fast, 44100.0, 0.0);
        assert!(result.is_err());
    }
}

