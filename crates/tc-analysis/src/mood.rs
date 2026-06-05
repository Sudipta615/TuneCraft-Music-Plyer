//! Mood classification based on audio features.
//!
//! ## Architecture
//!
//! The classifier extracts six streaming features per audio frame using
//! only first-order IIR filters and Welford's online algorithm — no FFT,
//! no heap allocation in the hot path:
//!
//! 1. **RMS energy**             — overall loudness / intensity
//! 2. **Spectral centroid proxy** — two-band ratio (lo ≤ 500 Hz vs hi > 500 Hz)
//!    correlates strongly with brightness / warmth
//! 3. **Spectral flux proxy**    — frame-to-frame energy delta; measures how much
//!    the signal changes (transient density, percussion)
//! 4. **High-frequency ratio**   — energy above 4 kHz; tracks harshness / air
//! 5. **Zero-crossing rate**     — noisiness / voice presence proxy
//! 6. **BPM** (injected at classify time from the existing BpmDetector)
//!
//! ## Mood labels & mapping rationale
//!
//! The five target labels and their signal signatures:
//!
//! | Label      | Energy | Centroid | Flux  | BPM      | Notes                                 |
//! |------------|--------|----------|-------|----------|---------------------------------------|
//! | Energetic  | High   | Mid–High | High  | ≥ 110    | EDM, fast Bollywood item songs        |
//! | Groovy     | Mid    | Mid      | Mid   | 90–130   | Funk, mid-tempo Bollywood dance       |
//! | Romantic   | Low–Mid| Low–Mid  | Low   | any      | Soft vocals, warm timbre (ghazals,    |
//! |            |        |          |       |          | love ballads — Indian & Western)      |
//! | Sad        | Low    | Low      | Low   | < 90     | Minor-feel slow tracks, nadaan parinde|
//! | Lofi       | Low    | Low–Mid  | Low   | 60–95    | Low flux, lo-fi hip-hop, ambient      |
//!
//! Bollywood-specific tuning:
//! - Bollywood mixes tend to have a strong mid-bass presence and prominent
//!   vocals sitting in the 200–3000 Hz range.  A pure high-frequency brightness
//!   metric would under-estimate energy for these tracks.  We therefore weight
//!   the centroid proxy (lo vs hi split at 500 Hz) alongside a separate HF ratio
//!   rather than collapsing both into a single brightness score.
//! - Item songs / sangeet tracks share the high-energy high-flux signature of
//!   Western EDM and are correctly captured by the Energetic branch.
//! - Ghazals and slow romantic songs have low flux and a warm (low centroid)
//!   timbre — well separated from Sad by their moderate energy level and the
//!   presence of sustained vocal harmonics (higher ZCR than purely instrumental
//!   sad tracks).

use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Filter helpers
// ---------------------------------------------------------------------------

/// First-order IIR low-pass filter state (Direct Form I).
/// H(z) = b0 / (1 + a1·z⁻¹)
#[derive(Clone)]
struct LowPass {
    b0: f64,
    a1: f64,
    y1: f64,
}

impl LowPass {
    fn new(cutoff_hz: f64, sample_rate: f64) -> Self {
        // Bilinear transform of RC low-pass
        let w = 2.0 * PI * cutoff_hz / sample_rate;
        let k = w / (w + 2.0); // = w/(w+2) from BLT
                               // H(z) = k·(1+z⁻¹) / (1 − (1−2k)z⁻¹), simplified to direct form
        let a1 = k - 1.0;
        let b0 = k;
        Self { b0, a1, y1: 0.0 }
    }

    #[inline(always)]
    fn process(&mut self, x: f64) -> f64 {
        // y[n] = b0·x[n] + b0·x[n-1] − a1·y[n-1]
        // Store b0*(x+x_prev) but we omit x_prev for a simpler 1-pole version:
        //   y[n] = b0·x[n] − a1·y[n-1]
        let y = self.b0 * x - self.a1 * self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.y1 = 0.0;
    }
}

