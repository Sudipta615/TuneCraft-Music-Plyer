//! i18n wrapper for the UI crate.
 them and adds a convenience
//! `t!()` macro that calls `tr()` at the call site.

/// Re-export the core i18n initialization function.
pub use tunecraft_core::util::i18n::init_i18n;

/// Re-export the core translation function.
pub use tunecraft_core::util::i18n::tr;

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
