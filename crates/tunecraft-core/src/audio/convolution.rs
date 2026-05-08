//! Room correction convolution engine — overlap-add FFT convolution.
//!
//! ## Algorithm
//!
//! Uses the **overlap-add** method with a real-valued FFT (Cooley-Tukey radix-2
//! via the `rustfft` crate).  For an IR of length M and a block size of N:
//!
//!   1. Pad the IR with zeros to length (N + M - 1), rounded up to a power of
//!      two, and compute its DFT once at load time → `ir_fft`.
//!   2. On each call to `process_block()`, zero-pad the input block to the same
//!      FFT size, compute its DFT, multiply pointwise with `ir_fft`, compute
//!      the inverse DFT, and overlap-add the tail of the previous block.
//!   3. Emit only the first N samples as output; the remaining M-1 samples are
//!      saved as the overlap tail for the next block.
//!
//! The per-sample `process_advance()` entry point buffers samples into an
//! internal block accumulator and flushes via `process_block()` whenever a
//! full block is ready.  The tail is zero-padded on the first call.
//!
//! ## Complexity
//!
//! O(N log N) per block instead of O(N·M) — for a 1024-sample block and a
//! 4096-sample IR the FFT path is roughly 12× cheaper than direct convolution.
//!
//! ## Previous approach (removed)
//!
//! The old code multiplied each sample by `ir[pos % ir_len] / ir_len` and
//! advanced a circular position counter.  That is *not* convolution — it is
//! a periodically-weighted multiplication that distorts the spectrum by the
//! IR's sample values rather than its frequency response.

use anyhow::{Context, Result};
use gstreamer::prelude::*;
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;
use std::sync::Arc;

/// Minimum IR length accepted.
const MIN_IR_SAMPLES: usize = 64;

/// Block (partition) size N.  Must be a power of two.
/// 512 gives a good latency/efficiency tradeoff at typical sample rates:
/// 512 / 48000 ≈ 10.7 ms per block.
const BLOCK_SIZE: usize = 512;

/// FFT size = next power of two ≥ BLOCK_SIZE + max_ir_len − 1.
/// We allow IRs up to 65536 samples long before the FFT size grows beyond this.
/// Longer IRs automatically use a larger FFT.
fn fft_size_for_ir(ir_len: usize) -> usize {
    let min = BLOCK_SIZE + ir_len - 1;
    min.next_power_of_two()
}

/// Room correction convolution engine using overlap-add FFT convolution.
pub struct ConvolutionEngine {
    /// Pre-computed DFT of the zero-padded mono IR.
    ir_fft: Vec<Complex<f32>>,
    /// Length of the original (non-padded) IR.
    ir_len: usize,
    /// FFT size (power of two).
    fft_sz: usize,

    fft_plan: Arc<dyn rustfft::Fft<f32>>,
    ifft_plan: Arc<dyn rustfft::Fft<f32>>,

    /// Overlap tail from the previous block — length = ir_len − 1.
    overlap_l: Vec<f32>,
    overlap_r: Vec<f32>,

    /// Reusable FFT buffer for left channel.
    fft_buf_l: Vec<Complex<f32>>,
    /// Reusable FFT buffer for right channel.
    fft_buf_r: Vec<Complex<f32>>,
    /// Reusable temp output for left channel (process_advance).
    tmp_out_l: Vec<f32>,
    /// Reusable temp output for right channel (process_advance).
    tmp_out_r: Vec<f32>,
    /// Reusable buffer for overlap tail copy (left).
    tail_buf_l: Vec<f32>,
    /// Reusable buffer for overlap tail copy (right).
    tail_buf_r: Vec<f32>,

    input_l: Vec<f32>,
    input_r: Vec<f32>,
    output_l: Vec<f32>,
    output_r: Vec<f32>,
    /// Next read position in the output queue.
    out_head: usize,
    /// Number of valid samples currently in the output queue.
    out_len: usize,

    /// Number of channels in the original IR file (informational).
    _ir_channels: u16,
    /// IR sample rate.
    sample_rate: u32,

    /// Bypass flag — when false, `process_advance()` returns input unchanged.
    pub enabled: bool,
}

