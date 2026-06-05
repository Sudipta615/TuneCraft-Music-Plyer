//! EQ service — equalizer state and DSP pipeline parameters
//!
//! Encapsulates all EQ-related state and operations, separating
//! EQ concerns from the main playback and UI logic.
//!
//! EngineCommand channel instead of requiring engine_mutex access.
//! This eliminates ~85% of engine mutex contention.

/// EQ state managed by the service.
#[derive(Debug, Clone)]
pub struct EqState {
    /// Whether the EQ panel is visible in the UI
    pub show_panel: bool,
    /// Whether EQ is enabled
    pub enabled: bool,
    /// Gain for each of the 10 EQ bands (dB)
    pub bands: [f64; 10],
    /// Current EQ preset name
    pub preset: String,
    /// Preamp gain (dB)
    pub preamp: f64,
    /// Bass shelf gain (dB)
    pub bass_shelf: f64,
    /// Treble shelf gain (dB)
    pub treble_shelf: f64,
    /// Stereo width (0.0 to 2.0, 1.0 = normal)
    ///
    /// Previously stored as percentage (100.0) in some places and ratio
    /// (1.0) in others, causing a 50x amplification bug. Now consistently
    /// a ratio: 0.0 = mono, 1.0 = normal, 2.0 = extra wide.
    pub stereo_width: f64,
    /// Stereo balance (-1.0 left to 1.0 right)
    pub balance: f64,
    /// Whether dither is enabled
    pub dither: bool,
    /// Whether Mid/Side EQ mode is enabled
    pub midside: bool,
    /// Cached dither enabled state (for UI comparison)
    pub cached_dither_enabled: bool,
    /// Cached mid/side enabled state (for UI comparison)
    pub cached_midside_enabled: bool,
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            show_panel: false,
            enabled: false,
            bands: [0.0; 10],
            preset: "Custom".to_string(),
            preamp: 0.0,
            bass_shelf: 0.0,
            treble_shelf: 0.0,
            stereo_width: 1.0,
            balance: 0.0,
            dither: true,
            midside: false,
            cached_dither_enabled: true,
            cached_midside_enabled: false,
        }
    }
}

