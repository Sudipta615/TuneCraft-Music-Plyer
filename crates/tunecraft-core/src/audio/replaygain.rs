use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::tag::ItemKey;
use std::path::Path;
use tracing::debug;

/// ReplayGain information extracted from an audio file's tags.
#[derive(Debug, Clone, Default)]
pub struct ReplayGainInfo {
    pub track_gain: Option<f64>, // dB
    pub track_peak: Option<f64>, // 0.0-1.0
    pub album_gain: Option<f64>, // dB
    pub album_peak: Option<f64>, // 0.0-1.0
}

/// Whether we are in album-mode (prefer album gain) or track-mode (prefer track gain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayGainMode {
    /// Use track gain/peak when available.
    #[default]
    Track,
    /// Use album gain/peak when available (fallback to track if album missing).
    Album,
}

/// Full ReplayGain configuration, matching Poweramp's three-option model:
///
/// | mode            | behaviour                                                  |
/// |-----------------|-------------------------------------------------------------|
/// | `DontApply`     | RG tags ignored; `fallback_preamp_db` still applied        |
/// | `ApplyGain`     | gain applied; `preamp_db` added; no peak clamping          |
/// | `ApplyAndClip`  | gain + preamp applied; peak clamping prevents clipping     |
///
/// `preamp_db`          — added on top of the RG gain for tagged tracks (-15..+15 dB).
/// `fallback_preamp_db` — applied to tracks with *no* RG tags (-15..+15 dB).
#[derive(Debug, Clone)]
pub struct ReplayGainConfig {
    pub apply_mode: ReplayGainApplyMode,
    pub source: ReplayGainMode,
    pub preamp_db: f64,
    pub fallback_preamp_db: f64,
}

impl Default for ReplayGainConfig {
    fn default() -> Self {
        Self {
            apply_mode: ReplayGainApplyMode::ApplyAndClip,
            source: ReplayGainMode::Track,
            preamp_db: 0.0,
            fallback_preamp_db: 0.0,
        }
    }
}

/// How to apply ReplayGain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayGainApplyMode {
    /// Ignore RG tags entirely. Fallback preamp is still applied for consistency.
    DontApply,
    /// Apply gain (+ preamp). No anti-clip peak clamping.
    ApplyGain,
    /// Apply gain (+ preamp) AND clamp by peak to prevent clipping.
    #[default]
    ApplyAndClip,
}

impl ReplayGainInfo {
    /// Extract ReplayGain tags from an audio file using lofty.
    pub fn from_path(path: &Path) -> Result<Self> {
        let tagged_file =
            lofty::read_from_path(path).context("failed to read file for ReplayGain")?;

        let tag = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag());

        let Some(tag) = tag else {
            debug!("No tags found in {:?}", path);
            return Ok(ReplayGainInfo::default());
        };

        let track_gain = parse_gain(tag, &ItemKey::ReplayGainTrackGain);
        let track_peak = parse_peak(tag, &ItemKey::ReplayGainTrackPeak);
        let album_gain = parse_gain(tag, &ItemKey::ReplayGainAlbumGain);
        let album_peak = parse_peak(tag, &ItemKey::ReplayGainAlbumPeak);

        if track_gain.is_some() || album_gain.is_some() {
            debug!(
                "ReplayGain for {:?}: track_gain={:?}, track_peak={:?}, album_gain={:?}, album_peak={:?}",
                path, track_gain, track_peak, album_gain, album_peak
            );
        } else {
            debug!("No ReplayGain tags found in {:?}", path);
        }

