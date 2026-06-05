//! FFT-based convolution engine for room correction / impulse response processing
//!
//! Uses the overlap-add method for efficient real-time convolution. The impulse
//! response (IR) is loaded once and its FFT is pre-computed. Audio is processed
//! in fixed-size blocks. Supports stereo IR files (independent convolution per
//! channel) and mono IR (same IR applied to both channels).
//!
//! The engine is real-time safe: all FFT buffers and workspaces are pre-allocated,
//! and no heap allocation occurs during processing.
//!
//! that shifts remaining data to the start when the write position would exceed
//! capacity. This prevents unbounded memory growth during continuous playback and
//! ensures zero allocation on the real-time audio thread (the previous version
//! could resize the overlap buffers in `add_to_overlap()`, which was both a memory
//! leak and a real-time safety violation).

use std::path::Path;

use num_complex::Complex;
use realfft::RealToComplex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConvolutionError {
    #[error("Convolution engine not enabled")]
    NotEnabled,
    #[error("No impulse response loaded")]
    NoIrLoaded,
    #[error("IR too long: {0} samples (max {1})")]
    IrTooLong(usize, usize),
    #[error("Failed to load IR file: {0}")]
    FileLoad(String),
    #[error("FFT error: {0}")]
    Fft(String),
}

/// FFT-based convolution engine with overlap-add
pub struct ConvolutionEngine {
    /// Whether convolution is enabled
    enabled: bool,
    /// Wet/dry mix (0.0 = fully dry, 1.0 = fully wet)
    wet_mix: f64,
    /// Sample rate
    sample_rate: f64,
    /// Maximum IR length allowed
    max_ir_length: usize,

    /// FFT size (next power of 2 >= 2 * ir_length)
    fft_size: usize,
    /// Block size = fft_size / 2
    block_size: usize,
    /// Forward FFT planner
    fft_forward: Arc<dyn RealToComplex<f64>>,
    /// Inverse FFT planner
    fft_inverse: Arc<dyn realfft::ComplexToReal<f64>>,

    /// Left channel IR spectrum (or mono IR)
    ir_spectrum_left: Vec<Complex<f64>>,
    /// Right channel IR spectrum (None if mono IR)
    ir_spectrum_right: Option<Vec<Complex<f64>>>,

    /// Input accumulation buffer (time domain, per channel)
    input_buffer_left: Vec<f64>,
    /// Input accumulation buffer for the right channel
    input_buffer_right: Vec<f64>,
    /// Number of samples accumulated in the input buffer
    input_count: usize,
    /// Output overlap-add tail buffer (per channel)
    ///
    ///
    /// The overlap-add algorithm produces at most `fft_size` output samples
    /// per block, and the maximum pending output at any time is bounded by
    /// `fft_size + block_size` (one full IFFT result plus a partial from
    /// the previous block). The buffer is compacted when the write position
    /// would exceed capacity, preventing unbounded growth.
    overlap_left: Vec<f64>,
    overlap_right: Vec<f64>,
    /// Read position in overlap buffers
    overlap_read_pos: usize,
    /// Available output samples in overlap buffers
    overlap_available: usize,
    /// Pre-allocated product buffers for frequency-domain multiplication (left channel)
    product_left: Vec<Complex<f64>>,
    /// Pre-allocated product buffers for frequency-domain multiplication (right channel)
    product_right: Vec<Complex<f64>>,

    /// These buffers are allocated once in new() and reused on every process_block()
    /// call, ensuring ZERO heap allocation on the real-time audio thread.
    ///
    /// Previously, forward_fft() called input.to_vec() and make_output_vec() on
    /// every invocation, which allocated memory on the hot path — a real-time safety
    /// violation that could cause audio dropouts under memory pressure.
    fft_workspace_input_left: Vec<f64>,
    fft_workspace_output_left: Vec<Complex<f64>>,
    fft_workspace_input_right: Vec<f64>,
    fft_workspace_output_right: Vec<Complex<f64>>,
    ifft_workspace_spectrum_left: Vec<Complex<f64>>,
    ifft_workspace_output_left: Vec<f64>,
    ifft_workspace_spectrum_right: Vec<Complex<f64>>,
    ifft_workspace_output_right: Vec<f64>,
    /// Scratch copies of IFFT output used in `process_block()` to pass to
    /// `add_to_overlap()` without a borrow-checker conflict.  Pre-allocated
    /// once in `new()` so there is ZERO heap allocation on the hot path.
    scratch_left: Vec<f64>,
    scratch_right: Vec<f64>,