impl ConvolutionEngine {
    /// Create a `ConvolutionEngine` from pre-loaded **mono** IR data.
    ///
    /// The IR must have at least `MIN_IR_SAMPLES` (64) samples.
    pub fn new(ir_data: Vec<f32>, ir_channels: u16, sample_rate: u32) -> Result<Self> {
        let ir_len = ir_data.len();
        anyhow::ensure!(
            ir_len >= MIN_IR_SAMPLES,
            "IR too short: {} samples (minimum {})",
            ir_len,
            MIN_IR_SAMPLES
        );

        let fft_sz = fft_size_for_ir(ir_len);
        let mut planner = FftPlanner::<f32>::new();
        let fft_plan = planner.plan_fft_forward(fft_sz);
        let ifft_plan = planner.plan_fft_inverse(fft_sz);

        let mut ir_buf: Vec<Complex<f32>> = ir_data
            .iter()
            .map(|&s| Complex { re: s, im: 0.0 })
            .chain(std::iter::repeat_n(Complex::default(), fft_sz - ir_len))
            .collect();
        fft_plan.process(&mut ir_buf);
        let ir_fft = ir_buf;

        let tail_len = ir_len - 1;
        Ok(Self {
            ir_fft,
            ir_len,
            fft_sz,
            fft_plan,
            ifft_plan,
            overlap_l: vec![0.0; tail_len],
            overlap_r: vec![0.0; tail_len],
            fft_buf_l: vec![Complex::default(); fft_sz],
            fft_buf_r: vec![Complex::default(); fft_sz],
            tmp_out_l: vec![0.0f32; BLOCK_SIZE],
            tmp_out_r: vec![0.0f32; BLOCK_SIZE],
            tail_buf_l: vec![0.0f32; tail_len],
            tail_buf_r: vec![0.0f32; tail_len],
            input_l: Vec::with_capacity(BLOCK_SIZE),
            input_r: Vec::with_capacity(BLOCK_SIZE),
            output_l: vec![0.0; BLOCK_SIZE],
            output_r: vec![0.0; BLOCK_SIZE],
            out_head: 0,
            out_len: 0,
            _ir_channels: ir_channels,
            sample_rate,
            enabled: true,
        })
    }

