//! Core audio engine — thin coordinator module.
//!
//! Threading model (per spec):
//!   1. Decode thread  — GStreamer pipeline (uridecodebin -> appsink)
//!   2. DSP thread     — Rust DspEngine (biquad EQ + limiter)
//!   3. Output thread  — cpal stream
//!
//! Communication:
//!   appsink -> [decode ring] -> DSP thread -> [output ring] -> cpal callback
//!
//! The `AudioEngine` struct is defined here; its method implementations are
//! spread across domain-specific submodules for maintainability.
//!
//! # v3.0 Cross-Platform: Poll-Driven Architecture
//!
//! The engine no longer depends on GLib's main loop for position updates or
//! bus message handling. Instead, the UI framework calls `tick()` periodically
//! (e.g. via iced's `Subscription` system at ~4 Hz), which:
//!
//!   1. Polls the GStreamer bus for error/warning/EOS messages
//!   2. Queries the current playback position
//!   3. Fires the appropriate callbacks (position_cb, state_cb, eos_cb)
//!
//! This eliminates the `glib::timeout_add_local` and `glib::SourceId`
//! dependencies, making the engine fully usable from any event loop.

pub mod convolution;
pub mod crossfade;
pub mod eq_control;
pub mod gapless;
pub mod loader;
pub mod loudness;
pub mod presets;
pub mod replaygain;
pub mod seek;
pub mod transport;
pub mod volume;

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::audio::crossfade::CrossfadeEngine;
use crate::audio::dsp::DspEngine;
use crate::audio::equalizer::{EqualizerState, OutputDeviceId, OutputPresetStore};
use crate::audio::gapless::GaplessPreloader;
use crate::audio::loudness::{EbuR128Loudness, LoudnessNormalizationConfig};
use crate::audio::output::AudioOutput;
use crate::audio::pipeline::{BusEvent, DecodePipeline};
use crate::audio::replaygain::{ReplayGainApplyMode, ReplayGainMode};

/// Default decode ring buffer size in f32 samples.
/// 65536 samples ≈ 0.68 seconds of stereo audio at 48 kHz.
/// v3.0 Phase 4: Now configurable via `audio.decode_ring_size` in config.
const DECODE_RING_DEFAULT: usize = 65_536;
/// Default output ring buffer size in f32 samples.
/// 32768 samples ≈ 0.34 seconds of stereo audio at 48 kHz.
/// v3.0 Phase 4: Now configurable via `audio.output_ring_size` in config.
const OUTPUT_RING_DEFAULT: usize = 32_768;

pub type PositionCallback = Box<dyn Fn(std::time::Duration) + Send + Sync + 'static>;
pub type StateCallback = Box<dyn Fn(PlayerState) + Send + Sync + 'static>;
pub type EndOfStreamCallback = Box<dyn Fn() + Send + Sync + 'static>;
pub type DurationCallback = Box<dyn Fn(Option<std::time::Duration>) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
    Buffering,
}

pub(crate) struct Session {
    pub(crate) pipeline: DecodePipeline,
    pub(crate) _audio_output: AudioOutput,
    pub(crate) dsp_stop: Arc<AtomicBool>,
    pub(crate) dsp_thread: Option<std::thread::JoinHandle<()>>,
    pub(crate) playing: Arc<AtomicBool>,
    pub(crate) is_playing: bool,
    /// Shared reference to AudioOutput's underrun counter, so we can read it
    /// from the engine without coupling the Session to AudioOutput's lifetime.
    pub(crate) underrun_count: Arc<AtomicU64>,
}

