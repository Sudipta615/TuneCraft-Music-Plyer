//! Notifications panel component.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::AppState;

/// Notifications panel overlay component.
pub fn NotificationsPanel() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let mut signals: ReactivitySignals = use_context();
    let _ = *signals.ui.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);

    let notifs: Vec<(String, String, String, String)> = {
        let state_ref = state.read();
        let notifications = state_ref
            .notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        notifications
            .iter()
            .map(|n| {
                (
                    n.title.clone(),
                    n.body.clone(),
                    n.timestamp.clone(),
                    n.unique_id.clone(),
                )
            })
            .collect()
    };

    rsx! {
        div { class: "overlay-panel notifications-panel",
            class: if dark { "dark" } else { "light" },
            role: "dialog",
            aria_label: "Notifications panel",

            div { class: "panel-header",
                h3 { "Notifications" }
                button {
                    class: "panel-close-btn",
                    aria_label: "Close notifications panel",
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        s.notifications_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            s.notifications_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "✕"
                }
            }

            div {
                class: "notifications-list",
                role: "list",
                aria_label: "Notifications list",

                if notifs.is_empty() {
                    div { class: "notifications-empty", "No notifications" }
                }
                for (title, body, timestamp, unique_id) in notifs.iter() {
                    div {
                        class: "notification-item",
                        key: "{unique_id}",
                        role: "listitem",
                        aria_label: "{title}: {body} at {timestamp}",
                        div { class: "notification-title", "{title}" }
                        div { class: "notification-body", "{body}" }
                        div { class: "notification-time", "{timestamp}" }
                    }
                }
            }
        }
    }
}
