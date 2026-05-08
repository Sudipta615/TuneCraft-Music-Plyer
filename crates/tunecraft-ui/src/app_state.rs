//! Shared application state for the Dioxus UI.
//!
//! Reuses the same core data structures from the iced version but adapted
//! for Dioxus signals. The core state (engine, db, queue) remains in
//! Arc<Mutex<>> for thread-safe access from background tasks.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use lru::LruCache;

use tunecraft_core::audio::pcm_cache::PcmCache;
use tunecraft_core::audio::{AudioEngine, PlayerState};
use tunecraft_core::Database;

/// Navigation view types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewType {
    AllTracks,
    Albums,
    Artists,
    Playlists,
    Mood(String),
    Search,
    Settings,
    AlbumDetail(String),
    ArtistDetail(String),
    PlaylistDetail(String, Option<i64>),
    Filter {
        genre: String,
        year_from: Option<i32>,
        year_to: Option<i32>,
    },
}

/// Track list layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLayout {
    List,
    Grid,
}

/// Repeat mode for the play queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    None,
    All,
    One,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::None => Self::All,
            Self::All => Self::One,
            Self::One => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::All => "all",
            Self::One => "one",
        }
    }
}

/// EQ preset names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqPreset {
    Flat,
    BassBoost,
    TrebleBoost,
    Vocal,
    Rock,
    Pop,
    Jazz,
    Classical,
    Electronic,
    Custom,
}

impl EqPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::BassBoost => "Bass Boost",
            Self::TrebleBoost => "Treble Boost",
            Self::Vocal => "Vocal",
            Self::Rock => "Rock",
            Self::Pop => "Pop",
            Self::Jazz => "Jazz",
            Self::Classical => "Classical",
            Self::Electronic => "Electronic",
            Self::Custom => "Custom",
        }
    }

    pub fn gains(self) -> [f32; 10] {
        match self {
            Self::Flat => [0.0; 10],
            Self::BassBoost => [8.0, 6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Self::TrebleBoost => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 8.0],
            Self::Vocal => [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0],
            Self::Rock => [4.0, 2.5, 0.0, -1.0, -1.0, 0.0, 2.0, 3.5, 4.0, 4.0],
            Self::Pop => [-1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0, -1.0, 1.0],
            Self::Jazz => [3.0, 2.0, 0.5, 1.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0],
            Self::Classical => [4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0],
            Self::Electronic => [5.0, 4.0, 1.0, 0.0, -1.0, 0.0, 1.0, 3.0, 4.0, 5.0],
            Self::Custom => [0.0; 10],
        }
    }

    pub fn all() -> &'static [EqPreset] {
        &[
            EqPreset::Flat,
            EqPreset::BassBoost,
            EqPreset::TrebleBoost,
            EqPreset::Vocal,
            EqPreset::Rock,
            EqPreset::Pop,
            EqPreset::Jazz,
            EqPreset::Classical,
            EqPreset::Electronic,
            EqPreset::Custom,
        ]
    }
}

/// Sort mode for the track list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Default,
    TitleAsc,
    TitleDesc,
    ArtistAsc,
    ArtistDesc,
    AlbumAsc,
    AlbumDesc,
    DurationAsc,
    DurationDesc,
}

impl SortMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Default => Self::TitleAsc,
            Self::TitleAsc => Self::TitleDesc,
            Self::TitleDesc => Self::ArtistAsc,
            Self::ArtistAsc => Self::ArtistDesc,
            Self::ArtistDesc => Self::AlbumAsc,
            Self::AlbumAsc => Self::AlbumDesc,
            Self::AlbumDesc => Self::DurationAsc,
            Self::DurationAsc => Self::DurationDesc,
            Self::DurationDesc => Self::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::TitleAsc => "Title A-Z",
            Self::TitleDesc => "Title Z-A",
            Self::ArtistAsc => "Artist A-Z",
            Self::ArtistDesc => "Artist Z-A",
            Self::AlbumAsc => "Album A-Z",
            Self::AlbumDesc => "Album Z-A",
            Self::DurationAsc => "Duration Up",
            Self::DurationDesc => "Duration Down",
        }
    }
}

/// Play queue.
pub struct PlayQueue {
    pub tracks: Vec<tunecraft_core::Track>,
    pub current_index: Option<usize>,
    pub shuffle: bool,
    pub repeat_mode: RepeatMode,
    pub shuffle_order: Vec<usize>,
}

pub const MAX_QUEUE_SIZE: usize = 10000;