    /// Load an impulse response from a WAV file using a GStreamer decode pipeline.
    ///
    /// The IR is decoded to F32LE. Mono IR files are handled natively;
    /// stereo IR files are downmixed to mono by averaging channels. The number
    /// of channels is detected from the decoded audio caps rather than assumed.
    pub fn load_from_wav(path: &Path) -> Result<Self> {
        super::engine::ensure_gstreamer_initialized()?;

        let uri = glib::filename_to_uri(path, None)
            .with_context(|| format!("path→URI: {}", path.display()))?;

        let pipeline = gstreamer::Pipeline::new();
        let uridecodebin = gstreamer::ElementFactory::make("uridecodebin")
            .build()
            .context("uridecodebin")?;
        uridecodebin.set_property("uri", uri.as_str());
        let audioconvert = gstreamer::ElementFactory::make("audioconvert")
            .build()
            .context("audioconvert")?;

        let caps = gstreamer::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .build();
        let capsfilter = gstreamer::ElementFactory::make("capsfilter")
            .build()
            .context("capsfilter")?;
        capsfilter.set_property("caps", &caps);

        let appsink_elem = gstreamer::ElementFactory::make("appsink")
            .build()
            .context("appsink")?;
        appsink_elem.set_property("emit-signals", true);
        appsink_elem.set_property("sync", false);
        appsink_elem.set_property("max-buffers", 100u32);

        let appsink: gstreamer_app::AppSink = appsink_elem
            .dynamic_cast()
            .map_err(|_| anyhow::anyhow!("appsink cast"))?;

        pipeline.add_many([
            &uridecodebin,
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;
        gstreamer::Element::link_many([
            &audioconvert,
            &capsfilter,
            appsink.upcast_ref::<gstreamer::Element>(),
        ])?;

        let ac_weak = audioconvert.downgrade();
        uridecodebin.connect_pad_added(move |_, src_pad| {
            let caps = src_pad
                .current_caps()
                .or_else(|| Some(src_pad.query_caps(None)));
            if let Some(caps) = caps {
                if let Some(s) = caps.structure(0) {
                    if !s.name().starts_with("audio/") {
                        return;
                    }
                }
            }
            let Some(ac) = ac_weak.upgrade() else { return };
            let Ok(sink) = ac.static_pad("sink").ok_or(()) else {
                return;
            };
            if !sink.is_linked() {
                src_pad.link(&sink).ok();
            }
        });

        let detected_rate = std::sync::Arc::new(std::sync::Mutex::new(44_100u32));
        let rate_ref = detected_rate.clone();
        let detected_channels = std::sync::Arc::new(std::sync::Mutex::new(1u16));
        let channels_ref = detected_channels.clone();
        let samples: std::sync::Arc<std::sync::Mutex<Vec<f32>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let samples_ref = samples.clone();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;

                    if let Some(caps) = sample.caps() {
                        if let Some(s) = caps.structure(0) {
                            if let Ok(rate) = s.get::<i32>("rate") {
                                *rate_ref.lock().unwrap_or_else(|e| e.into_inner()) = rate as u32;
                            }
                            if let Ok(ch) = s.get::<i32>("channels") {
                                *channels_ref.lock().unwrap_or_else(|e| e.into_inner()) = ch as u16;
                            }
                        }
                    }

                    let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
                    let map = buffer
                        .map_readable()
                        .map_err(|_| gstreamer::FlowError::Error)?;

                    let Some(raw) = crate::util::cast_u8_to_f32(&map) else {
                        return Err(gstreamer::FlowError::Error);
                    };
                    samples_ref
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .extend_from_slice(raw);
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        pipeline
            .set_state(gstreamer::State::Playing)
            .context("IR pipeline play")?;
        let bus = pipeline.bus().context("pipeline bus")?;

        let msg = bus.timed_pop_filtered(
            gstreamer::ClockTime::from_seconds(10),
            &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
        );

        if msg.is_none() {
            tracing::warn!(
                "IR decode pipeline did not finish within 10 s — forcing Null state \
                 and continuing with {} samples collected so far",
                samples.lock().unwrap_or_else(|e| e.into_inner()).len()
            );
            pipeline.set_state(gstreamer::State::Null)?;
        } else {
            pipeline.set_state(gstreamer::State::Null)?;
        }

        let raw = std::mem::take(&mut *samples.lock().unwrap_or_else(|e| e.into_inner()));
        let rate = *detected_rate.lock().unwrap_or_else(|e| e.into_inner());
        let channels = *detected_channels.lock().unwrap_or_else(|e| e.into_inner());

        let (mono, ir_channels) = if channels == 1 {
            (raw, 1)
        } else if channels == 2 {
            let downmixed: Vec<f32> = raw
                .chunks_exact(2)
                .map(|ch| (ch[0] + ch[1]) * 0.5)
                .collect();
            (downmixed, 2)
        } else {
            tracing::info!(
                "IR file has {} channels — downmixing to mono by averaging",
                channels
            );
            let downmixed: Vec<f32> = raw
                .chunks_exact(channels as usize)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();
            (downmixed, channels)
        };

        anyhow::ensure!(
            mono.len() >= MIN_IR_SAMPLES,
            "IR too short: {} samples (minimum {})",
            mono.len(),
            MIN_IR_SAMPLES,
        );

        Self::new(mono, ir_channels, rate)
    }

    /// Convolve one block of stereo samples using overlap-add.
    ///
    /// `in_l` / `in_r` must both be exactly `BLOCK_SIZE` samples long.
    /// Writes `BLOCK_SIZE` output samples to `out_l` / `out_r`.
    fn process_block(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        debug_assert_eq!(in_l.len(), BLOCK_SIZE);
        debug_assert_eq!(in_r.len(), BLOCK_SIZE);

        let tail_len = self.ir_len - 1;

        {
            let buf = &mut self.fft_buf_l;
            buf.fill(Complex::default());
            for (i, &s) in in_l.iter().enumerate() {
                buf[i] = Complex { re: s, im: 0.0 };
            }
            self.fft_plan.process(buf);

            let scale = 1.0 / self.fft_sz as f32;
            for (x, h) in buf.iter_mut().zip(self.ir_fft.iter()) {
                *x = *x * h * scale;
            }
            self.ifft_plan.process(buf);

            for (i, val) in self.overlap_l.iter().enumerate() {
                buf[i].re += val;
            }

            for (i, sample) in out_l.iter_mut().enumerate() {
                *sample = buf[i].re;
            }
            for (i, c) in buf[BLOCK_SIZE..BLOCK_SIZE + tail_len].iter().enumerate() {
                self.tail_buf_l[i] = c.re;
            }
            self.overlap_l.copy_from_slice(&self.tail_buf_l);
        }

        {
            let buf = &mut self.fft_buf_r;
            buf.fill(Complex::default());
            for (i, &s) in in_r.iter().enumerate() {
                buf[i] = Complex { re: s, im: 0.0 };
            }
            self.fft_plan.process(buf);

            let scale = 1.0 / self.fft_sz as f32;
            for (x, h) in buf.iter_mut().zip(self.ir_fft.iter()) {
                *x = *x * h * scale;
            }
            self.ifft_plan.process(buf);

            for (i, val) in self.overlap_r.iter().enumerate() {
                buf[i].re += val;
            }

            for (i, sample) in out_r.iter_mut().enumerate() {
                *sample = buf[i].re;
            }
            for (i, c) in buf[BLOCK_SIZE..BLOCK_SIZE + tail_len].iter().enumerate() {
                self.tail_buf_r[i] = c.re;
            }
            self.overlap_r.copy_from_slice(&self.tail_buf_r);
        }
    }

    /// Process one stereo sample using the overlap-add engine.
    ///
    /// Internally accumulates samples into a block buffer; when `BLOCK_SIZE`
    /// samples are ready, `process_block()` is invoked and the output queue
    /// is refilled.  Returns the convolved sample for this input, with a
    /// latency of `BLOCK_SIZE` samples (≈ 10.7 ms at 48 kHz).
    ///
    /// When `enabled` is false, accumulates samples and keeps the convolution
    /// state warm, but outputs the unconvolved input instead of the convolved
    /// output (bypass with same latency as the enabled path).
    #[inline]
    pub fn process_advance(&mut self, input_l: f32, input_r: f32) -> (f32, f32) {
        if self.ir_len == 0 {
            return (input_l, input_r);
        }

        self.input_l.push(input_l);
        self.input_r.push(input_r);

        if self.input_l.len() == BLOCK_SIZE {
            self.tmp_out_l.fill(0.0f32);
            self.tmp_out_r.fill(0.0f32);
            let in_l: Vec<f32> = std::mem::take(&mut self.input_l);
            let in_r: Vec<f32> = std::mem::take(&mut self.input_r);
            let mut tmp_out_l = std::mem::take(&mut self.tmp_out_l);
            let mut tmp_out_r = std::mem::take(&mut self.tmp_out_r);
            self.process_block(&in_l, &in_r, &mut tmp_out_l, &mut tmp_out_r);
            self.tmp_out_l = tmp_out_l;
            self.tmp_out_r = tmp_out_r;

            if self.enabled {
                self.output_l.copy_from_slice(&self.tmp_out_l);
                self.output_r.copy_from_slice(&self.tmp_out_r);
            } else {
                self.output_l.copy_from_slice(&in_l);
                self.output_r.copy_from_slice(&in_r);
            }
            self.out_head = 0;
            self.out_len = BLOCK_SIZE;
        }

        if self.out_len > 0 {
            let i = self.out_head;
            self.out_head += 1;
            self.out_len -= 1;
            (self.output_l[i], self.output_r[i])
        } else {
            (0.0, 0.0)
        }
    }

    /// Non-advancing `process()` stub kept for API compatibility.
    ///
    /// This delegates to `process_advance()` — call sites that previously used
    /// the stateless (and incorrect) `process()` should be migrated to
    /// `process_advance()`.  This method is intentionally `&mut self` to
    /// force the migration.
    #[inline]
    pub fn process(&mut self, input_l: f32, input_r: f32) -> (f32, f32) {
        self.process_advance(input_l, input_r)
    }

    /// Reset all convolution state (overlap tails and block accumulators).
    pub fn reset(&mut self) {
        self.overlap_l.fill(0.0);
        self.overlap_r.fill(0.0);
        self.fft_buf_l.fill(Complex::default());
        self.fft_buf_r.fill(Complex::default());
        self.tmp_out_l.fill(0.0f32);
        self.tmp_out_r.fill(0.0f32);
        self.tail_buf_l.fill(0.0f32);
        self.tail_buf_r.fill(0.0f32);
        self.input_l.clear();
        self.input_r.clear();
        self.output_l.fill(0.0);
        self.output_r.fill(0.0);
        self.out_head = 0;
        self.out_len = 0;
    }

    /// Update the sample rate (informational; IR is not resampled).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate <= 0.0 {
            tracing::warn!(
                "ConvolutionEngine: invalid sample rate {}, ignoring",
                sample_rate
            );
            return;
        }
        self.sample_rate = sample_rate as u32;
    }

