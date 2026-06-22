//! Service layer — encapsulates backend subsystems behind clean APIs
//!
//! Each service owns its backend resource and exposes a focused API.
//! The UI never directly accesses `Arc<Mutex<>>` or `Arc<RwLock<>>` handles;
//! instead, it calls service methods that handle synchronization internally.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────┐    method calls    ┌─────────────────┐    commands     ┌──────────┐
//! │  tc-ui   │ ─────────────────► │  Service Layer  │ ──────────────► │tc-engine │
//! │(egui)    │ ◄──────────────── │  (this module)  │ ◄────────────── │(audio)   │
//! └──────────┘    return values   └─────────────────┘    state       └──────────┘
//!                                       │     │
//!                                  ┌────┘     └────┐
//!                                  ▼               ▼
//!                               tc-db          tc-platform
//! ```
//!
//! ## Services
//!
//! - **PlaybackService** — audio playback, queue management, shuffle/repeat (v0.8.3: thread-safe,
//!   single source of truth for queue)
//! - **LibraryService** — track queries, scan management, playlist CRUD
//! - **EqService** — equalizer state and DSP pipeline parameters
//! - **ScrobbleService** — Local offline scrobbling (play journal)
//! - **ConfigService** — configuration persistence and dirty tracking
//! - **PlatformService** — MPRIS status, media keys, OS integration
//! - **LyricsService** — Synced lyrics via LRCLIB (v3.0.0). Network access
//!   runs on a dedicated tokio runtime; results are cached in the `tracks`
//!   table so subsequent plays never hit the network.

pub mod config;
pub mod eq;
pub mod library;
pub mod lyrics;
pub mod platform;
pub mod playback;
pub mod scrobble;

pub use config::ConfigService;
pub use eq::EqService;
pub use library::LibraryService;
pub use lyrics::LyricsService;
pub use platform::PlatformService;
pub use playback::PlaybackService;
pub use scrobble::ScrobbleService;
