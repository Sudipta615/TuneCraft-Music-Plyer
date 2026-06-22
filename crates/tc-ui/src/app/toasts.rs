//! Toast notification system for TuneCraft.
//!
//! Toast storage and expiry logic is preserved from the egui version.
//! Rendering is now done by the Slint `Toasts` component; this module
//! just manages the in-memory toast queue.

/// Severity level for toast notifications.
#[derive(Clone, PartialEq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

use super::TuneCraftApp;

impl TuneCraftApp {
    pub fn push_toast(&mut self, message: impl Into<String>, level: ToastLevel) {
        self.push_toast_with_id(message, level, 0)
    }

    /// Push a toast with a unique identifier.
    pub fn push_toast_with_id(&mut self, message: impl Into<String>, level: ToastLevel, id: u64) {
        let expiry = std::time::Instant::now() + std::time::Duration::from_secs(4);
        self.toasts.push((message.into(), expiry, level, id));
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }
}
