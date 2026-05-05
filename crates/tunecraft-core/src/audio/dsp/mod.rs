//! Rust DSP engine — stereo audio processing pipeline.
//!
//! # Signal chain (mandatory order)
//!
//! ```text
//!  1. HPF 20 Hz + LPF 20 kHz     (always-on protection filters)
//!  2. Channel balance scalar
//!  3. ReplayGain volume scaling
//!  4. EBU R128 loudness gain
//!  5. Preamp (auto-headroom compensation)
//!  6. Low-shelf (bass) biquad
//!  7. Parametric EQ cascade        (10 bands, coefficient smoothing)
//!  8. High-shelf (treble) biquad
//!  9. Master volume (+ seek-fade ramp)   ← post-EQ: fader after tone shaping
//! 10. Mid-Side stereo widening
//! 11. Parametric M/S EQ            (5 bands, mastering-grade)
//! 12. Coupled stereo limiter       (with lookahead + true-peak detection)
//! 13. TPDF dither                  (only when output is 16-bit)
//! ```
//!
//! ## Why master volume is post-EQ
//!
//! Placing the master fader *before* the EQ (as in v3.6) means the EQ sees a
//! different input level at every volume setting. While the parametric filters
//! used here are linear, the perceptual loudness balance shifts because
//! Fletcher-Munson curves are level-dependent. The standard studio convention
//! is: tone shaping (EQ) → level (fader). Moving volume to step 9 fixes this.
//!
//! # Real-time constraints
//! - No heap allocation in the audio thread
//! - No locks in the processing path
//! - Fixed-size stack state throughout
//! - No dynamic dispatch in the hot loop

pub mod biquad;
pub mod dynamics;
pub mod gapless_smoother;
pub mod ms_eq;
pub mod seek_fade;

// Re-export the types callers need without forcing them to dig into submodules.
pub use biquad::{Biquad, SmoothedBand};
pub use dynamics::{Limiter, StereoLimiter, TpdfDither};
pub use gapless_smoother::GaplessSmoother;
pub use ms_eq::{MsEqBand, MsEqStage, MAX_MS_EQ_BANDS};
pub use seek_fade::SeekFadeRamp;

// ── EQ band descriptor ────────────────────────────────────────────────

pub const MAX_EQ_BANDS: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct EqBandParams { pub freq_hz: f32, pub gain_db: f32, pub q: f32 }

impl EqBandParams {
    pub fn flat(freq_hz: f32) -> Self { Self { freq_hz, gain_db: 0.0, q: 1.0 } }
}

// ── DspEngine ────────────────────────────────────────────────────────

/// Stereo DSP engine. All state is stack-allocated; no dynamic dispatch.
pub struct DspEngine {
    // ── Protection filters (always-on) ──────────────────────────
    hpf: [Biquad; 2],
    lpf: [Biquad; 2],

    // ── Gains ───────────────────────────────────────────────────
    pub balance:          f32,   // [-1, +1]
    pub stereo_width:     f32,   // [0, 3]
    preamp_gain:          f32,
    replaygain_factor:    f32,
    loudness_gain:        f32,
    volume_gain:          f32,

    // ── Seek-fade ramp ──────────────────────────────────────────
    seek_fade_ramp: Option<SeekFadeRamp>,

    // ── Bass / treble shelves ───────────────────────────────────
    bass_shelf:       [Biquad; 2],
    treble_shelf:     [Biquad; 2],
    pub bass_db:      f32,
    pub treble_db:    f32,
    pub bass_freq_hz:   f32,
    pub treble_freq_hz: f32,

    // ── Parametric EQ ───────────────────────────────────────────
    pub num_bands:    usize,
    band_params:      [EqBandParams; MAX_EQ_BANDS],
    bands:            [SmoothedBand; MAX_EQ_BANDS],

    // ── M/S EQ stage ────────────────────────────────────────────
    ms_eq: MsEqStage,

