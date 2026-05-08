//! Top bar component with search, notifications, theme toggle, and add music button.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, ViewType};
use crate::i18n::tr;

const SEARCH_DEBOUNCE_MS: u64 = 300;

/// Top bar component.
pub fn TopBar() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let mut signals: ReactivitySignals = use_context();
    let _ = *signals.ui.read();
    let _ = *signals.playback.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);
    let search_query = state
        .read()
        .search_query
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let notification_count = state
        .read()
        .notification_count
        .load(std::sync::atomic::Ordering::Relaxed);

    let mut search_value = use_signal(|| search_query.clone());

    let mut last_keystroke: Signal<Option<std::time::Instant>> = use_signal(|| None);

    rsx! {
        div { class: if dark { "topbar dark" } else { "topbar light" },
            role: "banner",
            aria_label: "Top bar",

            div { class: "search-bar",
                span { class: "search-icon", "🔍" }
                input {
                    r#type: "text",
                    class: "search-input",
                    aria_label: "{tr(\"Search songs, artists, albums\")}",
                    placeholder: "{tr(\"Search songs, artists, albums...\")}",
                    value: "{search_value}",
                    oninput: move |e| {
                        let new_value = e.value().clone();
                        search_value.set(new_value.clone());
                        last_keystroke.set(Some(std::time::Instant::now()));
                        let s = state.read().clone();
                        let keystroke_time = std::time::Instant::now();
                        let mut lib_signal = signals.library;
                        let mut ui_signal = signals.ui;
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS)).await;
                            let elapsed = keystroke_time.elapsed();
                            if elapsed >= std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS) {
                                *s.search_query.lock().unwrap_or_else(|e| e.into_inner()) = new_value.clone();
                                let current_view = s.current_view.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                if current_view != ViewType::Search {
                                    *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Search;
                                }
                                let gen = *lib_signal.read();
                                lib_signal.set(gen.wrapping_add(1));
                                let gen = *ui_signal.read();
                                ui_signal.set(gen.wrapping_add(1));
                            }
                        });
                    },
                    onsubmit: move |_| {
                        let s = state.read().clone();
                        let val = search_value.read().clone();
                        *s.search_query.lock().unwrap_or_else(|e| e.into_inner()) = val;
                        *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::Search;
                        let gen = *signals.library.read();
                        signals.library.set(gen.wrapping_add(1));
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                }
            }

            div { class: "topbar-spacer" }

            button {
                class: "topbar-icon-btn",
                aria_label: "{tr(\"Notifications\")}, {notification_count} unread",
                tabindex: "0",
                onclick: move |_| {
                    let s = state.read().clone();
                    let visible = s.notifications_visible.load(std::sync::atomic::Ordering::Relaxed);
                    s.notifications_visible.store(!visible, std::sync::atomic::Ordering::Relaxed);
                    if !visible {
                        s.notification_count.store(0, std::sync::atomic::Ordering::Relaxed);
                    }
                    let gen = *signals.ui.read();
                    signals.ui.set(gen.wrapping_add(1));
                },
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                        let s = state.read().clone();
                        let visible = s.notifications_visible.load(std::sync::atomic::Ordering::Relaxed);
                        s.notifications_visible.store(!visible, std::sync::atomic::Ordering::Relaxed);
                        if !visible {
                            s.notification_count.store(0, std::sync::atomic::Ordering::Relaxed);
                        }
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    }
                },
                span { "🔔" }
                if notification_count > 0 {
                    span { class: "notification-badge", "{notification_count}" }
                }
            }

            button {
                class: "topbar-icon-btn",
                aria_label: if dark { tr("Switch to light theme") } else { tr("Switch to dark theme") },
                tabindex: "0",
                onclick: move |_| {
                    let s = state.read().clone();
                    let d = s.dark_mode.load(std::sync::atomic::Ordering::Relaxed);
                    s.dark_mode.store(!d, std::sync::atomic::Ordering::Relaxed);
                    let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
                    config.general.theme = if !d { "dark".into() } else { "light".into() };
                    if let Err(e) = tunecraft_core::config::save(&config) {
                        tracing::warn!("Failed to persist theme: {}", e);
                    }
                    let gen = *signals.ui.read();
                    signals.ui.set(gen.wrapping_add(1));
                },
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                        let s = state.read().clone();
                        let d = s.dark_mode.load(std::sync::atomic::Ordering::Relaxed);
                        s.dark_mode.store(!d, std::sync::atomic::Ordering::Relaxed);
                        let mut config = s.config.write().unwrap_or_else(|e| e.into_inner());
                        config.general.theme = if !d { "dark".into() } else { "light".into() };
                        if let Err(e) = tunecraft_core::config::save(&config) {
                            tracing::warn!("Failed to persist theme: {}", e);
                        }
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    }
                },
                span { if dark { "☀️" } else { "🌙" } }
            }

            button {
                class: "add-music-btn",
                aria_label: "{tr(\"Add music files\")}",
                tabindex: "0",
                onclick: move |_| {
                    let s = state.read().clone();
                    let db = {
                        let db_guard = s.db.read().unwrap_or_else(|e| e.into_inner());
                        db_guard.clone()
                    };
                    if let Some(db) = db {
                        let mut lib_signal = signals.library;
                        spawn(async move {
                            let files = rfd::AsyncFileDialog::new()
                                .add_filter("Audio Files", &["mp3", "flac", "wav", "ogg", "aac", "m4a"])
                                .pick_files()
                                .await;
                            if let Some(paths) = files {
                                let mut added = 0usize;
                                for file in &paths {
                                    let path = file.path().to_path_buf();
                                    if tunecraft_core::library::scanner::LibraryScanner::is_audio(&path) {
                                        match tunecraft_core::library::metadata::read_metadata(&path) {
                                            Ok(mut track) => {
                                                if let Ok(hash) = tunecraft_core::util::hash::file_sha256(&path) {
                                                    track.file_hash = Some(hash);
                                                }
                                                if let Ok(id) = db.insert_track(&track) {
                                                    if id > 0 { added += 1; }
                                                }
                                            }
                                            Err(e) => tracing::warn!("Failed to read {}: {:?}", path.display(), e),
                                        }
                                    }
                                }
                                tracing::info!("Added {} tracks from file dialog", added);
                                if added > 0 {
                                    s.sidebar_cache_valid.store(false, std::sync::atomic::Ordering::Relaxed);
                                    let gen = *lib_signal.read();
                                    lib_signal.set(gen.wrapping_add(1));
                                }
                            }
                        });
                    }
                },
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                    }
                },
                span { "＋" }
                span { "{tr(\"Add Music\")}" }
            }
        }
    }
}