    pub fn ir_length(&self) -> usize {
        self.ir_len
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine(ir: Vec<f32>) -> ConvolutionEngine {
        ConvolutionEngine::new(ir, 1, 44_100).unwrap()
    }

    #[test]
    fn bypass_returns_input_with_latency() {
        let ir = vec![1.0; 128]; // non-identity IR (all 1.0 = averaging filter)
        let mut e = make_engine(ir);
        e.enabled = false;
        for _ in 0..BLOCK_SIZE {
            let (l, r) = e.process_advance(0.5, -0.3);
            assert!(l.abs() < 1e-6, "latency window L should be 0, got {}", l);
            assert!(r.abs() < 1e-6, "latency window R should be 0, got {}", r);
        }
        let (l, r) = e.process_advance(0.5, -0.3);
        assert!((l - 0.5).abs() < 1e-4, "bypass L should be 0.5, got {}", l);
        assert!((r + 0.3).abs() < 1e-4, "bypass R should be -0.3, got {}", r);
    }

    #[test]
    fn identity_ir_passes_signal_with_latency() {
        let mut ir = vec![0.0; 128];
        ir[0] = 1.0;
        let mut e = make_engine(ir);

        let sine: Vec<f32> = (0..BLOCK_SIZE + 1)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44_100.0).sin())
            .collect();