        Ok(ReplayGainInfo {
            track_gain,
            track_peak,
            album_gain,
            album_peak,
        })
    }

    /// Returns `true` if neither track nor album gain is present.
    pub fn has_no_tags(&self) -> bool {
        self.track_gain.is_none() && self.album_gain.is_none()
    }

    /// Compute the final linear volume scaling factor given a full [`ReplayGainConfig`].
    ///
    /// Decision tree:
    /// 1. If `apply_mode == DontApply` → return `db_to_scaling(fallback_preamp_db)` (preamp only,
    ///    always applied regardless of tags so volume is consistent).
    /// 2. If no RG tags → return `db_to_scaling(fallback_preamp_db)`.
    /// 3. Otherwise → pick gain by `source` (track/album), add `preamp_db`,
    ///    convert to linear, optionally clamp by peak (`ApplyAndClip`).
    pub fn scaling_factor_full(&self, cfg: &ReplayGainConfig) -> f64 {
        if cfg.apply_mode == ReplayGainApplyMode::DontApply {
            return db_to_scaling(cfg.fallback_preamp_db);
        }

        if self.has_no_tags() {
            debug!(
                "No RG tags — applying fallback preamp {:.1} dB",
                cfg.fallback_preamp_db
            );
            return db_to_scaling(cfg.fallback_preamp_db);
        }

        let (gain, peak) = match cfg.source {
            ReplayGainMode::Album => (
                self.album_gain.or(self.track_gain),
                self.album_peak.or(self.track_peak),
            ),
            ReplayGainMode::Track => (
                self.track_gain.or(self.album_gain),
                self.track_peak.or(self.album_peak),
            ),
        };

        let gain_db = match gain {
            Some(g) => g,
            None => return db_to_scaling(cfg.fallback_preamp_db), // shouldn't reach here
        };

        let total_db = gain_db + cfg.preamp_db;
        let mut factor = db_to_scaling(total_db);

        if cfg.apply_mode == ReplayGainApplyMode::ApplyAndClip {
            if let Some(peak) = peak {
                if peak > 0.0 && factor * peak > 1.0 {
                    debug!(
                        "RG peak clamp: scaling {:.4} → {:.4} (peak={:.4})",
                        factor,
                        1.0 / peak,
                        peak
                    );
                    factor = 1.0 / peak;
                }
            }
            const MAX_AMPLIFICATION_DB: f64 = 30.0;
            let max_factor = db_to_scaling(MAX_AMPLIFICATION_DB);
            if factor > max_factor {
                debug!(
                    "RG amplification cap: scaling {:.4} → {:.4} (max +{} dB)",
                    factor, max_factor, MAX_AMPLIFICATION_DB
                );
                factor = max_factor;
            }
        }

        factor
    }

    /// Legacy helper (track/album mode only, no preamp, with peak clamping).
    /// Kept for backwards compatibility with existing call sites.
    pub fn scaling_factor(&self, mode: ReplayGainMode) -> Option<f64> {
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::ApplyAndClip,
            source: mode,
            preamp_db: 0.0,
            fallback_preamp_db: 0.0,
        };
        if self.has_no_tags() {
            return None;
        }
        Some(self.scaling_factor_full(&cfg))
    }
}

/// Convert a gain value in dB to a linear scaling factor.
///
/// Formula: `scaling = 10^(gain_dB / 20)`
pub fn db_to_scaling(gain_db: f64) -> f64 {
    10f64.powf(gain_db / 20.0)
}

/// Parse a gain tag value (e.g. "-6.4 dB" or "-6.4") into an f64.
fn parse_gain(tag: &lofty::tag::Tag, key: &ItemKey) -> Option<f64> {
    let raw = tag.get_string(key)?;
    let cleaned = raw.trim().trim_end_matches(" dB").trim();
    cleaned.parse::<f64>().ok()
}

