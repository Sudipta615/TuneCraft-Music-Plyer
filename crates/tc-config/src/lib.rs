//! TuneCraft Configuration (`tc-config`)
//!
//! Provides TOML-based configuration with validation, migration, atomic
//! persistence, and change notifications.
//!
//! # Quick Start
//!
//! ```no_run
//! use tc_config::ConfigPersistence;
//!
//! // Load config (falls back to defaults on any error)
//! let config = ConfigPersistence::load_or_default();
//!
//! // Save config atomically
//! ConfigPersistence::save(&config).unwrap();
//! ```
//!
//! # Key Types
//!
//! - [`AppConfig`] — Root configuration struct
//! - [`ConfigPersistence`] — Load/save/subscribe API
//! - [`ConfigError`] — Errors from load/save operations
//! - [`ConfigChangedEvent`] / [`ConfigSection`] — Change notification types

pub mod persistence;
pub mod types;

// Re-export the primary persistence API
pub use persistence::{ConfigPersistence, ConfigError};

// Re-export all config types (AppConfig, EngineConfig, LibraryConfig, etc.)
pub use types::*;

