//! Keyboard shortcut definitions and matching logic
//!
//! Contains [`KeyboardShortcut`] implementation, default shortcuts,
//! and shortcut registration/matching methods on [`PlatformIntegration`].

use crate::types::{KeyboardShortcut, MediaKeyAction};
use crate::PlatformIntegration;

impl KeyboardShortcut {
    /// Create a new keyboard shortcut
    pub fn new(key: &str, action: MediaKeyAction) -> Self {
        Self {
            key: key.to_string(),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            action,
        }
    }

    /// Add Ctrl modifier
    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Add Alt modifier
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Add Shift modifier
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Add Meta/Super modifier
    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }

    /// Get a display string for the shortcut
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push("Meta");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

/// Get the default keyboard shortcuts for TuneCraft
pub fn default_shortcuts() -> Vec<KeyboardShortcut> {
    vec![
        KeyboardShortcut::new("Space", MediaKeyAction::PlayPause),
        KeyboardShortcut::new("Right", MediaKeyAction::Next).ctrl(),
        KeyboardShortcut::new("Left", MediaKeyAction::Previous).ctrl(),
        KeyboardShortcut::new("Up", MediaKeyAction::VolumeUp),
        KeyboardShortcut::new("Down", MediaKeyAction::VolumeDown),
        KeyboardShortcut::new("M", MediaKeyAction::Mute).ctrl(),
        KeyboardShortcut::new("S", MediaKeyAction::Stop).ctrl(),
    ]
}

impl PlatformIntegration {
    /// Get the list of registered keyboard shortcuts
    pub fn shortcuts(&self) -> &[KeyboardShortcut] {
        &self.shortcuts
    }

    /// Register a custom keyboard shortcut.
    ///
    ///
    /// exists, it is replaced instead of creating a duplicate.
    pub fn add_shortcut(&mut self, shortcut: KeyboardShortcut) {
        let existing = self.shortcuts.iter().position(|s| {
            s.key.eq_ignore_ascii_case(&shortcut.key)
                && s.ctrl == shortcut.ctrl
                && s.alt == shortcut.alt
                && s.shift == shortcut.shift
                && s.meta == shortcut.meta
        });
        if let Some(idx) = existing {
            self.shortcuts[idx] = shortcut;
        } else {
            self.shortcuts.push(shortcut);
        }
    }

    /// Process a keyboard event and return the matching action, if any.
    pub fn process_key_event(
        &self,
        key: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) -> Option<MediaKeyAction> {
        for shortcut in &self.shortcuts {
            if shortcut.key.eq_ignore_ascii_case(key)
                && shortcut.ctrl == ctrl
                && shortcut.alt == alt
                && shortcut.shift == shift
                && shortcut.meta == meta
            {
                return Some(shortcut.action.clone());
            }
        }
        None
    }
}
