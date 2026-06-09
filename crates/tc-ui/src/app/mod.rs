//! TuneCraft App — main application state and eframe App implementation
//!
//! Refactored to use a service-layer architecture:
//!
//! - **Services** encapsulate backend subsystems behind clean APIs
//! - **AppContext** holds `Arc<Service>` references, not raw `Arc<Mutex<Backend>>`
//! - **TuneCraftApp** is a thin UI state holder that delegates to services
//!
//! ## Service Layer
//!
//! ```text
//! TuneCraftApp (UI state only)
//!   └── AppContext (service registry)
//!       ├── PlaybackService  → EngineHandle (channel + RwLock<PlaybackInfo>) + engine_mutex
//!       ├── LibraryService   → Database (snapshot-based reads)
//!       ├── EqService        → EngineHandle (channel for EQ commands)
//!       ├── ScrobbleService  → SQLite (local play journal)
//!       ├── LyricsService    → LyricsClient (async + result sink)
//!       ├── ConfigService    → RwLock<AppConfig> (periodic save)
//!       └── PlatformService  → PlatformIntegration (RwLock, thread-safe)
//! ```
//!
//! eframe::App trait is implemented for TuneCraftApp with full update loop.
//! Drop is implemented for graceful shutdown. Engine JoinHandle is stored
//! and joined on exit. Scrobble events are consumed for user feedback.
//! Volume is clamped. Stereo width is standardized as ratio (0.0-2.0).

mod context;
mod library_actions;
mod playback_actions;
mod scrobble_actions;
mod sync;
mod toasts;

use std::sync::Arc;

pub use context::AppContext;
use egui::Vec2;
use log::info;
pub use tc_config::RepeatMode;
pub use tc_db::{Playlist, Track};
pub use tc_engine::buffer::{EngineCommand, PlaybackInfo, PlaybackState as EnginePlaybackState};
pub use toasts::ToastLevel;

use crate::{sidebar::NavSection, theme::TuneCraftColors};

/// The main TuneCraft application state.
///
/// This struct holds ONLY UI state — no business logic. All operations
/// are delegated to the service layer via `self.ctx`.
///
/// Responsibilities:
/// - Syncs state from services each frame
/// - Polls media keys
/// - Checks scrobble thresholds
/// - Consumes scrobble events for user feedback
/// - Polls lyrics results
/// - Draws sidebar, main content, player bar
/// - Handles toasts and config saves
pub struct TuneCraftApp {
    pub ctx: Arc<AppContext>,

    pub dark_mode: bool,
    pub colors_cache: Option<TuneCraftColors>,

    pub nav: NavSection,

    pub tracks: Vec<Track>,
    pub cached_favorite_ids: std::collections::HashSet<i64>,
    pub search_query: String,
    pub list_view: bool,
    pub playlists_loaded: bool,
    pub badge_cache: std::collections::HashMap<String, u32>,
    pub selected_track_id: Option<i64>,
    pub total_track_count: usize,
    pub track_page: usize,
    pub tracks_per_page: usize,

    pub current_track_id: Option<i64>,
    pub is_playing: bool,
    pub is_favorited: bool,
    pub position_secs: f32,
    pub duration_secs: f32,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub speed: f32,

    pub play_queue: Vec<i64>,
    pub play_queue_index: Option<usize>,
    pub shuffle_order: Vec<usize>,
    pub shuffle_position: usize,

    pub playlists: Vec<tc_db::Playlist>,
    pub selected_playlist_id: Option<i64>,
    pub show_create_playlist_dialog: bool,
    pub new_playlist_name: String,

    pub show_eq_panel: bool,
    pub eq_enabled: bool,
    pub eq_bands: [f32; 10],
    pub eq_preset: String,
    pub eq_preamp: f32,
    pub eq_bass_shelf: f32,
    pub eq_treble_shelf: f32,
    pub eq_stereo_width: f32,
    pub eq_balance: f32,
    pub eq_dither: bool,
    pub eq_midside: bool,
    pub cached_dither_enabled: bool,
    pub cached_midside_enabled: bool,

