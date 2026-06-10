use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Maximum number of frames in the decode-to-DSP buffer
pub const DECODE_BUFFER_FRAMES: usize = 16384;
/// Maximum number of frames in the DSP-to-output buffer
pub const OUTPUT_BUFFER_FRAMES: usize = 8192;
/// Default sample rate
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;
/// Maximum channels we support
pub const MAX_CHANNELS: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("FixedFrameBuffer capacity must be > 0, got {0}")]
    InvalidCapacity(usize),
    #[error("AudioFrame channel count must be 1 or 2, got {0}")]
    InvalidChannelCount(u8),
}

/// A single audio frame (interleaved, up to MAX_CHANNELS)
#[derive(Debug, Clone, Copy)]
pub struct AudioFrame {
    pub channels: [f32; MAX_CHANNELS],
    pub num_channels: u8,
}

impl AudioFrame {
    #[inline]
    pub fn stereo(left: f32, right: f32) -> Self {
        Self {
            channels: [left, right],
            num_channels: 2,
        }
    }

    /// Create a mono frame. The sample is duplicated to both channels so that
    /// downstream stereo code (output device, stereo pipeline) receives the
    /// correct signal on both L and R instead of silence on the right channel.
    #[inline]
    pub fn mono(sample: f32) -> Self {
        Self {
            channels: [sample, sample],
            num_channels: 1,
        }
    }

    #[inline]
    pub fn zero(num_channels: u8) -> Result<Self, BufferError> {
        if num_channels == 0 || num_channels > MAX_CHANNELS as u8 {
            return Err(BufferError::InvalidChannelCount(num_channels));
        }
        Ok(Self {
            channels: [0.0; MAX_CHANNELS],
            num_channels,
        })
    }

    #[inline]
    pub fn zero_stereo() -> Self {
        Self {
            channels: [0.0; MAX_CHANNELS],
            num_channels: 2,
        }
    }

    #[inline]
    pub fn get(&self, channel: usize) -> f32 {
        self.channels.get(channel).copied().unwrap_or(0.0)
    }

    #[inline]
    pub fn set(&mut self, channel: usize, value: f32) {
        if channel < MAX_CHANNELS {
            self.channels[channel] = value;
        }
    }

    /// Scale all channel slots by `gain`.
    ///
    /// Previously only the first `num_channels` slots were scaled. If a
    /// mono frame was later promoted to stereo (e.g., by lerp), the unscaled
    /// channel[1] slot would produce incorrect gain. Scaling all MAX_CHANNELS
    /// slots is negligible overhead and prevents stale values from escaping.
    #[inline]
    pub fn scale(&mut self, gain: f32) {
        for ch in &mut self.channels {
            *ch *= gain;
        }
    }

    /// Interpolate between two frames.
    ///
    /// When mixing frames of different channel counts (e.g., mono + stereo),
    /// the result is promoted to the larger channel count. The missing channel
    /// in the narrower frame is treated as the value of channel[0] (centre
    /// duplication) rather than 0.0 (silence), which was the previous behaviour.
    /// Using 0.0 caused an abrupt amplitude drop on the wider channel during
    /// crossfades between mono and stereo sources.
    #[inline]
    pub fn lerp(&self, other: &AudioFrame, t: f32) -> AudioFrame {
        let max_ch = self.num_channels.max(other.num_channels) as usize;
        let mut result = *self;
        for i in 0..max_ch {
            // For a narrower frame, repeat channel[0] instead of using 0.0
            // to avoid a silent channel on the wider side of the crossfade.
            let a = if i < self.num_channels as usize {
                self.channels[i]
            } else {
                self.channels[0]
            };
            let b = if i < other.num_channels as usize {
                other.channels[i]
            } else {
                other.channels[0]
            };
            result.channels[i] = a * (1.0 - t) + b * t;
        }
        result.num_channels = max_ch as u8;
        result
    }
}

/// A chunk of audio frames for batch processing
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub frames: Vec<AudioFrame>,
    pub sample_rate: u32,
}

