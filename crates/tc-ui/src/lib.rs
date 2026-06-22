//! TuneCraft UI — Slint 1.16.1-based graphical interface
//!
//! A clean, polished music player UI fully wired to the audio engine:
//! - Dark/light theme with cyan accent (#35C8E1) plus 8 chromatic themes
//! - Sidebar navigation (Library, Playlists, Settings)
//! - Track list view with real database tracks
//! - Bottom player bar with transport controls connected to AudioEngine
//! - 10-band parametric EQ panel connected to DspPipeline
//! - LRCLIB synced-lyrics integration (v3.0.0) — see `services::lyrics`
//! - Local offline scrobbling (SQLite play journal)
//! - Platform media key support
//!
//! ## Architecture (v3.1.5 — Slint migration)
//!
//! The UI uses Slint 1.16.1 with the femtovg (GPU) renderer. Slint's
//! retained-mode declarative model eliminates the per-frame repaint loop
//! that caused CPU spikes on low-end hardware with egui.
//!
//! ```text
//! Slint App component (declarative .slint markup)
//!   ▲                            │
//!   │ set_property / model       │ callback handlers
//!   │                            ▼
//! AppController (Rust glue)
//!   ├── TuneCraftApp (UI state holder, identical to egui version)
//!   └── AppContext (service registry — unchanged)
//!       ├── PlaybackService  → EngineHandle
//!       ├── LibraryService   → Database
//!       ├── EqService        → EngineHandle
//!       ├── ScrobbleService  → SQLite
//!       ├── LyricsService    → LRCLIB HTTP client
//!       ├── ConfigService    → RwLock<AppConfig>
//!       └── PlatformService  → PlatformIntegration
//! ```
//!
//! The `AppController` runs a 200ms timer that syncs state from services
//! into Slint properties. Callbacks from Slint are dispatched synchronously
//! to the existing `*_actions` methods on `TuneCraftApp`.

pub mod app;
pub mod converters;
pub mod eq_panel;
pub mod folders_view;
pub mod player_bar;
pub mod services;
pub mod settings_view;
pub mod sidebar;
pub mod theme;
pub mod track_list;

// Re-export the Slint-generated App component. The name `App` matches the
// `export component App inherits Window` in ui/app.slint. Slint generates
// a Rust type `App` with `set_<property>()`, `get_<property>()`,
// `on_<callback>()`, and `invoke_<callback>()` methods.
slint::include_modules!();

pub use app::{run, AppContext, TuneCraftApp};
