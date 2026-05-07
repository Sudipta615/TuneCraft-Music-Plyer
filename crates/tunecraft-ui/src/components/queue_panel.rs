//! Queue panel component.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::AppState;
use crate::i18n::tr;

/// Queue panel overlay component.
pub fn QueuePanel() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let signals: ReactivitySignals = use_context();
    // Issue #5: Subscribe to queue signal for queue state changes
    let _ = *signals.queue.read();
    let _ = *signals.playback.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);

    let queue_tracks: Vec<(usize, String, String, String)> = {
        let queue = state.read().queue_lock();
        queue
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let title = t.title.clone().unwrap_or_else(|| "Unknown".into());
                let artist = t.artist.clone().unwrap_or_else(|| "Unknown".into());
                let duration = t
                    .duration
                    .map(|d| format!("{}:{:02}", d / 60, d % 60))
                    .unwrap_or_else(|| "--:--".into());
                (i, title, artist, duration)
            })
            .collect()
    };
    let current_index = state.read().queue_lock().current_index;
    // Bug #33 fix: When shuffle is enabled, current_index is a logical index
    // that maps to a physical index via shuffle_order. Comparing it directly
    // with the physical index from enumerate() highlights the wrong track.
    let shuffle_on = state.read().queue_lock().shuffle;
    let shuffle_order = state.read().queue_lock().shuffle_order.clone();
    let physical_current = current_index.and_then(|logical| {
        if shuffle_on && logical < shuffle_order.len() {
            Some(shuffle_order[logical])
        } else {
            current_index
        }
    });

    rsx! {
        div { class: "overlay-panel queue-panel",
            class: if dark { "dark" } else { "light" },
            role: "dialog",
            aria_label: "Queue panel",

            div { class: "panel-header",
                h3 { "Queue" }
                div { class: "panel-header-actions",
                    button {
                        class: "panel-action-btn",
                        // Issue #6: Accessibility
                        aria_label: "Clear all tracks from queue",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            // Fix H13: Stop the audio engine when clearing the queue.
                            // Previously, clearing the queue left the engine playing
                            // with no corresponding queue entry, causing stale playback.
                            if let Ok(engine) = s.engine.lock() {
                                if let Some(ref e) = *engine {
                                    let _ = e.stop();
                                }
                            }
                            *s.player_state.lock().unwrap_or_else(|e| e.into_inner()) = tunecraft_core::audio::PlayerState::Stopped;
                            let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                            queue.tracks.clear();
                            queue.current_index = None;
                            queue.shuffle_order.clear();
                            // Issue #5: Bump signals after queue clear
                            drop(queue);
                            let gen = *signals.queue.read();
                            signals.queue.set(gen.wrapping_add(1));
                            let gen = *signals.playback.read();
                            signals.playback.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                if let Ok(engine) = s.engine.lock() {
                                    if let Some(ref e) = *engine {
                                        let _ = e.stop();
                                    }
                                }
                                *s.player_state.lock().unwrap_or_else(|e| e.into_inner()) = tunecraft_core::audio::PlayerState::Stopped;
                                let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                                queue.tracks.clear();
                                queue.current_index = None;
                                queue.shuffle_order.clear();
                                drop(queue);
                                let gen = *signals.queue.read();
                                signals.queue.set(gen.wrapping_add(1));
                                let gen = *signals.playback.read();
                                signals.playback.set(gen.wrapping_add(1));
                            }
                        },
                        "Clear All"
                    }
                    button {
                        class: "panel-close-btn",
                        // Issue #6: Accessibility
                        aria_label: "Close queue panel",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            s.queue_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            // Issue #5: Bump UI signal after panel close
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                s.queue_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        "✕"
                    }
                }
            }

            div {
                class: "queue-list",
                role: "list",
                aria_label: "Queue tracks",

                for (idx, title, artist, duration) in queue_tracks.iter() {
                    {
                        let is_current = physical_current == Some(*idx);
                        let idx_for_closure = *idx;
                        rsx! {
                            div {
                                class: if is_current { "queue-item current" } else { "queue-item" },
                                key: "{idx}",
                                // Issue #6: Accessibility
                                role: "listitem",
                                tabindex: "0",
                                aria_label: "{title} by {artist}, {duration}",

                                span { class: "queue-item-num",
                                    if is_current { "▶" } else { "{idx + 1}" }
                                }
                                div { class: "queue-item-info",
                                    div { class: "queue-item-title", "{title}" }
                                    div { class: "queue-item-artist", "{artist}" }
                                }
                                span { class: "queue-item-duration", "{duration}" }
                                button {
                                    class: "queue-item-remove",
                                    // Issue #6: Accessibility
                                    aria_label: "Remove {title} from queue",
                                    tabindex: "-1",
                                    onclick: move |_| {
                                        let s = state.read().clone();
                                        let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                                        if idx_for_closure < queue.tracks.len() {
                                            queue.tracks.remove(idx_for_closure);
                                            // #5: Use preserving variant so we don't jump to a random track
                                            if queue.shuffle {
                                                queue.regenerate_shuffle_order_preserving_current();
                                            }
                                            // Adjust current_index
                                            if let Some(cur) = queue.current_index {
                                                if idx_for_closure < cur {
                                                    queue.current_index = Some(cur - 1);
                                                } else if idx_for_closure == cur {
                                                    // Current track was removed: advance or stop
                                                    if queue.tracks.is_empty() {
                                                        queue.current_index = None;
                                                    } else if cur < queue.tracks.len() {
                                                        queue.current_index = Some(cur);
                                                    } else {
                                                        queue.current_index = Some(0);
                                                    }
                                                    drop(queue);
                                                    // Reload and play next track or stop
                                                    let queue2 = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                                                    if let Some(idx) = queue2.current_index {
                                                        if let Some(effective) = queue2.effective_index(idx) {
                                                            if let Some(t) = queue2.tracks.get(effective) {
                                                                let path = std::path::PathBuf::from(&t.file_path);
                                                                drop(queue2);
                                                                if let Ok(engine) = s.engine.lock() {
                                                                    if let Some(ref e) = *engine {
                                                                        if let Err(err) = e.load(&path) {
                                                                            tracing::error!("Failed to load track after queue removal: {}", err);
                                                                        } else {
                                                                            let _ = e.play();
                                                                            *s.player_state.lock().unwrap_or_else(|e| e.into_inner()) = tunecraft_core::audio::PlayerState::Playing;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // Issue #5: Bump signals after queue removal
                                        let gen = *signals.queue.read();
                                        signals.queue.set(gen.wrapping_add(1));
                                        let gen = *signals.playback.read();
                                        signals.playback.set(gen.wrapping_add(1));
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