impl AudioChunk {
    pub fn new(sample_rate: u32, capacity: usize) -> Self {
        let mut frames = Vec::with_capacity(capacity);
        frames.resize(capacity, AudioFrame::stereo(0.0, 0.0));
        Self {
            frames,
            sample_rate,
        }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn num_channels(&self) -> u8 {
        self.frames.first().map(|f| f.num_channels).unwrap_or(2)
    }
}

// Previously push() and pop() were both &self methods on the same type,
// meaning nothing stopped two threads from calling push() concurrently
// (violating the SPSC invariant). We now model the invariant in the type
// system: create() returns the two halves, and only the Producer half
// exposes push(), while only the Consumer half exposes pop(). Arc ensures
// both halves can exist simultaneously without allowing two writers or
// two readers.

struct SharedBuffer {
    frames: UnsafeCell<Vec<AudioFrame>>,
    read_pos: AtomicUsize,
    write_pos: AtomicUsize,
    capacity: usize,
}

// SAFETY: The SharedBuffer is only accessed through the Producer/Consumer
// split. Producer (write path) is single-threaded by construction; Consumer
// (read path) is single-threaded by construction.  Atomic ordering ensures
// the writes of one are visible to the other.
unsafe impl Send for SharedBuffer {}
unsafe impl Sync for SharedBuffer {}

/// The write-half of the SPSC ring buffer.
/// Only one `Producer` may exist per buffer (enforced by Arc ownership pattern).
pub struct Producer {
    inner: Arc<SharedBuffer>,
}

/// The read-half of the SPSC ring buffer.
/// Only one `Consumer` may exist per buffer.
pub struct Consumer {
    inner: Arc<SharedBuffer>,
}

/// Create a linked Producer/Consumer pair for a lock-free SPSC ring buffer.
///
/// Returns an error if `capacity` is 0.
pub fn create_fixed_frame_buffer(capacity: usize) -> Result<(Producer, Consumer), BufferError> {
    if capacity == 0 {
        return Err(BufferError::InvalidCapacity(capacity));
    }
    let mut frames = Vec::with_capacity(capacity);
    frames.resize(capacity, AudioFrame::stereo(0.0, 0.0));
    let shared = Arc::new(SharedBuffer {
        frames: UnsafeCell::new(frames),
        read_pos: AtomicUsize::new(0),
        write_pos: AtomicUsize::new(0),
        capacity,
    });
    Ok((
        Producer {
            inner: Arc::clone(&shared),
        },
        Consumer { inner: shared },
    ))
}

impl Producer {
    /// Write a single frame. Returns false if the buffer is full.
    #[inline]
    pub fn push(&self, frame: AudioFrame) -> bool {
        let write = self.inner.write_pos.load(Ordering::Relaxed);
        let next = (write + 1) % self.inner.capacity;
        let read = self.inner.read_pos.load(Ordering::Acquire);
        if next == read {
            return false;
        }
        // SAFETY: Single producer; we've verified space exists above.
        unsafe {
            let ptr = (*self.inner.frames.get()).as_mut_ptr();
            *ptr.add(write) = frame;
        }
        self.inner.write_pos.store(next, Ordering::Release);
        true
    }

    /// Reset both positions.
    ///
    /// # Safety
    ///
    /// This violates the SPSC invariant by writing to `read_pos` from the
    /// producer side. The caller MUST ensure that both the producer and
    /// consumer are quiescent (neither is actively calling push/pop) before
    /// calling this method. Calling reset() while the consumer is reading
    /// creates a data race on `read_pos`.
    pub unsafe fn reset(&self) {
        self.inner.write_pos.store(0, Ordering::Release);
        self.inner.read_pos.store(0, Ordering::Release);
    }

    /// Approximate number of frames available (informational only — may be stale).
    ///
    /// This performs two separate atomic loads, so the value can be transiently
    /// inconsistent. Do not use for synchronization decisions.
    pub fn available_approx(&self) -> usize {
        let write = self.inner.write_pos.load(Ordering::Acquire);
        let read = self.inner.read_pos.load(Ordering::Acquire);
        if write >= read {
            write - read
        } else {
            self.inner.capacity - read + write
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }
}

impl Consumer {
    /// Read a single frame. Returns None if the buffer is empty.
    #[inline]
    pub fn pop(&self) -> Option<AudioFrame> {
        let read = self.inner.read_pos.load(Ordering::Relaxed);
        let write = self.inner.write_pos.load(Ordering::Acquire);
        if read == write {
            return None;
        }
        // SAFETY: Single consumer; we've verified data exists above.
        let frame = unsafe {
            let ptr = (*self.inner.frames.get()).as_ptr();
            *ptr.add(read)
        };
        let next = (read + 1) % self.inner.capacity;
        self.inner.read_pos.store(next, Ordering::Release);
        Some(frame)
    }