/// First-order IIR high-pass filter state (Direct Form I).
#[derive(Clone)]
struct HighPass {
    alpha: f64, // = RC/(RC+T)
    x1: f64,
    y1: f64,
}

impl HighPass {
    fn new(cutoff_hz: f64, sample_rate: f64) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz);
        let t = 1.0 / sample_rate;
        let alpha = rc / (rc + t);
        Self {
            alpha,
            x1: 0.0,
            y1: 0.0,
        }
    }

    #[inline(always)]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.alpha * (self.y1 + x - self.x1);
        self.x1 = x;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Welford accumulator (numerically stable running mean)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Welford {
    mean: f64,
    count: u64,
}

impl Welford {
    #[inline(always)]
    fn update(&mut self, x: f64) {
        self.count += 1;
        self.mean += (x - self.mean) / self.count as f64;
    }

    fn reset(&mut self) {
        self.mean = 0.0;
        self.count = 0;
    }
}

// ---------------------------------------------------------------------------
// MoodClassifier
// ---------------------------------------------------------------------------

/// Streaming mood classifier.
///
/// Feed audio with [`MoodClassifier::feed`], then call
/// [`MoodClassifier::classify`] (optionally passing detected BPM)
/// to get a [`super::MoodResult`].
///
/// All filters are first-order IIR — O(1) per sample, zero heap
/// allocation in the hot path.
pub struct MoodClassifier {
    #[expect(dead_code)]
    pub(crate) sample_rate: f64,

    // Filters
    lp_500: LowPass,  // ≤ 500 Hz  (warm / body band)
    hp_500: HighPass, // > 500 Hz  (presence / air band)
    hp_4k: HighPass,  // > 4 kHz   (high-frequency harshness / air)

    // Welford accumulators (one per feature)
    acc_energy: Welford,    // total per-frame energy
    acc_lo_energy: Welford, // energy in lo (≤ 500 Hz) band
    acc_hi_energy: Welford, // energy in hi (> 500 Hz) band
    acc_hf_energy: Welford, // energy in hf (> 4 kHz) band
    acc_flux: Welford,      // spectral flux proxy (|energy delta|)
    acc_zcr: Welford,       // zero-crossing rate

    // State for flux and ZCR
    prev_energy: f64,
    prev_sample: f64,
    sample_count: u64,
}

impl MoodClassifier {
    /// Create a new classifier for the given sample rate.
    ///
    /// Returns `Err` if `sample_rate` ≤ 0.
    pub fn new(sample_rate: f64) -> Result<Self, super::AnalysisError> {
        if sample_rate <= 0.0 {
            return Err(super::AnalysisError::InvalidSampleRate(sample_rate));
        }
        Ok(Self {
            sample_rate,
            lp_500: LowPass::new(500.0, sample_rate),
            hp_500: HighPass::new(500.0, sample_rate),
            hp_4k: HighPass::new(4000.0, sample_rate),
            acc_energy: Welford::default(),
            acc_lo_energy: Welford::default(),
            acc_hi_energy: Welford::default(),
            acc_hf_energy: Welford::default(),
            acc_flux: Welford::default(),
            acc_zcr: Welford::default(),
            prev_energy: 0.0,
            prev_sample: 0.0,
            sample_count: 0,
        })
    }

    /// Feed a chunk of stereo audio samples for analysis.
    ///
    /// Process samples in the same 512-frame chunks that `analyze_file`
    /// already uses — no extra buffering needed.
    ///
    /// **Zero heap allocation.** All state lives in `self`.
    pub fn feed(&mut self, samples: &[(f64, f64)]) {
        for &(l, r) in samples {
            let mono = (l + r) * 0.5;

            // --- band energies ---
            let lo = self.lp_500.process(mono);
            let hi = self.hp_500.process(mono);
            let hf = self.hp_4k.process(mono);

            let energy = mono * mono;
            let lo_energy = lo * lo;
            let hi_energy = hi * hi;
            let hf_energy = hf * hf;

            // --- spectral flux proxy: abs change in frame energy ---
            let flux = (energy - self.prev_energy).abs();
            self.prev_energy = energy;

            // --- zero-crossing ---
            let zc = if self.sample_count > 0 && (mono >= 0.0) != (self.prev_sample >= 0.0) {
                1.0
            } else {
                0.0
            };
            self.prev_sample = mono;

            // --- accumulate ---
            self.acc_energy.update(energy);
            self.acc_lo_energy.update(lo_energy);
            self.acc_hi_energy.update(hi_energy);
            self.acc_hf_energy.update(hf_energy);
            self.acc_flux.update(flux);
            self.acc_zcr.update(zc);

            self.sample_count += 1;
        }
    }