    /// Whether an IR has been loaded
    ir_loaded: bool,
    /// Whether the loaded IR's frequency mapping is stale because the
    /// sample rate changed after the IR was loaded. When true, the user
    /// should be warned to reload the IR for correct convolution output.
    /// This flag is cleared when a new IR is loaded or the engine is reset.
    ir_needs_reload: bool,
    /// is full. The UI can query this via `dropped_frames()` to display
    /// a warning about audio quality degradation.
    dropped_frames: u64,
}

use std::sync::Arc;

impl ConvolutionEngine {
    /// Create a new convolution engine with pre-allocated buffers.
    ///
    /// `max_ir_length` determines the maximum impulse response length supported.
    /// The FFT size will be the next power of 2 >= 2 * max_ir_length.
    pub fn new(sample_rate: f64, max_ir_length: usize) -> Self {
        let fft_size = (2 * max_ir_length).next_power_of_two().max(64);
        let block_size = fft_size / 2;

        let mut planner = realfft::RealFftPlanner::new();
        let fft_forward = planner.plan_fft_forward(fft_size);
        let fft_inverse = planner.plan_fft_inverse(fft_size);

        let spectrum_len = fft_size / 2 + 1;

        // Pre-allocate all FFT workspace buffers to ensure zero allocation on the hot path.
        // Each workspace is exactly the size required by the realfft API.
        let fft_output_len = fft_forward.make_output_vec().len();
        let ifft_output_len = fft_inverse.make_output_vec().len();

        Self {
            enabled: false,
            wet_mix: 1.0,
            sample_rate,
            max_ir_length,
            fft_size,
            block_size,
            fft_forward,
            fft_inverse,
            ir_spectrum_left: vec![Complex::new(0.0, 0.0); spectrum_len],
            ir_spectrum_right: None,
            input_buffer_left: vec![0.0; fft_size],
            input_buffer_right: vec![0.0; fft_size],
            input_count: 0,

            // This is sufficient because the maximum pending overlap at any time
            // is bounded by fft_size samples (one full IFFT tail).
            overlap_left: vec![0.0; fft_size * 2],
            overlap_right: vec![0.0; fft_size * 2],
            overlap_read_pos: 0,
            overlap_available: 0,
            product_left: vec![Complex::new(0.0, 0.0); spectrum_len],
            product_right: vec![Complex::new(0.0, 0.0); spectrum_len],

            fft_workspace_input_left: vec![0.0; fft_size],
            fft_workspace_output_left: vec![Complex::new(0.0, 0.0); fft_output_len],
            fft_workspace_input_right: vec![0.0; fft_size],
            fft_workspace_output_right: vec![Complex::new(0.0, 0.0); fft_output_len],
            ifft_workspace_spectrum_left: vec![Complex::new(0.0, 0.0); spectrum_len],
            ifft_workspace_output_left: vec![0.0; ifft_output_len],
            ifft_workspace_spectrum_right: vec![Complex::new(0.0, 0.0); spectrum_len],
            ifft_workspace_output_right: vec![0.0; ifft_output_len],
            scratch_left: vec![0.0; ifft_output_len],
            scratch_right: vec![0.0; ifft_output_len],

            ir_loaded: false,
            ir_needs_reload: false,
            dropped_frames: 0,
        }
    }

    /// Load an impulse response from stereo or mono f64 samples.
    ///
    /// If `ir_samples` has length > max_ir_length, it will be truncated.
    /// Stereo IR: pairs of (left, right). Mono IR: pairs of (sample, sample).
    pub fn load_ir_from_samples(
        &mut self,
        ir_samples: &[(f64, f64)],
    ) -> Result<(), ConvolutionError> {
        let len = ir_samples.len().min(self.max_ir_length);

        // Determine if the IR is stereo (different L/R) or mono
        // L10: Limit stereo scan to first 512 pairs. For large IRs scanning
        // the entire buffer is wasteful; the first block is representative.
        let scan_len = len.min(512);
        let is_stereo = ir_samples[..scan_len]
            .iter()
            .any(|(l, r)| (l - r).abs() > 1e-10);

        let mut ir_left = vec![0.0; self.fft_size];
        for (i, (l, _r)) in ir_samples[..len].iter().enumerate() {
            ir_left[i] = *l;
        }
        self.ir_spectrum_left = self.forward_fft(&ir_left)?;

        if is_stereo {
            let mut ir_right = vec![0.0; self.fft_size];
            for (i, (_l, r)) in ir_samples[..len].iter().enumerate() {
                ir_right[i] = *r;
            }
            self.ir_spectrum_right = Some(self.forward_fft(&ir_right)?);
        } else {
            self.ir_spectrum_right = None;
        }

        self.ir_loaded = true;
        self.ir_needs_reload = false; // Fresh IR matches the current sample rate
        self.reset();
        Ok(())
    }