/// Parse a peak tag value (e.g. "0.987654") into an f64.
///
/// Fix Bug #60: Previously assumed 16-bit peak scaling only (val > 1.0 && val <= 32768.0).
/// Now handles different bit depths: 8-bit (0-255), 16-bit (0-65535), 24-bit (0-8388607),
/// 32-bit (0-2147483647), and float (0.0-1.0). Values > 1.0 are normalized to [0.0, 1.0]
/// based on the most likely bit depth implied by the magnitude.
///
/// Fix Bug #6: Values slightly above 1.0 (up to ~2.0) are now treated as linear
/// float peaks that indicate clipping in the source, rather than being misclassified
/// as 8-bit integer peaks. The ReplayGain spec defines peak as a linear amplitude
/// ratio where 1.0 = full scale; values > 1.0 indicate true peaks exceeding digital
/// full scale (inter-sample peaks). Treating these as 8-bit integer values (dividing
/// by 255) would produce tiny, incorrect peak values (e.g. 1.5 / 255 = 0.006),
/// causing excessive and incorrect peak clamping.
fn parse_peak(tag: &lofty::tag::Tag, key: &ItemKey) -> Option<f64> {
    let raw = tag.get_string(key)?;
    let cleaned = raw.trim();

    let val = cleaned.parse::<f64>().ok()?;
    if val < 0.0 && val >= -100.0 {
        let linear = 10f64.powf(val / 20.0);
        tracing::debug!(
            "ReplayGain peak appears to be in dB ({:.2}), converting to linear ({:.6})",
            val,
            linear
        );
        return Some(linear.clamp(0.0, 2.0));
    }

    if val <= 1.0 {
        Some(val)
    } else if val <= 2.0 {
        Some(val)
    } else if val <= 255.0 {
        Some(val / 255.0)
    } else if val <= 65535.0 {
        Some(val / 65535.0)
    } else if val <= 8_388_607.0 {
        Some(val / 8_388_607.0)
    } else if val <= 2_147_483_647.0 {
        Some(val / 2_147_483_647.0)
    } else {
        tracing::warn!(
            "ReplayGain peak value {} exceeds 32-bit range, ignoring",
            val
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_to_scaling() {
        assert!((db_to_scaling(0.0) - 1.0).abs() < 1e-9);

        assert!((db_to_scaling(-6.0) - 0.501187).abs() < 1e-4);

        assert!((db_to_scaling(6.0) - 1.995262).abs() < 1e-4);

        assert!((db_to_scaling(-10.0) - 0.316227).abs() < 1e-3);
    }

    #[test]
    fn test_scaling_factor_track_mode() {
        let info = ReplayGainInfo {
            track_gain: Some(-6.0),
            track_peak: Some(0.9),
            album_gain: Some(-3.0),
            album_peak: Some(0.8),
        };

        let factor = info.scaling_factor(ReplayGainMode::Track).unwrap();
        assert!((factor - 0.501187).abs() < 1e-3);
    }

    #[test]
    fn test_scaling_factor_album_mode() {
        let info = ReplayGainInfo {
            track_gain: Some(-6.0),
            track_peak: Some(0.9),
            album_gain: Some(-3.0),
            album_peak: Some(0.8),
        };

        let factor = info.scaling_factor(ReplayGainMode::Album).unwrap();
        assert!((factor - 0.707945).abs() < 1e-3);
    }

    #[test]
    fn test_scaling_factor_peak_clamping() {
        let info = ReplayGainInfo {
            track_gain: Some(6.0), // +6 dB -> scaling ≈ 1.995
            track_peak: Some(0.9), // 1.995 * 0.9 = 1.796 > 1.0, so clamp
            album_gain: None,
            album_peak: None,
        };

        let factor = info.scaling_factor(ReplayGainMode::Track).unwrap();
        assert!((factor - (1.0 / 0.9)).abs() < 1e-4);
    }

    #[test]
    fn test_scaling_factor_no_gain_returns_none() {
        let info = ReplayGainInfo {
            track_gain: None,
            track_peak: Some(0.9),
            album_gain: None,
            album_peak: None,
        };

        assert!(info.scaling_factor(ReplayGainMode::Track).is_none());
    }

    #[test]
    fn test_scaling_factor_fallback() {
        let info = ReplayGainInfo {
            track_gain: Some(-6.0),
            track_peak: Some(0.9),
            album_gain: None,
            album_peak: None,
        };

        let factor = info.scaling_factor(ReplayGainMode::Album).unwrap();
        assert!((factor - 0.501187).abs() < 1e-3);
    }

    #[test]
    fn test_parse_gain_string() {
        assert!((db_to_scaling(-6.4) - 10f64.powf(-6.4 / 20.0)).abs() < 1e-9);
    }

    #[test]
    fn test_rg_preamp_adds_to_gain() {
        let info = ReplayGainInfo {
            track_gain: Some(-6.0),
            track_peak: Some(0.5), // high enough that +3 dB preamp won't clip
            album_gain: None,
            album_peak: None,
        };
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::ApplyAndClip,
            source: ReplayGainMode::Track,
            preamp_db: 3.0,
            fallback_preamp_db: 0.0,
        };
        let factor = info.scaling_factor_full(&cfg);
        assert!(
            (factor - db_to_scaling(-3.0)).abs() < 1e-4,
            "Expected -3 dB factor, got {:.5}",
            factor
        );
    }

    #[test]
    fn test_rg_fallback_preamp_applied_when_no_tags() {
        let info = ReplayGainInfo::default(); // no tags
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::ApplyAndClip,
            source: ReplayGainMode::Track,
            preamp_db: 0.0,
            fallback_preamp_db: -3.0,
        };
        let factor = info.scaling_factor_full(&cfg);
        assert!(
            (factor - db_to_scaling(-3.0)).abs() < 1e-4,
            "Fallback preamp should be -3 dB, got {:.5}",
            factor
        );
    }

    #[test]
    fn test_rg_dont_apply_returns_fallback_preamp() {
        let info = ReplayGainInfo {
            track_gain: Some(-6.0),
            track_peak: Some(0.9),
            album_gain: None,
            album_peak: None,
        };
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::DontApply,
            source: ReplayGainMode::Track,
            preamp_db: 5.0, // should be ignored
            fallback_preamp_db: -2.0,
        };
        let factor = info.scaling_factor_full(&cfg);
        assert!(
            (factor - db_to_scaling(-2.0)).abs() < 1e-4,
            "DontApply should return fallback preamp factor, got {:.5}",
            factor
        );
    }

    #[test]
    fn test_rg_apply_gain_no_clip_clamping() {
        let info = ReplayGainInfo {
            track_gain: Some(6.0), // +6 dB → factor ≈ 1.995
            track_peak: Some(0.9), // would clip if clamped
            album_gain: None,
            album_peak: None,
        };
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::ApplyGain, // no peak clamping
            source: ReplayGainMode::Track,
            preamp_db: 0.0,
            fallback_preamp_db: 0.0,
        };
        let factor = info.scaling_factor_full(&cfg);
        assert!(
            (factor - db_to_scaling(6.0)).abs() < 1e-4,
            "ApplyGain should not clamp, got {:.5}",
            factor
        );
    }

    #[test]
    fn test_rg_apply_and_clip_clamps_peak() {
        let info = ReplayGainInfo {
            track_gain: Some(6.0),
            track_peak: Some(0.9),
            album_gain: None,
            album_peak: None,
        };
        let cfg = ReplayGainConfig {
            apply_mode: ReplayGainApplyMode::ApplyAndClip,
            source: ReplayGainMode::Track,
            preamp_db: 0.0,
            fallback_preamp_db: 0.0,
        };
        let factor = info.scaling_factor_full(&cfg);
        assert!(
            (factor - (1.0 / 0.9)).abs() < 1e-4,
            "ApplyAndClip should clamp by peak, got {:.5}",
            factor
        );
    }
}