/// Standard EQ band frequencies.
pub const EQ_FREQUENCIES: [f64; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Default Q factor for EQ bands.
pub const DEFAULT_Q: f64 = 1.4;

/// The EQ service manages equalizer state and applies changes to the DSP pipeline
/// via lock-free EngineCommand channel.
///
/// Thread safety follows PlaybackService's threading model. This prevents runtime panics if the
/// service is accessed from multiple threads through its Arc wrapper.
pub struct EqService {
    state: std::sync::RwLock<EqState>,
    #[cfg(feature = "audio-output")]
    engine_cmd_tx: Option<crossbeam::channel::Sender<tc_engine::buffer::EngineCommand>>,
}

impl EqService {
    /// Create a new EQ service with initial state from config.
    #[cfg(feature = "audio-output")]
    pub fn new(
        engine_cmd_tx: crossbeam::channel::Sender<tc_engine::buffer::EngineCommand>,
        enabled: bool,
        preamp: f64,
        dither: bool,
        bands: [f64; 10],
    ) -> Self {
        let state = EqState {
            enabled,
            preamp,
            dither,
            bands,
            cached_dither_enabled: dither,
            ..EqState::default()
        };

        let service = Self {
            state: std::sync::RwLock::new(state),
            engine_cmd_tx: Some(engine_cmd_tx),
        };

        // Apply initial state to engine via lock-free channel
        service.apply_eq_enabled(enabled);
        service.apply_preamp(preamp);
        for (i, &gain) in bands.iter().enumerate() {
            service.apply_eq_band(
                i,
                EQ_FREQUENCIES[i],
                gain,
                DEFAULT_Q,
                enabled && gain != 0.0,
            );
        }

        service
    }

    #[cfg(not(feature = "audio-output"))]
    pub fn new(enabled: bool, preamp: f64, dither: bool, bands: [f64; 10]) -> Self {
        let state = EqState {
            enabled,
            preamp,
            dither,
            bands,
            cached_dither_enabled: dither,
            ..EqState::default()
        };

        Self {
            state: std::sync::RwLock::new(state),
        }
    }

    /// Get a clone of the EQ state (snapshot for UI reads).
    /// Using a clone avoids holding a lock across the UI frame.
    pub fn state_snapshot(&self) -> EqState {
        self.state.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get a reference to the EQ state.
    pub fn state(&self) -> std::sync::RwLockReadGuard<'_, EqState> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Get a mutable reference to the EQ state.
    pub fn state_mut(&self) -> std::sync::RwLockWriteGuard<'_, EqState> {
        self.state.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Toggle the EQ panel visibility.
    pub fn toggle_panel(&self) {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        state.show_panel = !state.show_panel;
    }

    /// Set EQ enabled state.
    pub fn set_enabled(&self, enabled: bool) {
        self.apply_eq_enabled(enabled);
        self.state
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .enabled = enabled;
    }

    /// Set a specific EQ band gain.
    pub fn set_band(&self, index: usize, gain_db: f64) {
        if index >= 10 {
            return;
        }
        let enabled = self.state.read().unwrap_or_else(|e| e.into_inner()).enabled;

        self.apply_eq_band(
            index,
            EQ_FREQUENCIES[index],
            gain_db,
            DEFAULT_Q,
            enabled && gain_db != 0.0,
        );
        self.state.write().unwrap_or_else(|e| e.into_inner()).bands[index] = gain_db;
    }

    /// Set a specific EQ band with full parameters (frequency, gain, Q, enabled).
    pub fn set_band_with_params(
        &self,
        index: usize,
        frequency: f64,
        gain_db: f64,
        q: f64,
        enabled: bool,
    ) {
        if index >= 10 {
            return;
        }
        let eq_enabled = self.state.read().unwrap_or_else(|e| e.into_inner()).enabled;
        self.apply_eq_band(index, frequency, gain_db, q, eq_enabled && enabled);
        self.state.write().unwrap_or_else(|e| e.into_inner()).bands[index] = gain_db;
    }

    /// Set preamp gain.
    pub fn set_preamp(&self, db: f64) {
        self.apply_preamp(db);
        self.state.write().unwrap_or_else(|e| e.into_inner()).preamp = db;
    }

    /// Set stereo width.
    pub fn set_stereo_width(&self, width: f64) {
        let clamped = width.clamp(0.0, 2.0);
        self.apply_stereo_width(clamped);
        self.state
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .stereo_width = clamped;
    }

    /// Set balance.
    pub fn set_balance(&self, balance: f64) {
        self.apply_balance(balance);
        self.state
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .balance = balance;
    }

    /// Set dither enabled.
    pub fn set_dither(&self, enabled: bool) {
        self.apply_dither(enabled);
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        state.dither = enabled;
        state.cached_dither_enabled = enabled;
    }

    /// Set Mid/Side EQ mode.
    pub fn set_midside(&self, enabled: bool) {
        self.apply_midside(enabled);
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        state.midside = enabled;
        state.cached_midside_enabled = enabled;
    }

    /// Apply EQ enabled state to the engine via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_eq_enabled(&self, enabled: bool) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetEqEnabled(enabled));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_eq_enabled(&self, _enabled: bool) {}

    /// Apply an EQ band change to the engine via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_eq_band(&self, index: usize, frequency: f64, gain_db: f64, q: f64, enabled: bool) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetEqBand {
                index,
                frequency,
                gain_db,
                q,
                enabled,
            });
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_eq_band(
        &self,
        _index: usize,
        _frequency: f64,
        _gain_db: f64,
        _q: f64,
        _enabled: bool,
    ) {
    }

    /// Apply preamp change via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_preamp(&self, db: f64) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetPreamp(db));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_preamp(&self, _db: f64) {}

    /// Apply stereo width change via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_stereo_width(&self, width: f64) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetStereoWidth(width));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_stereo_width(&self, _width: f64) {}

    /// Apply balance change via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_balance(&self, balance: f64) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetBalance(balance));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_balance(&self, _balance: f64) {}

    /// Apply dither change via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_dither(&self, enabled: bool) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetDitherEnabled(enabled));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_dither(&self, _enabled: bool) {}

    /// Apply mid/side EQ change via lock-free channel.
    #[cfg(feature = "audio-output")]
    fn apply_midside(&self, enabled: bool) {
        if let Some(ref tx) = self.engine_cmd_tx {
            let _ = tx.send(tc_engine::buffer::EngineCommand::SetMidsideEq(enabled));
        }
    }

    #[cfg(not(feature = "audio-output"))]
    fn apply_midside(&self, _enabled: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create EqService without audio engine.
    fn make_service() -> EqService {
        EqService::new(false, 0.0, true, [0.0; 10])
    }

    #[test]
    fn test_eq_state_default() {
        let state = EqState::default();
        assert!(!state.enabled);
        assert_eq!(state.bands, [0.0; 10]);
        assert_eq!(state.preset, "Custom");
        assert!((state.preamp - 0.0).abs() < 1e-12);
        assert!((state.stereo_width - 1.0).abs() < 1e-12);
        assert!((state.balance - 0.0).abs() < 1e-12);
        assert!(state.dither);
        assert!(!state.midside);
    }

    #[test]
    fn test_eq_frequencies() {
        assert!((EQ_FREQUENCIES[0] - 31.25).abs() < 1e-12);
        assert!((EQ_FREQUENCIES[9] - 16000.0).abs() < 1e-12);
    }

    #[test]
    fn test_service_initial_state() {
        let svc = make_service();
        let state = svc.state_snapshot();
        assert!(!state.enabled);
        assert_eq!(state.bands, [0.0; 10]);
        assert!(state.dither);
    }

    #[test]
    fn test_set_enabled() {
        let svc = make_service();
        svc.set_enabled(true);
        assert!(svc.state_snapshot().enabled);
        svc.set_enabled(false);
        assert!(!svc.state_snapshot().enabled);
    }

    #[test]
    fn test_set_band() {
        let svc = make_service();
        svc.set_band(0, 3.0);
        assert!((svc.state_snapshot().bands[0] - 3.0).abs() < 1e-12);
        // Other bands unchanged
        assert!((svc.state_snapshot().bands[1] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_band_out_of_range_ignored() {
        let svc = make_service();
        svc.set_band(15, 5.0); // index >= 10, should be no-op
        assert_eq!(svc.state_snapshot().bands, [0.0; 10]);
    }

    #[test]
    fn test_set_preamp() {
        let svc = make_service();
        svc.set_preamp(-3.0);
        assert!((svc.state_snapshot().preamp - (-3.0)).abs() < 1e-12);
    }

    #[test]
    fn test_set_stereo_width_clamped() {
        let svc = make_service();
        svc.set_stereo_width(3.0); // clamped to 2.0
        assert!((svc.state_snapshot().stereo_width - 2.0).abs() < 1e-12);
        svc.set_stereo_width(-1.0); // clamped to 0.0
        assert!((svc.state_snapshot().stereo_width - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_balance() {
        let svc = make_service();
        svc.set_balance(-0.5);
        assert!((svc.state_snapshot().balance - (-0.5)).abs() < 1e-12);
    }

    #[test]
    fn test_set_dither() {
        let svc = make_service();
        svc.set_dither(false);
        assert!(!svc.state_snapshot().dither);
        assert!(!svc.state_snapshot().cached_dither_enabled);
    }

    #[test]
    fn test_set_midside() {
        let svc = make_service();
        svc.set_midside(true);
        assert!(svc.state_snapshot().midside);
        assert!(svc.state_snapshot().cached_midside_enabled);
    }

    #[test]
    fn test_toggle_panel() {
        let svc = make_service();
        assert!(!svc.state_snapshot().show_panel);
        svc.toggle_panel();
        assert!(svc.state_snapshot().show_panel);
        svc.toggle_panel();
        assert!(!svc.state_snapshot().show_panel);
    }

    #[test]
    fn test_set_band_with_params() {
        let svc = make_service();
        svc.set_band_with_params(2, 250.0, 4.5, 2.0, true);
        let state = svc.state_snapshot();
        assert!((state.bands[2] - 4.5).abs() < 1e-12);
    }
}
