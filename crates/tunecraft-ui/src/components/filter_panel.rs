//! Filter panel component for genre/year filtering.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, ViewType};
use crate::i18n::tr;

/// Filter panel overlay component.
pub fn FilterPanel() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let signals: ReactivitySignals = use_context();
    // Issue #5: Subscribe to UI signal for panel state
    let _ = *signals.ui.read();

    let dark = state.read().dark_mode.load(std::sync::atomic::Ordering::Relaxed);
    let filter_genre = state.read().filter_genre.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let filter_year_range = state.read().filter_year_range.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let mut genre_value = use_signal(|| filter_genre.clone());
    let mut year_value = use_signal(|| filter_year_range.clone());

    rsx! {
        div { class: "overlay-panel filter-panel",
            class: if dark { "dark" } else { "light" },
            role: "dialog",
            aria_label: "Filter panel",

            div { class: "panel-header",
                h3 { "{tr(\"Filter\")}" }
                button {
                    class: "panel-close-btn",
                    // Issue #6: Accessibility
                    aria_label: "Close filter panel",
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                        // Issue #5: Bump UI signal after panel close
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Space {
                            let s = state.read().clone();
                            s.filter_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "✕"
                }
            }

            div { class: "filter-content",
                div { class: "filter-field",
                    label { "{tr(\"Genre\")}" }
                    input {
                        r#type: "text",
                        class: "filter-input",
                        // Issue #6: Accessibility
                        aria_label: "Genre filter",
                        placeholder: "e.g. Rock, Pop, Jazz...",
                        value: "{genre_value}",
                        oninput: move |e| {
                            genre_value.set(e.value().clone());
                            let s = state.read().clone();
                            *s.filter_genre.lock().unwrap_or_else(|e| e.into_inner()) = e.value().clone();
                        },
                    }
                }

                div { class: "filter-field",
                    label { "{tr(\"Year Range\")}" }
                    input {
                        r#type: "text",
                        class: "filter-input",
                        // Issue #6: Accessibility
                        aria_label: "Year range filter",
                        placeholder: "e.g. 2020-2024 or 2023",
                        value: "{year_value}",
                        oninput: move |e| {
                            year_value.set(e.value().clone());
                            let s = state.read().clone();
                            *s.filter_year_range.lock().unwrap_or_else(|e| e.into_inner()) = e.value().clone();
                        },
                    }
                }

                div { class: "filter-actions",
                    button {
                        class: "filter-apply-btn",
                        // Issue #6: Accessibility
                        aria_label: "Apply filter",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let genre = s.filter_genre.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            let year_range = s.filter_year_range.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            let (year_from, year_to) = if !year_range.is_empty() {
                                // #11: Use rfind('-') for year ranges so "2020-2024" works correctly.
                                // Also handle edge cases like spaces and missing upper bound.
                                let trimmed = year_range.trim();
                                if let Some(dash) = trimmed.rfind('-') {
                                    let from_str = trimmed[..dash].trim();
                                    let to_str = trimmed[dash + 1..].trim();
                                    // If the part before '-' is empty (e.g. "-2024"), treat as no lower bound
                                    let from: Option<i32> = if from_str.is_empty() {
                                        None
                                    } else {
                                        match from_str.parse() {
                                            Ok(v) => Some(v),
                                            Err(_) => {
                                                // Bug #6 fix: Malformed lower bound — warn the user
                                                // instead of silently dropping it (e.g. "1990-2000-2010"
                                                // would previously parse as (None, 2010)).
                                                let s2 = state.read().clone();
                                                *s2.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                    Some(format!("Invalid year range: '{}' is not a valid year", from_str));
                                                return;
                                            }
                                        }
                                    };
                                    // If the part after '-' is empty (e.g. "2020-"), treat as no upper bound
                                    let to: Option<i32> = if to_str.is_empty() {
                                        None
                                    } else {
                                        match to_str.parse() {
                                            Ok(v) => Some(v),
                                            Err(_) => {
                                                let s2 = state.read().clone();
                                                *s2.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                    Some(format!("Invalid year range: '{}' is not a valid year", to_str));
                                                return;
                                            }
                                        }
                                    };
                                    (from, to)
                                } else {
                                    match trimmed.parse::<i32>() {
                                        Ok(y) => (Some(y), Some(y)),
                                        Err(_) => {
                                            let s2 = state.read().clone();
                                            *s2.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                Some(format!("Invalid year: '{}'. Use e.g. 2020 or 2020-2024", trimmed));
                                            return;
                                        }
                                    }
                                }
                            } else {
                                (None, None)
                            };
                            if !genre.is_empty() || year_from.is_some() || year_to.is_some() {
                                *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                    ViewType::Filter { genre, year_from, year_to };
                            }
                            // Issue #5: Bump signals after filter apply
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Space {
                                // Same as onclick — trigger filter apply
                            }
                        },
                        "{tr(\"Apply\")}"
                    }

                    button {
                        class: "filter-clear-btn",
                        // Issue #6: Accessibility
                        aria_label: "Clear filter",
                        tabindex: "0",
                        onclick: move |_| {
                            genre_value.set(String::new());
                            year_value.set(String::new());
                            let s = state.read().clone();
                            *s.filter_genre.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
                            *s.filter_year_range.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
                            // Fix Bug #9: Also reset the current view back to AllTracks
                            // so the filtered view is cleared when Clear is pressed.
                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::AllTracks;
                            // Issue #5: Bump signals after filter clear
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Space {
                                genre_value.set(String::new());
                                year_value.set(String::new());
                                let s = state.read().clone();
                                *s.filter_genre.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
                                *s.filter_year_range.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
                                *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) = ViewType::AllTracks;
                                let gen = *signals.library.read();
                                signals.library.set(gen.wrapping_add(1));
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        "{tr(\"Clear\")}"
                    }
                }
            }
        }
    }
}