    // ── Dynamics ────────────────────────────────────────────────
    limiter:   StereoLimiter,
    pub dither: TpdfDither,

    // ── Gapless smoother ────────────────────────────────────────
    gapless:           GaplessSmoother,
    pending_new_track: bool,
    /// Set by `mark_new_track()`; cleared after `capture_tail()` so subsequent
    /// buffers skip the copy until the next track boundary.
    tail_dirty:        bool,

    pub enabled:  bool,
    sample_rate:  f32,
}

impl DspEngine {
    pub fn new(sample_rate: f32) -> Self {
        const ISO: [f32; 10] = [32.0, 64.0, 125.0, 250.0, 500.0,
                                 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
        let mut engine = Self {
            hpf: [Biquad::identity(); 2],
            lpf: [Biquad::identity(); 2],
            balance:          0.0,
            stereo_width:     1.0,
            preamp_gain:      1.0,
            replaygain_factor: 1.0,
            loudness_gain:    1.0,
            volume_gain:      1.0,
            seek_fade_ramp:   None,
            bass_shelf:       [Biquad::identity(); 2],
            treble_shelf:     [Biquad::identity(); 2],
            bass_db:          0.0,
            treble_db:        0.0,
            bass_freq_hz:     90.0,
            treble_freq_hz:   10_000.0,
            num_bands:        10,
            band_params:      ISO.map(EqBandParams::flat),
            bands:            std::array::from_fn(|_| SmoothedBand::new()),
            ms_eq:            MsEqStage::new(sample_rate),
            limiter:          StereoLimiter::for_rate(sample_rate),
            dither:           TpdfDither::new(),
            gapless:          GaplessSmoother::new(),
            pending_new_track: false,
            tail_dirty:        true,
            enabled:           true,
            sample_rate,
        };
        engine.rebuild_protection_filters();
        engine
    }

    // ── Parametric EQ setters ────────────────────────────────────

    pub fn set_band_gain(&mut self, index: usize, gain_db: f32) {
        if index >= self.num_bands { return; }
        self.band_params[index].gain_db = gain_db.clamp(-24.0, 24.0);
        self.recompute_band(index);
        self.update_preamp();
    }

    pub fn set_band_freq(&mut self, index: usize, freq_hz: f32) {
        if index >= self.num_bands { return; }
        self.band_params[index].freq_hz = freq_hz.clamp(20.0, self.sample_rate / 2.0 - 1.0);
        self.recompute_band(index);
    }

    pub fn set_band_q(&mut self, index: usize, q: f32) {
        if index >= self.num_bands { return; }
        self.band_params[index].q = q.clamp(0.1, 10.0);
        self.recompute_band(index);
    }

    pub fn set_all_gains(&mut self, gains_db: &[f32]) {
        let n = gains_db.len().min(self.num_bands);
        for i in 0..n {
            self.band_params[i].gain_db = gains_db[i].clamp(-24.0, 24.0);
            self.recompute_band(i);
        }
        self.update_preamp();
    }

    pub fn band_gain(&self, index: usize) -> f32 {
        if index >= self.num_bands { return 0.0; }
        self.band_params[index].gain_db
    }

    // ── Shelf setters ────────────────────────────────────────────

    pub fn set_bass(&mut self, gain_db: f32) {
        self.bass_db = gain_db.clamp(-12.0, 12.0);
        let bq = Biquad::low_shelf(self.bass_freq_hz, self.bass_db, self.sample_rate);
        self.bass_shelf[0].copy_coeffs_from(&bq);
        self.bass_shelf[1].copy_coeffs_from(&bq);
        // Fix Bug #12: Bass shelf gain affects auto-headroom compensation.
        self.update_preamp();
    }

    pub fn set_bass_freq(&mut self, freq_hz: f32) {
        self.bass_freq_hz = freq_hz.clamp(20.0, 500.0);
        self.set_bass(self.bass_db);
    }

    pub fn set_treble(&mut self, gain_db: f32) {
        self.treble_db = gain_db.clamp(-12.0, 12.0);
        let bq = Biquad::high_shelf(self.treble_freq_hz, self.treble_db, self.sample_rate);
        self.treble_shelf[0].copy_coeffs_from(&bq);
        self.treble_shelf[1].copy_coeffs_from(&bq);
        // Fix Bug #12: Treble shelf gain affects auto-headroom compensation.
        self.update_preamp();
    }

    pub fn set_treble_freq(&mut self, freq_hz: f32) {
        self.treble_freq_hz = freq_hz.clamp(1000.0, 20_000.0);
        self.set_treble(self.treble_db);
    }

    // ── Volume / gain setters ────────────────────────────────────

    pub fn set_balance(&mut self, balance: f32) { self.balance = balance.clamp(-1.0, 1.0); }
    pub fn set_stereo_width(&mut self, width: f32) { self.stereo_width = width.clamp(0.0, 3.0); }
    pub fn set_dither_enabled(&mut self, enabled: bool) { self.dither.enabled = enabled; }

    pub fn set_replaygain_factor(&mut self, factor: f32) {
        self.replaygain_factor = factor.clamp(0.0, 4.0);
    }
    pub fn replaygain_factor(&self) -> f32 { self.replaygain_factor }

    pub fn set_loudness_gain(&mut self, gain: f32) {
        self.loudness_gain = gain.clamp(0.0, 10.0);
    }
    pub fn loudness_gain(&self) -> f32 { self.loudness_gain }

    pub fn set_volume_gain(&mut self, gain: f32) {
        self.volume_gain = gain.clamp(0.0, 1.0);
    }
    pub fn volume_gain(&self) -> f32 { self.volume_gain }
    pub fn preamp_gain(&self) -> f32 { self.preamp_gain }

    // ── Seek-fade ────────────────────────────────────────────────

    /// Initiate a smooth seek-fade cycle (fade-out → fade-in) on the volume.
    /// `fade_ms == 0` performs a hard mute/unmute (backward compatible).
    pub fn start_seek_fade(&mut self, base_volume: f32, fade_ms: u32) {
        if fade_ms == 0 {
            self.volume_gain = 0.0;
            self.seek_fade_ramp = Some(SeekFadeRamp {
                target: base_volume,
                step_per_sample: base_volume,
                phase: seek_fade::SeekFadePhase::FadeIn,
            });
            return;
        }
        self.seek_fade_ramp = SeekFadeRamp::new(base_volume, fade_ms, self.sample_rate);
    }

    /// Called by the engine tick after `process_buffer()`.
    ///
    /// Fix CRITICAL BUG: The previous implementation prematurely completed the
    /// fade-in by immediately setting `volume_gain = ramp.target` and clearing
    /// the ramp when the phase was `FadeIn`. This happened because `advance()`
    /// sets `phase = FadeIn` as soon as the fade-out reaches zero, and then
    /// `tick_seek_fade()` saw `FadeIn` and instantly jumped to the target
    /// volume, skipping the entire gradual fade-in. This caused:
    ///   1. An audible click on every seek (instant volume jump from 0 to target)
    ///   2. The seek-fade mechanism was effectively broken — only fade-out worked
    ///
    /// The correct behavior: `advance()` handles the sample-by-sample ramping
    /// (both fade-out and fade-in) inside `process_buffer()`. When `advance()`
    /// returns `true`, the ramp is complete and `process_buffer()` clears it.
    /// `tick_seek_fade()` should only handle the phase transition notification
    /// and should NOT interfere with the ongoing fade-in.
    pub fn tick_seek_fade(&mut self) {
        // The seek-fade ramp is now fully driven by `advance()` inside
        // `process_buffer()`. When the ramp completes (advance returns true),
        // process_buffer clears it. We no longer prematurely complete the
        // fade-in here.
        //
        // This method is kept as a no-op hook for potential future use
        // (e.g., signalling the UI thread that the seek-fade transition
        // point has been reached, so the actual seek can be performed).
    }

    // ── M/S EQ passthrough ───────────────────────────────────────

    pub fn set_ms_eq_band(&mut self, index: usize, band: MsEqBand) {
        self.ms_eq.set_band(index, band);
    }
    pub fn ms_eq_band(&self, index: usize) -> MsEqBand { self.ms_eq.band(index) }
    pub fn set_ms_eq_enabled(&mut self, enabled: bool) { self.ms_eq.set_enabled(enabled); }

    pub fn apply_ms_eq_from_state(&mut self, state: &crate::audio::equalizer::EqualizerState) {
        self.ms_eq.apply_from_state(state);
    }

    // ── Gapless ─────────────────────────────────────────────────

    /// Signal that a new track is about to begin.
    pub fn mark_new_track(&mut self) {
        self.pending_new_track = true;
        self.tail_dirty = true;
    }

    pub fn capture_gapless_tail(&mut self, buf: &[f32]) {
        self.gapless.capture_tail(buf);
    }

    /// Fix Bug #10: Copy all DSP settings from another DspEngine instance.
    ///
    /// Used during gapless track swap to apply the current engine's EQ,
    /// volume, balance, ReplayGain, stereo width, and other settings to
    /// the preloaded session's DspEngine, which starts with flat defaults.
    /// Does NOT copy runtime state (biquad delay lines, limiter envelope,
    /// gapless smoother, seek-fade ramp) — those are session-specific.
    pub fn copy_settings_from(&mut self, other: &DspEngine) {
        // Gains
        self.balance = other.balance;
        self.stereo_width = other.stereo_width;
        self.replaygain_factor = other.replaygain_factor;
        self.loudness_gain = other.loudness_gain;
        self.volume_gain = other.volume_gain;
        self.enabled = other.enabled;
        self.dither.enabled = other.dither.enabled;

        // Bass/treble shelves (coefficients + gain values)
        self.bass_db = other.bass_db;
        self.bass_freq_hz = other.bass_freq_hz;
        self.treble_db = other.treble_db;
        self.treble_freq_hz = other.treble_freq_hz;
        for ch in 0..2 {
            self.bass_shelf[ch].copy_coeffs_from(&other.bass_shelf[ch]);
            self.treble_shelf[ch].copy_coeffs_from(&other.treble_shelf[ch]);
        }

        // Parametric EQ (band parameters + smoothed coefficients)
        self.num_bands = other.num_bands;
        for i in 0..self.num_bands {
            self.band_params[i] = other.band_params[i];
            // Snap current to target so the preloaded engine starts with
            // the correct coefficients immediately (no smoothing ramp).
            self.bands[i].target[0].copy_coeffs_from(&other.bands[i].target[0]);
            self.bands[i].target[1].copy_coeffs_from(&other.bands[i].target[1]);
            self.bands[i].current[0].copy_coeffs_from(&other.bands[i].target[0]);
            self.bands[i].current[1].copy_coeffs_from(&other.bands[i].target[1]);
            self.bands[i].steps = 0;
        }

        // Preamp (auto-headroom compensation)
        self.preamp_gain = other.preamp_gain;

        // M/S EQ
        self.ms_eq.copy_settings_from(&other.ms_eq);
    }

    // ── State management ─────────────────────────────────────────

    pub fn reset_state(&mut self) {
        for ch in 0..2 {
            self.hpf[ch].reset(); self.lpf[ch].reset();
            self.bass_shelf[ch].reset(); self.treble_shelf[ch].reset();
        }
        // Fix Bug #13: Use SmoothedBand::reset() instead of manually resetting
        // only the biquad state nodes. The old code left `steps` at its previous
        // value, so after reset_state() the smoothing counter still thought it
        // was mid-interpolation, causing stale coefficient snapshots.
        for b in 0..self.num_bands { self.bands[b].reset(); }
        self.ms_eq.reset();
        self.limiter.reset();
        self.seek_fade_ramp = None;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.limiter.set_sample_rate(sample_rate);
        self.ms_eq.set_sample_rate(sample_rate);
        self.rebuild_protection_filters();
        self.set_bass(self.bass_db);
        self.set_treble(self.treble_db);
        for i in 0..self.num_bands { self.recompute_band(i); }
    }

    // ── Hot path ─────────────────────────────────────────────────

    /// Process one interleaved stereo frame through the full signal chain.
    #[inline(always)]
    pub fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.enabled { return (l, r); }

        // 1. Protection filters
        let mut l = self.lpf[0].process(self.hpf[0].process(l));
        let mut r = self.lpf[1].process(self.hpf[1].process(r));

        // 2. Channel balance
        l *= 1.0 - self.balance.max(0.0);
        r *= 1.0 + self.balance.min(0.0);

        // 3. ReplayGain
        l *= self.replaygain_factor;
        r *= self.replaygain_factor;

        // 4. EBU R128 loudness
        l *= self.loudness_gain;
        r *= self.loudness_gain;

        // 5. Preamp
        l *= self.preamp_gain;
        r *= self.preamp_gain;

        // 6. Bass shelf  (EQ before the master fader — tone shaping first)
        l = self.bass_shelf[0].process(l);
        r = self.bass_shelf[1].process(r);

        // 7. Parametric EQ cascade
        for b in 0..self.num_bands {
            let (nl, nr) = self.bands[b].process(l, r);
            l = nl; r = nr;
        }

        // 8. Treble shelf
        l = self.treble_shelf[0].process(l);
        r = self.treble_shelf[1].process(r);

        // 9. Master volume POST-EQ (seek-fade ramp runs sample-by-sample in
        //    process_buffer). The fader is placed after tone shaping so that
        //    the EQ always sees the same input level regardless of volume,
        //    preserving the intended tonal balance at all listening levels.
        l *= self.volume_gain;
        r *= self.volume_gain;

        // 10. Mid-Side stereo widening
        let mid  = (l + r) * 0.5;
        let side = (l - r) * 0.5 * self.stereo_width;
        l = mid + side;
        r = mid - side;

        // 11. M/S EQ
        let (l, r) = self.ms_eq.process_frame(l, r);

        // 12. Coupled stereo limiter (lookahead + true-peak)
        let (l, r) = self.limiter.process(l, r);

        // 13. TPDF dither
        let l = self.dither.process(l);
        let r = self.dither.process(r);

        (l, r)
    }