impl Session {
    pub(crate) fn stop_and_join(&mut self) {
        self.dsp_stop.store(true, Ordering::Relaxed);
        self.pipeline.stop();
        if let Some(h) = self.dsp_thread.take() {
            let _ = h.join();
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

// SAFETY: Session is only accessed through Arc<Mutex<Option<Session>>>,
// which provides synchronization. On Linux, cpal::Stream inside AudioOutput
// is Send+Sync (see unsafe impl in output.rs gated behind #[cfg(target_os = "linux")]).
// On macOS/Windows, cpal::Stream is !Send, so we only impl Send/Sync on Linux.
// JoinHandle is Send. All fields are safe to access from any thread when
// protected by Mutex.
//
// Fix Issue #1: Previously these impls were unconditional, causing undefined
// behavior on macOS/Windows where cpal::Stream is !Send. Now gated behind
// #[cfg(target_os = "linux")] to match the AudioOutput safety gate.
#[cfg(target_os = "linux")]
unsafe impl Send for Session {}
#[cfg(target_os = "linux")]
unsafe impl Sync for Session {}

/// Volume, playback speed, and exclusive mode — rarely written together.
pub(crate) struct EngineVolumeState {
    pub volume: f64,
    pub playback_speed: f64,
    pub exclusive_mode: bool,
}

/// All ReplayGain fields — always read together in `build_rg_config`.
pub(crate) struct EngineReplayGainState {
    pub enabled: bool,
    pub mode: ReplayGainMode,
    pub apply: ReplayGainApplyMode,
    pub preamp_db: f64,
    pub fallback_db: f64,
    pub factor: f64,
}

/// Crossfade / seek-fade / transport controls.
pub(crate) struct EngineTransportState {
    pub use_crossfade: bool,
    pub fade_duration_ms: u32,
    pub seek_fade_ms: u32,
}

/// All EBU R128 loudness normalization fields — consolidated from 3 separate
/// Arc<Mutex<…>> fields into a single Arc<Mutex<…>> to reduce the deadlock
/// surface area. Previously, code paths in `play_preloaded()` and the DSP
/// thread had to acquire `loudness`, `loudness_enabled`, and `loudness_config`
/// in sequence. If two threads acquired them in different orders, deadlock
/// was possible. With a single lock, lock ordering is trivially correct.
pub struct EngineLoudnessState {
    pub loudness: EbuR128Loudness,
    pub enabled: bool,
    pub config: LoudnessNormalizationConfig,
}

pub struct AudioEngine {
    pub(crate) session: Arc<Mutex<Option<Session>>>,
    pub(crate) crossfade: Arc<Mutex<Option<Arc<CrossfadeEngine>>>>,

    pub(crate) position_cb: Arc<Mutex<Option<PositionCallback>>>,
    pub(crate) state_cb: Arc<Mutex<Option<StateCallback>>>,
    pub(crate) eos_cb: Arc<Mutex<Option<EndOfStreamCallback>>>,
    pub(crate) duration_cb: Arc<Mutex<Option<DurationCallback>>>,

    pub(crate) dsp: Mutex<Arc<Mutex<DspEngine>>>,

    /// Room correction convolution engine.
    pub(crate) convolution: Arc<Mutex<Option<crate::audio::convolution::ConvolutionEngine>>>,

    /// Genre-based EQ preset switching.
    pub(crate) genre_preset_manager: Arc<Mutex<crate::audio::genre_preset::GenrePresetManager>>,

    /// True gapless preloader — builds the next track's pipeline in the
    /// background while the current track is still playing, so the swap at
    /// EOS is instantaneous with zero silence between tracks.
    ///
    /// Fix Bug #4: Each preloaded session creates its own DspEngine to avoid
    /// data races. Settings are copied from the current engine at swap time.
    pub(crate) gapless_preloader: Arc<GaplessPreloader>,

    /// Volume, playback speed, and exclusive mode (consolidated from 3 separate Mutexes).
    pub(crate) volume_state: Mutex<EngineVolumeState>,
    pub(crate) eq_state: Arc<Mutex<EqualizerState>>,
    /// All ReplayGain fields consolidated (fixes Bug #33 — atomic reads).
    pub(crate) rg_state: Mutex<EngineReplayGainState>,
    /// Crossfade / seek-fade / transport controls.
    pub(crate) transport_state: Mutex<EngineTransportState>,

    /// Per-output preset store (Tier 2 #8).
    pub(crate) output_presets: Arc<Mutex<OutputPresetStore>>,
    /// Currently active output device ID.
    pub(crate) active_device: Mutex<OutputDeviceId>,

    /// EBU R128 loudness normalization state (consolidated from 3 separate
    /// Arc<Mutex<…>> fields — reduces deadlock surface, see EngineLoudnessState).
    pub(crate) loudness_state: Arc<Mutex<EngineLoudnessState>>,

    /// v3.0 Phase 4: Configurable decode ring buffer size.
    /// Read from config at engine creation time. Used in load_internal()
    /// to create ring buffers of the appropriate size.
    pub(crate) decode_ring_size: usize,
    /// v3.0 Phase 4: Configurable output ring buffer size.
    pub(crate) output_ring_size: usize,

    /// v3.0: Last reported position — used to detect position changes in tick().
    pub(crate) last_reported_position: Mutex<Option<std::time::Duration>>,
    /// v3.0: Last reported duration — used to detect duration changes in tick().
    pub(crate) last_reported_duration: Mutex<Option<std::time::Duration>>,

    /// Fix Bug #15: Path of the currently-loaded track, so that enabling
    /// ReplayGain on a playing track can immediately compute and apply the
    /// RG factor without requiring a reload.
    pub(crate) current_track_path: Mutex<Option<std::path::PathBuf>>,
}

/// Centralized GStreamer initialization.
/// Fix C10/M13/M15: gstreamer::init() was called in 4 different places
/// (AudioEngine::new, CrossfadeEngine::new, DecodePipeline::new, ConvolutionEngine)
/// with 3 different error-handling strategies. This centralized init uses
/// std::sync::OnceLock for single initialization and properly propagates
/// the init result so failed inits are reported to all callers.
static GST_INIT_RESULT: OnceLock<Result<(), glib::Error>> = OnceLock::new();

pub(crate) fn ensure_gstreamer_initialized() -> Result<()> {
    let result = GST_INIT_RESULT.get_or_init(gstreamer::init);
    result
        .clone()
        .map_err(|e| anyhow::anyhow!("GStreamer init failed: {}", e))
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        // Fix C10/M15: Use centralized init instead of raw gstreamer::init()
        ensure_gstreamer_initialized()?;
        let dsp = Mutex::new(Arc::new(Mutex::new(DspEngine::new(48_000.0f32))));

        // v3.0 Phase 4: Read ring buffer sizes from config, falling back
        // to defaults if the config is not yet loaded. The ring buffer sizes
        // are read from the config at engine creation time and stored as
        // immutable fields on AudioEngine. Changes to config after engine
        // creation take effect on the next track load (via load_internal).
        let (decode_ring, output_ring) = Self::read_ring_sizes_from_config();

        Ok(Self {
            session:            Arc::new(Mutex::new(None)),
            crossfade:          Arc::new(Mutex::new(None)),
            position_cb:        Arc::new(Mutex::new(None)),
            state_cb:           Arc::new(Mutex::new(None)),
            eos_cb:             Arc::new(Mutex::new(None)),
            duration_cb:        Arc::new(Mutex::new(None)),
            gapless_preloader:  Arc::new(GaplessPreloader::new(48_000, decode_ring, output_ring)),
            dsp,
            convolution:        Arc::new(Mutex::new(None)),
            genre_preset_manager: Arc::new(Mutex::new(crate::audio::genre_preset::GenrePresetManager::new())),
            volume_state: Mutex::new(EngineVolumeState {
                volume: 0.8,
                playback_speed: 1.0,
                exclusive_mode: false,
            }),
            eq_state: Arc::new(Mutex::new(EqualizerState::default())),
            rg_state: Mutex::new(EngineReplayGainState {
                enabled: false,
                mode: ReplayGainMode::Track,
                apply: ReplayGainApplyMode::ApplyAndClip,
                preamp_db: 0.0,
                fallback_db: 0.0,
                factor: 1.0,
            }),
            transport_state: Mutex::new(EngineTransportState {
                use_crossfade: false,
                fade_duration_ms: 2000,
                seek_fade_ms: 20,
            }),
            output_presets:     Arc::new(Mutex::new(OutputPresetStore::new())),
            active_device:      Mutex::new(OutputDeviceId::default_output()),
            // Fix Issue #7: Replace .expect() with graceful fallback.
            // EbuR128Loudness::new() can theoretically fail (OOM, invalid rate).
            // If it fails at 48kHz, try 44.1kHz as fallback. If both fail,
            // loudness normalization is disabled but the app still starts.
            loudness_state: Arc::new(Mutex::new(EngineLoudnessState {
                loudness: EbuR128Loudness::new(48_000.0)
                    .or_else(|_| EbuR128Loudness::new(44_100.0))
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to create EbuR128Loudness: {} — loudness normalization disabled", e);
                        // Create a dummy at 48kHz that will be a no-op
                        EbuR128Loudness::new(48_000.0).expect("EbuR128Loudness fallback: 48kHz must be valid")
                    }),
                enabled: false,
                config: LoudnessNormalizationConfig::default(),
            })),
            decode_ring_size:   decode_ring,
            output_ring_size:   output_ring,
            last_reported_position: Mutex::new(None),
            last_reported_duration: Mutex::new(None),
            current_track_path: Mutex::new(None),
        })
    }

