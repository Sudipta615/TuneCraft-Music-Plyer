//! Track list component showing the library content.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, ViewLayout, ViewType};
use crate::i18n::tr;

const ROW_HEIGHT: i64 = 48;
const BUFFER_ROWS: usize = 5;

/// Track list component.
pub fn TrackList() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let mut signals: ReactivitySignals = use_context();
    let _ = *signals.library.read();
    let _ = *signals.playback.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);

    let current_view = state
        .read()
        .current_view
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let view_layout = *state
        .read()
        .view_layout
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let sort_mode = *state
        .read()
        .sort_mode
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let tracks = state.read().load_tracks_for_view();

    let total_duration_secs: i64 = tracks.iter().filter_map(|t| t.duration).sum::<u64>() as i64;
    let total_hours = total_duration_secs / 3600;
    let total_mins = (total_duration_secs % 3600) / 60;

    let view_title = match &current_view {
        ViewType::AllTracks => tr("All Tracks"),
        ViewType::Albums => tr("Albums"),
        ViewType::Artists => tr("Artists"),
        ViewType::Playlists => tr("Playlists"),
        ViewType::Search => tr("Search Results"),
        ViewType::Mood(m) => m.clone(),
        ViewType::Settings => tr("Settings"),
        ViewType::AlbumDetail(a) => format!("{}: {}", tr("Album"), a),
        ViewType::ArtistDetail(a) => format!("{}: {}", tr("Artist"), a),
        ViewType::PlaylistDetail(name, _) => format!("{}: {}", tr("Playlist"), name),
        ViewType::Filter { genre, .. } => format!("{}: {}", tr("Filter"), genre),
    };

    if current_view == ViewType::Settings {
        let config = state
            .read()
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let current_volume = state.read().volume();
        let current_speed = *state
            .read()
            .playback_speed
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let crossfade_ms = config.audio.crossfade_duration_ms;
        let rescan_on_startup = config.library.rescan_on_startup;
        let watch_dirs = config.library.watch_dirs.join(", ");
        let scrobble_enabled = config.scrobble.enabled;
        let config_path_display = {
            tunecraft_core::config::config_dir()
                .map(|d| d.join("tunecraft.toml").display().to_string())
                .unwrap_or_else(|_| "Unknown".to_string())
        };

        let crossfade_display: u32 = if crossfade_ms > 0 { crossfade_ms } else { 2000 };

        return rsx! {
            div { class: if dark { "track-list-container dark" } else { "track-list-container light" },
                div { class: "track-list-header",
                    div { class: "track-list-header-info",
                        h2 { class: "track-list-title", "{tr(\"Settings\")}" }
                    }
                }
                div { class: "settings-panel",
                    div { class: "settings-section",
                        h3 { class: "settings-section-title", "🔊 {tr(\"Audio\")}" }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Volume\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "range",
                                    min: "0",
                                    max: "100",
                                    value: "{(current_volume * 100.0) as i32}",
                                    class: "settings-slider",
                                    aria_label: "Volume",
                                    oninput: move |e: Event<FormData>| {
                                        if let Ok(v) = e.value().parse::<f64>() {
                                            let s = state.read().clone();
                                            s.set_volume(v / 100.0);
                                            s.save_playback_state_to_config();
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                }
                                span { class: "settings-value", "{(current_volume * 100.0) as i32}%" }
                            }
                        }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Playback Speed\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "range",
                                    min: "25",
                                    max: "400",
                                    value: "{(current_speed * 100.0) as i32}",
                                    class: "settings-slider",
                                    aria_label: "Playback speed",
                                    oninput: move |e: Event<FormData>| {
                                        if let Ok(v) = e.value().parse::<f32>() {
                                            let s = state.read().clone();
                                            s.set_playback_speed(v / 100.0);
                                            s.save_playback_state_to_config();
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                }
                                span { class: "settings-value", "{(current_speed * 100.0) as i32}%" }
                            }
                        }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Crossfade\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "checkbox",
                                    checked: crossfade_ms > 0,
                                    aria_label: "Toggle crossfade",
                                    onchange: move |e: Event<FormData>| {
                                        let enabled = e.value() == "true";
                                        let s = state.read().clone();
                                        let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                                        cfg.audio.crossfade_duration_ms = if enabled { 2000 } else { 0 };
                                        drop(cfg);
                                        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                                        let _ = tunecraft_core::config::save(&cfg);
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                }
                                span { class: "settings-checkbox-label",
                                    if crossfade_ms > 0 { "On" } else { "Off" }
                                }
                            }
                        }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Fade Duration\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "range",
                                    min: "500",
                                    max: "10000",
                                    step: "500",
                                    value: "{crossfade_display}",
                                    class: "settings-slider",
                                    disabled: crossfade_ms == 0,
                                    aria_label: "Crossfade duration",
                                    oninput: move |e: Event<FormData>| {
                                        if let Ok(v) = e.value().parse::<u32>() {
                                            let s = state.read().clone();
                                            let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                                            cfg.audio.crossfade_duration_ms = v;
                                            drop(cfg);
                                            let cfg = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                                            let _ = tunecraft_core::config::save(&cfg);
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                }
                                span { class: "settings-value",
                                    "{crossfade_display / 1000}s"
                                }
                            }
                        }
                    }

                    div { class: "settings-section",
                        h3 { class: "settings-section-title", "📁 {tr(\"Library\")}" }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Watch Directories\")}" }
                            div { class: "settings-control",
                                span { class: "settings-value settings-dirs",
                                    if watch_dirs.is_empty() { "None configured" } else { "{watch_dirs}" }
                                }
                            }
                        }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Rescan on Startup\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "checkbox",
                                    checked: rescan_on_startup,
                                    aria_label: "Rescan on startup",
                                    onchange: move |e: Event<FormData>| {
                                        let enabled = e.value() == "true";
                                        let s = state.read().clone();
                                        let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                                        cfg.library.rescan_on_startup = enabled;
                                        drop(cfg);
                                        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                                        let _ = tunecraft_core::config::save(&cfg);
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                }
                                span { class: "settings-checkbox-label",
                                    if rescan_on_startup { "On" } else { "Off" }
                                }
                            }
                        }
                    }

                    div { class: "settings-section",
                        h3 { class: "settings-section-title", "🎵 {tr(\"Scrobble (Last.fm)\")}" }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Status\")}" }
                            div { class: "settings-control",
                                input {
                                    r#type: "checkbox",
                                    checked: scrobble_enabled,
                                    aria_label: "Toggle scrobble",
                                    onchange: move |e: Event<FormData>| {
                                        let enabled = e.value() == "true";
                                        let s = state.read().clone();
                                        let mut cfg = s.config.write().unwrap_or_else(|e| e.into_inner());
                                        cfg.scrobble.enabled = enabled;
                                        drop(cfg);
                                        let cfg = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                                        let _ = tunecraft_core::config::save(&cfg);
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                }
                                span { class: "settings-checkbox-label",
                                    if scrobble_enabled { "Enabled" } else { "Disabled" }
                                }
                            }
                        }
                    }

                    div { class: "settings-section",
                        h3 { class: "settings-section-title", "🔧 {tr(\"Advanced\")}" }
                        div { class: "settings-row",
                            label { class: "settings-label", "{tr(\"Config File\")}" }
                            div { class: "settings-control",
                                span { class: "settings-value settings-path", "{config_path_display}" }
                                button {
                                    class: "settings-btn",
                                    aria_label: "Open config file location",
                                    onclick: move |_| {
                                        if let Ok(dir) = tunecraft_core::config::config_dir() {
                                            #[cfg(target_os = "linux")]
                                            { let _ = std::process::Command::new("xdg-open").arg(&dir).spawn(); }
                                            #[cfg(target_os = "macos")]
                                            { let _ = std::process::Command::new("open").arg(&dir).spawn(); }
                                            #[cfg(target_os = "windows")]
                                            { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
                                        }
                                    },
                                    "{tr(\"Open Config File\")}"
                                }
                            }
                        }
                    }
                }
            }
        };
    }

    if current_view == ViewType::Albums {
        let albums = state.read().get_all_albums();
        return rsx! {
            div { class: if dark { "track-list-container dark" } else { "track-list-container light" },
                div { class: "track-list-header",
                    div { class: "track-list-header-info",
                        h2 { class: "track-list-title", "{tr(\"Albums\")}" }
                        span { class: "track-list-subtitle", "{albums.len()} {tr(\"albums\")}" }
                    }
                }
                div { class: "track-grid",
                    for (album, artists, track_count, total_duration) in albums.iter() {
                        {
                            let album_name = album.clone();
                            let total_mins = total_duration / 60;
                            let state_ref = state;
                            rsx! {
                                div {
                                    class: "album-card",
                                    key: "{album_name}",
                                    role: "listitem",
                                    tabindex: "0",
                                    aria_label: "{album} by {artists}, {track_count} tracks, {total_mins} minutes",
                                    onclick: move |_| {
                                        let s = state_ref.read().clone();
                                        *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                            ViewType::AlbumDetail(album_name.clone());
                                        let gen = *signals.library.read();
                                        signals.library.set(gen.wrapping_add(1));
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                            let s = state_ref.read().clone();
                                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                                ViewType::AlbumDetail(album_name.clone());
                                            let gen = *signals.library.read();
                                            signals.library.set(gen.wrapping_add(1));
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                    div { class: "album-card-title", "{album}" }
                                    div { class: "album-card-artist", "{artists}" }
                                    div { class: "album-card-meta",
                                        "{track_count} tracks • {total_mins} min"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
    }

    if current_view == ViewType::Artists {
        let artists = state.read().get_all_artists();
        return rsx! {
            div { class: if dark { "track-list-container dark" } else { "track-list-container light" },
                div { class: "track-list-header",
                    div { class: "track-list-header-info",
                        h2 { class: "track-list-title", "{tr(\"Artists\")}" }
                        span { class: "track-list-subtitle", "{artists.len()} {tr(\"artists\")}" }
                    }
                }
                div { class: "track-grid",
                    for (artist, track_count, album_count) in artists.iter() {
                        {
                            let artist_name = artist.clone();
                            let state_ref = state;
                            rsx! {
                                div {
                                    class: "artist-card",
                                    key: "{artist_name}",
                                    role: "listitem",
                                    tabindex: "0",
                                    aria_label: "{artist}, {album_count} albums, {track_count} tracks",
                                    onclick: move |_| {
                                        let s = state_ref.read().clone();
                                        *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                            ViewType::ArtistDetail(artist_name.clone());
                                        let gen = *signals.library.read();
                                        signals.library.set(gen.wrapping_add(1));
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                            let s = state_ref.read().clone();
                                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                                ViewType::ArtistDetail(artist_name.clone());
                                            let gen = *signals.library.read();
                                            signals.library.set(gen.wrapping_add(1));
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                    div { class: "artist-card-name", "{artist}" }
                                    div { class: "artist-card-meta",
                                        "{album_count} albums • {track_count} tracks"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
    }

    if current_view == ViewType::Playlists {
        let playlists = state.read().get_all_playlists();
        return rsx! {
            div { class: if dark { "track-list-container dark" } else { "track-list-container light" },
                div { class: "track-list-header",
                    div { class: "track-list-header-info",
                        h2 { class: "track-list-title", "{tr(\"Playlists\")}" }
                        span { class: "track-list-subtitle", "{playlists.len()} {tr(\"playlists\")}" }
                    }
                }
                div { class: "track-grid",
                    for playlist in playlists.iter() {
                        {
                            let pl_name = playlist.name.clone();
                            let pl_id = playlist.id;
                            let state_ref = state;
                            rsx! {
                                div {
                                    class: "album-card",
                                    key: "{pl_name}-{pl_id:?}",
                                    role: "listitem",
                                    tabindex: "0",
                                    aria_label: "{playlist.name} playlist",
                                    onclick: move |_| {
                                        let s = state_ref.read().clone();
                                        *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                            ViewType::PlaylistDetail(pl_name.clone(), pl_id);
                                        let gen = *signals.library.read();
                                        signals.library.set(gen.wrapping_add(1));
                                        let gen = *signals.ui.read();
                                        signals.ui.set(gen.wrapping_add(1));
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                            let s = state_ref.read().clone();
                                            *s.current_view.lock().unwrap_or_else(|e| e.into_inner()) =
                                                ViewType::PlaylistDetail(pl_name.clone(), pl_id);
                                            let gen = *signals.library.read();
                                            signals.library.set(gen.wrapping_add(1));
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        }
                                    },
                                    div { class: "album-card-title", "{playlist.name}" }
                                    div { class: "album-card-artist",
                                        if playlist.is_smart { "Smart Playlist" } else { "Manual Playlist" }
                                    }
                                    if let Some(desc) = &playlist.description {
                                        div { class: "album-card-meta", "{desc}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
    }

    let mut scroll_top: Signal<i64> = use_signal(|| 0);
    let total_tracks = tracks.len();
    let total_height = total_tracks as i64 * ROW_HEIGHT;

    let visible_height = 600i64; // Will be refined by onscroll events
    let start_idx = ((*scroll_top.read() / ROW_HEIGHT) as usize).saturating_sub(BUFFER_ROWS);
    let visible_count = (visible_height / ROW_HEIGHT) as usize + 1;
    let end_idx = (start_idx + visible_count + 2 * BUFFER_ROWS).min(total_tracks);
    let top_spacer_height = start_idx as i64 * ROW_HEIGHT;
    let bottom_spacer_height = (total_tracks.saturating_sub(end_idx)) as i64 * ROW_HEIGHT;

    rsx! {
        div { class: if dark { "track-list-container dark" } else { "track-list-container light" },
            div { class: "track-list-header",
                div { class: "track-list-header-info",
                    h2 { class: "track-list-title", "{view_title}" }
                    span { class: "track-list-subtitle",
                        "{tracks.len()} tracks • {total_hours} hours {total_mins} minutes"
                    }
                }
                div { class: "track-list-header-actions",
                    button {
                        class: "toolbar-btn",
                        aria_label: "Open filter panel",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let v = s.filter_visible.load(std::sync::atomic::Ordering::Relaxed);
                            s.filter_visible.store(!v, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let v = s.filter_visible.load(std::sync::atomic::Ordering::Relaxed);
                                s.filter_visible.store(!v, std::sync::atomic::Ordering::Relaxed);
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        "⚙ {tr(\"Filter\")}"
                    }

                    button {
                        class: "toolbar-btn",
                        aria_label: "Sort by {sort_mode.label()}",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let mut sort = s.sort_mode.lock().unwrap_or_else(|e| e.into_inner());
                            *sort = sort.cycle();
                            drop(sort);
                            let gen = *signals.library.read();
                            signals.library.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let mut sort = s.sort_mode.lock().unwrap_or_else(|e| e.into_inner());
                                *sort = sort.cycle();
                                drop(sort);
                                let gen = *signals.library.read();
                                signals.library.set(gen.wrapping_add(1));
                            }
                        },
                        "↕ {sort_mode.label()}"
                    }

                    button {
                        class: "toolbar-btn",
                        aria_label: if view_layout == ViewLayout::Grid { "Switch to list view" } else { "Switch to grid view" },
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let mut layout = s.view_layout.lock().unwrap_or_else(|e| e.into_inner());
                            *layout = match *layout {
                                ViewLayout::List => ViewLayout::Grid,
                                ViewLayout::Grid => ViewLayout::List,
                            };
                            drop(layout);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let mut layout = s.view_layout.lock().unwrap_or_else(|e| e.into_inner());
                                *layout = match *layout {
                                    ViewLayout::List => ViewLayout::Grid,
                                    ViewLayout::Grid => ViewLayout::List,
                                };
                                drop(layout);
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        if view_layout == ViewLayout::Grid { "▦" } else { "☰" }
                    }

                    button {
                        class: "toolbar-btn eq-btn",
                        aria_label: "Open equalizer panel",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let v = s.eq_visible.load(std::sync::atomic::Ordering::Relaxed);
                            s.eq_visible.store(!v, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let v = s.eq_visible.load(std::sync::atomic::Ordering::Relaxed);
                                s.eq_visible.store(!v, std::sync::atomic::Ordering::Relaxed);
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            }
                        },
                        "EQ"
                    }
                }
            }

            div {
                class: "track-table",
                role: "list",
                aria_label: "Track list",

                onscroll: move |_e: Event<ScrollData>| {
                    // Read scroll position via JS eval for cross-platform compatibility
                    spawn(async move {
                        if let Ok(result) = dioxus::document::eval(r#"return document.querySelector('.track-table').scrollTop || 0"#).await {
                            if let Some(n) = result.as_f64() {
                                scroll_top.set(n as i64);
                            }
                        }
                    });
                },

                div { class: "track-table-header",
                    span { class: "col-num", "#" }
                    span { class: "col-title", "{tr(\"TITLE\")}" }
                    span { class: "col-album", "{tr(\"ALBUM\")}" }
                    span { class: "col-duration", "⏱" }
                    span { class: "col-mood", "{tr(\"MOOD\")}" }
                    span { class: "col-actions", "" }
                }

                div {
                    style: "height: {top_spacer_height}px;",
                    aria_hidden: "true",
                }

                for idx in start_idx..end_idx {
                    {
                        let track: tunecraft_core::Track = match tracks.get(idx) {
                            Some(t) => t.clone(),
                            None => return rsx! {},
                        };
                        let title = track.title.clone().unwrap_or_else(|| "Unknown".into());
                        let artist = track.artist.clone().unwrap_or_else(|| "Unknown".into());
                        let album = track.album.clone().unwrap_or_else(|| "Unknown".into());
                        let duration = track.duration
                            .map(|d| format!("{}:{:02}", d / 60, d % 60))
                            .unwrap_or_else(|| "--:--".into());
                        let mood = track.mood.clone().unwrap_or_else(|| "".into());
                        let is_loved = state.read().is_track_loved_with_tracks(idx, &tracks);
                        let mood_color = match mood.as_str() {
                            "Dance" | "dance" => "#ef4444",
                            "Romantic" | "romantic" => "#8b5cf6",
                            "Sad" | "sad" => "#3b82f6",
                            "Sufi" | "sufi" => "#f97316",
                            "Chill" | "chill" => "#22c55e",
                            _ => "#6b7280",
                        };

                        let track_idx = idx;
                        let state_ref = state;

                        rsx! {
                            div {
                                class: "track-row",
                                key: "{track_idx}",
                                role: "listitem",
                                tabindex: "0",
                                aria_label: "{title} by {artist}, {album}, {duration}",

                                span { class: "col-num",
                                    button {
                                        class: "track-play-btn",
                                        aria_label: "Play {title}",
                                        tabindex: "-1",
                                        onclick: move |_| {
                                            let s = state_ref.read().clone();
                                            s.play_track_from_view(track_idx);
                                            s.notify_track_change();
                                            let gen = *signals.queue.read();
                                            signals.queue.set(gen.wrapping_add(1));
                                            let gen = *signals.playback.read();
                                            signals.playback.set(gen.wrapping_add(1));
                                        },
                                        "▶"
                                    }
                                }

                                span { class: "col-title",
                                    div { class: "track-title", "{title}" }
                                    div { class: "track-artist", "{artist}" }
                                }

                                span { class: "col-album", "{album}" }

                                span { class: "col-duration", "{duration}" }

                                span { class: "col-mood",
                                    if !mood.is_empty() {
                                        span {
                                            class: "mood-tag",
                                            style: "background-color: {mood_color}",
                                            "{mood}"
                                        }
                                    }
                                }

                                span { class: "col-actions",
                                    button {
                                        class: if is_loved { "love-btn loved" } else { "love-btn" },
                                        aria_label: if is_loved { "Unlove {title}" } else { "Love {title}" },
                                        tabindex: "-1",
                                        onclick: move |_| {
                                            let s = state_ref.read().clone();
                                            let tracks = s.load_tracks_for_view();
                                            if let Some(t) = tracks.get(track_idx) {
                                                let key = t.file_hash.clone()
                                                    .unwrap_or_else(|| t.file_path.clone());
                                                s.toggle_track_loved(&key);
                                            }
                                            let gen = *signals.library.read();
                                            signals.library.set(gen.wrapping_add(1));
                                        },
                                        if is_loved { "♥" } else { "♡" }
                                    }
                                    button {
                                        class: "more-btn",
                                        aria_label: "More options for {title}",
                                        tabindex: "-1",
                                        onclick: move |evt| {
                                            let s = state_ref.read().clone();
                                            let mut target = s.context_menu_target.lock().unwrap_or_else(|e| e.into_inner());
                                            *target = if *target == Some(track_idx) { None } else { Some(track_idx) };
                                            *s.context_menu_position.lock().unwrap_or_else(|e| e.into_inner()) = (evt.page_coordinates().x, evt.page_coordinates().y);
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        },
                                        "⋯"
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    style: "height: {bottom_spacer_height}px;",
                    aria_hidden: "true",
                }
            }
        }
    }
}
