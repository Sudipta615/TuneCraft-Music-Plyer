//! TuneCraft UI — egui/eframe-based graphical interface
//!
//! A clean, elegant music player UI fully wired to the audio engine:
//! - Dark/light theme with purple accent (#4231F1)
//! - Sidebar navigation (Library, Playlists, Mood, Settings)
//! - Track list view with real database tracks
//! - Bottom player bar with transport controls connected to AudioEngine
//! - 10-band parametric EQ panel connected to DspPipeline
//! - Lyrics display from LRCLIB
//! - Local offline scrobbling (SQLite play journal)
//! - Platform media key support
//!
//! ## Architecture (v0.8.0)
//!
//! The UI uses a **service-layer pattern** where backend subsystems are
//! encapsulated behind typed service objects. The UI never directly
//! accesses `Arc<Mutex<Backend>>` — instead, it calls service methods
//! that handle synchronization internally.
//!
//! ```text
//! TuneCraftApp (UI state only)
//!   └── AppContext (service registry)
//!       ├── PlaybackService  → EngineHandle (channel + RwLock)
//!       ├── LibraryService   → Database (snapshot-based reads)
//!       ├── EqService        → EngineHandle (channel)
//!       ├── ScrobbleService  → SQLite (local play journal)
//!       ├── LyricsService    → LyricsClient (async + result sink)
//!       ├── ConfigService    → RwLock<AppConfig>
//!       └── PlatformService  → PlatformIntegration (RefCell)
//! ```

pub mod app;
pub mod eq_panel;
pub mod folders_view;
pub mod player_bar;
pub mod services;
pub mod settings_view;
pub mod sidebar;
pub mod theme;
pub mod track_list;

pub use app::{run, AppContext, TuneCraftApp};