    pub show_lyrics: bool,
    pub current_lyrics: Option<Vec<tc_lyrics::SyncedLyricLine>>,
    pub lyrics_loading: bool,

    pub scrobble_enabled: bool,
    pub last_scrobbled_track_id: Option<i64>,
    pub play_started_at: Option<std::time::Instant>,
    /// Accumulated play seconds.
    pub accumulated_play_secs: f32,

    pub toasts: Vec<(String, std::time::Instant, ToastLevel, u64)>,

    pub(crate) last_synced_playback_version: u64,

    pub sort_ascending: bool,

    /// Whether the resampler has been disabled due to excessive rebuild failures
    pub resampler_disabled: bool,
    /// Whether a DSP warning toast has already been shown (to avoid spam)
    pub dsp_warning_shown: bool,

    pub status_message: String,
    pub is_scanning: bool,

    pub theme_needs_detection: bool,

    pub(crate) close_requested: bool,

    /// Whether the "add music folder" dialog is open
    pub show_add_music_dialog: bool,
    /// Pending folder path text when user types it manually
    pub add_music_folder_path: String,

    /// In-memory cache of decoded cover art textures keyed by track_id.
    /// Populated lazily on first access per track.
    pub album_art_cache: std::collections::HashMap<i64, egui::TextureHandle>,

    /// Whether the sidebar is in collapsed/narrow mode
    pub sidebar_collapsed: bool,
}

impl TuneCraftApp {
    /// Create a new TuneCraftApp with the given AppContext.
    pub fn new(ctx: AppContext) -> Self {
        let playback_state = ctx.playback.state();
        let volume = playback_state.volume;
        let shuffle = playback_state.shuffle;
        let repeat = playback_state.repeat;
        let speed = playback_state.speed;
        let accumulated_play_secs = playback_state.accumulated_play_secs;
        let playback_version = playback_state.version;
        drop(playback_state);

        let scrobble_enabled = ctx.scrobble.is_enabled();

        let (eq_enabled, eq_preamp, eq_dither, eq_bands) = ctx
            .config
            .read(|c| {
                let mut bands = [0.0; 10];
                for (i, band) in c.engine.eq.bands.iter().enumerate() {
                    if i < 10 {
                        bands[i] = band.gain_db;
                    }
                }
                (
                    c.engine.eq.enabled,
                    c.engine.eq.preamp_db,
                    c.engine.dither_enabled,
                    bands,
                )
            })
            .unwrap_or((false, 0.0, true, [0.0; 10]));

        let tracks_per_page = ctx
            .config
            .read(|c| c.library.tracks_per_page)
            .unwrap_or(500);

        let lib_snapshot = ctx.library.snapshot();
        let tracks = lib_snapshot.tracks.clone();
        let cached_favorite_ids: std::collections::HashSet<i64> = lib_snapshot.favorite_ids.clone();
        let playlists = lib_snapshot.playlists.clone();
        let total_track_count = lib_snapshot.total_track_count;
        let is_scanning = lib_snapshot.is_scanning;
        let status_message = lib_snapshot.status_message.clone();
        drop(lib_snapshot);

        let dark_mode = ctx
            .config
            .read(|c| match c.ui.theme {
                tc_config::Theme::Dark => true,
                tc_config::Theme::Light => false,
                tc_config::Theme::System => true,
            })
            .unwrap_or(true);

        let theme_needs_detection = matches!(ctx.config.theme(), tc_config::Theme::System);

        let initial_queue: Vec<i64> = ctx.library.get_all_track_ids();
        ctx.playback.set_play_queue(initial_queue.clone());

        let app = Self {
            ctx: Arc::new(ctx),

            dark_mode,
            colors_cache: None,

            nav: NavSection::AllTracks,

            tracks: tracks.clone(),
            cached_favorite_ids,
            search_query: String::new(),
            list_view: true,
            playlists_loaded: true,
            badge_cache: std::collections::HashMap::new(),
            selected_track_id: None,
            total_track_count,
            track_page: 0,
            tracks_per_page,

            current_track_id: None,
            is_playing: false,
            is_favorited: false,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume,
            shuffle,
            repeat,
            speed,

            play_queue: initial_queue,
            play_queue_index: None,
            shuffle_order: Vec::new(),
            shuffle_position: 0,

            playlists,
            selected_playlist_id: None,
            show_create_playlist_dialog: false,
            new_playlist_name: String::new(),

            show_eq_panel: false,
            eq_enabled,
            eq_bands,
            eq_preset: "Custom".to_string(),
            eq_preamp,
            eq_bass_shelf: 0.0,
            eq_treble_shelf: 0.0,

            eq_stereo_width: 1.0,
            eq_balance: 0.0,
            eq_dither,
            eq_midside: false,
            cached_dither_enabled: eq_dither,
            cached_midside_enabled: false,

            show_lyrics: false,
            current_lyrics: None,
            lyrics_loading: false,

            scrobble_enabled,
            last_scrobbled_track_id: None,
            play_started_at: None,
            accumulated_play_secs,

            toasts: Vec::new(),

            last_synced_playback_version: playback_version,

            sort_ascending: true,

            resampler_disabled: false,
            dsp_warning_shown: false,

            status_message: status_message.clone(),
            is_scanning,

            theme_needs_detection,

            close_requested: false,

            show_add_music_dialog: false,
            add_music_folder_path: String::new(),

            album_art_cache: std::collections::HashMap::new(),

            sidebar_collapsed: false,
        };

        app.trigger_background_analysis();
        app
    }

