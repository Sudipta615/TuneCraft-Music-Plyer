//! Off-thread audio analysis for visualization and metadata
//!
//! This module re-exports analysis types from the `tc-analysis` crate,
//! which is the canonical implementation. Previously, a duplicate
//! implementation lived here; it has been consolidated to avoid drift.
//!
//! The `AnalysisBuffer` type is still defined here because it is tightly
//! coupled to the engine's real-time audio callback path (it uses the same
//! lock-free patterns as `FixedFrameBuffer`), but its write-ordering bug
//! has been fixed.

pub mod spectrum;
pub mod waveform;

// Re-export the canonical analysis types from tc-analysis
use std::{
    cell::UnsafeCell,
    sync::atomic::{fence, AtomicUsize, Ordering},
};

// Re-export engine-internal analysis types
pub use spectrum::SpectrumAnalyzer;
pub use tc_analysis::{
    BpmDetector, MoodClassifier, TrackAnalysis, WaveformGenerator as FileWaveformGenerator,
};
pub use waveform::WaveformGenerator;

/// Error type for analysis construction failures.
///
/// Replaces the previous `assert!` / `panic!` paths with recoverable errors.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("AnalysisBuffer capacity must be > 0, got {0}")]
    InvalidCapacity(usize),
    #[error("AnalysisBuffer decimation_factor must be > 0, got {0}")]
    InvalidDecimationFactor(usize),
    #[error("FFT size must be >= 2 and a power of two, got {0}")]
    InvalidFftSize(usize),
    #[error("Sample rate must be > 0, got {0}")]
    InvalidSampleRate(f64),
    #[error("WaveformGenerator samples_per_pixel must be > 0, got {0}")]
    InvalidSamplesPerPixel(usize),
}

/// A decimated analysis buffer that captures audio for analysis
/// without affecting the audio thread performance.
///
/// ## Write ordering
///
/// The buffer writes the sample data **before** advancing the write position,
/// so readers never observe uninitialized slots. This fixes the previous
/// publish-before-write hazard where `write_pos` was advanced before the
/// sample was stored.
pub struct AnalysisBuffer {
    /// Decimated capture buffer (stores every Nth sample) wrapped in UnsafeCell
    /// to declare interior mutability to the compiler, making mutations sound.
    buffer: UnsafeCell<Vec<(f64, f64)>>, // (left, right) pairs
    write_pos: AtomicUsize,
    decimation_factor: usize,
    sample_counter: AtomicUsize,
    capacity: usize,
}

impl AnalysisBuffer {
    /// Create a new AnalysisBuffer.
    ///
    /// Returns an error (instead of panicking) if `capacity` is 0 or
    /// `decimation_factor` is 0.
    pub fn new(capacity: usize, decimation_factor: usize) -> Result<Self, AnalysisError> {
        if capacity == 0 {
            return Err(AnalysisError::InvalidCapacity(capacity));
        }
        if decimation_factor == 0 {
            return Err(AnalysisError::InvalidDecimationFactor(decimation_factor));
        }
        Ok(Self {
            buffer: UnsafeCell::new(vec![(0.0, 0.0); capacity]),
            write_pos: AtomicUsize::new(0),
            decimation_factor,
            sample_counter: AtomicUsize::new(0),
            capacity,
        })
    }

    /// Feed a stereo sample (called from DSP thread)
    /// Only stores every Nth sample based on decimation factor.
    ///
    /// The sample is written **before** the write position is advanced,
    /// ensuring readers never see stale/uninitialized data.
    #[inline]
    pub fn feed(&self, left: f64, right: f64) {
        let count = self.sample_counter.fetch_add(1, Ordering::Relaxed);
        if count % self.decimation_factor == 0 {
            let current_write = self.write_pos.load(Ordering::Relaxed);
            let pos = current_write % self.capacity;
            // SAFETY: Single writer (DSP thread), atomic position
            // never observe an uninitialized slot.
            // Note: A torn read of (f64, f64) is possible since 16 bytes
            // is not atomic on any mainstream architecture. This is
            // acceptable for visualization/analysis data — the reader
            // may occasionally see a mismatched left/right pair, but
            // this is documented and preferred over blocking or UB.
            unsafe {
                let ptr = (*self.buffer.get()).as_mut_ptr();
                *ptr.add(pos) = (left, right);
            }
            // Use a proper atomic fence (not compiler_fence) so that
            // on weakly-ordered architectures (ARM, AArch64) the CPU
            // cannot reorder the buffer write past the write_pos store.
            // The acquire fence on the reader side guarantees the sample
            // is visible once write_pos is observed as advanced.
            fence(Ordering::Release);
            self.write_pos.fetch_add(1, Ordering::Release);
        }
    }