    /// Process an interleaved stereo buffer in-place (L, R, L, R, …).
    /// Fix M4: Changed from &mut [f32] to &mut Vec<f32> to allow padding
    /// odd-length buffers with a zero sample instead of silently dropping
    /// the last sample.
    pub fn process_buffer(&mut self, buf: &mut Vec<f32>) {
        if !self.enabled {
            self.seek_fade_ramp = None;
            return;
        }

        // Fix M4: Pad odd-length buffers with a zero sample instead of silently
        // dropping the last sample. chunks_exact_mut(2) below silently drops
        // the final sample when the buffer has odd length, causing a one-sample
        // click at the end of every buffer if the upstream source produces
        // odd-length buffers.
        if buf.len() % 2 != 0 {
            tracing::warn!(
                "process_buffer: odd-length buffer ({}), padding with one zero sample",
                buf.len()
            );
            buf.push(0.0);
        }

        // Capture gapless tail BEFORE mark_new_track processing.
        // Fix M2: Previously tail capture happened after apply_to_head, which
        // meant the first buffer of the new track was stored as the tail for
        // the next transition. For subsequent non-gapless track loads, the
        // gapless tail would be from the first buffer of the current track
        // rather than the last buffer of the outgoing track. Now we capture
        // the tail from the raw input BEFORE any gapless blending.
        if self.tail_dirty {
            self.gapless.capture_tail(buf);
            self.tail_dirty = false;
        }

        if self.pending_new_track {
            self.gapless.apply_to_head(buf);
            self.pending_new_track = false;
        }

        for frame in buf.chunks_exact_mut(2) {
            // Advance seek-fade ramp one sample at a time (smooth, click-free).
            if let Some(ref mut ramp) = self.seek_fade_ramp {
                if seek_fade::advance(ramp, &mut self.volume_gain) {
                    self.seek_fade_ramp = None;
                }
            }
            let (l, r) = self.process_frame(frame[0], frame[1]);
            frame[0] = l; frame[1] = r;
        }
    }