    pub fn colors(&self) -> TuneCraftColors {
        if self.dark_mode {
            TuneCraftColors::dark()
        } else {
            TuneCraftColors::light()
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_track_id
            .and_then(|id| self.tracks.iter().find(|t| t.id == id))
    }
}

// eframe::App Implementation (v0.9.3: C-01 fix)

impl eframe::App for TuneCraftApp {
    /// Called every frame by the eframe runtime.
    ///
    ///
    /// . It orchestrates all per-frame operations:
    /// 1. Sync playback and EQ state from services
    /// 2. Poll media key events
    /// 3. Check scrobble thresholds
    /// 4. Poll scrobble events for user feedback
    /// 5. Poll lyrics results
    /// 6. Update scan state
    /// 7. Draw sidebar, main content area, and player bar
    /// 8. Draw EQ panel overlay
    /// 9. Draw toast notifications
    /// 10. Save config if dirty
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_from_playback_service();
        self.sync_from_eq_service();

        self.poll_media_keys();

        self.check_and_scrobble();

        self.poll_scrobble_events();

        self.poll_lyrics();

        self.update_scan_state();

        self.check_dsp_warnings();

        self.ctx.platform.update_mpris_position(self.position_secs);

        // theme toggles take effect immediately.  When `dark_mode` changes we
        // current palette and would otherwise serve stale values.
        let config_dark_mode = self
            .ctx
            .config
            .read(|c| {
                match c.ui.theme {
                    tc_config::Theme::Dark => true,
                    tc_config::Theme::Light => false,
                    tc_config::Theme::System => self.dark_mode, // keep existing detection
                }
            })
            .unwrap_or(self.dark_mode);

        if config_dark_mode != self.dark_mode {
            self.dark_mode = config_dark_mode;
            self.colors_cache = None; // force re-render with new palette
            self.badge_cache.clear(); // badges use theme colours; invalidate
        }

        if self.colors_cache.is_none() {
            self.colors_cache = Some(self.colors());
            let visuals = if self.dark_mode {
                crate::theme::dark_visuals()
            } else {
                crate::theme::light_visuals()
            };
            ctx.set_visuals(visuals);
        }

        let sidebar_w = if self.sidebar_collapsed {
            60.0
        } else if ctx.screen_rect().width() < 700.0 {
            180.0
        } else {
            240.0
        };
        egui::SidePanel::left("sidebar")
            .min_width(if self.sidebar_collapsed { 60.0 } else { 180.0 })
            .max_width(if self.sidebar_collapsed { 60.0 } else { 260.0 })
            .default_width(sidebar_w)
            .show(ctx, |ui| {
                crate::sidebar::draw(self, ui);
            });

