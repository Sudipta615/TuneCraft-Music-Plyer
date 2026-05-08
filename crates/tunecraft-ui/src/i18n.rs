//! i18n wrapper for the UI crate.
//!
//! Issue #17: Re-exports the core i18n translation function and macro
//! so that UI components can use `t!()` for translatable strings.
//!
//! The core crate provides `tunecraft_core::util::i18n::tr()` and
//! `init_i18n()`. This module re-exports them and adds a convenience
//! `t!()` macro that calls `tr()` at the call site.

/// Re-export the core i18n initialization function.
pub use tunecraft_core::util::i18n::init_i18n;

/// Re-export the core translation function.
pub use tunecraft_core::util::i18n::tr;

/// Re-export the plural translation function.

/// Translate a string using gettext.
///
/// This macro wraps `tr()` for ergonomic use in UI code.
/// It marks the string for translation and returns the translated
/// version based on the current locale.
///
/// # Usage
///
/// ```rust,ignore
/// use crate::i18n::t;
///
/// let label = t!("sidebar.library");
/// let placeholder = t!("topbar.search_placeholder");
/// ```
#[macro_export]
macro_rules! t {
    ($msgid:expr) => {
        $crate::i18n::tr($msgid)
    };
}