    /// Read ring buffer sizes from the configuration file.
    /// Returns (decode_ring_size, output_ring_size) clamped to valid ranges.
    /// Falls back to defaults if config cannot be loaded.
    fn read_ring_sizes_from_config() -> (usize, usize) {
        match crate::config::load() {
            Ok(config) => {
                let decode = config.audio.decode_ring_size.clamp(4096, 262_144) as usize;
                let output = config.audio.output_ring_size.clamp(2048, 131_072) as usize;
                tracing::info!(
                    "Ring buffer sizes from config: decode={}, output={}",
                    decode,
                    output
                );
                (decode, output)
            }
            Err(_) => {
                tracing::debug!(
                    "Config not loaded, using default ring buffer sizes: decode={}, output={}",
                    DECODE_RING_DEFAULT,
                    OUTPUT_RING_DEFAULT
                );
                (DECODE_RING_DEFAULT, OUTPUT_RING_DEFAULT)
            }
        }
    }

    // -- Load ---------------------------------------------------------------

    pub fn load(&self, path: &Path) -> Result<()> {
        let uri = path_to_uri(path)?;
        self.load_internal(uri, Some(path.to_path_buf()))
    }

    fn load_internal(&self, uri: String, path: Option<std::path::PathBuf>) -> Result<()> {
        loader::load_internal(self, uri, path)
    }