        egui::TopBottomPanel::bottom("player_bar")
            .exact_height(crate::player_bar::PLAYER_BAR_HEIGHT)
            .show(ctx, |ui| {
                crate::player_bar::draw(self, ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            crate::track_list::draw(self, ui);
        });

        // EQ panel as a floating window overlay — responsive sizing
        if self.show_eq_panel {
            let screen_rect = ctx.screen_rect();
            let screen_w = screen_rect.width();
            let screen_h = screen_rect.height();
            // Responsive: scale EQ panel to viewport
            let window_width = if screen_w < 500.0 {
                screen_w * 0.95
            } else {
                (screen_w * 0.75).clamp(400.0, 780.0)
            };
            let window_height = if screen_h < 500.0 {
                screen_h * 0.85
            } else {
                420.0_f32.min(screen_h * 0.6)
            };
            let window_x = screen_rect.left() + (screen_rect.width() - window_width) / 2.0;
            let window_y = screen_rect.top() + (screen_rect.height() - window_height) / 2.0;

            let mut open = self.show_eq_panel;
            egui::Window::new("EQ")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_pos(egui::Pos2::new(window_x, window_y))
                .default_size(egui::Vec2::new(window_width, window_height))
                .min_width(480.0)
                .min_height(360.0)
                .title_bar(false)
                .show(ctx, |ui| {
                    crate::eq_panel::draw(self, ui);
                });
            if !open {
                self.show_eq_panel = false;
                self.ctx.eq.state_mut().show_panel = false;
            }
        }

        if self.show_create_playlist_dialog {
            egui::Window::new("Create Playlist")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Playlist name:");
                    ui.text_edit_singleline(&mut self.new_playlist_name);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            if !self.new_playlist_name.is_empty() {
                                self.create_playlist(&self.new_playlist_name.clone());
                                self.new_playlist_name.clear();
                            }
                            self.show_create_playlist_dialog = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_create_playlist_dialog = false;
                        }
                    });
                });
        }

        // Add Music dialog
        if self.show_add_music_dialog {
            egui::Window::new("Add Music Folder")
                .collapsible(false)
                .resizable(false)
                .min_width(420.0)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.label("Enter the path to a folder containing music files:");
                    ui.add_space(4.0);
                    ui.text_edit_singleline(&mut self.add_music_folder_path);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Supported formats: MP3, FLAC, OGG, WAV, AAC, M4A")
                            .font(egui::FontId::proportional(11.0))
                            .color(self.colors().text_dim),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Add Folder").clicked() {
                            let path = self.add_music_folder_path.trim().to_string();
                            if !path.is_empty() {
                                self.add_music_folder(&path);
                            }
                            self.show_add_music_dialog = false;
                            self.add_music_folder_path.clear();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_add_music_dialog = false;
                            self.add_music_folder_path.clear();
                        }
                    });
                    ui.add_space(4.0);
                });
        }

        self.draw_toasts(ctx);

        self.save_config_if_dirty();

        if self.is_playing {
            ctx.request_repaint();
        }
    }

    /// Called when the user requests to close the application.
    ///
    /// Stops playback immediately, then returns `true` to let eframe destroy
    /// the window and drop `TuneCraftApp`.  The real cleanup — flushing the
    /// scrobble queue, persisting it to disk, and saving the config — happens
    /// synchronously in `AppContext::Drop`.  Those operations run on a
    /// dedicated OS thread (via `std::thread::scope`) so they are safe even
    /// though `Drop` may be called from within the tokio runtime context.
    fn on_exit(&mut self) {
        info!("Application close requested — initiating graceful shutdown");
        self.close_requested = true;

        self.ctx.playback.stop_playback();
    }
}

/// Launch the TuneCraft GUI.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_context = AppContext::init()?;
    let app = TuneCraftApp::new(app_context);

    // silently drops the receiver and breaks scrobble UI feedback (blocker #1).

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(1200.0, 800.0))
            .with_min_inner_size(Vec2::new(800.0, 600.0))
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../icon.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };

    eframe::run_native("TuneCraft", options, Box::new(|_cc| Ok(Box::new(app))))
        .map_err(|e| format!("eframe error: {}", e).into())
}
