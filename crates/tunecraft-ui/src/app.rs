//! TuneCraft v5.0 — Dioxus Application root component.

#![allow(non_snake_case)]

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app_state::AppState;
use crate::components::*;
use crate::styles;

#[derive(Clone, Copy)]
pub struct ReactivitySignals {
    pub playback: Signal<u64>,
    pub queue: Signal<u64>,
    pub library: Signal<u64>,
    pub ui: Signal<u64>,
}

/// The root TuneCraft application component.
pub fn App() -> Element {
    use_context_provider(|| Signal::new(Arc::new(AppState::new())));

    let state: Signal<Arc<AppState>> = use_context();

    let playback_signal: Signal<u64> = use_signal(|| 0);
    let queue_signal: Signal<u64> = use_signal(|| 0);
    let library_signal: Signal<u64> = use_signal(|| 0);
    let ui_signal: Signal<u64> = use_signal(|| 0);

    let signals = ReactivitySignals {
        playback: playback_signal,
        queue: queue_signal,
        library: library_signal,
        ui: ui_signal,
    };
    use_context_provider(|| signals);

    {
        let mut signals = signals;
        spawn(async move {
            let s = state.read().clone();
            match s.init_engine() {
                Ok(()) => {
                    tracing::info!("Audio engine initialized successfully");
                    s.engine_ready
                        .store(true, std::sync::atomic::Ordering::Relaxed);

                    crate::media_keys::init_media_keys(s.clone());

                    let config = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                    s.set_volume(config.general.volume);
                    s.set_playback_speed(config.general.playback_speed as f32);
                    {
                        let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                        queue.shuffle = config.general.shuffle;
                        if queue.shuffle {
                            queue.regenerate_shuffle_order();
                        }
                        queue.repeat_mode = match config.general.repeat_mode.as_str() {
                            "all" => crate::app_state::RepeatMode::All,
                            "one" => crate::app_state::RepeatMode::One,
                            _ => crate::app_state::RepeatMode::None,
                        };
                    }
                    {
                        let sc = s.clone();
                        if let Ok(engine) = s.engine.lock() {
                            if let Some(ref engine) = *engine {
                                let sc2 = sc.clone();
                                engine.on_state_changed(Box::new(move |new_state| {
                                    *sc2.player_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                        new_state;
                                }));
                                let eos_state = sc.clone();
                                engine.on_end_of_stream(Box::new(move || {
                                    tracing::debug!("End of stream - setting EOS flag");
                                    eos_state
                                        .eos_flag
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                }));
                            }
                        }
                    }
                    let gen = *signals.playback.read();
                    signals.playback.set(gen.wrapping_add(1));
                    let gen = *signals.ui.read();
                    signals.ui.set(gen.wrapping_add(1));
                }
                Err(e) => tracing::error!("Failed to initialize audio engine: {}", e),
            }
            match s.open_database() {
                Ok(()) => {
                    tracing::info!("Database opened successfully");
                    s.db_ready.store(true, std::sync::atomic::Ordering::Relaxed);
                    s.load_loved_tracks_from_db();
                    let config = s.config.read().unwrap_or_else(|e| e.into_inner()).clone();
                    if config.library.rescan_on_startup
                        && !s.is_scanning.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        let scan_db = {
                            let db = s.db.read().unwrap_or_else(|e| e.into_inner());
                            db.clone()
                        };
                        if let Some(db) = scan_db {
                            let watch_paths: Vec<std::path::PathBuf> = config
                                .library
                                .watch_dirs
                                .iter()
                                .map(|p| expand_tilde(p))
                                .collect();
                            if !watch_paths.is_empty() {
                                let state_ref = s.clone();
                                let mut lib_signal = signals.library;
                                s.is_scanning
                                    .store(true, std::sync::atomic::Ordering::Relaxed);
                                let state_ref2 = state_ref.clone();
                                dioxus::prelude::spawn(async move {
                                    let (added, removed) = tokio::task::spawn_blocking(move || {
                                        let result = std::panic::catch_unwind(
                                            std::panic::AssertUnwindSafe(|| {
                                                let scanner = tunecraft_core::library::scanner::LibraryScanner::new(watch_paths);
                                                scanner.scan_and_import_with_mood(
                                                    &db,
                                                    state_ref.pcm_cache.clone(),
                                                )
                                            }),
                                        );
                                        match result {
                                            Ok(res) => res,
                                            Err(panic_info) => {
                                                tracing::error!("Library scanner panicked: {:?}", panic_info);
                                                (0, 0)
                                            }
                                        }
                                    }).await.unwrap_or((0, 0));
                                    tracing::info!(
                                        "Startup scan: {} added, {} removed",
                                        added,
                                        removed
                                    );

                                    state_ref2
                                        .is_scanning
                                        .store(false, std::sync::atomic::Ordering::Relaxed);
                                    state_ref2
                                        .sidebar_cache_valid
                                        .store(false, std::sync::atomic::Ordering::Relaxed);
                                    let gen = *lib_signal.read();
                                    lib_signal.set(gen.wrapping_add(1));
                                });
                            }
                        }
                    }
                    let gen = *signals.library.read();
                    signals.library.set(gen.wrapping_add(1));
                    let gen = *signals.ui.read();
                    signals.ui.set(gen.wrapping_add(1));
                }
                Err(e) => tracing::warn!("Failed to open database: {} - library will be empty", e),
            }
        });
    }

    {
        let mut signals = signals;
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let s = state.read().clone();
                let prev_index = s.queue_lock().current_index;
                s.engine_tick();
                let gen = *signals.playback.read();
                signals.playback.set(gen.wrapping_add(1));
                let new_index = s.queue_lock().current_index;
                if prev_index != new_index {
                    let gen = *signals.queue.read();
                    signals.queue.set(gen.wrapping_add(1));
                    crate::media_keys::update_media_metadata(&s);
                    crate::media_keys::update_playback_status(&s);
                }
            }
        });
    }

    {
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                let s = state.read().clone();
                if s.is_playing() {
                    let should_scrobble = {
                        let mut scrobble = s.scrobble.lock().unwrap_or_else(|e| e.into_inner());
                        scrobble.accumulated_secs += 15;
                        let threshold = {
                            let dur_secs = s.duration().map(|d| d.as_secs()).unwrap_or(0);
                            let percent_threshold = (dur_secs as f64 * 0.5) as u64;
                            let absolute_threshold = 240u64;
                            if percent_threshold > 0 {
                                percent_threshold.min(absolute_threshold)
                            } else {
                                absolute_threshold
                            }
                        };
                        if !scrobble.submitted && scrobble.accumulated_secs >= threshold {
                            scrobble.submitted = true;
                            true
                        } else {
                            false
                        }
                    };
                    if should_scrobble {
                        let track_id = s
                            .scrobble
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .track_id;
                        let db = s.db.read().unwrap_or_else(|e| e.into_inner());
                        if let Some(ref db) = *db {
                            if let Some(track_id) = track_id {
                                let track_info = {
                                    let queue = s.queue_lock();
                                    queue.current_track().map(|t| {
                                        (
                                            t.artist.clone().unwrap_or_default(),
                                            t.title.clone().unwrap_or_default(),
                                            t.album.clone(),
                                        )
                                    })
                                };
                                if let Some((artist, title, album)) = track_info {
                                    let timestamp = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs()
                                        as i64;
                                    if let Err(e) = db.queue_scrobble(
                                        track_id,
                                        &artist,
                                        &title,
                                        album.as_deref(),
                                        timestamp,
                                    ) {
                                        tracing::warn!("Failed to queue scrobble: {}", e);
                                    }
                                }
                                if let Err(e) = db.increment_play_count(track_id) {
                                    tracing::warn!("Failed to increment play count: {}", e);
                                }
                                s.sidebar_cache_valid
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
        });
    }

    let _ = *signals.ui.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);
    let eq_visible = state
        .read()
        .eq_visible
        .load(std::sync::atomic::Ordering::Relaxed);
    let filter_visible = state
        .read()
        .filter_visible
        .load(std::sync::atomic::Ordering::Relaxed);
    let queue_visible = state
        .read()
        .queue_visible
        .load(std::sync::atomic::Ordering::Relaxed);
    let notifications_visible = state
        .read()
        .notifications_visible
        .load(std::sync::atomic::Ordering::Relaxed);

    let engine_ready = state
        .read()
        .engine_ready
        .load(std::sync::atomic::Ordering::Relaxed);
    let db_ready = state
        .read()
        .db_ready
        .load(std::sync::atomic::Ordering::Relaxed);
    let is_loading = !engine_ready || !db_ready;

    rsx! {
        style { {styles::TUNECRAFT_CSS} }
        div {
            class: if dark { "app-container dark" } else { "app-container light" },

            if is_loading {
                div { class: "loading-overlay",
                    div { class: "loading-spinner" }
                    div { class: "loading-text", "Loading TuneCraft..." }
                }
            }

            div { class: "main-layout",
                sidebar::Sidebar {}
                div { class: "content-area",
                    topbar::TopBar {}
                    track_list::TrackList {}
                }
            }
            playback_bar::PlaybackBar {}

            if eq_visible {
                eq_panel::EqPanel {}
            }
            if filter_visible {
                filter_panel::FilterPanel {}
            }
            if queue_visible {
                queue_panel::QueuePanel {}
            }
            if notifications_visible {
                notifications_panel::NotificationsPanel {}
            }

            context_menu::ContextMenuOverlay {}
        }
    }
}

/// Expand tilde in path strings.
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path == "~" {
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home;
        }
    } else if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(stripped);
        }
    }
    std::path::PathBuf::from(path)
}