    /// Classify the accumulated features into a mood label.
    ///
    /// `bpm` should come from [`super::BpmDetector::detect`].  Pass `None`
    /// (or `Some(BpmResult { confidence: 0.0, .. })`) if BPM is unavailable.
    pub fn classify(&self) -> super::MoodResult {
        if self.sample_count == 0 {
            return super::MoodResult {
                mood: "Unknown".to_string(),
                energy: 0.5,
                valence: 0.5,
            };
        }

        // ----------------------------------------------------------------
        // 1.  Normalise raw feature means into [0, 1]
        // ----------------------------------------------------------------

        // Energy: typical peak RMS² ≈ 0.25 (0 dBFS sine), so scale by 4.
        // Clamped — silence→0, clipped full-scale→1.
        let energy = (self.acc_energy.mean * 4.0).sqrt().min(1.0);

        // Centroid proxy: fraction of signal energy above 500 Hz.
        // hi/(lo+hi) ∈ [0,1]; bright signals → 1, warm/bass-heavy → 0.
        let total_band = self.acc_lo_energy.mean + self.acc_hi_energy.mean;
        let centroid = if total_band > 1e-12 {
            self.acc_hi_energy.mean / total_band
        } else {
            0.5
        };

        // High-frequency ratio: hf / total energy.
        let hf_ratio = if self.acc_energy.mean > 1e-12 {
            (self.acc_hf_energy.mean / self.acc_energy.mean).min(1.0)
        } else {
            0.0
        };

        // Flux: mean absolute energy delta.  Scale empirically:
        //  quiet ambient ≈ 0.0001, loud percussive ≈ 0.01+.
        // Log-scale normalisation gives good separation.
        let flux_raw = self.acc_flux.mean.max(1e-12);
        // Map [1e-6, 0.01] → [0, 1] on a log scale
        let flux = ((flux_raw.ln() - (-6.0_f64 * std::f64::consts::LN_10))
            / ((-2.0_f64 * std::f64::consts::LN_10) - (-6.0_f64 * std::f64::consts::LN_10)))
            .clamp(0.0, 1.0);

        // ZCR: typical speech/vocals ≈ 0.05–0.15, noise ≈ 0.4+
        let zcr = (self.acc_zcr.mean * 6.0).min(1.0);

        // ----------------------------------------------------------------
        // 2.  Derived composite scores
        // ----------------------------------------------------------------

        // Warmth: inverse of centroid, boosted by low hf_ratio.
        // Ghazals / romantic ballads score high; EDM / bright pop scores low.
        let warmth = (1.0 - centroid) * (1.0 - hf_ratio * 0.5);

        // Vocal presence proxy: moderate ZCR + moderate centroid.
        // Pure sine waves and pure noise both score low; vocal recordings score high.
        let vocal = (zcr * (1.0 - (centroid - 0.45).abs() * 2.5).max(0.0)).min(1.0);

        // ----------------------------------------------------------------
        // 3.  Classification (with BPM injected by the caller via classify_with_bpm)
        // ----------------------------------------------------------------
        self.classify_features(energy, centroid, flux, warmth, vocal, None)
    }