impl PlayQueue {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current_index: None,
            shuffle: false,
            repeat_mode: RepeatMode::None,
            shuffle_order: Vec::new(),
        }
    }

    /// #2: Toggle shuffle on/off. When disabling, convert the current logical
    /// (shuffled) index back to a physical index *before* clearing the shuffle flag,
    /// so `effective_index()` doesn't misinterpret the result.
    pub fn toggle_shuffle(&mut self) -> bool {
        let new_shuffle = !self.shuffle;
        if new_shuffle {
            self.shuffle = true;
            self.regenerate_shuffle_order_preserving_current();
        } else {
            if let Some(logical) = self.current_index {
                if logical < self.shuffle_order.len() {
                    self.current_index = Some(self.shuffle_order[logical]);
                }
            }
            self.shuffle = false;
            self.shuffle_order.clear();
        }
        self.shuffle
    }

    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat_mode = self.repeat_mode.cycle();
        self.repeat_mode
    }

    pub fn regenerate_shuffle_order(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        let len = self.tracks.len();
        self.shuffle_order = (0..len).collect();
        self.shuffle_order.shuffle(&mut rng);
    }

    pub fn regenerate_shuffle_order_preserving_current(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        let len = self.tracks.len();
        if len == 0 {
            self.shuffle_order.clear();
            return;
        }
        let current_physical = self
            .current_index
            .and_then(|logical| self.effective_index(logical));
        self.shuffle_order = (0..len).collect();
        self.shuffle_order.shuffle(&mut rng);
        if let Some(physical) = current_physical {
            if let Some(logical) = self.current_index {
                if logical < self.shuffle_order.len() {
                    if let Some(pos) = self.shuffle_order.iter().position(|&p| p == physical) {
                        self.shuffle_order.swap(logical, pos);
                    }
                }
            }
        }
    }

    pub fn current_track(&self) -> Option<&tunecraft_core::Track> {
        let idx = self.effective_index(self.current_index?)?;
        self.tracks.get(idx)
    }

    pub fn next_index(&self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let current = self.current_index?;
        match self.repeat_mode {
            RepeatMode::One => Some(current),
            RepeatMode::All => {
                let next = current + 1;
                Some(next % self.tracks.len())
            }
            RepeatMode::None => {
                if current + 1 < self.tracks.len() {
                    Some(current + 1)
                } else {
                    None
                }
            }
        }
    }

    /// Manual next index for user-initiated skips.
    /// Fix: next_index() returns Some(current) for RepeatMode::One, which
    /// is correct for EOS auto-advance but wrong when the user clicks "Next".
    /// This method ignores RepeatMode::One and always advances.
    pub fn manual_next_index(&self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let current = self.current_index?;

        if current + 1 < self.tracks.len() {
            Some(current + 1)
        } else if self.repeat_mode == RepeatMode::All {
            Some(0)
        } else {
            None
        }
    }

    pub fn prev_index(&self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let current = self.current_index?;

        if self.repeat_mode == RepeatMode::One {
            return Some(current);
        }

        if current > 0 {
            Some(current - 1)
        } else if self.repeat_mode == RepeatMode::All {
            Some(self.tracks.len() - 1)
        } else {
            None
        }
    }

    pub fn effective_index(&self, logical: usize) -> Option<usize> {
        if self.shuffle && logical < self.shuffle_order.len() {
            Some(self.shuffle_order[logical])
        } else {
            Some(logical)
        }
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        let was_shuffle = self.shuffle;
        self.shuffle = enabled;
        if enabled && !was_shuffle {
            self.regenerate_shuffle_order_preserving_current();
        } else if !enabled && was_shuffle {
            if let Some(logical) = self.current_index {
                if logical < self.shuffle_order.len() {
                    self.current_index = Some(self.shuffle_order[logical]);
                }
            }
            self.shuffle_order.clear();
        }
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat_mode
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

/// Scrobble state.
pub(crate) struct ScrobbleState {
    pub track_id: Option<i64>,
    pub accumulated_secs: u64,
    pub submitted: bool,
}

/// Notification entry.
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    pub title: String,
    pub body: String,
    pub timestamp: String,
    /// Unique ID to prevent key collisions when multiple tracks with
    /// the same title are played in the same minute.
    /// Fix Bug #10.
    pub unique_id: String,
}

/// Shared mutable application state (thread-safe for background tasks).
pub struct AppState {
    pub engine: Mutex<Option<AudioEngine>>,
    pub db: RwLock<Option<Arc<Database>>>,
    pub queue: Mutex<PlayQueue>,
    pub player_state: Mutex<PlayerState>,
    pub volume: AtomicU64,
    pub current_view: Mutex<ViewType>,
    pub search_query: Mutex<String>,
    pub config: RwLock<tunecraft_core::config::TunecraftConfig>,
    pub cover_art_cache: Mutex<LruCache<String, Option<Vec<u8>>>>,
    pub is_scanning: AtomicBool,
    pub dark_mode: AtomicBool,
    pub sidebar_collapsed: AtomicBool,
    pub queue_visible: AtomicBool,
    pub scrobble: Mutex<ScrobbleState>,
    pub toast_message: Mutex<Option<String>>,
    pub pcm_cache: Arc<PcmCache>,
    pub crossfade: Mutex<Option<Arc<tunecraft_core::audio::crossfade::CrossfadeEngine>>>,
    pub view_layout: Mutex<ViewLayout>,
    pub eq_visible: AtomicBool,
    pub filter_visible: AtomicBool,
    pub notifications_visible: AtomicBool,
    pub notification_count: AtomicU64,
    pub sort_mode: Mutex<SortMode>,
    pub context_menu_target: Mutex<Option<usize>>,
    pub context_menu_position: Mutex<(f64, f64)>,
    pub loved_tracks: Mutex<std::collections::HashSet<String>>,
    pub eq_bands: Mutex<[f32; 10]>,
    pub eq_enabled: AtomicBool,
    pub eq_preset: Mutex<EqPreset>,
    pub eq_bass_db: Mutex<f32>,
    pub eq_treble_db: Mutex<f32>,
    pub eq_stereo_width: Mutex<f32>,
    pub eq_balance: Mutex<f32>,
    pub eq_dither_enabled: AtomicBool,
    pub eq_ms_enabled: AtomicBool,
    pub eq_preamp: Mutex<f32>,
    pub filter_genre: Mutex<String>,
    pub filter_year_range: Mutex<String>,
    pub notifications: Mutex<Vec<NotificationEntry>>,
    pub playback_speed: Mutex<f32>,
    pub volume_muted: AtomicBool,
    pub volume_before_mute: AtomicU64,
    pub eos_flag: AtomicBool,
    pub playback_error: Mutex<Option<String>>,