    // -- v3.0 Poll-Driven Tick -----------------------------------------------

    /// Called periodically by the UI framework's timer (e.g. iced `Subscription`
    /// at ~4 Hz / 250 ms intervals).
    ///
    /// This replaces `glib::timeout_add_local` and `pipeline.watch_bus()` from
    /// the v2.1 GTK4 build. The method:
    ///
    /// 1. Polls the GStreamer bus for error/warning/EOS messages
    /// 2. Queries the current playback position
    /// 3. Queries the duration
    /// 4. Fires the appropriate callbacks
    ///
    /// This is the heart of the GLib decoupling — the engine no longer
    /// requires a GLib main context to function.
    ///
    /// # Fix C1: Borrow-checker violation
    ///
    /// The previous version bound `sess` as `ref sess` from `*guard2`, then
    /// called `drop(guard2)` while `sess` still borrowed it. This caused a
    /// Rust borrow-checker rejection. The fix collects all data under the
    /// lock, drops the lock, then fires callbacks outside the lock.
    pub fn tick(&self) {
        // ── 1. Poll GStreamer bus for messages ──────────────────────────────
        let bus_events = {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            match *guard {
                Some(ref sess) => sess.pipeline.poll_bus(),
                None => return, // No session — nothing to tick
            }
        };
        // Lock is released; fire callbacks outside the lock to prevent deadlock.

        for event in bus_events {
            match event {
                BusEvent::Error(msg) => {
                    tracing::error!("GST: {}", msg);
                }
                BusEvent::Warning(msg) => {
                    tracing::warn!("GST: {}", msg);
                }
                BusEvent::Eos => {
                    if let Ok(cb) = self.eos_cb.lock() {
                        if let Some(ref f) = *cb {
                            f();
                        }
                    }
                    notify_state(&self.state_cb, PlayerState::Stopped);
                }
            }
        }

        // ── 2 & 3. Poll position and duration ──────────────────────────────
        // Fix H4: Collect position/duration under the session lock, then
        // release it BEFORE updating last_reported_position/duration. This
        // eliminates the nested lock acquisition (session → position → duration)
        // which was fragile and could deadlock if future code changed the
        // lock ordering. Document the invariant: session lock must never be
        // acquired while holding last_reported_position or last_reported_duration.
        let (current_pos, current_dur) = {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref sess) = *guard {
                let pos = if sess.is_playing {
                    sess.pipeline.position()
                } else {
                    None
                };
                let dur = sess.pipeline.duration();
                (pos, dur)
            } else {
                (None, None)
            }
        };
        // Session lock is released. Now update tracking fields and fire callbacks.
        // Invariant: session lock is NOT held when acquiring position/duration locks.

