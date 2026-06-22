//! TuneCraft App — main application state and Slint event loop.
//!
//! Refactored from egui/eframe (v3.1.3) to Slint 1.16.1 (v3.1.4), with
//! post-migration wiring fixes in v3.1.5 (lyrics panel, sidebar badges,
//! track-list pagination).
//! The service-layer architecture is preserved unchanged.

mod context;
mod library_actions;
mod lyrics_actions;
mod playback_actions;
mod scrobble_actions;
mod sync;
mod toasts;

use std::sync::Arc;

use log::info;
use parking_lot::Mutex;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    converters::toast_to_item,
    eq_panel::sync_eq_panel,
    folders_view::sync_folders_view,
    player_bar::sync_player_bar,
    settings_view::sync_settings_view,
    sidebar::{parse_section, sync_sidebar, NavSection},
    track_list::sync_track_list,
    App,
};

pub use context::AppContext;
pub use tc_config::RepeatMode;
pub use tc_db::{Playlist, Track};
pub use tc_engine::buffer::{EngineCommand, PlaybackInfo, PlaybackState as EnginePlaybackState};
pub use toasts::ToastLevel;

/// The main TuneCraft application state.
///
/// Holds ONLY UI state — no business logic. All operations are delegated
/// to the service layer via `self.ctx`.
pub struct TuneCraftApp {
    pub ctx: Arc<AppContext>,
    pub theme: tc_config::Theme,

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
    pub volume_before_mute: Option<f32>,
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
    pub show_add_to_playlist_dialog: Option<i64>,
    pub show_track_info_dialog: Option<i64>,

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

    pub scrobble_enabled: bool,
    pub last_scrobbled_track_id: Option<i64>,
    pub play_started_at: Option<std::time::Instant>,
    pub accumulated_play_secs: f32,

    pub toasts: Vec<(String, std::time::Instant, ToastLevel, u64)>,

    pub(crate) last_synced_playback_version: u64,

    pub sort_active: bool,
    pub sort_ascending: bool,
    pub sort_field: String,
    pub filter_favorites: bool,

    pub resampler_disabled: bool,
    pub dsp_warning_shown: bool,
    pub last_engine_error: Option<String>,

    pub status_message: String,
    pub is_scanning: bool,

    pub theme_needs_detection: bool,

    pub(crate) close_requested: bool,

    pub show_add_music_dialog: bool,
    pub add_music_folder_path: String,

    pub folder_view_path: Option<std::path::PathBuf>,
    pub folder_tracks: Vec<Track>,

    pub sidebar_collapsed: bool,

    pub focus_search: bool,

    pub last_mpris_position_update: std::time::Instant,

    pub lyrics_fetched_for: Option<i64>,
    pub show_lyrics_panel: bool,
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

        let theme = ctx
            .config
            .read(|c| c.ui.theme)
            .unwrap_or(tc_config::Theme::Dark);

        let theme_needs_detection = matches!(theme, tc_config::Theme::System);

        let initial_queue: Vec<i64> = ctx.library.get_all_track_ids();
        ctx.playback.set_play_queue(initial_queue.clone());

        let app = Self {
            ctx: Arc::new(ctx),
            theme,
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
            volume_before_mute: None,
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
            show_add_to_playlist_dialog: None,
            show_track_info_dialog: None,
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
            scrobble_enabled,
            last_scrobbled_track_id: None,
            play_started_at: None,
            accumulated_play_secs,
            toasts: Vec::new(),
            last_synced_playback_version: playback_version,
            sort_active: false,
            sort_ascending: true,
            sort_field: String::new(),
            filter_favorites: false,
            resampler_disabled: false,
            dsp_warning_shown: false,
            last_engine_error: None,
            status_message: status_message.clone(),
            is_scanning,
            theme_needs_detection,
            close_requested: false,
            show_add_music_dialog: false,
            add_music_folder_path: String::new(),
            folder_view_path: None,
            folder_tracks: Vec::new(),
            sidebar_collapsed: false,
            focus_search: false,
            last_mpris_position_update: std::time::Instant::now()
                - std::time::Duration::from_secs(10),
            lyrics_fetched_for: None,
            show_lyrics_panel: false,
        };