    pub sidebar_album_count: AtomicU64,
    pub sidebar_artist_count: AtomicU64,
    pub sidebar_playlist_count: AtomicU64,
    pub sidebar_cache_valid: AtomicBool,

    pub sidebar_track_count: AtomicU64,
    pub sidebar_mood_counts: Mutex<std::collections::HashMap<String, u64>>,

    pub engine_ready: AtomicBool,
    pub db_ready: AtomicBool,

    pub cached_tracks: Mutex<Option<(String, Vec<tunecraft_core::Track>)>>,
}

impl AppState {
    pub fn new() -> Self {
        let loaded_config = tunecraft_core::config::load().unwrap_or_default();
        let is_dark = loaded_config.general.theme != "light";
        let speed = loaded_config.general.playback_speed as f32;
        let initial_volume_bits = loaded_config.general.volume.to_bits();
        let initial_muted = loaded_config.general.volume_muted;
        let initial_mute_restore = if loaded_config.general.volume_before_mute > 0.0 {
            loaded_config.general.volume_before_mute.to_bits()
        } else if loaded_config.general.volume > 0.0 {
            initial_volume_bits
        } else {
            0.2f64.to_bits() // Safe fallback: 20% volume instead of 100%
        };

        Self {
            engine: Mutex::new(None),
            db: RwLock::new(None),
            queue: Mutex::new(PlayQueue::new()),
            player_state: Mutex::new(PlayerState::Stopped),
            volume: AtomicU64::new(initial_volume_bits),
            current_view: Mutex::new(ViewType::AllTracks),
            search_query: Mutex::new(String::new()),
            config: RwLock::new(loaded_config),
            cover_art_cache: Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())),
            is_scanning: AtomicBool::new(false),
            dark_mode: AtomicBool::new(is_dark),
            sidebar_collapsed: AtomicBool::new(false),
            queue_visible: AtomicBool::new(false),
            scrobble: Mutex::new(ScrobbleState {
                track_id: None,
                accumulated_secs: 0,
                submitted: false,
            }),
            toast_message: Mutex::new(None),
            pcm_cache: Arc::new(PcmCache::with_default_capacity()),
            crossfade: Mutex::new(None),
            view_layout: Mutex::new(ViewLayout::List),
            eq_visible: AtomicBool::new(false),
            filter_visible: AtomicBool::new(false),
            notifications_visible: AtomicBool::new(false),
            notification_count: AtomicU64::new(0),
            sort_mode: Mutex::new(SortMode::Default),
            context_menu_target: Mutex::new(None),
            context_menu_position: Mutex::new((0.0, 0.0)),
            loved_tracks: Mutex::new(std::collections::HashSet::new()),
            eq_bands: Mutex::new([0.0; 10]),
            eq_enabled: AtomicBool::new(false),
            eq_preset: Mutex::new(EqPreset::Flat),
            eq_bass_db: Mutex::new(0.0),
            eq_treble_db: Mutex::new(0.0),
            eq_stereo_width: Mutex::new(1.0),
            eq_balance: Mutex::new(0.0),
            eq_dither_enabled: AtomicBool::new(false),
            eq_ms_enabled: AtomicBool::new(false),
            eq_preamp: Mutex::new(0.0),
            filter_genre: Mutex::new(String::new()),
            filter_year_range: Mutex::new(String::new()),
            notifications: Mutex::new(Vec::new()),
            playback_speed: Mutex::new(speed),
            volume_muted: AtomicBool::new(initial_muted),
            volume_before_mute: AtomicU64::new(initial_mute_restore),
            eos_flag: AtomicBool::new(false),
            playback_error: Mutex::new(None),
            sidebar_album_count: AtomicU64::new(0),
            sidebar_artist_count: AtomicU64::new(0),
            sidebar_playlist_count: AtomicU64::new(0),
            sidebar_cache_valid: AtomicBool::new(false),
            sidebar_track_count: AtomicU64::new(0),
            sidebar_mood_counts: Mutex::new(std::collections::HashMap::new()),
            engine_ready: AtomicBool::new(false),
            db_ready: AtomicBool::new(false),
            cached_tracks: Mutex::new(None),
        }
    }

    pub fn init_engine(&self) -> Result<(), String> {
        let engine = AudioEngine::new().map_err(|e| e.to_string())?;

        let vol = if self.volume_muted.load(Ordering::Relaxed) {
            0.0
        } else {
            self.volume()
        };
        let _ = engine.set_volume(vol);

        let speed = *self
            .playback_speed
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _ = engine.set_playback_speed(speed as f64);

        *self.engine.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);

        self.push_eq_to_engine();

        tracing::info!("Audio engine initialized (state synced)");
        Ok(())
    }

    pub fn open_database(&self) -> Result<(), String> {
        let path = Database::default_path().map_err(|e| e.to_string())?;
        let db = Database::open(&path).map_err(|e| e.to_string())?;
        *self.db.write().unwrap_or_else(|e| {
            tracing::warn!("db rwlock poisoned (write), recovering: {}", e);
            e.into_inner()
        }) = Some(Arc::new(db));
        tracing::info!("Database opened at {:?}", path);
        Ok(())
    }

    pub fn volume(&self) -> f64 {
        f64::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, vol: f64) {
        let vol = vol.clamp(0.0, 1.0);
        self.volume.store(vol.to_bits(), Ordering::Relaxed);
        if let Ok(engine) = self.engine.lock() {
            if let Some(ref e) = *engine {
                let _ = e.set_volume(vol);
            }
        }
    }

    pub fn position(&self) -> Option<std::time::Duration> {
        self.engine
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().and_then(|e| e.position()))
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        self.engine
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().and_then(|e| e.duration()))
    }

    pub fn is_playing(&self) -> bool {
        matches!(
            *self.player_state.lock().unwrap_or_else(|e| e.into_inner()),
            PlayerState::Playing
        )
    }

    pub fn play_track_from_view(&self, index: usize) {
        self.eos_flag
            .store(false, std::sync::atomic::Ordering::Release);

        let tracks = self.load_tracks_for_view();
        if index >= tracks.len() {
            return;
        }
        let path_str = tracks[index].file_path.clone();
        {
            let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.tracks = tracks;

            if queue.shuffle {
                queue.regenerate_shuffle_order();
                if let Some(pos) = queue.shuffle_order.iter().position(|&p| p == index) {
                    queue.shuffle_order.swap(0, pos);
                }
                queue.current_index = Some(0);
            } else {
                queue.current_index = Some(index);
            }
        }
        let path = std::path::PathBuf::from(&path_str);

        let cf_guard = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cf_engine) = *cf_guard {
            let _ = cf_engine.load_track_with_crossfade(path.to_string_lossy().as_ref());
            cf_engine.play();
            drop(cf_guard);
            *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) = PlayerState::Playing;
        } else {
            drop(cf_guard);
            let mut engine_guard = match self.engine.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::error!(
                        "AudioEngine mutex poisoned! C state may be corrupted. Reinitializing..."
                    );
                    let mut guard = poisoned.into_inner();
                    *guard = None;
                    return;
                }
            };
            if let Some(ref e) = *engine_guard {
                if let Err(err) = e.load(&path) {
                    tracing::error!("Failed to load track: {}", err);
                    *self
                        .playback_error
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) =
                        Some(format!("Failed to load track: {}", err));
                    *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                        PlayerState::Stopped;
                    self.next_track(); // Auto-advance past broken files
                } else if let Err(err) = e.play() {
                    tracing::error!("Failed to play track: {}", err);
                    *self
                        .playback_error
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) =
                        Some(format!("Failed to play track: {}", err));
                    *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                        PlayerState::Stopped;
                    self.next_track(); // Auto-advance past broken files
                } else {
                    *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                        PlayerState::Playing;
                }
            }
        }
    }

    pub fn play_track_at(&self, index: usize) {
        self.eos_flag
            .store(false, std::sync::atomic::Ordering::Release);

        let path = {
            let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.current_index = Some(index);
            let effective = queue.effective_index(index);
            effective.and_then(|i| queue.tracks.get(i).map(|t| t.file_path.clone()))
        };
        if let Some(path_str) = path {
            let path = std::path::PathBuf::from(&path_str);

            let cf_guard = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref cf_engine) = *cf_guard {
                let _ = cf_engine.load_track_with_crossfade(path.to_string_lossy().as_ref());
                cf_engine.play();
                drop(cf_guard);
                *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) = PlayerState::Playing;
            } else {
                drop(cf_guard);
                let mut engine_guard = match self.engine.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        tracing::error!("AudioEngine mutex poisoned! C state may be corrupted. Reinitializing...");
                        let mut guard = poisoned.into_inner();
                        *guard = None;
                        return;
                    }
                };
                if let Some(ref e) = *engine_guard {
                    if let Err(err) = e.load(&path) {
                        tracing::error!("Failed to load track: {}", err);
                        *self
                            .playback_error
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) =
                            Some(format!("Failed to load track: {}", err));
                        *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                            PlayerState::Stopped;
                        self.next_track(); // Auto-advance past broken files
                    } else if let Err(err) = e.play() {
                        tracing::error!("Failed to play track: {}", err);
                        *self
                            .playback_error
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) =
                            Some(format!("Failed to play track: {}", err));
                        *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                            PlayerState::Stopped;
                        self.next_track(); // Auto-advance past broken files
                    } else {
                        *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                            PlayerState::Playing;
                    }
                }
            }
        }
    }

    pub fn next_track(&self) {
        let next = {
            let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.manual_next_index()
        };
        if let Some(idx) = next {
            self.play_track_at(idx);
        }
    }

    pub fn prev_track(&self) {
        self.eos_flag
            .store(false, std::sync::atomic::Ordering::Release);

        let prev = {
            let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.prev_index()
        };
        if let Some(idx) = prev {
            self.play_track_at(idx);
        }
    }

    pub fn toggle_playback(&self) {
        let state = *self.player_state.lock().unwrap_or_else(|e| e.into_inner());
        match state {
            PlayerState::Playing | PlayerState::Buffering => self.pause(),
            PlayerState::Paused | PlayerState::Stopped => self.play(),
        }
    }

    pub fn play(&self) {
        let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cf_engine) = *cf {
            cf_engine.play();
            drop(cf);
            *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) = PlayerState::Playing;
        } else {
            let engine_result = {
                if let Ok(engine) = self.engine.lock() {
                    if let Some(ref e) = *engine {
                        e.play()
                    } else {
                        drop(cf);
                        return;
                    }
                } else {
                    drop(cf);
                    return;
                }
            };
            drop(cf);
            match engine_result {
                Ok(()) => {
                    *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                        PlayerState::Playing;
                }
                Err(e) => {
                    *self
                        .playback_error
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(format!("Play failed: {}", e));
                }
            }
        }
    }

    pub fn pause(&self) {
        let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cf_engine) = *cf {
            cf_engine.pause();
            drop(cf);
            *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) = PlayerState::Paused;
            self.save_playback_state_to_config();
        } else {
            let engine_result = {
                if let Ok(engine) = self.engine.lock() {
                    if let Some(ref e) = *engine {
                        e.pause()
                    } else {
                        drop(cf);
                        return;
                    }
                } else {
                    drop(cf);
                    return;
                }
            };
            drop(cf);
            match engine_result {
                Ok(()) => {
                    *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                        PlayerState::Paused;
                    self.save_playback_state_to_config();
                }
                Err(e) => {
                    *self
                        .playback_error
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(format!("Pause failed: {}", e));
                }
            }
        }
    }

    pub fn set_playback_speed(&self, speed: f32) {
        *self
            .playback_speed
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = speed;
        if let Ok(engine) = self.engine.lock() {
            if let Some(ref e) = *engine {
                let _ = e.set_playback_speed(speed as f64);
            }
        }
    }

    pub fn save_playback_state_to_config(&self) {
        let current_volume = self.volume();
        let current_speed = *self
            .playback_speed
            .lock()
            .unwrap_or_else(|e| e.into_inner()) as f64;

        let (is_shuffle, repeat_str) = {
            let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            (queue.shuffle, queue.repeat_mode.label().to_string())
        };

        let is_muted = self.volume_muted.load(Ordering::Relaxed);
        let vol_before_mute = f64::from_bits(self.volume_before_mute.load(Ordering::Relaxed));

        let mut config = self.config.write().unwrap_or_else(|e| e.into_inner());
        config.general.volume = current_volume;
        config.general.playback_speed = current_speed;
        config.general.shuffle = is_shuffle;
        config.general.repeat_mode = repeat_str;
        config.general.volume_muted = is_muted;
        config.general.volume_before_mute = vol_before_mute;

        if let Err(e) = tunecraft_core::config::save(&config) {
            tracing::warn!("Failed to save playback state: {}", e);
        }
    }

    pub fn load_all_tracks(&self) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        match db.as_ref() {
            Some(db) => match db.get_all_tracks() {
                Ok(tracks) => tracks,
                Err(e) => {
                    tracing::error!("Failed to load all tracks from database: {}", e);
                    Vec::new()
                }
            },
            None => Vec::new(),
        }
    }

    pub fn search_tracks(&self, query: &str) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        match db.as_ref() {
            Some(db) => match db.search_tracks(query) {
                Ok(tracks) => tracks,
                Err(e) => {
                    tracing::error!("Failed to search tracks: {}", e);
                    Vec::new()
                }
            },
            None => Vec::new(),
        }
    }

    pub fn search_tracks_advanced(
        &self,
        query: &str,
        genre: &str,
        year_from: Option<i32>,
        year_to: Option<i32>,
    ) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| {
                db.search_tracks_advanced(query, genre, year_from, year_to)
                    .ok()
            })
            .unwrap_or_default()
    }

    pub fn get_all_albums(&self) -> Vec<(String, String, i64, i64)> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_all_albums().ok())
            .unwrap_or_default()
    }

    pub fn get_tracks_by_album(&self, album: &str) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_tracks_by_album(album).ok())
            .unwrap_or_default()
    }

    pub fn get_all_artists(&self) -> Vec<(String, i64, i64)> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_all_artists().ok())
            .unwrap_or_default()
    }

    pub fn get_tracks_by_artist(&self, artist: &str) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_tracks_by_artist(artist).ok())
            .unwrap_or_default()
    }

    pub fn get_tracks_by_mood(&self, mood: &str) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_tracks_by_mood(mood, 0).ok()) // 0 = no limit
            .unwrap_or_default()
    }

    pub fn get_all_playlists(&self) -> Vec<tunecraft_core::database::models::Playlist> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_all_playlists().ok())
            .unwrap_or_default()
    }

    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Vec<tunecraft_core::Track> {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        db.as_ref()
            .and_then(|db| db.get_playlist_tracks(playlist_id).ok())
            .unwrap_or_default()
    }

    /// Issue #18: Compute a cache key from the current view, sort, search, and filter state.
    /// If the cache key matches the previously cached result, return the cached tracks
    /// instead of re-querying the database. The cache is invalidated whenever the view,
    /// search query, sort mode, or filters change, or when a scan completes.
    fn track_cache_key(&self) -> String {
        let view = self.current_view.lock().unwrap_or_else(|e| e.into_inner());
        let sort = self.sort_mode.lock().unwrap_or_else(|e| e.into_inner());
        let query = self.search_query.lock().unwrap_or_else(|e| e.into_inner());
        let genre = self.filter_genre.lock().unwrap_or_else(|e| e.into_inner());
        let year_range = self
            .filter_year_range
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        format!("{:?}|{:?}|{}|{}|{}", *view, sort, query, genre, year_range)
    }

    /// Issue #18: Invalidate the track list cache. Call this when the view changes,
    /// search query changes, sort mode changes, filters change, or a scan completes.
    pub fn invalidate_track_cache(&self) {
        let mut cache = self.cached_tracks.lock().unwrap_or_else(|e| e.into_inner());
        *cache = None;
    }

    pub fn load_tracks_for_view(&self) -> Vec<tunecraft_core::Track> {
        let cache_key = self.track_cache_key();
        {
            let cache = self.cached_tracks.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((ref key, ref tracks)) = *cache {
                if key == &cache_key {
                    return tracks.clone();
                }
            }
        }

        let view = self
            .current_view
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let sort = self
            .sort_mode
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut tracks = match view {
            ViewType::AllTracks => self.load_all_tracks(),
            ViewType::Search => {
                let query = self
                    .search_query
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if query.is_empty() {
                    self.load_all_tracks()
                } else {
                    self.search_tracks(&query)
                }
            }
            ViewType::AlbumDetail(ref album) => self.get_tracks_by_album(album),
            ViewType::ArtistDetail(ref artist) => self.get_tracks_by_artist(artist),
            ViewType::Mood(ref m) => match m.as_str() {
                "favorites" => {
                    let db = self.db.read().unwrap_or_else(|e| e.into_inner());
                    db.as_ref()
                        .and_then(|d| d.get_loved_track_records().ok())
                        .unwrap_or_default()
                }
                "recent" => {
                    let db = self.db.read().unwrap_or_else(|e| e.into_inner());
                    db.as_ref()
                        .and_then(|d| d.get_recent_tracks(50).ok())
                        .unwrap_or_default()
                }
                "most" => {
                    let db = self.db.read().unwrap_or_else(|e| e.into_inner());
                    db.as_ref()
                        .and_then(|d| d.get_most_played_tracks(50).ok())
                        .unwrap_or_default()
                }
                mood => self.get_tracks_by_mood(mood),
            },
            ViewType::PlaylistDetail(ref _name, id) => id
                .map(|pl_id| self.get_playlist_tracks(pl_id))
                .unwrap_or_default(),
            ViewType::Filter {
                ref genre,
                year_from,
                year_to,
            } => self.search_tracks_advanced("", genre, year_from, year_to),
            _ => self.load_all_tracks(),
        };
        self.apply_sort(&mut tracks, sort);
        {
            let mut cache = self.cached_tracks.lock().unwrap_or_else(|e| e.into_inner());
            *cache = Some((cache_key, tracks.clone()));
        }
        tracks
    }

    fn apply_sort(&self, tracks: &mut Vec<tunecraft_core::Track>, sort: SortMode) {
        match sort {
            SortMode::Default => {}
            SortMode::TitleAsc => tracks.sort_by(|a, b| a.title.cmp(&b.title)),
            SortMode::TitleDesc => tracks.sort_by(|a, b| b.title.cmp(&a.title)),
            SortMode::ArtistAsc => tracks.sort_by(|a, b| a.artist.cmp(&b.artist)),
            SortMode::ArtistDesc => tracks.sort_by(|a, b| b.artist.cmp(&a.artist)),
            SortMode::AlbumAsc => tracks.sort_by(|a, b| a.album.cmp(&b.album)),
            SortMode::AlbumDesc => tracks.sort_by(|a, b| b.album.cmp(&a.album)),
            SortMode::DurationAsc => tracks.sort_by(|a, b| a.duration.cmp(&b.duration)),
            SortMode::DurationDesc => tracks.sort_by(|a, b| b.duration.cmp(&a.duration)),
        }
    }

    /// #12/#13: Avoid re-querying the DB for every row; accept a pre-loaded track list.
    pub fn track_at_view_index(
        &self,
        idx: usize,
        tracks: &[tunecraft_core::Track],
    ) -> Option<tunecraft_core::Track> {
        tracks.get(idx).cloned()
    }

    /// #12: Check love status using a pre-loaded track list instead of a full DB query per row.
    pub fn is_track_loved_with_tracks(&self, idx: usize, tracks: &[tunecraft_core::Track]) -> bool {
        if let Some(t) = tracks.get(idx) {
            let key = t.file_hash.as_deref().unwrap_or(t.file_path.as_str());
            let loved = self.loved_tracks.lock().unwrap_or_else(|e| e.into_inner());
            loved.contains(key)
        } else {
            false
        }
    }

    pub fn track_count(&self) -> i64 {
        if self.sidebar_cache_valid.load(Ordering::Relaxed) {
            self.sidebar_track_count.load(Ordering::Relaxed) as i64
        } else {
            let db = self.db.read().unwrap_or_else(|e| e.into_inner());
            db.as_ref()
                .map(|db| db.track_count().unwrap_or(0))
                .unwrap_or(0)
        }
    }

    pub fn get_mood_track_count(&self, mood: &str) -> i64 {
        if self.sidebar_cache_valid.load(Ordering::Relaxed) {
            let counts = self
                .sidebar_mood_counts
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            counts.get(mood).copied().unwrap_or(0) as i64
        } else {
            let db = self.db.read().unwrap_or_else(|e| e.into_inner());
            db.as_ref()
                .map(|db| db.count_tracks_for_mood(mood).unwrap_or(0))
                .unwrap_or(0)
        }
    }

    /// Refresh the sidebar cache from the database.
    /// Fix H9: Caches album/artist/playlist counts to avoid querying the DB
    /// on every render (every 250ms).
    /// Bug #34 fix: Also cache track count and per-mood counts.
    pub fn refresh_sidebar_cache(&self) {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref db) = *db {
            let album_count = db.get_all_albums().map(|a| a.len() as u64).unwrap_or(0);
            let artist_count = db.get_all_artists().map(|a| a.len() as u64).unwrap_or(0);
            let playlist_count = db.get_all_playlists().map(|p| p.len() as u64).unwrap_or(0);
            let track_count = db.track_count().unwrap_or(0) as u64;
            let mut mood_counts = std::collections::HashMap::new();
            for mood in &["dance", "romantic", "sad", "sufi", "chill"] {
                if let Ok(count) = db.count_tracks_for_mood(mood) {
                    mood_counts.insert(mood.to_string(), count as u64);
                }
            }
            self.sidebar_album_count
                .store(album_count, Ordering::Relaxed);
            self.sidebar_artist_count
                .store(artist_count, Ordering::Relaxed);
            self.sidebar_playlist_count
                .store(playlist_count, Ordering::Relaxed);
            self.sidebar_track_count
                .store(track_count, Ordering::Relaxed);
            *self
                .sidebar_mood_counts
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = mood_counts;
            self.sidebar_cache_valid.store(true, Ordering::Relaxed);
        }
    }

    pub fn queue_lock(&self) -> std::sync::MutexGuard<'_, PlayQueue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Tick the engine (called periodically from the UI).
    pub fn engine_tick(&self) {
        let mut engine_guard = match self.engine.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    "AudioEngine mutex poisoned during tick! Dropping corrupted engine."
                );
                let mut guard = poisoned.into_inner();
                *guard = None;
                return;
            }
        };
        if let Some(ref engine) = *engine_guard {
            engine.tick();
        }
        if let Ok(cf) = self.crossfade.lock() {
            if let Some(ref cf_engine) = *cf {
                cf_engine.poll_and_dispatch();
            }
        }
        if self.eos_flag.swap(false, Ordering::Relaxed) {
            let has_next = {
                let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                queue.next_index().is_some()
            };
            if has_next {
                self.next_track();
                self.notify_track_change();
            } else {
                {
                    let mut engine_guard = match self.engine.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::error!("AudioEngine mutex poisoned during stop! Dropping corrupted engine.");
                            let mut guard = poisoned.into_inner();
                            *guard = None;
                            return;
                        }
                    };
                    if let Some(ref engine) = *engine_guard {
                        let _ = engine.stop();
                    }
                }
                *self.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    tunecraft_core::audio::PlayerState::Stopped;
                {
                    let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                    queue.current_index = None;
                }
            }
        }
        {
            let mut error = self
                .playback_error
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(msg) = error.take() {
                *self.toast_message.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
            }
        }
    }

    /// Take and clear the current toast message, if any.
    /// Returns None if there is no pending toast.
    /// Fix C6: Previously, toast_message was set but never consumed by any
    /// UI component. This method allows the UI to take() the message for
    /// display and automatically clear it.
    pub fn take_toast_message(&self) -> Option<String> {
        self.toast_message
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Toggle the love status of a track and persist to database.
    /// Fix C7: Previously, loved_tracks was purely in-memory — toggling love
    /// was lost on restart. Now the change is persisted to the database.
    pub fn toggle_track_loved(&self, track_key: &str) -> bool {
        let mut loved = self.loved_tracks.lock().unwrap_or_else(|e| e.into_inner());
        let is_now_loved = if loved.contains(track_key) {
            loved.remove(track_key);
            false
        } else {
            loved.insert(track_key.to_string());
            true
        };
        drop(loved);
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref db) = *db {
            if let Err(e) = db.set_track_loved(track_key, is_now_loved) {
                tracing::warn!("Failed to persist love status for {}: {}", track_key, e);
            }
        }
        is_now_loved
    }

    /// Load loved tracks from the database into the in-memory set.
    /// Fix C7: Call this after open_database() to restore love state.
    pub fn load_loved_tracks_from_db(&self) {
        let db = self.db.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref db) = *db {
            if let Ok(loved) = db.get_loved_tracks() {
                let mut set = self.loved_tracks.lock().unwrap_or_else(|e| e.into_inner());
                for key in loved {
                    set.insert(key);
                }
            }
        }
    }

    /// Push current EQ state from AppState to the audio engine.
    /// Fix: Previously, the UI modified eq_bands/eq_preamp/etc. in AppState
    /// but these values were never pushed down to the AudioEngine DSP context.
    pub fn push_eq_to_engine(&self) {
        if !self.eq_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        let bands = *self.eq_bands.lock().unwrap_or_else(|e| e.into_inner());
        let width = *self
            .eq_stereo_width
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let balance = *self.eq_balance.lock().unwrap_or_else(|e| e.into_inner());
        let ms_eq = self
            .eq_ms_enabled
            .load(std::sync::atomic::Ordering::Relaxed);

        if let Ok(engine_guard) = self.engine.lock() {
            if let Some(ref engine) = *engine_guard {
                for (i, &gain) in bands.iter().enumerate() {
                    let _ = engine.set_eq_band_gain(i, gain as f64);
                }
                let _ = engine.set_stereo_width(width as f64);
                let _ = engine.set_balance(balance as f64);
                let _ = ms_eq; // Used by future MS EQ integration
            }
        }
    }

    /// Toggle mute state and push the change to the audio engine.
    /// Fix: Previously, toggling volume_muted only updated the atomic boolean
    /// and config — the GStreamer pipeline volume remained unchanged.
    pub fn toggle_mute(&self) {
        let currently_muted = self.volume_muted.load(std::sync::atomic::Ordering::Relaxed);

        if currently_muted {
            let restore_vol = f64::from_bits(
                self.volume_before_mute
                    .load(std::sync::atomic::Ordering::Relaxed),
            );
            self.volume_muted
                .store(false, std::sync::atomic::Ordering::Relaxed);
            self.set_volume(restore_vol);
        } else {
            let current_vol = self.volume();
            if current_vol > 0.0 {
                self.volume_before_mute
                    .store(current_vol.to_bits(), std::sync::atomic::Ordering::Relaxed);
            }
            self.volume_muted
                .store(true, std::sync::atomic::Ordering::Relaxed);

            if let Ok(engine) = self.engine.lock() {
                if let Some(ref e) = *engine {
                    let _ = e.set_volume(0.0);
                }
            }
        }
    }

    /// Notify about track change.
    /// #4: Reset scrobble state when the track changes so accumulated time
    /// from previous tracks doesn't carry over.
    pub fn notify_track_change(&self) {
        let track_data = {
            let queue = self.queue_lock();
            queue.current_track().map(|t| {
                (
                    t.id,
                    t.title.clone().unwrap_or_default(),
                    t.artist.clone().unwrap_or_default(),
                    t.album.clone().unwrap_or_default(),
                )
            })
        };
        {
            let mut scrobble = self.scrobble.lock().unwrap_or_else(|e| e.into_inner());
            scrobble.track_id = track_data.as_ref().and_then(|d| d.0);
            scrobble.accumulated_secs = 0;
            scrobble.submitted = false;
        }
        if let Some((_id, title, artist, _album)) = track_data {
            let unique_id = format!(
                "{}-{}",
                _id.map(|i| i.to_string()).unwrap_or_default(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );
            let entry = NotificationEntry {
                title: title.clone(),
                body: format!("by {}", artist),
                timestamp: format!("{}", chrono::Local::now().format("%H:%M")),
                unique_id,
            };
            let mut notifs = self.notifications.lock().unwrap_or_else(|e| e.into_inner());
            notifs.insert(0, entry);
            if notifs.len() > 20 {
                notifs.truncate(20);
            }
            let prev = self.notification_count.load(Ordering::Relaxed);
            self.notification_count.store(prev + 1, Ordering::Relaxed);
        }
    }
}