    /// Approximate number of frames available (informational only — may be stale).
    ///
    /// This performs two separate atomic loads, so the value can be transiently
    /// inconsistent. Do not use for synchronization decisions.
    pub fn available_approx(&self) -> usize {
        let write = self.inner.write_pos.load(Ordering::Acquire);
        let read = self.inner.read_pos.load(Ordering::Acquire);
        if write >= read {
            write - read
        } else {
            self.inner.capacity - read + write
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }
}

/// Compatibility shim: wraps the split Producer/Consumer pair behind a single
/// shareable handle. New code should prefer `create_fixed_frame_buffer`.
///
/// # SPSC Invariant
///
/// Although this type exposes both `push()` and `pop()` through `&self`,
/// the SPSC invariant MUST still be upheld: only ONE thread should call
/// `push()` and only ONE (different) thread should call `pop()`. Violating
/// this creates data races. This type exists solely for backward compatibility
/// with code that holds an `Arc<FixedFrameBuffer>`.
pub struct FixedFrameBuffer {
    producer: Producer,
    consumer: Consumer,
}

impl FixedFrameBuffer {
    pub fn new(capacity: usize) -> Result<Self, BufferError> {
        let (producer, consumer) = create_fixed_frame_buffer(capacity)?;
        Ok(Self { producer, consumer })
    }

    #[inline]
    pub fn push(&self, frame: AudioFrame) -> bool {
        self.producer.push(frame)
    }
    #[inline]
    pub fn pop(&self) -> Option<AudioFrame> {
        self.consumer.pop()
    }

    /// Approximate available count. Informational only; not safe for flow control.
    #[inline]
    pub fn available(&self) -> usize {
        self.producer.available_approx()
    }