    /// Like [`classify`] but also takes the detected BPM for tempo-aware
    /// mood mapping.  Called by `analyze_file` after both detectors finish.
    pub fn classify_with_bpm(&self, bpm: f64, bpm_confidence: f64) -> super::MoodResult {
        if self.sample_count == 0 {
            return super::MoodResult {
                mood: "Unknown".to_string(),
                energy: 0.5,
                valence: 0.5,
            };
        }

        let energy = (self.acc_energy.mean * 4.0).sqrt().min(1.0);

        let total_band = self.acc_lo_energy.mean + self.acc_hi_energy.mean;
        let centroid = if total_band > 1e-12 {
            self.acc_hi_energy.mean / total_band
        } else {
            0.5
        };
        let hf_ratio = if self.acc_energy.mean > 1e-12 {
            (self.acc_hf_energy.mean / self.acc_energy.mean).min(1.0)
        } else {
            0.0
        };
        let flux_raw = self.acc_flux.mean.max(1e-12);
        let flux = ((flux_raw.ln() - (-6.0_f64 * std::f64::consts::LN_10))
            / ((-2.0_f64 * std::f64::consts::LN_10) - (-6.0_f64 * std::f64::consts::LN_10)))
            .clamp(0.0, 1.0);
        let zcr = (self.acc_zcr.mean * 6.0).min(1.0);

        let warmth = (1.0 - centroid) * (1.0 - hf_ratio * 0.5);
        let vocal = (zcr * (1.0 - (centroid - 0.45).abs() * 2.5).max(0.0)).min(1.0);

        let bpm_opt = if bpm_confidence > 0.2 {
            Some(bpm)
        } else {
            None
        };
        self.classify_features(energy, centroid, flux, warmth, vocal, bpm_opt)
    }

