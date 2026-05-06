//! Sidebar navigation component.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, ViewType};
use crate::i18n::tr;

/// Sidebar navigation component.
pub fn Sidebar() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let signals: ReactivitySignals = use_context();
    // Issue #5: Subscribe to library and UI signals
    let _ = *signals.library.read();
    let _ = *signals.ui.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);
    let collapsed = state
        .read()
        .sidebar_collapsed
        .load(std::sync::atomic::Ordering::Relaxed);
    let current_view = state
        .read()
        .current_view
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let track_count = state.read().track_count();

    // #8: Use cached album and artist counts (Fix H9)
    // Previously queried the DB on every render (every 250ms).
    // The cache is refreshed when the database is opened or a scan completes.
    if !state
        .read()
        .sidebar_cache_valid
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        state.read().refresh_sidebar_cache();
    }
    let album_count = state
        .read()
        .sidebar_album_count
        .load(std::sync::atomic::Ordering::Relaxed) as i64;
    let artist_count = state
        .read()
        .sidebar_artist_count
        .load(std::sync::atomic::Ordering::Relaxed) as i64;

    // #16: Use cached playlist count (Fix H9)
    let playlist_count = state
        .read()
        .sidebar_playlist_count
        .load(std::sync::atomic::Ordering::Relaxed) as i64;

    // #7: Compute active state for mood-based sidebar items
    let is_favorites_active = current_view == ViewType::Mood("favorites".into());
    let is_recent_active = current_view == ViewType::Mood("recent".into());
    let is_most_played_active = current_view == ViewType::Mood("most".into());

    let mood_items = vec![
        (tr("Dance"), "dance", "#ef4444"),
        (tr("Romantic"), "romantic", "#8b5cf6"),
        (tr("Sad"), "sad", "#3b82f6"),
        (tr("Sufi"), "sufi", "#f97316"),
        (tr("Chill"), "chill", "#22c55e"),
    ];

    rsx! {
        aside {
            class: if collapsed { "sidebar collapsed" } else { "sidebar" },
            class: if dark { "dark" } else { "light" },
            // Issue #6: Accessibility
            role: "navigation",
            aria_label: "Main navigation",

            // Logo
            div { class: "sidebar-logo",
                if !collapsed {
                    span { class: "logo-text", "TuneCraft" }
                }
                button {
                    class: "sidebar-toggle-btn",
                    // Issue #6: Accessibility
                    aria_label: if collapsed { "Expand sidebar" } else { "Collapse sidebar" },
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let c = s.sidebar_collapsed.load(std::sync::atomic::Ordering::Relaxed);
                        s.sidebar_collapsed.store(!c, std::sync::atomic::Ordering::Relaxed);
                        // Issue #5: Bump UI signal after sidebar toggle
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Space {
                            let s = state.read().clone();
                            let c = s.sidebar_collapsed.load(std::sync::atomic::Ordering::Relaxed);
                            s.sidebar_collapsed.store(!c, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    if collapsed { "☰" } else { "◀" }
                }
            }

            if !collapsed {
                // LIBRARY section
                div {
                    class: "sidebar-section",
                    // Issue #6: Accessibility
                    aria_label: "Library",

                    div { class: "sidebar-section-header", "{tr(\"LIBRARY\")}" }
                    {sidebar_nav_item(&tr("All Tracks"), "♫", track_count, current_view == ViewType::AllTracks, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            // Bug #4 fix: Close filter/EQ panels when navigating
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::AllTracks;
                            // Issue #5: Bump signals
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                    {sidebar_nav_item(&tr("Albums"), "💿", album_count, current_view == ViewType::Albums, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Albums;
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                    {sidebar_nav_item(&tr("Artists"), "👤", artist_count, current_view == ViewType::Artists, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Artists;
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                    // #16: Add Playlists nav item so the view is reachable
                    {sidebar_nav_item(&tr("Playlists"), "♬", playlist_count, current_view == ViewType::Playlists, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Playlists;
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                }

                // PLAYLISTS section
                div {
                    class: "sidebar-section",
                    // Issue #6: Accessibility
                    aria_label: "Playlists",

                    div { class: "sidebar-section-header", "{tr(\"PLAYLISTS\")}" }
                    // #7: Pass correct active state instead of hardcoded false
                    {sidebar_nav_item(&tr("Favorites"), "★", 0, is_favorites_active, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Mood("favorites".into());
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                    {sidebar_nav_item(&tr("Recently Played"), "🕐", 0, is_recent_active, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Mood("recent".into());
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                    {sidebar_nav_item(&tr("Most Played"), "📊", 0, is_most_played_active, dark, {
                        let state = state;
                        let signals = signals;
                        move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Mood("most".into());
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        }
                    })}
                }

                // MOOD section
                div {
                    class: "sidebar-section",
                    // Issue #6: Accessibility
                    aria_label: "Mood playlists",

                    div { class: "sidebar-section-header", "{tr(\"MOOD\")}" }
                    for (name, mood_key, color) in mood_items {
                        {
                            let count = state.read().get_mood_track_count(mood_key);
                            let is_active = current_view == ViewType::Mood(mood_key.to_string());
                            let mood_key_owned = mood_key.to_string();
                            let state_clone = state;
                            let signals_clone = signals;
                            rsx! {
                                button {
                                    class: if is_active { "sidebar-item active" } else { "sidebar-item" },
                                    key: "{mood_key}",
                                    // Issue #6: Accessibility
                                    aria_label: "{name} playlist, {count} tracks",
                                    tabindex: "0",
                                    onclick: move |_| {
                                        let s = state_clone.read().clone();
                                        *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Mood(mood_key_owned.clone());
                                        // Issue #5: Bump signals
                                        let gen = *signals_clone.library.read();
                                        signals_clone.library.set(gen.wrapping_add(1));
                                        let gen = *signals_clone.ui.read();
                                        signals_clone.ui.set(gen.wrapping_add(1));
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter || e.key() == Key::Space {
                                            let s = state_clone.read().clone();
                                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Mood(mood_key_owned.clone());
                                            let gen = *signals_clone.library.read();
                                            signals_clone.library.set(gen.wrapping_add(1));
                                            let gen = *signals_clone.ui.read();
                                            signals_clone.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                    span { class: "sidebar-item-icon", "{name.chars().next().unwrap_or('?')}" }
                                    span { class: "sidebar-item-text", "{name}" }
                                    span {
                                        class: "mood-badge",
                                        style: "background-color: {color}",
                                        "{count}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Settings
                div { class: "sidebar-section sidebar-bottom",
                    button {
                        class: if current_view == ViewType::Settings { "sidebar-item active" } else { "sidebar-item" },
                        // Issue #6: Accessibility
                        aria_label: "Settings",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Settings;
                            // Issue #5: Bump UI signal
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Space {
                                let s = state.read().clone();
                                s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                                s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                                *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Settings;
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        span { class: "sidebar-item-icon", "⚙" }
                        span { class: "sidebar-item-text", "{tr(\"Settings\")}" }
                    }
                }
            }
        }
    }
}

/// Helper: render a sidebar navigation item.
fn sidebar_nav_item(
    label: &str,
    icon: &str,
    count: i64,
    active: bool,
    _dark: bool,
    onclick: impl FnMut(Event<MouseData>) + 'static,
) -> Element {
    rsx! {
        button {
            class: if active { "sidebar-item active" } else { "sidebar-item" },
            onclick: onclick,
            // Issue #6: Accessibility
            aria_label: "{label}, {count} items",
            tabindex: "0",
            span { class: "sidebar-item-icon", "{icon}" }
            span { class: "sidebar-item-text", "{label}" }
            if count > 0 {
                span { class: "sidebar-badge", "{count}" }
            }
        }
    }
}