        app.trigger_background_analysis();
        app
    }

    pub fn colors(&self) -> crate::theme::TuneCraftColors {
        use crate::theme::TuneCraftColors;
        match self.theme {
            tc_config::Theme::Light => TuneCraftColors::light(),
            tc_config::Theme::Dark => TuneCraftColors::dark(),
            tc_config::Theme::System => TuneCraftColors::dark(),
            tc_config::Theme::Ocean => TuneCraftColors::ocean(),
            tc_config::Theme::Forest => TuneCraftColors::forest(),
            tc_config::Theme::Sunset => TuneCraftColors::sunset(),
            tc_config::Theme::Berry => TuneCraftColors::berry(),
            tc_config::Theme::Midnight => TuneCraftColors::midnight(),
            tc_config::Theme::Rose => TuneCraftColors::rose(),
            tc_config::Theme::Coffee => TuneCraftColors::coffee(),
            tc_config::Theme::Mint => TuneCraftColors::mint(),
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_track_id
            .and_then(|id| self.tracks.iter().find(|t| t.id == id))
    }

    /// Sync all UI state to the Slint App component.
    pub fn sync_to_slint(&self, slint_app: &App) {
        let theme_name = format!("{:?}", self.theme).to_lowercase();
        slint_app.set_theme_name(SharedString::from(theme_name));

        sync_sidebar(self, slint_app);
        sync_track_list(self, slint_app);
        sync_player_bar(self, slint_app);
        sync_eq_panel(self, slint_app);
        sync_folders_view(self, slint_app);
        sync_settings_view(self, slint_app);
        lyrics_actions::sync_lyrics_panel(self, slint_app);
        self.sync_toasts_to_slint(slint_app);
        self.sync_dialogs_to_slint(slint_app);
    }

    /// Push current toasts list to Slint.
    fn sync_toasts_to_slint(&self, slint_app: &App) {
        let now = std::time::Instant::now();
        let items: Vec<_> = self
            .toasts
            .iter()
            .filter(|(_, expiry, _, _)| *expiry > now)
            .map(|(msg, _, level, id)| {
                let level_str = match level {
                    ToastLevel::Info => "info",
                    ToastLevel::Success => "success",
                    ToastLevel::Warning => "warning",
                    ToastLevel::Error => "error",
                };
                toast_to_item(*id, msg, level_str)
            })
            .collect();
        slint_app.set_toasts(ModelRc::new(VecModel::from(items)));
    }

    /// Sync dialog visibility flags.
    fn sync_dialogs_to_slint(&self, slint_app: &App) {
        slint_app.set_show_create_playlist_dialog(self.show_create_playlist_dialog);
        slint_app.set_new_playlist_name(SharedString::from(self.new_playlist_name.clone()));
        slint_app.set_show_add_to_playlist_dialog(self.show_add_to_playlist_dialog.is_some());
        slint_app.set_add_to_playlist_track_id(self.show_add_to_playlist_dialog.unwrap_or(-1) as i32);
        slint_app.set_show_track_info_dialog(self.show_track_info_dialog.is_some());

        if let Some(track_id) = self.show_track_info_dialog {
            if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                slint_app.set_track_info_item(crate::converters::track_to_item(
                    track,
                    false,
                    false,
                    self.cached_favorite_ids.contains(&track.id),
                    slint::Image::default(),
                    false,
                ));
            }
        }
    }
}