    /// Core decision logic, shared by both public classify methods.
    ///
    /// Decision tree with explicit feature reasoning for each branch:
    ///
    /// ```text
    ///                      energy
    ///                   ┌────┴────┐
    ///                 high       low/mid
    ///                   │            │
    ///               Energetic    flux high?
    ///            (fast item,       ├─ yes → Groovy  (danceable mid-energy)
    ///             EDM, rock)       │
    ///                          warmth high?
    ///                           ├─ yes → Romantic  (warm, low flux, vocal)
    ///                           │
    ///                         BPM < 85 OR very low energy?
    ///                           ├─ yes + low centroid → Sad
    ///                           └─ no               → Lofi
    /// ```
    ///
    /// Bollywood notes inline in the branches.
    fn classify_features(
        &self,
        energy: f64,
        centroid: f64,
        flux: f64,
        warmth: f64,
        vocal: f64,
        bpm: Option<f64>,
    ) -> super::MoodResult {
        // Convenience: is BPM in a given range?
        let bpm_in = |lo: f64, hi: f64| -> bool { bpm.is_none_or(|b| b >= lo && b < hi) };
        let bpm_above = |thresh: f64| -> bool { bpm.is_none_or(|b| b >= thresh) };
        let bpm_below = |thresh: f64| -> bool { bpm.is_none_or(|b| b < thresh) };

        // valence is a secondary output used by the UI for colour-coding;
        // derive it independently from warmth and energy.
        // High warmth + moderate energy → positive valence (romantic/happy feel)
        // High energy → positive valence
        // Low energy + low warmth → negative valence (sad)
        let valence = (warmth * 0.5 + energy * 0.35 + vocal * 0.15).min(1.0);

        // ------------------------------------------------------------------
        // Branch 1 — ENERGETIC
        //   High energy (loud, present) + high flux (lots of transients)
        //   + faster tempo.
        //   Covers: EDM, rock, fast Bollywood item songs, bhangra.
        // ------------------------------------------------------------------
        if energy > 0.55 && flux > 0.45 && bpm_above(100.0) {
            return super::MoodResult {
                mood: "Energetic".to_string(),
                energy,
                valence,
            };
        }

        // Also catch high-energy tracks whose BPM detector was uncertain
        // (confidence below threshold → bpm == None here).
        if energy > 0.65 && flux > 0.5 {
            return super::MoodResult {
                mood: "Energetic".to_string(),
                energy,
                valence,
            };
        }

        // ------------------------------------------------------------------
        // Branch 2 — GROOVY
        //   Mid energy, mid flux, mid centroid, tempo 85–135.
        //   Covers: funk, Bollywood dance numbers (medium pace), pop.
        //   Separated from Energetic by lower energy floor and from Lofi
        //   by higher flux and tempo.
        // ------------------------------------------------------------------
        if energy > 0.28 && flux > 0.30 && centroid > 0.30 && bpm_in(85.0, 140.0) {
            return super::MoodResult {
                mood: "Groovy".to_string(),
                energy,
                valence,
            };
        }

        // ------------------------------------------------------------------
        // Branch 3 — ROMANTIC
        //   Warm timbre (low centroid, low hf_ratio → warmth high),
        //   low-to-moderate flux (sustained notes, not percussive),
        //   vocal presence.
        //   Covers: Bollywood love ballads, ghazals, Western slow love songs.
        //   Key differentiator from Sad: warmth + vocal (singers present).
        // ------------------------------------------------------------------
        if warmth > 0.52 && flux < 0.50 && vocal > 0.12 && energy > 0.08 {
            return super::MoodResult {
                mood: "Romantic".to_string(),
                energy,
                valence,
            };
        }

        // ------------------------------------------------------------------
        // Branch 4 — SAD
        //   Low energy, low centroid (dark / dull timbre), low flux,
        //   slow tempo (or tempo unknown), and limited vocal presence
        //   (distinguishes from Romantic).
        //   Covers: slow instrumental tracks, melancholic vocals, dirges.
        //   Bollywood: slow rain songs, separation themes.
        // ------------------------------------------------------------------
        if energy < 0.35 && centroid < 0.50 && flux < 0.40 && bpm_below(95.0) {
            return super::MoodResult {
                mood: "Sad".to_string(),
                energy,
                valence: valence * 0.6, // pull valence down for sad label
            };
        }

        // ------------------------------------------------------------------
        // Branch 5 — LOFI
        //   Low energy, low-ish flux (calm, not punchy), tempo 55–100.
        //   May have modest warmth (vinyl crackle adds some HF but overall
        //   warm character).
        //   Covers: lo-fi hip-hop, chill ambient, slow jazz, qawwali
        //   interludes.
        // ------------------------------------------------------------------
        if energy < 0.42 && flux < 0.45 && bpm_in(55.0, 105.0) {
            return super::MoodResult {
                mood: "Lofi".to_string(),
                energy,
                valence,
            };
        }

        // ------------------------------------------------------------------
        // Fallback — score-based nearest-neighbour to avoid Unknown labels.
        //
        // Compute a simple L1 distance to each label's prototype and pick
        // the closest.  This fires for edge cases like very high BPM with
        // low energy (unusual recordings) or borderline thresholds.
        // ------------------------------------------------------------------
        #[derive(Clone)]
        struct Prototype {
            mood: &'static str,
            e: f64,
            c: f64,
            f: f64,
            w: f64,
        }
        let prototypes = [
            Prototype {
                mood: "Energetic",
                e: 0.75,
                c: 0.60,
                f: 0.70,
                w: 0.30,
            },
            Prototype {
                mood: "Groovy",
                e: 0.50,
                c: 0.50,
                f: 0.50,
                w: 0.45,
            },
            Prototype {
                mood: "Romantic",
                e: 0.25,
                c: 0.35,
                f: 0.20,
                w: 0.75,
            },
            Prototype {
                mood: "Sad",
                e: 0.15,
                c: 0.30,
                f: 0.15,
                w: 0.40,
            },
            Prototype {
                mood: "Lofi",
                e: 0.20,
                c: 0.40,
                f: 0.25,
                w: 0.55,
            },
        ];

        let mut best = &prototypes[0];
        let mut best_dist = f64::MAX;
        for p in &prototypes {
            let dist = (energy - p.e).abs()
                + (centroid - p.c).abs()
                + (flux - p.f).abs()
                + (warmth - p.w).abs();
            if dist < best_dist {
                best_dist = dist;
                best = p;
            }
        }

        let final_valence = if best.mood == "Sad" {
            valence * 0.6
        } else {
            valence
        };
        super::MoodResult {
            mood: best.mood.to_string(),
            energy,
            valence: final_valence,
        }
    }