    /// Read the current buffer contents for analysis
    pub fn read(&self) -> Vec<(f64, f64)> {
        let write_pos = self.write_pos.load(Ordering::Acquire) % self.capacity;
        let mut result = Vec::with_capacity(self.capacity);
        for i in 0..self.capacity {
            let pos = (write_pos + i) % self.capacity;
            // SAFETY: Read-only from analysis thread
            unsafe {
                let ptr = (*self.buffer.get()).as_ptr();
                result.push(*ptr.add(pos));
            }
        }
        result
    }

    pub fn reset(&self) {
        self.write_pos.store(0, Ordering::Release);
        self.sample_counter.store(0, Ordering::Release);

        // after resetting positions so readers never observe stale data.
        for i in 0..self.capacity {
            unsafe {
                let ptr = (*self.buffer.get()).as_mut_ptr();
                *ptr.add(i) = (0.0, 0.0);
            }
        }
    }
}

// SAFETY: Atomic operations for write position, single-writer pattern
unsafe impl Send for AnalysisBuffer {}
unsafe impl Sync for AnalysisBuffer {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_buffer_new() {
        let buf = AnalysisBuffer::new(1024, 4).unwrap();
        // Just verify it was created without panicking
        let data = buf.read();
        assert_eq!(data.len(), 1024);
    }

    #[test]
    fn test_analysis_buffer_zero_capacity() {
        assert!(AnalysisBuffer::new(0, 4).is_err());
    }

    #[test]
    fn test_analysis_buffer_zero_decimation() {
        assert!(AnalysisBuffer::new(1024, 0).is_err());
    }

    #[test]
    fn test_analysis_buffer_feed_and_read() {
        let buf = AnalysisBuffer::new(64, 1).unwrap(); // decimation=1 captures every sample
        for i in 0..32 {
            buf.feed(i as f64 * 0.1, i as f64 * 0.05);
        }
        let data = buf.read();
        // All 32 samples should have been captured (decimation=1)
        let nonzero: Vec<_> = data
            .iter()
            .filter(|(l, r)| *l != 0.0 || *r != 0.0)
            .collect();
        assert_eq!(nonzero.len(), 32);
        // First sample
        assert!((nonzero[0].0 - 0.0).abs() < 1e-12);
        assert!((nonzero[0].1 - 0.0).abs() < 1e-12);
        // Last sample (index 31)
        assert!((nonzero[31].0 - 3.1).abs() < 1e-10);
    }

    #[test]
    fn test_analysis_buffer_decimation() {
        let buf = AnalysisBuffer::new(64, 3).unwrap(); // capture every 3rd sample
        for i in 0..30 {
            buf.feed(i as f64, i as f64);
        }
        let data = buf.read();
        let nonzero: Vec<_> = data
            .iter()
            .filter(|(l, r)| *l != 0.0 || *r != 0.0)
            .collect();
        // Samples at indices 0, 3, 6, 9, ... 27 = 10 samples
        assert_eq!(nonzero.len(), 10);
    }

    #[test]
    fn test_analysis_buffer_reset() {
        let buf = AnalysisBuffer::new(32, 1).unwrap();
        for i in 0..16 {
            buf.feed(i as f64, 0.0);
        }
        buf.reset();
        let data = buf.read();
        // After reset, all data should be zeroed
        for (l, r) in &data {
            assert!((l - 0.0).abs() < 1e-12);
            assert!((r - 0.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_analysis_buffer_wrap_around() {
        let buf = AnalysisBuffer::new(8, 1).unwrap(); // tiny buffer
        for i in 0..20 {
            buf.feed(i as f64, 0.0);
        }
        let data = buf.read();
        // Should contain the last 8 samples written (indices 12-19)
        // Oldest first (circular buffer ordering)
        let nonzero: Vec<_> = data.iter().filter(|(l, _)| *l != 0.0).collect();
        assert_eq!(nonzero.len(), 8);
        assert!((nonzero[0].0 - 12.0).abs() < 1e-12);
        assert!((nonzero[7].0 - 19.0).abs() < 1e-12);
    }
}