    // ── Private helpers ──────────────────────────────────────────

    fn rebuild_protection_filters(&mut self) {
        let hpf = Biquad::high_pass(20.0, self.sample_rate);
        let lpf = Biquad::low_pass((self.sample_rate * 0.45).min(20000.0), self.sample_rate);
        for ch in 0..2 {
            self.hpf[ch].copy_coeffs_from(&hpf);
            self.lpf[ch].copy_coeffs_from(&lpf);
        }
    }

    fn recompute_band(&mut self, index: usize) {
        let p = &self.band_params[index];
        let bq = Biquad::peaking(p.freq_hz, p.gain_db, p.q, self.sample_rate);
        self.bands[index].set_target(bq);
    }

    fn update_preamp(&mut self) {
        let mut max_boost = self.band_params[..self.num_bands]
            .iter().map(|b| b.gain_db).fold(0.0_f32, f32::max);
        // Fix Bug #12: Include bass/treble shelf boosts in auto-headroom.
        // Each shelf can add up to 12 dB; ignoring them caused clipping when
        // bass_db or treble_db was positive while parametric bands were flat.
        max_boost = max_boost.max(self.bass_db.max(0.0)).max(self.treble_db.max(0.0));
        self.preamp_gain = 10.0_f32.powf(-max_boost.max(0.0) / 20.0);
    }
}