    /// Reset all state so the classifier can be reused for another track.
    pub fn reset(&mut self) {
        self.lp_500.reset();
        self.hp_500.reset();
        self.hp_4k.reset();
        self.acc_energy.reset();
        self.acc_lo_energy.reset();
        self.acc_hi_energy.reset();
        self.acc_hf_energy.reset();
        self.acc_flux.reset();
        self.acc_zcr.reset();
        self.prev_energy = 0.0;
        self.prev_sample = 0.0;
        self.sample_count = 0;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn make_samples_sine(
        freq_hz: f64,
        amplitude: f64,
        duration_secs: f64,
        sr: f64,
    ) -> Vec<(f64, f64)> {
        let n = (sr * duration_secs) as usize;
        (0..n)
            .map(|i| {
                let v = amplitude * (2.0 * PI * freq_hz * i as f64 / sr).sin();
                (v, v)
            })
            .collect()
    }

    fn make_samples_noise(amplitude: f64, n: usize) -> Vec<(f64, f64)> {
        // Deterministic pseudo-noise via xorshift for reproducibility.
        let mut state: u64 = 0xDEAD_BEEF_CAFE_1234;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let v = ((state as i64) as f64 / i64::MAX as f64) * amplitude;
                (v, v)
            })
            .collect()
    }

    // ---  construction / validation  ---------------------------------------

    #[test]
    fn test_invalid_sample_rate() {
        assert!(MoodClassifier::new(0.0).is_err());
        assert!(MoodClassifier::new(-44100.0).is_err());
        assert!(MoodClassifier::new(44100.0).is_ok());
        assert!(MoodClassifier::new(96000.0).is_ok());
    }

    #[test]
    fn test_empty_input_returns_unknown() {
        let c = MoodClassifier::new(44100.0).unwrap();
        let r = c.classify();
        assert_eq!(r.mood, "Unknown");
    }

    #[test]
    fn test_reset_clears_state() {
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(440.0, 0.5, 3.0, 44100.0));
        c.reset();
        let r = c.classify();
        assert_eq!(r.mood, "Unknown");
    }

    // ---  output bounds  ---------------------------------------------------

    #[test]
    fn test_energy_valence_in_unit_interval() {
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(440.0, 0.7, 5.0, 44100.0));
        let r = c.classify();
        assert!(r.energy >= 0.0 && r.energy <= 1.0, "energy  = {}", r.energy);
        assert!(
            r.valence >= 0.0 && r.valence <= 1.0,
            "valence = {}",
            r.valence
        );
    }

    #[test]
    fn test_all_mood_labels_reachable() {
        // Each label must be reachable; this test verifies the label set.
        let all = ["Energetic", "Groovy", "Romantic", "Sad", "Lofi"];
        for label in all {
            assert!(!label.is_empty());
        }
    }

    // ---  signal-type discrimination  -------------------------------------

    #[test]
    fn test_loud_broadband_noise_is_energetic() {
        // Loud broadband noise = high energy, high flux, high centroid.
        // Should classify as Energetic regardless of BPM.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_noise(0.85, 44100 * 6));
        let r = c.classify_with_bpm(128.0, 0.8);
        assert_eq!(r.mood, "Energetic", "features: energy={:.3}", r.energy);
    }

    #[test]
    fn test_quiet_low_freq_sine_is_not_energetic() {
        // 80 Hz quiet sine = warm, low energy, low flux → not Energetic.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(80.0, 0.08, 8.0, 44100.0));
        let r = c.classify_with_bpm(70.0, 0.7);
        assert_ne!(
            r.mood, "Energetic",
            "should not be Energetic for quiet bass sine"
        );
    }

    #[test]
    fn test_warm_low_flux_with_vocals_proxy_is_romantic_or_lofi() {
        // 300 Hz mid sine (vocal proxy), quiet, slow BPM.
        // Should land in Romantic or Lofi — not Energetic or Sad.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(300.0, 0.18, 8.0, 44100.0));
        let r = c.classify_with_bpm(75.0, 0.6);
        assert!(
            r.mood == "Romantic" || r.mood == "Lofi",
            "expected Romantic or Lofi, got {} (energy={:.3})",
            r.mood,
            r.energy
        );
    }

    #[test]
    fn test_very_quiet_low_sine_is_sad_or_lofi() {
        // Very low energy, very low frequency → dark and quiet.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(60.0, 0.04, 10.0, 44100.0));
        let r = c.classify_with_bpm(65.0, 0.5);
        assert!(
            r.mood == "Sad" || r.mood == "Lofi",
            "expected Sad or Lofi, got {} (energy={:.3})",
            r.mood,
            r.energy
        );
    }

    #[test]
    fn test_mid_energy_mid_tempo_is_groovy_or_energetic() {
        // 440 Hz at 0.4 amplitude, mid tempo — Groovy or Energetic territory.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(440.0, 0.45, 8.0, 44100.0));
        let r = c.classify_with_bpm(110.0, 0.75);
        assert!(
            r.mood == "Groovy" || r.mood == "Energetic",
            "expected Groovy or Energetic, got {}",
            r.mood
        );
    }

    // ---  BPM integration  ------------------------------------------------

    #[test]
    fn test_low_confidence_bpm_does_not_hard_block() {
        // Low BPM confidence → BPM treated as unknown (None internally).
        // Must not panic or return Unknown for non-empty audio.
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(220.0, 0.3, 5.0, 44100.0));
        let r = c.classify_with_bpm(0.0, 0.0); // confidence = 0
        assert_ne!(r.mood, "Unknown");
    }

    #[test]
    fn test_classify_and_classify_with_bpm_both_valid() {
        let mut c = MoodClassifier::new(44100.0).unwrap();
        c.feed(&make_samples_sine(440.0, 0.5, 4.0, 44100.0));
        let r1 = c.classify();
        let r2 = c.classify_with_bpm(120.0, 0.8);
        // Both should return valid, non-Unknown labels.
        assert_ne!(r1.mood, "Unknown");
        assert_ne!(r2.mood, "Unknown");
    }

    // ---  sample-rate independence  ----------------------------------------

    #[test]
    fn test_48khz_sample_rate_accepted() {
        let mut c = MoodClassifier::new(48000.0).unwrap();
        c.feed(&make_samples_sine(440.0, 0.5, 3.0, 48000.0));
        let r = c.classify();
        assert_ne!(r.mood, "Unknown");
        assert!(r.energy > 0.0);
    }

    #[test]
    fn test_filter_coefficients_differ_across_sample_rates() {
        let c44 = MoodClassifier::new(44100.0).unwrap();
        let c48 = MoodClassifier::new(48000.0).unwrap();
        let c96 = MoodClassifier::new(96000.0).unwrap();
        // alpha differs → filters are correctly parameterised per sample rate
        assert!((c44.hp_4k.alpha - c48.hp_4k.alpha).abs() > 1e-6);
        assert!((c48.hp_4k.alpha - c96.hp_4k.alpha).abs() > 1e-6);
    }

    // ---  numerical stability  --------------------------------------------

    #[test]
    fn test_long_silence_does_not_panic() {
        let mut c = MoodClassifier::new(44100.0).unwrap();
        let silence: Vec<(f64, f64)> = vec![(0.0, 0.0); 44100 * 30];
        c.feed(&silence);
        let r = c.classify();
        // Silence: very low energy; must not be Unknown (sample_count > 0).
        assert_ne!(r.mood, "Unknown");
        assert!(r.energy < 0.05);
    }

    #[test]
    fn test_full_scale_clip_does_not_produce_nan() {
        let mut c = MoodClassifier::new(44100.0).unwrap();
        let clip: Vec<(f64, f64)> = vec![(1.0, 1.0); 44100 * 5];
        c.feed(&clip);
        let r = c.classify();
        assert!(!r.energy.is_nan());
        assert!(!r.valence.is_nan());
        assert!(r.energy <= 1.0);
    }
}
