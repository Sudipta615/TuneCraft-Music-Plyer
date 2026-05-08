//! Context menu overlay component for track actions.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, ViewType};

/// Context menu overlay component.
pub fn ContextMenuOverlay() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let mut signals: ReactivitySignals = use_context();
    let _ = *signals.ui.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);

    let context_target = *state
        .read()
        .context_menu_target
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let menu_pos = state
        .read()
        .context_menu_position
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let menu_style = format!("top: {}px; left: {}px;", menu_pos.1, menu_pos.0);

    let Some(target_idx) = context_target else {
        return rsx! {};
    };

    let mut playlist_picker: Signal<Vec<(i64, String)>> = use_signal(Vec::new);

    let mut focused_index: Signal<usize> = use_signal(|| 0usize);
    let menu_item_count = 6usize; // Play Next, Add to Queue, Add to Playlist, Go to Artist, Go to Album, Track Info

    let mut close_menu = move || {
        let s = state.read().clone();
        *s.context_menu_target
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        playlist_picker.set(Vec::new());
        let gen = *signals.ui.read();
        signals.ui.set(gen.wrapping_add(1));
    };

    rsx! {
        div {
            class: "context-menu-overlay",
            onclick: move |_| {
                close_menu();
            },

            div {
                class: if dark { "context-menu dark" } else { "context-menu light" },
                style: "{menu_style}",
                role: "menu",
                aria_label: "Track context menu",
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e: KeyboardEvent| {
                    match e.key() {
                        Key::Escape => {
                            close_menu();
                        }
                        Key::ArrowDown => {
                            let current = *focused_index.read();
                            let picker_len = playlist_picker.read().len();
                            let total = if picker_len > 0 { menu_item_count + picker_len + 1 } else { menu_item_count };
                            focused_index.set((current + 1) % total);
                        }
                        Key::ArrowUp => {
                            let current = *focused_index.read();
                            let picker_len = playlist_picker.read().len();
                            let total = if picker_len > 0 { menu_item_count + picker_len + 1 } else { menu_item_count };
                            focused_index.set(if current == 0 { total - 1 } else { current - 1 });
                        }
                        _ => {}
                    }
                },

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Play Next",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let tracks = s.load_tracks_for_view();
                        if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                            let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                            if queue.tracks.len() < crate::app_state::MAX_QUEUE_SIZE {
                                let insert_pos = queue.current_index.map(|c| c + 1).unwrap_or(0);
                                if insert_pos <= queue.tracks.len() {
                                    queue.tracks.insert(insert_pos, track);
                                    if queue.current_index.is_none() {
                                        queue.current_index = Some(0);
                                    }
                                    if queue.shuffle {
                                        queue.regenerate_shuffle_order_preserving_current();
                                    }
                                }
                            }
                        }
                        let gen = *signals.queue.read();
                        signals.queue.set(gen.wrapping_add(1));
                        close_menu();
                    },
                    "▶ Play Next"
                }

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Add to Queue",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let tracks = s.load_tracks_for_view();
                        if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                            let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                            if queue.tracks.len() < crate::app_state::MAX_QUEUE_SIZE {
                                queue.tracks.push(track);
                                if queue.current_index.is_none() && queue.tracks.len() == 1 {
                                    queue.current_index = Some(0);
                                }
                                if queue.shuffle {
                                    queue.regenerate_shuffle_order_preserving_current();
                                }
                            }
                        }
                        let gen = *signals.queue.read();
                        signals.queue.set(gen.wrapping_add(1));
                        close_menu();
                    },
                    "+ Add to Queue"
                }

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Add to Playlist",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let tracks = s.load_tracks_for_view();
                        if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                            if let Some(track_id) = track.id {
                                let playlists = s.get_all_playlists();
                                if playlists.is_empty() {
                                    *s.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                        Some("No playlists available. Create one first.".into());
                                } else if playlists.len() == 1 {
                                    if let Some(pl) = playlists.first() {
                                        if let Some(pl_id) = pl.id {
                                            let db = s.db.read().unwrap_or_else(|e| e.into_inner());
                                            if let Some(ref db) = *db {
                                                match db.add_track_to_playlist(pl_id, track_id) {
                                                    Ok(()) => {
                                                        *s.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                            Some(format!("Added to playlist '{}'", pl.name));
                                                    }
                                                    Err(e) => {
                                                        *s.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                            Some(format!("Failed to add to playlist: {}", e));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let choices: Vec<(i64, String)> = playlists.iter()
                                        .filter_map(|p| p.id.map(|id| (id, p.name.clone())))
                                        .collect();
                                    playlist_picker.set(choices);
                                    return;
                                }
                            }
                        }
                        close_menu();
                    },
                    "♫ Add to Playlist"
                }

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Go to Artist",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let tracks = s.load_tracks_for_view();
                        if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                            if let Some(ref artist) = track.artist {
                                *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                    ViewType::ArtistDetail(artist.clone());
                            }
                        }
                        let gen = *signals.library.read();
                        signals.library.set(gen.wrapping_add(1));
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                        close_menu();
                    },
                    "👤 Go to Artist"
                }

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Go to Album",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let tracks = s.load_tracks_for_view();
                        if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                            if let Some(ref album) = track.album {
                                *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                    ViewType::AlbumDetail(album.clone());
                            }
                        }
                        let gen = *signals.library.read();
                        signals.library.set(gen.wrapping_add(1));
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                        close_menu();
                    },
                    "💿 Go to Album"
                }

                button {
                    class: "context-menu-item",
                    role: "menuitem",
                    tabindex: "0",
                    aria_label: "Show Track Info",
                    onclick: move |_| {
                        close_menu();
                    },
                    "ℹ Track Info"
                }

                if !playlist_picker.read().is_empty() {
                    div { class: "context-menu-playlist-picker",
                        div { class: "context-menu-picker-label", "Add to playlist:" }
                        for (pl_id, pl_name) in playlist_picker.read().clone() {
                            {
                                let state_ref = state;
                                let pl_name_display = pl_name.clone();
                                rsx! {
                                    button {
                                        class: "context-menu-item context-menu-playlist-choice",
                                        role: "menuitem",
                                        tabindex: "0",
                                        aria_label: "Add to {pl_name_display}",
                                        onclick: move |_| {
                                            let s = state_ref.read().clone();
                                            let tracks = s.load_tracks_for_view();
                                            if let Some(track) = s.track_at_view_index(target_idx, &tracks) {
                                                if let Some(track_id) = track.id {
                                                    let db = s.db.read().unwrap_or_else(|e| e.into_inner());
                                                    if let Some(ref db) = *db {
                                                        match db.add_track_to_playlist(pl_id, track_id) {
                                                            Ok(()) => {
                                                                *s.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                                    Some(format!("Added to '{}'", pl_name_display));
                                                            }
                                                            Err(e) => {
                                                                *s.toast_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                                                    Some(format!("Failed: {}", e));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            close_menu();
                                        },
                                        "♫ {pl_name}"
                                    }
                                }
                            }
                        }
                        button {
                            class: "context-menu-item",
                            role: "menuitem",
                            tabindex: "0",
                            aria_label: "Cancel",
                            onclick: move |_| { playlist_picker.set(Vec::new()); },
                            "✕ Cancel"
                        }
                    }
                }
            }
        }
    }
}