/// Per-tick state sync. Called every 200ms by the Slint timer.
///
/// This is the equivalent of the egui `update()` method — it polls services,
/// checks scrobble thresholds, refreshes scan state, etc., then pushes
/// the updated state to the Slint App component.
pub fn tick(app_state: &mut TuneCraftApp, slint_app: &App) {
    let track_ended = app_state.sync_from_playback_service();
    if track_ended {
        app_state.play_next();
    }
    app_state.sync_from_eq_service();

    app_state.poll_media_keys();
    app_state.check_and_scrobble();
    app_state.poll_scrobble_events();
    app_state.update_scan_state();
    app_state.check_dsp_warnings();
    app_state.maybe_fetch_lyrics();
    app_state.poll_lyrics_events();

    const MPRIS_POSITION_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);
    if app_state.is_playing && app_state.last_mpris_position_update.elapsed() >= MPRIS_POSITION_INTERVAL {
        app_state.ctx.platform.update_mpris_position(app_state.position_secs);
        app_state.last_mpris_position_update = std::time::Instant::now();
    }

    let config_theme = app_state.ctx.config.read(|c| c.ui.theme).unwrap_or(app_state.theme);
    if config_theme != app_state.theme {
        app_state.theme = config_theme;
    }

    let now = std::time::Instant::now();
    app_state.toasts.retain(|(_, expiry, _, _)| *expiry > now);

    app_state.sync_to_slint(slint_app);
    app_state.save_config_if_dirty();
}

/// Launch the TuneCraft GUI.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_context = AppContext::init()?;
    let app_state = Arc::new(Mutex::new(TuneCraftApp::new(app_context)));

    let slint_app = App::new()?;

    // Initial sync.
    {
        let state = app_state.lock();
        state.sync_to_slint(&slint_app);
    }

    wire_callbacks(&slint_app, &app_state);

    // 200ms periodic timer for state sync. This is the heartbeat of the
    // application — equivalent to the egui update() loop, but at 5 Hz
    // instead of 30-60 Hz. Slint repaints only when properties change,
    // so this is cheap when nothing is happening.
    let weak_app = slint_app.as_weak();
    let state_for_timer = Arc::clone(&app_state);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            if let Some(s) = weak_app.upgrade() {
                let mut state = state_for_timer.lock();
                tick(&mut state, &s);
            }
        },
    );
    std::mem::forget(timer);

    slint_app.run()?;

    info!("Slint event loop exited, shutting down");
    {
        let state = app_state.lock();
        state.ctx.playback.stop_playback();
    }

    Ok(())
}