        let mut outputs = Vec::new();
        for &s in &sine {
            let (l, _) = e.process_advance(s, s);
            outputs.push(l);
        }

        let last = *outputs.last().unwrap();
        let expected = sine[0];
        assert!(
            (last - expected).abs() < 1e-3,
            "identity IR should reproduce input after latency: got {:.6}, expected {:.6}",
            last,
            expected
        );
    }

    #[test]
    fn reset_clears_state() {
        let ir = vec![0.5; 256];
        let mut e = make_engine(ir);
        for i in 0..BLOCK_SIZE {
            e.process_advance(i as f32 * 0.001, 0.0);
        }
        e.reset();
        assert!(e.overlap_l.iter().all(|&x| x == 0.0));
        assert!(e.overlap_r.iter().all(|&x| x == 0.0));
        assert!(e.input_l.is_empty());
    }

    #[test]
    fn output_finite_for_random_input() {
        let ir: Vec<f32> = (0..512)
            .map(|i| ((i as f32 / 512.0) * std::f32::consts::PI).sin())
            .collect();
        let mut e = make_engine(ir);
        for i in 0..(BLOCK_SIZE * 4) {
            let x = (i as f32 * 0.01).sin() * 0.9;
            let (l, r) = e.process_advance(x, -x);
            assert!(l.is_finite(), "L output is not finite at sample {}", i);
            assert!(r.is_finite(), "R output is not finite at sample {}", i);
        }
    }

    #[test]
    fn ir_length_reported_correctly() {
        let ir = vec![1.0; 300];
        let e = make_engine(ir);
        assert_eq!(e.ir_length(), 300);
    }

    #[test]
    fn set_sample_rate_valid() {
        let mut e = make_engine(vec![1.0; 128]);
        e.set_sample_rate(48_000.0);
        assert_eq!(e.sample_rate(), 48_000);
    }

    #[test]
    fn set_sample_rate_invalid_ignored() {
        let mut e = make_engine(vec![1.0; 128]);
        e.set_sample_rate(-1.0);
        assert_eq!(e.sample_rate(), 44_100);
    }
}
