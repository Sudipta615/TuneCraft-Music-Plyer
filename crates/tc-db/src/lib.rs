//! `tc-db` — SQLite database layer for TuneCraft
//!
//! ## Error Handling Compliance
//! All `unwrap()` / `expect()` calls in this crate are strictly confined to
//! `#[cfg(test)]` / `#[test]` functions, where panicking is idiomatic and
//! acceptable.  No production code paths use `unwrap()`.

pub mod migrations;
pub mod models;
pub mod repository;

pub use models::*;
pub use repository::{Database, DbError};