/// Wire up all Slint callbacks to invoke the corresponding `*_actions` methods.
fn wire_callbacks(slint_app: &App, app_state: &Arc<Mutex<TuneCraftApp>>) {
    // ── Sidebar: nav-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_nav_clicked(move |section_str| {
        let section = parse_section(&section_str);
        let mut s = state.lock();
        s.nav = section;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Sidebar: search-changed ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_search_changed(move |query| {
        let mut s = state.lock();
        s.search_query = query.to_string();
        // Re-filter tracks.
        s.refresh_tracks();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: play-pause ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_play_pause(move || {
        let mut s = state.lock();
        s.toggle_playback();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: prev / next ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_prev(move || {
        let mut s = state.lock();
        s.play_prev();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_next(move || {
        let mut s = state.lock();
        s.play_next();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: seek ──
    let state = Arc::clone(app_state);
    slint_app.on_seek(move |pos| {
        let s = state.lock();
        s.seek(pos);
    });

    // ── Player bar: set-volume ──
    let state = Arc::clone(app_state);
    slint_app.on_set_volume(move |vol| {
        let mut s = state.lock();
        s.set_volume(vol);
    });

    // ── Player bar: toggle-favorite-current ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_favorite_current(move || {
        let mut s = state.lock();
        s.toggle_favorite();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: toggle-eq-panel ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_eq_panel(move || {
        let mut s = state.lock();
        s.show_eq_panel = !s.show_eq_panel;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: toggle-shuffle ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_shuffle(move || {
        let mut s = state.lock();
        s.toggle_shuffle();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Player bar: cycle-repeat ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_cycle_repeat(move || {
        let mut s = state.lock();
        s.toggle_repeat();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: track-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_track_clicked(move |track_id| {
        let mut s = state.lock();
        s.play_track(track_id as i64);
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: toggle-favorite ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_favorite(move |track_id| {
        let track_id = track_id as i64;
        let mut s = state.lock();
        // Toggle the favorite for an arbitrary track (not necessarily the current one).
        let is_fav = s.cached_favorite_ids.contains(&track_id);
        let new_state = s.ctx.library.toggle_favorite(track_id, is_fav);
        if new_state {
            s.cached_favorite_ids.insert(track_id);
        } else {
            s.cached_favorite_ids.remove(&track_id);
        }
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── EQ panel: close ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_eq_close(move || {
        let mut s = state.lock();
        s.show_eq_panel = false;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── EQ panel: toggle-enabled ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_eq_toggle_enabled(move |enabled| {
        let mut s = state.lock();
        s.eq_enabled = enabled;
        s.ctx.eq.set_enabled(enabled);
        s.ctx.config.write(|c| c.engine.eq.enabled = enabled);
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── EQ panel: band-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_eq_band_changed(move |idx, gain| {
        let mut s = state.lock();
        if (idx as usize) < s.eq_bands.len() {
            s.eq_bands[idx as usize] = gain;
            s.ctx.eq.set_band(idx as usize, gain);
            s.ctx.config.write(|c| {
                if let Some(band) = c.engine.eq.bands.get_mut(idx as usize) {
                    band.gain_db = gain;
                }
            });
        }
    });

    // ── EQ panel: preamp-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_eq_preamp_changed(move |v| {
        let mut s = state.lock();
        s.eq_preamp = v;
        s.ctx.eq.set_preamp(v);
        s.ctx.config.write(|c| c.engine.eq.preamp_db = v);
    });

    // ── Settings: theme-changed ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_theme_changed(move |theme_str| {
        let mut s = state.lock();
        let theme = match theme_str.as_str() {
            "light" => tc_config::Theme::Light,
            "dark" => tc_config::Theme::Dark,
            "system" => tc_config::Theme::System,
            "ocean" => tc_config::Theme::Ocean,
            "forest" => tc_config::Theme::Forest,
            "sunset" => tc_config::Theme::Sunset,
            "berry" => tc_config::Theme::Berry,
            "midnight" => tc_config::Theme::Midnight,
            "rose" => tc_config::Theme::Rose,
            "coffee" => tc_config::Theme::Coffee,
            "mint" => tc_config::Theme::Mint,
            _ => tc_config::Theme::Dark,
        };
        s.theme = theme;
        s.ctx.config.write(|c| c.ui.theme = theme);
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Settings: volume-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_volume_changed(move |v| {
        let s = state.lock();
        s.ctx.config.write(|c| c.playback.volume = v);
    });

    // ── Settings: speed-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_speed_changed(move |v| {
        let s = state.lock();
        s.ctx.config.write(|c| c.playback.speed = v);
    });

    // ── Settings: dither-enabled-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_dither_enabled_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.engine.dither_enabled = b);
    });

    // ── Settings: tracks-per-page-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_tracks_per_page_changed(move |n| {
        let s = state.lock();
        s.ctx.config.write(|c| c.library.tracks_per_page = n as usize);
    });

    // ── Settings: scan-on-startup-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_scan_on_startup_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.library.scan_on_startup = b);
    });

    // ── Settings: lyrics-enabled-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_lyrics_enabled_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.lyrics.enabled = b);
    });

    // ── Settings: lyrics-fetch-on-play-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_lyrics_fetch_on_play_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.lyrics.fetch_on_play = b);
    });

    // ── Settings: lyrics-base-url-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_lyrics_base_url_changed(move |s_str| {
        let s = state.lock();
        s.ctx.config.write(|c| c.lyrics.base_url = s_str.to_string());
    });

    // ── Settings: scrobble-enabled-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_scrobble_enabled_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.scrobble.enabled = b);
    });

    // ── Settings: show-spectrum-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_show_spectrum_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.ui.show_spectrum = b);
    });

    // ── Settings: show-waveform-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_show_waveform_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.ui.show_waveform = b);
    });

    // ── Settings: minimize-to-tray-changed ──
    let state = Arc::clone(app_state);
    slint_app.on_minimize_to_tray_changed(move |b| {
        let s = state.lock();
        s.ctx.config.write(|c| c.ui.minimize_to_tray = b);
    });

    // ── Settings: add-watch-dir ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_add_watch_dir(move || {
        // Use rfd to prompt for a folder.
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let mut s = state.lock();
            s.add_music_folder(&path.to_string_lossy());
            if let Some(ui) = weak.upgrade() {
                s.sync_to_slint(&ui);
            }
        }
    });

    // ── Settings: remove-watch-dir ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_remove_watch_dir(move |dir_str| {
        let s = state.lock();
        let path = std::path::PathBuf::from(dir_str.as_str());
        s.ctx.config.write(|c| c.library.watch_dirs.retain(|d| d != &path));
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Settings: rescan-now ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_rescan_now(move || {
        let s = state.lock();
        s.ctx.library.refresh_tracks();
        drop(s);
        if let Some(ui) = weak.upgrade() {
            let s = state.lock();
            s.sync_to_slint(&ui);
        }
    });

    // ── Folders: folder-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_folder_clicked(move |path_str| {
        let mut s = state.lock();
        let path = std::path::PathBuf::from(path_str.as_str());
        s.folder_view_path = Some(path.clone());
        s.folder_tracks = s.tracks.iter().filter(|t| {
            std::path::Path::new(&t.path).parent() == Some(path.as_path())
        }).cloned().collect();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Folders: back-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_back_clicked(move || {
        let mut s = state.lock();
        s.folder_view_path = None;
        s.folder_tracks.clear();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Folders: add-folder-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_add_folder_clicked(move || {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let mut s = state.lock();
            s.add_music_folder(&path.to_string_lossy());
            if let Some(ui) = weak.upgrade() {
                s.sync_to_slint(&ui);
            }
        }
    });

    // ── Dialogs: create-playlist ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_create_playlist(move |name| {
        let mut s = state.lock();
        if !name.is_empty() {
            s.create_playlist(&name);
        }
        s.show_create_playlist_dialog = false;
        s.new_playlist_name.clear();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Dialogs: cancel-create-playlist ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_cancel_create_playlist(move || {
        let mut s = state.lock();
        s.show_create_playlist_dialog = false;
        s.new_playlist_name.clear();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Dialogs: add-to-playlist ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_add_to_playlist(move |playlist_id, track_id| {
        let mut s = state.lock();
        s.add_track_to_playlist(playlist_id as i64, track_id as i64);
        s.show_add_to_playlist_dialog = None;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Dialogs: cancel-add-to-playlist ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_cancel_add_to_playlist(move || {
        let mut s = state.lock();
        s.show_add_to_playlist_dialog = None;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Dialogs: open-create-from-add ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_open_create_from_add(move || {
        let mut s = state.lock();
        s.show_add_to_playlist_dialog = None;
        s.show_create_playlist_dialog = true;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Dialogs: close-track-info ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_close_track_info(move || {
        let mut s = state.lock();
        s.show_track_info_dialog = None;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Sidebar: create-playlist-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_create_playlist_clicked(move || {
        let mut s = state.lock();
        s.show_create_playlist_dialog = true;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Sidebar: playlist-clicked ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_playlist_clicked(move |playlist_id| {
        let mut s = state.lock();
        s.selected_playlist_id = Some(playlist_id as i64);
        // For now, just navigate to All Tracks and filter (playlist filtering
        // is a future enhancement — the egui version also struggled with this).
        s.nav = NavSection::AllTracks;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: track-double-clicked (show info) ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_track_double_clicked(move |track_id| {
        let mut s = state.lock();
        s.show_track_info_dialog = Some(track_id as i64);
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: sort-by ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_sort_by(move |field| {
        let mut s = state.lock();
        s.sort_active = true;
        // Toggle direction only when re-clicking the same column;
        // otherwise default to ascending for the new column.
        if s.sort_field == field.as_str() {
            s.sort_ascending = !s.sort_ascending;
        } else {
            s.sort_field = field.to_string();
            s.sort_ascending = true;
        }
        let ascending = s.sort_ascending;
        // Apply sort to tracks (basic implementation).
        match field.as_str() {
            "title" => s.tracks.sort_by(|a, b| a.title.cmp(&b.title)),
            "artist" => s.tracks.sort_by(|a, b| a.artist.cmp(&b.artist)),
            "album" => s.tracks.sort_by(|a, b| a.album.cmp(&b.album)),
            "duration" => s.tracks.sort_by(|a, b| a.duration_secs.partial_cmp(&b.duration_secs).unwrap_or(std::cmp::Ordering::Equal)),
            _ => {}
        }
        if !ascending {
            s.tracks.reverse();
        }
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: page-changed ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_page_changed(move |new_page| {
        let mut s = state.lock();
        // Compute the highest valid page index. For 0 tracks we stay on
        // page 0; otherwise the last page holds tracks [(N-1)/per_page]*per_page
        // through N-1, so max_page = (N-1) / per_page. The previous formula
        // (N / per_page).saturating_sub(0) let the user navigate one page
        // past the end whenever N was an exact multiple of per_page.
        let max_page: usize = if s.total_track_count == 0 {
            0
        } else {
            (s.total_track_count - 1) / s.tracks_per_page
        };
        let new_page = new_page.max(0).min(max_page as i32) as usize;
        // Bug fix: previously this only updated the UI-side `track_page`
        // mirror without telling LibraryService to actually fetch the new
        // page from the DB, so `s.tracks` (and therefore the rendered rows)
        // never changed when paging — only the "Page X of Y" label moved.
        // LibraryService pages by ±1 (next_page/prev_page), so step it the
        // required number of times and then pull the resulting tracks.
        while s.track_page < new_page {
            s.ctx.library.next_page();
            s.track_page += 1;
        }
        while s.track_page > new_page {
            s.ctx.library.prev_page();
            s.track_page -= 1;
        }
        s.refresh_tracks();
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: toggle-list-view ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_list_view(move || {
        let mut s = state.lock();
        s.list_view = !s.list_view;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Track list: toggle-favorite-filter ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_favorite_filter(move || {
        let mut s = state.lock();
        s.filter_favorites = !s.filter_favorites;
        if s.filter_favorites {
            // Clone the favorite IDs first to avoid borrow conflict with s.tracks.
            let fav_ids = s.cached_favorite_ids.clone();
            s.tracks.retain(|t| fav_ids.contains(&t.id));
        } else {
            s.refresh_tracks();
        }
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });

    // ── Lyrics: toggle ──
    let weak = slint_app.as_weak();
    let state = Arc::clone(app_state);
    slint_app.on_toggle_lyrics(move || {
        // Bug fix: this previously flipped `config.lyrics.enabled` (the
        // global "fetch lyrics at all" setting, also editable from the
        // Settings page) instead of opening/closing the lyrics panel —
        // so the player-bar button silently disabled lyrics fetching
        // rather than showing the panel.
        let mut s = state.lock();
        s.show_lyrics_panel = !s.show_lyrics_panel;
        if let Some(ui) = weak.upgrade() {
            s.sync_to_slint(&ui);
        }
    });
}