// ── Integration tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn no_clipping_at_10db_boost() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_all_gains(&[10.0; 10]);
        let mut max_out = 0.0_f32;
        for i in 0..48000 {
            let x = (2.0 * PI * 1000.0 * i as f32 / 48000.0).sin();
            let (l, _) = engine.process_frame(x, x);
            if l.abs() > max_out { max_out = l.abs(); }
        }
        assert!(max_out <= 1.0, "output clipped at {}", max_out);
    }

    #[test]
    fn dsp_disabled_is_passthrough() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_all_gains(&[10.0; 10]);
        engine.enabled = false;
        let (l, r) = engine.process_frame(0.5, -0.5);
        assert!((l - 0.5).abs() < 1e-6);
        assert!((r - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn preamp_compensates_max_boost() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_band_gain(0, 12.0);
        assert!((engine.preamp_gain - 10.0_f32.powf(-12.0 / 20.0)).abs() < 1e-4);
    }

    #[test]
    fn buffer_processing_matches_frame() {
        let mut a = DspEngine::new(48000.0);
        let mut b = DspEngine::new(48000.0);
        let gains = [3.0, -2.0, 4.0, 0.0, 1.0, -1.0, 2.0, 0.0, -3.0, 1.0];
        a.set_all_gains(&gains);
        b.set_all_gains(&gains);
        let input: Vec<f32> = (0..512).map(|i| (i as f32 / 512.0 * 2.0 - 1.0) * 0.5).collect();
        let mut frame_out = Vec::with_capacity(512);
        for chunk in input.chunks_exact(2) {
            let (l, r) = a.process_frame(chunk[0], chunk[1]);
            frame_out.push(l); frame_out.push(r);
        }
        let mut buf = input.clone();
        b.process_buffer(&mut buf);
        for (f, bv) in frame_out.iter().zip(buf.iter()) {
            assert!((f - bv).abs() < 1e-6, "mismatch: {} vs {}", f, bv);
        }
    }

    #[test]
    fn mono_at_zero_width() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_stereo_width(0.0);
        for _ in 0..100 { let _ = engine.process_frame(0.8, -0.3); }
        let (l, r) = engine.process_frame(0.6, -0.4);
        assert!((l - r).abs() < 1e-4, "zero width should be mono: l={} r={}", l, r);
    }

    #[test]
    fn balance_mutes_right_at_neg1() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_balance(-1.0);
        let (_, r) = engine.process_frame(0.5, 0.5);
        assert!(r.abs() < 1e-4, "full-left balance should silence right: {}", r);
    }

    #[test]
    fn hpf_attenuates_dc() {
        let mut engine = DspEngine::new(48000.0);
        let mut out = 0.0;
        for _ in 0..48000 { let (l, _) = engine.process_frame(1.0, 1.0); out = l; }
        assert!(out.abs() < 0.01, "HPF must attenuate DC: {}", out);
    }

    #[test]
    fn replaygain_factor_scales_output() {
        let mut engine = DspEngine::new(48000.0);
        let (l1, _) = engine.process_frame(0.8, 0.0);
        engine.set_replaygain_factor(0.5);
        engine.reset_state();
        let (l2, _) = engine.process_frame(0.8, 0.0);
        assert!(l2 < l1 * 0.6, "RG 0.5 should halve output: l1={} l2={}", l1, l2);
        assert!(l2 > l1 * 0.35, "RG 0.5 should not over-attenuate: l1={} l2={}", l1, l2);
    }

    #[test]
    fn gapless_in_process_buffer() {
        let mut engine = DspEngine::new(48000.0);
        let mut buf1 = vec![0.5f32; 512];
        engine.process_buffer(&mut buf1);
        engine.mark_new_track();
        let mut buf2 = vec![0.0f32; 512];
        engine.process_buffer(&mut buf2);
        assert!(buf2[0].abs() > 0.001, "gapless should blend tail into head: buf2[0]={}", buf2[0]);
    }

    #[test]
    fn volume_is_applied_post_eq() {
        // With volume pre-EQ the EQ sees a lower signal level when volume < 1,
        // which would affect the preamp compensation logic. With volume post-EQ
        // the preamp_gain must be identical regardless of volume setting.
        let mut a = DspEngine::new(48000.0);
        let mut b = DspEngine::new(48000.0);
        a.set_band_gain(3, 12.0);
        b.set_band_gain(3, 12.0);
        // Same EQ, different volumes.
        a.set_volume_gain(1.0);
        b.set_volume_gain(0.5);
        // Preamp is computed from EQ gains only — must be the same.
        assert!((a.preamp_gain - b.preamp_gain).abs() < 1e-6,
            "preamp_gain should not depend on volume: a={} b={}", a.preamp_gain, b.preamp_gain);
        // The output of b should be exactly half of a for the same input.
        let x = 0.3_f32;
        // Warm up both engines identically (HPF/LPF transient).
        for _ in 0..200 { a.process_frame(x, x); b.process_frame(x, x); }
        let (al, _) = a.process_frame(x, x);
        let (bl, _) = b.process_frame(x, x);
        assert!((bl - al * 0.5).abs() < 1e-4,
            "vol=0.5 output should be half of vol=1.0: al={} bl={}", al, bl);
    }

    #[test]
    fn eq_boost_produces_gain() {
        let mut engine = DspEngine::new(48000.0);
        engine.set_band_gain(5, 6.0);
        let sr = 48000.0;
        let freq = 1000.0;
        let mut max_val = 0.0_f32;
        for i in 0..15000 {
            let x = (2.0 * PI * freq * i as f32 / sr).sin() * 0.1;
            let (l, _) = engine.process_frame(x, x);
            if i > 10000 && l.abs() > max_val { max_val = l.abs(); }
        }
        let gain_db = 20.0 * (max_val / 0.1).log10();
        assert!(gain_db > 0.0, "EQ +6 dB should produce positive gain, got {:.2} dB", gain_db);
    }
}