    /// Reset both positions.
    ///
    /// # Safety
    ///
    /// The caller MUST ensure both the producer and consumer are quiescent.
    /// See [`Producer::reset()`] for details.
    pub unsafe fn reset(&self) {
        self.producer.reset();
    }
    pub fn capacity(&self) -> usize {
        self.producer.capacity()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    Play,
    Pause,
    Stop,
    /// Seek to position in seconds. Must be finite and >= 0; invalid values are ignored.
    Seek(f32),
    SetVolume(f32),
    SetSpeed(f32),
    NextTrack,
    PrevTrack,
    LoadTrack(u64),
    Shutdown,
    SetEqEnabled(bool),
    SetEqBand {
        index: usize,
        frequency: f32,
        gain_db: f32,
        q: f32,
        enabled: bool,
    },
    SetBassShelf(f32),
    SetTrebleShelf(f32),
    SetPreamp(f32),
    SetStereoWidth(f32),
    SetBalance(f32),
    SetDitherEnabled(bool),
    SetMidsideEq(bool),
    SetCrossfeedEnabled(bool),
    SetCrossfeedProfile(tc_config::types::enums::CrossfeedProfile),
    SetCrossfeedCustomParams {
        frequency_hz: f32,
        q: f32,
        delay_ms: f32,
        mix_db: f32,
    },
    SetCompressorEnabled(bool),
    SetCompressorBandParams {
        band: usize, // 0=Low, 1=Mid, 2=High
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    },
    /// Set shuffle on/off (used by MPRIS integration to propagate shuffle state to the engine)
    SetShuffle(bool),
    /// Set loop status: "None", "Track", "Playlist" (MPRIS-style)
    SetLoopStatus(String),
    /// Open a URI for playback (file:// URIs only)
    OpenUri(String),
    /// Prepare the next track for crossfading by pre-opening its decoder.
    /// The path is stored and the decoder is created when the crossfade
    /// trigger fires (track enters its final N seconds).
    PrepareNextTrack(std::path::PathBuf),
    /// Request stream recovery after a device disconnection or error.
    /// The engine will attempt to re-detect the output device, rebuild
    /// the resampler, and hot-swap the output stream.
    RecoverStream,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
}

#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    pub state: PlaybackState,
    pub position_secs: f32,
    pub duration_secs: f32,
    pub volume: f32,
    pub speed: f32,
    pub track_id: Option<u64>,
    pub sample_rate: u32,
    pub cpu_usage_pct: f32,
    /// Number of audio dropouts / CPU overloads detected
    pub cpu_overloads: u32,
    /// Whether the resampler has been disabled due to creation or rebuild failures.
    /// UI should display a warning when true.
    pub resampler_disabled: bool,
    /// Whether the convolution engine's loaded IR has a stale frequency
    /// mapping due to a sample rate change and needs to be reloaded.
    /// UI should display a warning (e.g., "Convolution IR may be inaccurate —
    /// please reload") when this is true. Cleared when a new IR is loaded
    /// or the engine is reset.
    pub convolution_ir_needs_reload: bool,
}

impl Default for PlaybackInfo {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume: 1.0,
            speed: 1.0,
            track_id: None,
            sample_rate: DEFAULT_SAMPLE_RATE,
            cpu_usage_pct: 0.0,
            cpu_overloads: 0,
            resampler_disabled: false,
            convolution_ir_needs_reload: false,
        }
    }
}

pub const DENORMAL_OFFSET: f32 = 1e-15;

#[inline(always)]
pub fn flush_denormal(sample: f32) -> f32 {
    // Branchless bitwise check for true denormals (exponent == 0)
    let bits = sample.to_bits();
    if (bits & 0x7F80_0000) == 0 {
        0.0
    } else {
        sample
    }
}

#[inline(always)]
pub fn prevent_denormal(sample: f32) -> f32 {
    // Add a tiny DC offset to prevent true denormals
    sample + DENORMAL_OFFSET
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_frame_stereo() {
        let f = AudioFrame::stereo(0.5, -0.3);
        assert_eq!(f.num_channels, 2);
        assert!((f.get(0) - 0.5).abs() < 1e-6);
        assert!((f.get(1) - (-0.3)).abs() < 1e-6);
        assert!((f.get(2) - 0.0).abs() < 1e-6); // out of range returns 0
    }

    #[test]
    fn test_audio_frame_mono() {
        let f = AudioFrame::mono(0.75);
        assert_eq!(f.num_channels, 1);
        assert!((f.get(0) - 0.75).abs() < 1e-6);
        assert!((f.get(1) - 0.75).abs() < 1e-6); // mono duplicates to ch1
    }

    #[test]
    fn test_audio_frame_zero() {
        let f = AudioFrame::zero(2).unwrap();
        assert_eq!(f.num_channels, 2);
        assert!((f.get(0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_audio_frame_zero_invalid_channels() {
        assert!(AudioFrame::zero(0).is_err());
        assert!(AudioFrame::zero(3).is_err());
    }

    #[test]
    fn test_audio_frame_scale() {
        let mut f = AudioFrame::stereo(1.0, 2.0);
        f.scale(0.5);
        assert!((f.get(0) - 0.5).abs() < 1e-6);
        assert!((f.get(1) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_audio_frame_lerp_same_channels() {
        let a = AudioFrame::stereo(0.0, 1.0);
        let b = AudioFrame::stereo(1.0, 0.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid.num_channels, 2);
        assert!((mid.get(0) - 0.5).abs() < 1e-6);
        assert!((mid.get(1) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_audio_frame_lerp_mono_stereo_promotes() {
        let a = AudioFrame::mono(0.4);
        let b = AudioFrame::stereo(0.6, 0.8);
        let result = a.lerp(&b, 0.5);
        assert_eq!(result.num_channels, 2);
        assert!((result.get(0) - 0.5).abs() < 1e-6);
        assert!((result.get(1) - 0.6).abs() < 1e-6); // mono ch0 duplicated, not 0
    }

    #[test]
    fn test_audio_frame_set() {
        let mut f = AudioFrame::stereo(0.0, 0.0);
        f.set(0, 0.5);
        assert!((f.get(0) - 0.5).abs() < 1e-6);
        f.set(5, 1.0); // out of range, should be no-op
        assert!((f.get(1) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_create_buffer_capacity() {
        let (prod, cons) = create_fixed_frame_buffer(16).unwrap();
        assert_eq!(prod.capacity(), 16);
        assert_eq!(cons.capacity(), 16);
    }

    #[test]
    fn test_create_buffer_zero_capacity_fails() {
        assert!(create_fixed_frame_buffer(0).is_err());
    }

    #[test]
    fn test_spsc_push_pop_single() {
        let (prod, cons) = create_fixed_frame_buffer(4).unwrap();
        let frame = AudioFrame::stereo(0.1, 0.2);
        assert!(prod.push(frame));
        let popped = cons.pop();
        assert!(popped.is_some());
        let f = popped.unwrap();
        assert!((f.get(0) - 0.1).abs() < 1e-6);
        assert!((f.get(1) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn test_spsc_pop_empty_returns_none() {
        let (_, cons) = create_fixed_frame_buffer(4).unwrap();
        assert!(cons.pop().is_none());
    }

    #[test]
    fn test_spsc_fill_and_drain() {
        let (prod, cons) = create_fixed_frame_buffer(16).unwrap();
        for i in 0..15 {
            assert!(prod.push(AudioFrame::stereo(i as f32, (i + 1) as f32)));
        }
        for i in 0..15 {
            let f = cons.pop().unwrap();
            assert!((f.get(0) - i as f32).abs() < 1e-6);
        }
        assert!(cons.pop().is_none());
    }

    #[test]
    fn test_spsc_wrap_around() {
        let (prod, cons) = create_fixed_frame_buffer(4).unwrap();
        // Fill 3 slots (capacity-1 usable in ring buffer)
        for i in 0..3 {
            assert!(prod.push(AudioFrame::stereo(i as f32, 0.0)));
        }
        // Buffer is full
        assert!(!prod.push(AudioFrame::stereo(99.0, 0.0)));
        // Drain one
        let f = cons.pop().unwrap();
        assert!((f.get(0) - 0.0).abs() < 1e-6);
        // Now we can push again
        assert!(prod.push(AudioFrame::stereo(3.0, 0.0)));
        // Drain remaining
        for expected in [1.0, 2.0, 3.0] {
            let f = cons.pop().unwrap();
            assert!((f.get(0) - expected).abs() < 1e-6);
        }
        assert!(cons.pop().is_none());
    }

    #[test]
    fn test_spsc_available_approx() {
        let (prod, cons) = create_fixed_frame_buffer(8).unwrap();
        assert_eq!(prod.available_approx(), 0);
        prod.push(AudioFrame::stereo(1.0, 0.0));
        let avail = prod.available_approx();
        assert!(avail >= 1);
        cons.pop();
    }

    #[test]
    fn test_fixed_frame_buffer_compat() {
        let buf = FixedFrameBuffer::new(8).unwrap();
        assert_eq!(buf.capacity(), 8);
        buf.push(AudioFrame::stereo(0.5, 0.5));
        let f = buf.pop().unwrap();
        assert!((f.get(0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_playback_info_default() {
        let info = PlaybackInfo::default();
        assert_eq!(info.state, PlaybackState::Stopped);
        assert_eq!(info.position_secs, 0.0);
        assert!((info.volume - 1.0).abs() < 1e-6);
        assert_eq!(info.cpu_overloads, 0);
        assert!(!info.resampler_disabled);
        assert!(!info.convolution_ir_needs_reload);
    }

    #[test]
    fn test_engine_command_debug_clone() {
        let cmd = EngineCommand::Seek(42.5);
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Seek"));
    }

    #[test]
    fn test_flush_denormal() {
        assert!((flush_denormal(0.0) - 0.0).abs() < 1e-15);
        // 1e-40 is a true denormal
        assert!((flush_denormal(1e-40) - 0.0).abs() < 1e-45);
        assert!((flush_denormal(1e-20) - 1e-20).abs() < 1e-25);
        assert!((flush_denormal(0.5) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn test_prevent_denormal() {
        let val = prevent_denormal(0.0);
        assert!((val - DENORMAL_OFFSET).abs() < 1e-15);
    }

    #[test]
    fn test_audio_chunk() {
        let chunk = AudioChunk::new(44100, 100);
        assert_eq!(chunk.len(), 100);
        assert_eq!(chunk.sample_rate, 44100);
        assert!(!chunk.is_empty());
        assert_eq!(chunk.num_channels(), 2);
    }

    #[test]
    fn test_audio_chunk_empty() {
        let chunk = AudioChunk::new(44100, 0);
        assert!(chunk.is_empty());
        assert_eq!(chunk.len(), 0);
    }
}
