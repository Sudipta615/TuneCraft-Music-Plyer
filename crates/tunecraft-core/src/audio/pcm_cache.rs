//! Shared PCM sample cache for avoiding dual-decode overhead.
//!
//! Mood analysis previously decoded each file independently using Symphonia,
//! while playback used GStreamer — meaning each track was decoded twice.
//! This module provides a bounded LRU cache of decoded PCM samples keyed by
//! file hash, so that mood analysis can reuse already-decoded audio data.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Maximum number of seconds of audio to cache per track (for mood analysis,
/// only the first 90 seconds are needed).
#[allow(dead_code)]
const MAX_CACHE_DURATION_SECS: u64 = 90;

/// A cached PCM buffer for a single track.
#[derive(Debug, Clone)]
pub struct PcmBuffer {
    /// Interleaved stereo F32 samples.
    pub samples: Vec<f32>,
    /// Sample rate of the cached audio.
    pub sample_rate: u32,
    /// Number of channels (typically 2 for stereo).
    pub channels: u16,
}

impl PcmBuffer {
    /// Create a new PCM buffer.
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    /// Returns the duration of the cached audio in seconds.
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    /// Truncate the buffer to the first `max_secs` seconds of audio.
    /// Used to limit memory usage — mood analysis only needs the first 90s.
    ///
    /// Fix: Uses u64 arithmetic for the sample count calculation to prevent
    /// integer overflow on 32-bit platforms. At 192 kHz / 8 channels / 90 s,
    /// the product is 192000 × 8 × 90 = 138,240,000 which fits in u32, but
    /// intermediate overflows are possible with edge cases (e.g. very high
    /// sample rates combined with many channels). Using u64 throughout is safe.
    pub fn truncate_to_duration(&mut self, max_secs: u64) {
        if self.sample_rate == 0 || self.channels == 0 {
            return;
        }
        let max_samples = (self.sample_rate as u64)
            .saturating_mul(self.channels as u64)
            .saturating_mul(max_secs);
        let max_samples = max_samples.min(usize::MAX as u64) as usize;
        if self.samples.len() > max_samples {
            self.samples.truncate(max_samples);
        }
    }
}

/// A bounded LRU cache of decoded PCM buffers keyed by file hash.
///
/// The cache stores up to `capacity` entries, evicting the least-recently-used
/// entry when full. Each entry holds a `PcmBuffer` containing decoded F32
/// stereo samples for a track, limited to the first 90 seconds.
///
/// # Memory Usage
///
/// At 48 kHz stereo F32, 90 seconds = 48000 × 2 × 4 × 90 = 34,560,000 bytes ≈ 33 MB per entry.
/// Fix Issue #8: The previous comment claimed "~34.6 MB" and "~346 MB peak" (10 entries),
/// but the actual calculation is 48000 × 2 × 4 × 90 = 34,560,000 bytes = ~33 MB (not 34.6 MB).
/// With 5 entries (default), peak usage is ~165 MB — tuned for broad hardware compatibility
/// including older machines with 2-4 GB RAM. Since mood analysis scans each track once
/// and the cache primarily avoids dual-decode for the current + upcoming tracks, 5 entries
/// is sufficient without sacrificing quality or introducing noticeable re-decode overhead.
pub struct PcmCache {
    cache: Mutex<LruCache<String, PcmBuffer>>,
}

impl PcmCache {
    /// Create a new PCM cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(5).unwrap()),
            )),
        }
    }

    /// Create a new PCM cache with the default capacity (5 entries).
    ///
    /// At 48 kHz stereo F32, 90 seconds per entry, this uses ~165 MB peak —
    /// suitable for both modern and older hardware. Since mood analysis typically
    /// processes each track once, a smaller cache avoids unnecessary memory
    /// pressure without impacting audio quality.
    pub fn with_default_capacity() -> Self {
        Self::new(5)
    }

    /// Insert a PCM buffer into the cache, keyed by file hash.
    /// If the cache is full, the least-recently-used entry is evicted.
    pub fn insert(&self, file_hash: &str, buffer: PcmBuffer) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.put(file_hash.to_string(), buffer);
    }

    /// Look up a PCM buffer by file hash.
    /// Returns `None` if the buffer is not in the cache.
    /// A hit promotes the entry to the most-recently-used position.
    pub fn get(&self, file_hash: &str) -> Option<PcmBuffer> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(file_hash).cloned()
    }

    /// Check if a file hash is in the cache without promoting it.
    pub fn contains(&self, file_hash: &str) -> bool {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.contains(file_hash)
    }

    /// Remove a specific entry from the cache.
    pub fn remove(&self, file_hash: &str) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.pop(file_hash);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }

    /// Returns the number of entries currently in the cache.
    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcm_buffer_duration() {
        let buf = PcmBuffer::new(vec![0.0f32; 96000], 48000, 2);
        assert!((buf.duration_secs() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_pcm_buffer_truncate() {
        let mut buf = PcmBuffer::new(vec![0.0f32; 48000 * 2 * 180], 48000, 2);
        assert!((buf.duration_secs() - 180.0).abs() < 0.1);
        buf.truncate_to_duration(90);
        assert!((buf.duration_secs() - 90.0).abs() < 0.1);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = PcmCache::new(2);
        let buf = PcmBuffer::new(vec![0.5f32; 1000], 44100, 2);
        cache.insert("hash1", buf);
        assert!(cache.contains("hash1"));
        let retrieved = cache.get("hash1").unwrap();
        assert_eq!(retrieved.sample_rate, 44100);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = PcmCache::new(2);
        cache.insert("hash1", PcmBuffer::new(vec![0.0; 100], 44100, 2));
        cache.insert("hash2", PcmBuffer::new(vec![0.0; 100], 44100, 2));
        cache.insert("hash3", PcmBuffer::new(vec![0.0; 100], 44100, 2));
        assert!(!cache.contains("hash1")); // evicted
        assert!(cache.contains("hash2"));
        assert!(cache.contains("hash3"));
    }

    #[test]
    fn test_cache_clear() {
        let cache = PcmCache::new(5);
        cache.insert("hash1", PcmBuffer::new(vec![0.0; 100], 44100, 2));
        cache.clear();
        assert!(cache.is_empty());
    }
}
