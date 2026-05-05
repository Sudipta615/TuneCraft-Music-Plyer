//! Internationalization (i18n) support for Tunecraft.
//!
//! This module provides the `tr!()` macro for marking translatable strings
//! and the `init_i18n()` function for initializing gettext at startup.
//!
//! # Usage
//!
//! ```rust,ignore
//! use tunecraft_core::util::i18n::{tr, init_i18n};
//!
//! // Call once at startup (before any UI code)
//! init_i18n();
//!
//! // Mark strings for translation
//! let label = tr!("Search songs, artists, albums...");
//! ```

use gettextrs::{gettext, bindtextdomain, bind_textdomain_codeset, textdomain};
use std::path::PathBuf;

/// Initialize the i18n system.
///
/// Sets up gettext with the "tunecraft" text domain, binding to the
/// appropriate locale directory. On Linux, this typically resolves to
/// `/usr/share/locale`. For development builds, it checks alongside
/// the binary and in the project's `po` directory.
///
/// If the locale directory cannot be found, the function falls back to
/// the current locale (which means strings remain in English).
pub fn init_i18n() {
    let domain = "tunecraft";

    // Fix M10: Build platform-appropriate locale search paths.
    // Previously hardcoded to Linux paths only; now we include macOS and
    // Windows paths, and use `directories::ProjectDirs` for a portable
    // data directory.
    let mut locale_dirs: Vec<String> = Vec::new();

    // Platform-specific data dir from directories crate
    if let Some(proj_dirs) = directories::ProjectDirs::from("org", "Tunecraft", "Tunecraft") {
        let data_dir = proj_dirs.data_dir().join("locale");
        locale_dirs.push(data_dir.to_string_lossy().to_string());
    }

    // macOS: Homebrew and local installs
    #[cfg(target_os = "macos")]
    {
        locale_dirs.push("/usr/local/share/locale".to_string());
        // App bundle Resources path
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                // Typical macOS app bundle: .../Tunecraft.app/Contents/MacOS/
                // Locale data lives in .../Tunecraft.app/Contents/Resources/share/locale/
                if let Some(resources) = exe_dir.parent() {
                    locale_dirs.push(resources.join("Resources/share/locale").to_string_lossy().to_string());
                }
            }
        }
    }

    // Windows: executable directory's share/locale
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                locale_dirs.push(exe_dir.join("share/locale").to_string_lossy().to_string());
            }
        }
    }

    // Linux: standard system locale paths
    #[cfg(target_os = "linux")]
    {
        locale_dirs.push("/usr/share/locale".to_string());
        locale_dirs.push("/usr/local/share/locale".to_string());
    }

    // Development/build directory (CARGO_MANIFEST_DIR/po)
    locale_dirs.push(format!("{}/po", std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()));

    // Add the po/ directory relative to the running executable.
    // This ensures that development builds (where the po/ folder lives
    // alongside the binary in the target directory) find translations
    // without requiring a system-wide installation.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            locale_dirs.push(exe_dir.join("po").to_string_lossy().to_string());
        }
    }

    let mut found_dir = false;
    for dir in &locale_dirs {
        let path = PathBuf::from(dir);
        if path.exists() {
            bindtextdomain(domain, dir).ok();
            found_dir = true;
            break;
        }
    }

    if !found_dir {
        tracing::warn!(
            "i18n: no locale directory found among {:?} — translations will not be available (falling back to English)",
            locale_dirs
        );
    }

    bind_textdomain_codeset(domain, "UTF-8").ok();
    textdomain(domain).ok();

    tracing::debug!("i18n initialized with domain '{}'", domain);
}

/// Translate a string using gettext.
///
/// This is the primary i18n function. It looks up the string in the
/// current locale's message catalog and returns the translated version.
/// If no translation is found, returns the original English string.
///
/// # Arguments
///
/// * `msgid` - The English string to translate
///
/// # Returns
///
/// The translated string, or the original if no translation exists.
pub fn tr(msgid: &str) -> String {
    gettext(msgid)
}

/// Translate a string with plural forms.
///
/// Uses ngettext to select the correct plural form based on the count.
pub fn tr_n(msgid_singular: &str, msgid_plural: &str, n: u32) -> String {
    gettextrs::ngettext(msgid_singular, msgid_plural, n)
}

/// Mark a string for translation without translating at runtime.
///
/// Use this for strings that should be extracted by xgettext but
/// translated at a later point (e.g. in UI code that initializes
/// after i18n is set up).
pub fn tr_mark(msgid: &str) -> &str {
    msgid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tr_returns_original_for_english() {
        // Without any translation catalogs, gettext returns the original string
        let result = tr("Hello, World!");
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_tr_n_english_singular() {
        let result = tr_n("1 track", "{} tracks", 1);
        assert_eq!(result, "1 track");
    }

    #[test]
    fn test_tr_n_english_plural() {
        let result = tr_n("1 track", "{} tracks", 5);
        // English plural for n != 1 returns the plural form
        assert!(!result.is_empty());
    }

    #[test]
    fn test_tr_mark_is_passthrough() {
        assert_eq!(tr_mark("Search"), "Search");
    }

    #[test]
    fn test_init_i18n_does_not_panic() {
        // Should be safe to call multiple times
        init_i18n();
        init_i18n();
    }
}