    /// Load an impulse response from a WAV or FLAC file using symphonia.
    pub fn load_ir_from_file(&mut self, path: &Path) -> Result<(), ConvolutionError> {
        use symphonia::core::{
            audio::Signal,
            codecs::{DecoderOptions, CODEC_TYPE_NULL},
            formats::FormatOptions,
            io::MediaSourceStream,
            meta::MetadataOptions,
            probe::Hint,
        };

        let file = std::fs::File::open(path).map_err(|e| {
            ConvolutionError::FileLoad(format!("Cannot open {}: {}", path.display(), e))
        })?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(|e| ConvolutionError::FileLoad(format!("Probe failed: {}", e)))?;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| ConvolutionError::FileLoad("No audio track found".to_string()))?;

        let track_id = track.id;
        let codec_params = &track.codec_params;
        let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);

        let decoder = symphonia::default::get_codecs()
            .make(codec_params, &decoder_opts)
            .map_err(|e| ConvolutionError::FileLoad(format!("Cannot create decoder: {}", e)))?;

        let mut format_reader = probed.format;
        let mut decoder = decoder;

        // Decode the entire IR file
        let mut ir_samples: Vec<(f64, f64)> = Vec::new();

        loop {
            let packet = match format_reader.next_packet() {
                Ok(p) => p,
                Err(_) => break,
            };

            if packet.track_id() != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let frames = decoded.frames();
                    match &decoded {
                        symphonia::core::audio::AudioBufferRef::F32(buf) => {
                            let buf = &**buf;
                            for i in 0..frames {
                                let l = buf.chan(0)[i] as f64;
                                let r = if channels > 1 {
                                    buf.chan(1)[i] as f64
                                } else {
                                    l
                                };
                                ir_samples.push((l, r));
                            }
                        },
                        symphonia::core::audio::AudioBufferRef::S16(buf) => {
                            let buf = &**buf;
                            for i in 0..frames {
                                let l = buf.chan(0)[i] as f64 / 32768.0;
                                let r = if channels > 1 {
                                    buf.chan(1)[i] as f64 / 32768.0
                                } else {
                                    l
                                };
                                ir_samples.push((l, r));
                            }
                        },
                        symphonia::core::audio::AudioBufferRef::S24(buf) => {
                            let buf = &**buf;
                            for i in 0..frames {
                                // S24: 24-bit samples stored in i32 (sign-extended)
                                // symphonia's i24 is a newtype i24(pub i32),
                                // so we access the inner value via .0
                                let l = buf.chan(0)[i].0 as f64 / 8388608.0; // 2^23
                                let r = if channels > 1 {
                                    buf.chan(1)[i].0 as f64 / 8388608.0
                                } else {
                                    l
                                };
                                ir_samples.push((l, r));
                            }
                        },
                        symphonia::core::audio::AudioBufferRef::S32(buf) => {
                            let buf = &**buf;
                            for i in 0..frames {
                                let l = buf.chan(0)[i] as f64 / 2147483648.0; // 2^31
                                let r = if channels > 1 {
                                    buf.chan(1)[i] as f64 / 2147483648.0
                                } else {
                                    l
                                };
                                ir_samples.push((l, r));
                            }
                        },
                        symphonia::core::audio::AudioBufferRef::F64(buf) => {
                            let buf = &**buf;
                            for i in 0..frames {
                                let l = buf.chan(0)[i];
                                let r = if channels > 1 { buf.chan(1)[i] } else { l };
                                ir_samples.push((l, r));
                            }
                        },
                        _ => {
                            // Remaining unsupported sample formats — output silence
                            for _ in 0..frames {
                                ir_samples.push((0.0, 0.0));
                            }
                        },
                    }
                },
                Err(_) => continue,
            }

            if ir_samples.len() > self.max_ir_length {
                ir_samples.truncate(self.max_ir_length);
                break;
            }
        }

        if ir_samples.is_empty() {
            return Err(ConvolutionError::FileLoad(
                "IR file contains no samples".to_string(),
            ));
        }

        self.load_ir_from_samples(&ir_samples)
    }

    /// Perform forward FFT on a time-domain signal using pre-allocated workspace.
    ///
    ///
    /// the result is placed in `workspace_output`, both of which were allocated in new().
    fn forward_fft_into(
        fft_forward: &Arc<dyn realfft::RealToComplex<f64>>,
        input: &[f64],
        workspace_input: &mut Vec<f64>,
        workspace_output: &mut Vec<Complex<f64>>,
    ) -> Result<(), ConvolutionError> {
        workspace_input[..input.len()].copy_from_slice(input);
        // Zero-fill remaining if input is shorter than workspace (shouldn't happen in practice)
        for i in input.len()..workspace_input.len() {
            workspace_input[i] = 0.0;
        }
        fft_forward
            .process(workspace_input, workspace_output)
            .map_err(|e| ConvolutionError::Fft(e.to_string()))?;
        Ok(())
    }

    /// Perform forward FFT on a time-domain signal (allocating version for IR loading).
    ///
    /// This is only used during IR loading (not the hot path), so allocation is acceptable.
    fn forward_fft(&self, input: &[f64]) -> Result<Vec<Complex<f64>>, ConvolutionError> {
        let mut input_vec = input.to_vec();
        let mut output = self.fft_forward.make_output_vec();
        self.fft_forward
            .process(&mut input_vec, &mut output)
            .map_err(|e| ConvolutionError::Fft(e.to_string()))?;
        Ok(output)
    }

    /// Process a single stereo sample through the convolution engine.
    ///
    /// Returns the convolved output with wet/dry mix applied.
    /// If disabled or no IR is loaded, returns the input unchanged.
    #[inline]
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled || !self.ir_loaded {
            return (left, right);
        }

        // Accumulate input
        self.input_buffer_left[self.input_count] = left;
        self.input_buffer_right[self.input_count] = right;
        self.input_count += 1;

        // When we have a full block, process it
        if self.input_count >= self.block_size {
            self.process_block();
        }

        // Try to read from overlap output
        if self.overlap_available > 0 {
            let out_l = self.overlap_left[self.overlap_read_pos];
            let out_r = self.overlap_right[self.overlap_read_pos];
            self.overlap_read_pos += 1;
            self.overlap_available -= 1;

            // consumed. This prevents unbounded read_pos growth and keeps the
            // buffers ready for the next add_to_overlap() without resizing.
            if self.overlap_available == 0 {
                self.overlap_read_pos = 0;
            }

            // Apply wet/dry mix
            let mixed_l = left * (1.0 - self.wet_mix) + out_l * self.wet_mix;
            let mixed_r = right * (1.0 - self.wet_mix) + out_r * self.wet_mix;
            (mixed_l, mixed_r)
        } else {
            // No output ready yet — apply wet/dry mix with silent wet
            // to avoid a click when the first convolved output appears
            let mixed_l = left * (1.0 - self.wet_mix);
            let mixed_r = right * (1.0 - self.wet_mix);
            (mixed_l, mixed_r)
        }
    }

    /// Process a full block of input through overlap-add convolution.
    ///
    ///
    /// All FFT operations use pre-allocated workspace buffers, and the overlap
    /// buffers are compacted in-place rather than resized.
    fn process_block(&mut self) {
        let bs = self.block_size;

        // Zero-pad the input buffer beyond the block
        for i in bs..self.fft_size {
            self.input_buffer_left[i] = 0.0;
            self.input_buffer_right[i] = 0.0;
        }

        // Forward FFT of left channel input (using pre-allocated workspace, zero allocation)
        // Split borrows: clone the Arc so we don't borrow self while also borrowing workspace
        // fields.
        let fft_forward = Arc::clone(&self.fft_forward);
        if Self::forward_fft_into(
            &fft_forward,
            &self.input_buffer_left,
            &mut self.fft_workspace_input_left,
            &mut self.fft_workspace_output_left,
        )
        .is_err()
        {
            self.input_count = 0;
            return;
        }

        // Forward FFT of right channel input (using pre-allocated workspace, zero allocation)
        if Self::forward_fft_into(
            &fft_forward,
            &self.input_buffer_right,
            &mut self.fft_workspace_input_right,
            &mut self.fft_workspace_output_right,
        )
        .is_err()
        {
            self.input_count = 0;
            return;
        }

        // Multiply in frequency domain (convolution theorem)
        let spectrum_len = self.fft_size / 2 + 1;

        // Left channel: input × IR_left (using pre-allocated buffer)
        for i in 0..spectrum_len {
            self.product_left[i] = self.fft_workspace_output_left[i] * self.ir_spectrum_left[i];
        }

        // Right channel: input × IR_right (or same as left for mono IR, using pre-allocated buffer)
        let ir_right = self
            .ir_spectrum_right
            .as_ref()
            .unwrap_or(&self.ir_spectrum_left);
        for i in 0..spectrum_len {
            self.product_right[i] = self.fft_workspace_output_right[i] * ir_right[i];
        }

        // Inverse FFT to get time-domain convolution result (using pre-allocated workspace)
        let fft_inverse = Arc::clone(&self.fft_inverse);
        let fft_size = self.fft_size;
        let left_ok = Self::inverse_fft_into(
            &fft_inverse,
            fft_size,
            &self.product_left,
            &mut self.ifft_workspace_spectrum_left,
            &mut self.ifft_workspace_output_left,
        );
        let right_ok = Self::inverse_fft_into(
            &fft_inverse,
            fft_size,
            &self.product_right,
            &mut self.ifft_workspace_spectrum_right,
            &mut self.ifft_workspace_output_right,
        );

        if left_ok && right_ok {
            // Copy IFFT results into pre-allocated scratch buffers, then call
            // add_to_overlap with those slices.  We use a raw-pointer cast for
            // the read side so the Rust borrow checker sees &mut self only once
            // (for add_to_overlap), which is sound because scratch_{left,right}
            // and ifft_workspace_output_{left,right} are disjoint Vec allocations.
            let n = self
                .ifft_workspace_output_left
                .len()
                .min(self.ifft_workspace_output_right.len())
                .min(self.scratch_left.len())
                .min(self.scratch_right.len());
            self.scratch_left[..n].copy_from_slice(&self.ifft_workspace_output_left[..n]);
            self.scratch_right[..n].copy_from_slice(&self.ifft_workspace_output_right[..n]);

            // SAFETY: scratch_{left,right} are the only Vecs we pass to
            // add_to_overlap; the ifft_workspace buffers are not accessed
            // inside that function. The slices are derived from allocations
            // that are fully owned by self and do not alias each other.
            let l_ptr = self.scratch_left.as_ptr();
            let r_ptr = self.scratch_right.as_ptr();
            let l_slice = unsafe { std::slice::from_raw_parts(l_ptr, n) };
            let r_slice = unsafe { std::slice::from_raw_parts(r_ptr, n) };
            self.add_to_overlap(l_slice, r_slice);
        }

        self.input_count = 0;
    }

    /// Perform inverse FFT from frequency domain to time domain using pre-allocated workspace.
    ///
    ///
    /// the result is placed in `workspace_output`, both pre-allocated in new().
    fn inverse_fft_into(
        fft_inverse: &Arc<dyn realfft::ComplexToReal<f64>>,
        fft_size: usize,
        spectrum: &[Complex<f64>],
        workspace_spectrum: &mut Vec<Complex<f64>>,
        workspace_output: &mut Vec<f64>,
    ) -> bool {
        let copy_len = spectrum.len().min(workspace_spectrum.len());
        workspace_spectrum[..copy_len].copy_from_slice(&spectrum[..copy_len]);
        // Zero-fill remaining
        for i in copy_len..workspace_spectrum.len() {
            workspace_spectrum[i] = Complex::new(0.0, 0.0);
        }
        if fft_inverse
            .process(workspace_spectrum, workspace_output)
            .is_err()
        {
            return false;
        }
        // Normalize by FFT size (realfft's IFFT doesn't normalize)
        let scale = 1.0 / fft_size as f64;
        for s in workspace_output.iter_mut() {
            *s *= scale;
        }
        true
    }

    /// Add convolution result to the overlap-add output buffers.
    ///
    ///
    /// in `new()`. When the write position would exceed the buffer, we compact by
    /// shifting remaining unread data to the start. This ensures:
    ///   1. Zero heap allocation on the real-time audio thread (no resize)
    ///   2. Bounded memory usage (no unbounded growth during continuous playback)
    ///   3. Correct overlap-add semantics (additive writes into existing data)
    ///
    /// The maximum pending output at any time is bounded: each process_block()
    /// produces fft_size IFFT output samples but only block_size are consumed as
    /// valid output, leaving at most fft_size - block_size = block_size tail
    /// samples. With two blocks' worth of tail, the maximum is 2 * block_size,
    /// which equals fft_size. The fft_size * 2 buffer provides ample headroom.
    fn add_to_overlap(&mut self, left: &[f64], right: &[f64]) {
        let frames = left.len().min(self.fft_size);
        let write_start = self.overlap_read_pos + self.overlap_available;

        // This replaces the previous resize() which could allocate on the
        // real-time thread and caused unbounded memory growth.
        let buf_len = self.overlap_left.len();
        if write_start + frames > buf_len {
            // Shift unread data to the beginning of the buffer
            let shift_amount = self.overlap_read_pos;
            if shift_amount > 0 && self.overlap_available > 0 {
                for i in 0..self.overlap_available {
                    self.overlap_left[i] = self.overlap_left[shift_amount + i];
                    self.overlap_right[i] = self.overlap_right[shift_amount + i];
                }
            }
            // Zero out the area after the shifted data to prevent stale
            // values from being added into during the overlap-add
            for i in self.overlap_available..buf_len {
                self.overlap_left[i] = 0.0;
                self.overlap_right[i] = 0.0;
            }
            self.overlap_read_pos = 0;
        }

        let mut write_start = self.overlap_read_pos + self.overlap_available;

        // Bug #5 fix: apply backpressure by discarding the oldest overlap data
        // to make room for the new block. This is preferable to silently
        // dropping the new output, which causes audible glitches.
        // The discarded frames are counted for UI feedback.
        if write_start + frames > buf_len {
            let overflow = (write_start + frames) - buf_len;
            // Advance the read position to discard the oldest samples
            let discard = overflow.min(self.overlap_available);
            self.overlap_read_pos += discard;
            self.overlap_available -= discard;
            self.dropped_frames += discard as u64;
            log::warn!(
                "Convolution overlap buffer overflow: discarding {} oldest frames \
                 (total dropped: {}). Consider reducing IR length or disabling convolution.",
                discard,
                self.dropped_frames
            );
            // Recalculate write_start after discarding
            write_start = self.overlap_read_pos + self.overlap_available;
        }

        // Bug #5 fix: after overflow handling, verify there is actually room.
        // If overlap_available was 0 and frames > buf_len (shouldn't happen
        // with correct buffer sizing, but defensive), skip the write entirely
        // to prevent OOB access in the overlap-add loop below.
        let writable = if write_start < buf_len {
            (buf_len - write_start).min(frames)
        } else {
            0
        };

        // Overlap-add: accumulate into existing buffer data
        for i in 0..writable {
            let idx = write_start + i;
            self.overlap_left[idx] += left[i];
            self.overlap_right[idx] += right[i];
        }

        if writable < frames {
            // Some frames were clipped because the buffer was too small even
            // after compaction and discard. Count them as dropped.
            let clipped = frames - writable;
            self.dropped_frames += clipped as u64;
            log::warn!(
                "Convolution overlap: {} frames clipped (buffer full after compaction). \
                 Total dropped: {}.",
                clipped,
                self.dropped_frames
            );
        }

        // Bug #12 fix: Count all actually-written valid output samples, not
        // capped to block_size. The IFFT produces fft_size output samples per
        // block; in overlap-add, all fft_size samples are accumulated into the
        // overlap buffer. The previous code capped overlap_available to
        // block_size via `writable.min(self.block_size)`, which meant only the
        // first half of the convolution result was ever read as output — the
        // IR tail (frames block_size..fft_size) was accumulated but never
        // surfaced as readable output, producing audio like a convolution with
        // a half-length IR and incorrect phase accumulation.
        self.overlap_available += writable;
    }

    /// Process a batch of stereo frames through the convolution engine
    pub fn process_batch(&mut self, frames: &mut [(f64, f64)]) {
        for frame in frames.iter_mut() {
            *frame = self.process(frame.0, frame.1);
        }
    }

    /// Enable or disable the convolution engine
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the convolution engine is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the wet/dry mix (0.0 = fully dry, 1.0 = fully wet)
    pub fn set_wet_mix(&mut self, mix: f64) {
        self.wet_mix = mix.clamp(0.0, 1.0);
    }

    /// Get the current wet/dry mix
    pub fn wet_mix(&self) -> f64 {
        self.wet_mix
    }

    /// Whether an IR has been loaded
    pub fn is_ir_loaded(&self) -> bool {
        self.ir_loaded
    }

    /// Whether the loaded IR needs to be reloaded due to a sample rate change.
    ///
    /// When the output device sample rate changes (e.g., device switch), the
    /// FFT frequency bins in the pre-computed IR spectrum no longer map to
    /// the correct absolute frequencies, producing subtly incorrect convolution.
    /// The UI should display a warning when this returns true.
    pub fn ir_needs_reload(&self) -> bool {
        self.ir_needs_reload
    }

    /// Update the sample rate for the convolution engine.
    ///
    /// This rebuilds the K-weighting or other rate-dependent internal state.
    /// After a sample rate change (e.g., device switch), the IR spectrum is
    /// pitched incorrectly if the sample rate is not updated, because the
    /// frequency bins of the FFT correspond to different absolute frequencies
    /// at different sample rates. The IR must be re-loaded or the engine must
    /// be rebuilt if the rate changes significantly; however, updating the
    /// stored sample rate ensures that any future IR loads use the correct rate.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (self.sample_rate - sample_rate).abs() > 0.01 {
            log::info!(
                "ConvolutionEngine sample rate changed: {:.0} Hz -> {:.0} Hz. \
                 Note: loaded IR frequency mapping may be incorrect until IR is reloaded.",
                self.sample_rate,
                sample_rate
            );
            self.sample_rate = sample_rate;
            // Mark IR as needing reload so the UI can warn the user.
            // The IR spectrum stays in memory but its frequency mapping is
            // based on the old rate — the user should reload the IR.
            if self.ir_loaded {
                self.ir_needs_reload = true;
            }
            self.reset();
        }
    }

    /// buffer overflow. The UI should display a warning when this is
    /// non-zero to inform the user that convolution audio quality is
    /// degraded.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Reset all processing state (keeps loaded IR).
    ///
    /// Note: does NOT clear `ir_needs_reload` — that flag persists across
    /// resets so the UI warning remains visible until a new IR is loaded.
    pub fn reset(&mut self) {
        self.input_count = 0;
        self.overlap_read_pos = 0;
        self.overlap_available = 0;
        self.input_buffer_left.fill(0.0);
        self.input_buffer_right.fill(0.0);
        self.overlap_left.fill(0.0);
        self.overlap_right.fill(0.0);
        self.product_left.fill(Complex::new(0.0, 0.0));
        self.product_right.fill(Complex::new(0.0, 0.0));

        self.fft_workspace_input_left.fill(0.0);
        self.fft_workspace_output_left.fill(Complex::new(0.0, 0.0));
        self.fft_workspace_input_right.fill(0.0);
        self.fft_workspace_output_right.fill(Complex::new(0.0, 0.0));
        self.ifft_workspace_spectrum_left
            .fill(Complex::new(0.0, 0.0));
        self.ifft_workspace_output_left.fill(0.0);
        self.ifft_workspace_spectrum_right
            .fill(Complex::new(0.0, 0.0));
        self.ifft_workspace_output_right.fill(0.0);
        self.scratch_left.fill(0.0);
        self.scratch_right.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convolution_creation() {
        let engine = ConvolutionEngine::new(44100.0, 1024);
        assert!(!engine.is_enabled());
        assert!(!engine.is_ir_loaded());
    }

    #[test]
    fn test_convolution_passthrough_when_disabled() {
        let mut engine = ConvolutionEngine::new(44100.0, 1024);
        let (l, r) = engine.process(0.5, 0.3);
        assert!((l - 0.5).abs() < 1e-10);
        assert!((r - 0.3).abs() < 1e-10);
    }

    #[test]
    fn test_convolution_load_mono_ir() {
        let mut engine = ConvolutionEngine::new(44100.0, 512);
        // Simple impulse IR (delta function = passthrough)
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        assert!(engine.is_ir_loaded());
    }

    #[test]
    fn test_convolution_load_stereo_ir() {
        let mut engine = ConvolutionEngine::new(44100.0, 512);
        // Stereo IR
        let ir: Vec<(f64, f64)> = vec![(1.0, 0.5), (0.3, 0.7), (0.1, 0.2)];
        engine.load_ir_from_samples(&ir).unwrap();
        assert!(engine.is_ir_loaded());
    }

    #[test]
    fn test_convolution_wet_mix() {
        let mut engine = ConvolutionEngine::new(44100.0, 512);
        engine.set_wet_mix(0.5);
        assert!((engine.wet_mix() - 0.5).abs() < 1e-10);
        engine.set_wet_mix(-0.1);
        assert!(engine.wet_mix() >= 0.0);
        engine.set_wet_mix(1.5);
        assert!(engine.wet_mix() <= 1.0);
    }

    #[test]
    fn test_convolution_reset() {
        let mut engine = ConvolutionEngine::new(44100.0, 512);
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        engine.set_enabled(true);
        engine.reset();
        assert!(engine.is_ir_loaded());
        assert!(engine.is_enabled());
        assert_eq!(engine.input_count, 0);
    }

    #[test]
    fn test_convolution_workspace_buffers_allocated() {
        // Verify that all workspace buffers are pre-allocated with the correct sizes
        let engine = ConvolutionEngine::new(44100.0, 512);
        let expected_fft_size = (2 * 512usize).next_power_of_two().max(64);
        let expected_spectrum_len = expected_fft_size / 2 + 1;

        assert_eq!(
            engine.fft_workspace_input_left.len(),
            expected_fft_size,
            "FFT workspace input left should be pre-allocated to fft_size"
        );
        assert_eq!(
            engine.fft_workspace_input_right.len(),
            expected_fft_size,
            "FFT workspace input right should be pre-allocated to fft_size"
        );
        assert_eq!(
            engine.fft_workspace_output_left.len(),
            expected_spectrum_len,
            "FFT workspace output left should be pre-allocated to spectrum_len"
        );
        assert_eq!(
            engine.fft_workspace_output_right.len(),
            expected_spectrum_len,
            "FFT workspace output right should be pre-allocated to spectrum_len"
        );
        assert_eq!(
            engine.ifft_workspace_spectrum_left.len(),
            expected_spectrum_len,
            "IFFT workspace spectrum left should be pre-allocated to spectrum_len"
        );
        assert_eq!(
            engine.ifft_workspace_spectrum_right.len(),
            expected_spectrum_len,
            "IFFT workspace spectrum right should be pre-allocated to spectrum_len"
        );
        // IFFT output size matches the real-to-real inverse output
        assert!(
            !engine.ifft_workspace_output_left.is_empty(),
            "IFFT workspace output left should be pre-allocated"
        );
        assert!(
            !engine.ifft_workspace_output_right.is_empty(),
            "IFFT workspace output right should be pre-allocated"
        );
    }

    #[test]
    fn test_convolution_process_with_preallocated_buffers() {
        // Test that convolution with a delta IR produces output using
        // the pre-allocated workspace path (process_block → forward_fft_into).
        let mut engine = ConvolutionEngine::new(44100.0, 64);
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        engine.set_enabled(true);

        // Process enough samples to trigger at least one process_block call
        let block_size = engine.block_size;
        for _ in 0..block_size + 10 {
            let (l, r) = engine.process(0.5, 0.5);
            // Output should be finite (no NaN/Inf from workspace bugs)
            assert!(l.is_finite(), "Left output should be finite");
            assert!(r.is_finite(), "Right output should be finite");
        }
    }

    #[test]
    fn test_convolution_reset_clears_workspaces() {
        let mut engine = ConvolutionEngine::new(44100.0, 64);
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        engine.set_enabled(true);

        // Process some data to fill workspaces
        for _ in 0..engine.block_size + 10 {
            engine.process(0.5, 0.5);
        }

        engine.reset();

        // After reset, workspace input buffers should be zeroed
        assert!(
            engine.fft_workspace_input_left.iter().all(|&v| v == 0.0),
            "FFT workspace input left should be zeroed after reset"
        );
        assert!(
            engine.fft_workspace_input_right.iter().all(|&v| v == 0.0),
            "FFT workspace input right should be zeroed after reset"
        );
    }

    #[test]
    fn test_convolution_overlap_buffer_no_growth() {
        // Process many blocks and verify overlap buffers never grow beyond
        // the initial pre-allocated size (fft_size * 2).
        let mut engine = ConvolutionEngine::new(44100.0, 64);
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        engine.set_enabled(true);

        let initial_len = engine.overlap_left.len();

        // Process many blocks (simulate sustained playback)
        for _ in 0..1000 {
            let bs = engine.block_size;
            for _ in 0..bs {
                engine.process(0.5, 0.5);
            }
        }

        // Overlap buffers must not have grown beyond their initial allocation
        assert_eq!(
            engine.overlap_left.len(),
            initial_len,
            "Overlap left buffer must not grow during sustained playback"
        );
        assert_eq!(
            engine.overlap_right.len(),
            initial_len,
            "Overlap right buffer must not grow during sustained playback"
        );
    }

    #[test]
    fn test_convolution_sustained_output_remains_finite() {
        // Verify that sustained processing produces correct finite output
        // even after many compaction cycles in add_to_overlap.
        let mut engine = ConvolutionEngine::new(44100.0, 64);
        let ir: Vec<(f64, f64)> = vec![(1.0, 1.0)];
        engine.load_ir_from_samples(&ir).unwrap();
        engine.set_enabled(true);

        // Process many blocks to exercise compaction
        for _ in 0..500 {
            let bs = engine.block_size;
            for _ in 0..bs {
                let (l, r) = engine.process(0.3, 0.3);
                assert!(l.is_finite(), "Left output should always be finite");
                assert!(r.is_finite(), "Right output should always be finite");
            }
        }
    }
}