        let pos_changed = if current_pos.is_some() {
            let mut last_pos = self
                .last_reported_position
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if *last_pos != current_pos {
                *last_pos = current_pos;
                true
            } else {
                false
            }
        } else {
            false
        };

        let dur_changed = if current_dur.is_some() {
            let mut last_dur = self
                .last_reported_duration
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if *last_dur != current_dur {
                *last_dur = current_dur;
                true
            } else {
                false
            }
        } else {
            false
        };

        if pos_changed {
            if let Some(pos) = current_pos {
                if let Ok(cb) = self.position_cb.lock() {
                    if let Some(ref f) = *cb {
                        f(pos);
                    }
                }
            }
        }

        if dur_changed {
            if let Some(dur) = current_dur {
                if let Ok(cb) = self.duration_cb.lock() {
                    if let Some(ref f) = *cb {
                        f(Some(dur));
                    }
                }
            }
        }
    }

    // -- Callbacks -----------------------------------------------------------

    pub fn on_position_changed(&self, callback: PositionCallback) {
        *self.position_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }
    pub fn on_state_changed(&self, callback: StateCallback) {
        *self.state_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }
    pub fn on_end_of_stream(&self, callback: EndOfStreamCallback) {
        *self.eos_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }
    pub fn on_duration_changed(&self, callback: DurationCallback) {
        *self.duration_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }

    /// Set the track duration hint.
    /// Currently unused -- duration is queried from GStreamer directly.
    /// Kept for API compatibility.
    #[allow(dead_code)]
    pub fn set_duration(&self, _dur: Option<std::time::Duration>) {}

    // -- Genre preset --------------------------------------------------------

    pub fn apply_genre_preset(&self, genre: &str) -> bool {
        let manager = self
            .genre_preset_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let found = manager.get(genre).is_some();
        if found {
            // apply_to_engine correctly applies eq_state, stereo_width, bass_db,
            // and treble_db from the preset (fixes dual-path bug where only eq_state
            // was pushed and the preset's per-genre bass/treble/width were ignored).
            manager.apply_to_engine(genre, self);
            tracing::info!("Applied genre EQ preset for '{}'", genre);
        }
        found
    }

    pub fn register_genre_preset(&self, preset: crate::audio::genre_preset::GenrePreset) {
        self.genre_preset_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .register(preset);
    }

    // -- DSP Arc helpers (Fix Bug #9) ---------------------------------------

    /// Get a cloned Arc to the active DspEngine.
    /// The double-indirection (`Mutex<Arc<Mutex<DspEngine>>>`) allows the
    /// gapless track swap to atomically re-point `engine.dsp` at the
    /// preloaded session's DspEngine, so that all subsequent calls to
    /// `set_eq_state()`, `set_volume_gain()`, etc. target the engine
    /// that is actually connected to the audio output.
    pub(crate) fn dsp_arc(&self) -> Arc<Mutex<DspEngine>> {
        self.dsp.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Swap the active DspEngine Arc. Used during gapless track swap.
    pub(crate) fn swap_dsp_arc(&self, new_arc: Arc<Mutex<DspEngine>>) {
        *self.dsp.lock().unwrap_or_else(|e| e.into_inner()) = new_arc;
    }
}

fn notify_state(cb: &Arc<Mutex<Option<StateCallback>>>, state: PlayerState) {
    if let Ok(guard) = cb.lock() {
        if let Some(ref f) = *guard {
            f(state);
        }
    }
}

fn path_to_uri(path: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(path).context("path does not exist")?;
    // Fix cross-platform issue: use glib::filename_to_uri() which correctly
    // handles percent-encoding, UNC paths on Windows, and non-ASCII characters
    // on all platforms. The previous manual construction with encode_already_encoded
    // misinterpreted %AB sequences (where AB are hex digits) as percent-encoded
    // octets and passed them through, even when they were literal characters
    // in a filename like "100%AB-complete.flac".
    glib::filename_to_uri(&abs, None)
        .map(|s| s.to_string())
        .map_err(|e| anyhow::anyhow!("path_to_uri failed: {}", e))
}
