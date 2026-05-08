//! Playback bar component at the bottom of the screen.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, RepeatMode};

const SLIDER_DEBOUNCE_MS: u64 = 50;

/// Playback bar component.
pub fn PlaybackBar() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let mut signals: ReactivitySignals = use_context();
    let _ = *signals.playback.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);

    let current_track = {
        let state_ref = state.read();
        let queue = state_ref.queue_lock();
        queue.current_track().map(|t| {
            (
                t.title.clone().unwrap_or_default(),
                t.artist.clone().unwrap_or_default(),
                t.album.clone().unwrap_or_default(),
            )
        })
    };

    let is_playing = state.read().is_playing();
    let volume = state.read().volume();
    let is_muted = state
        .read()
        .volume_muted
        .load(std::sync::atomic::Ordering::Relaxed);
    let shuffle = state.read().queue_lock().shuffle;
    let repeat_mode = state.read().queue_lock().repeat_mode;

    let position = state.read().position().unwrap_or_default();
    let duration = state.read().duration().unwrap_or_default();
    let pos_secs = position.as_secs();
    let dur_secs = duration.as_secs();
    let progress = if dur_secs > 0 {
        pos_secs as f64 / dur_secs as f64
    } else {
        0.0
    };

    let pos_str = format!("{}:{:02}", pos_secs / 60, pos_secs % 60);
    let dur_str = format!("{}:{:02}", dur_secs / 60, dur_secs % 60);

    let (title, artist, _album) = current_track.unwrap_or_default();

    let mut seek_gen: Signal<u64> = use_signal(|| 0);
    let mut volume_gen: Signal<u64> = use_signal(|| 0);

    rsx! {
        div {
            class: if dark { "playback-bar dark" } else { "playback-bar light" },
            role: "region",
            aria_label: "Playback controls",

            div { class: "pb-track-info",
                div { class: "pb-track-art", "♫" }
                div { class: "pb-track-text",
                    div { class: "pb-track-title", "{title}" }
                    div { class: "pb-track-artist", "{artist}" }
                }
                button {
                    class: {
                        let state_ref = state.read();
                        let track = state_ref.queue_lock();
                        let track = track.current_track();
                        let is_loved = if let Some(t) = track.as_ref() {
                            let key = t.file_hash.as_deref().unwrap_or(t.file_path.as_str());
                            state_ref.loved_tracks.lock().unwrap_or_else(|e| e.into_inner()).contains(key)
                        } else {
                            false
                        };
                        if track.is_some() && is_loved { "pb-love-btn loved" } else { "pb-love-btn" }
                    },
                    aria_label: {
                        let state_ref2 = state.read();
                        let q = state_ref2.queue_lock();
                        let is_loved = q.current_track().map(|t| {
                            let key = t.file_hash.as_deref().unwrap_or(t.file_path.as_str());
                            state_ref2.loved_tracks.lock().unwrap_or_else(|e| e.into_inner()).contains(key)
                        }).unwrap_or(false);
                        if q.current_track().is_some() && is_loved { "Unlove track" } else { "Love track" }
                    },
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let track = s.queue_lock().current_track().cloned();
                        if let Some(t) = track {
                            let key = t.file_hash.clone().unwrap_or_else(|| t.file_path.clone());
                            s.toggle_track_loved(&key);
                        }
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            let track = s.queue_lock().current_track().cloned();
                            if let Some(t) = track {
                                let key = t.file_hash.clone().unwrap_or_else(|| t.file_path.clone());
                                s.toggle_track_loved(&key);
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "♥"
                }
            }

            div { class: "pb-controls",
                div { class: "pb-main-controls",
                    button {
                        class: if shuffle { "pb-ctrl-btn active" } else { "pb-ctrl-btn" },
                        aria_label: if shuffle { "Disable shuffle" } else { "Enable shuffle" },
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                            queue.toggle_shuffle();
                            drop(queue);
                            let gen = *signals.queue.read();
                            signals.queue.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                                queue.toggle_shuffle();
                                drop(queue);
                                let gen = *signals.queue.read();
                                signals.queue.set(gen.wrapping_add(1));
                            }
                        },
                        "⇄"
                    }

                    button {
                        class: "pb-ctrl-btn",
                        aria_label: "Previous track",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            s.prev_track();
                            s.notify_track_change();
                            let gen = *signals.queue.read();
                            signals.queue.set(gen.wrapping_add(1));
                            let gen = *signals.playback.read();
                            signals.playback.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                s.prev_track();
                                s.notify_track_change();
                                let gen = *signals.queue.read();
                                signals.queue.set(gen.wrapping_add(1));
                                let gen = *signals.playback.read();
                                signals.playback.set(gen.wrapping_add(1));
                            }
                        },
                        "⏮"
                    }

                    button {
                        class: "pb-play-btn",
                        aria_label: if is_playing { "Pause" } else { "Play" },
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            s.toggle_playback();
                            let gen = *signals.playback.read();
                            signals.playback.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                s.toggle_playback();
                                let gen = *signals.playback.read();
                                signals.playback.set(gen.wrapping_add(1));
                            }
                        },
                        if is_playing { "⏸" } else { "▶" }
                    }

                    button {
                        class: "pb-ctrl-btn",
                        aria_label: "Next track",
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            s.next_track();
                            s.notify_track_change();
                            let gen = *signals.queue.read();
                            signals.queue.set(gen.wrapping_add(1));
                            let gen = *signals.playback.read();
                            signals.playback.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                s.next_track();
                                s.notify_track_change();
                                let gen = *signals.queue.read();
                                signals.queue.set(gen.wrapping_add(1));
                                let gen = *signals.playback.read();
                                signals.playback.set(gen.wrapping_add(1));
                            }
                        },
                        "⏭"
                    }

                    button {
                        class: if repeat_mode != RepeatMode::None { "pb-ctrl-btn active" } else { "pb-ctrl-btn" },
                        aria_label: match repeat_mode {
                            RepeatMode::One => "Repeat one",
                            RepeatMode::All => "Repeat all",
                            RepeatMode::None => "No repeat",
                        },
                        tabindex: "0",
                        onclick: move |_| {
                            let s = state.read().clone();
                            let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                            queue.cycle_repeat();
                            drop(queue);
                            let gen = *signals.queue.read();
                            signals.queue.set(gen.wrapping_add(1));
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                                let s = state.read().clone();
                                let mut queue = s.queue.lock().unwrap_or_else(|e| e.into_inner());
                                queue.cycle_repeat();
                                drop(queue);
                                let gen = *signals.queue.read();
                                signals.queue.set(gen.wrapping_add(1));
                            }
                        },
                        match repeat_mode {
                            RepeatMode::One => "🔂",
                            RepeatMode::All => "🔁",
                            RepeatMode::None => "➡️",
                        }
                    }
                }

                div { class: "pb-progress",
                    span { class: "pb-time", "{pos_str}" }
                    div { class: "pb-progress-bar",
                        div {
                            class: "pb-progress-fill",
                            style: "width: {progress * 100.0}%",
                        }
                        input {
                            r#type: "range",
                            class: "pb-progress-slider",
                            min: "0",
                            max: "1000",
                            value: "{(progress * 1000.0) as i32}",
                            aria_label: "Seek",
                            oninput: move |e| {
                                let ratio: f64 = e.value().parse().unwrap_or(0.0) / 1000.0;
                                let gen = *seek_gen.read() + 1;
                                seek_gen.set(gen);
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(SLIDER_DEBOUNCE_MS)).await;
                                    if *seek_gen.read() == gen {
                                        let s = state.read().clone();
                                        if let Some(duration) = s.duration() {
                                            let pos = std::time::Duration::from_secs_f64(duration.as_secs_f64() * ratio);
                                            if let Ok(engine) = s.engine.lock() {
                                                if let Some(ref engine) = *engine {
                                                    let _ = engine.seek(pos);
                                                }
                                            }
                                        }
                                    }
                                });
                            },
                        }
                    }
                    span { class: "pb-time", "{dur_str}" }
                }
            }

            div { class: "pb-right",
                button {
                    class: "pb-ctrl-btn",
                    aria_label: if is_muted || volume == 0.0 { "Unmute" } else { "Mute" },
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let muted = s.volume_muted.load(std::sync::atomic::Ordering::Relaxed);
                        if muted {
                            let prev_vol = f64::from_bits(s.volume_before_mute.load(std::sync::atomic::Ordering::Relaxed));
                            s.set_volume(prev_vol);
                            s.volume_muted.store(false, std::sync::atomic::Ordering::Relaxed);
                        } else {
                            let current_vol = s.volume();
                            s.volume_before_mute.store(current_vol.to_bits(), std::sync::atomic::Ordering::Relaxed);
                            s.set_volume(0.0);
                            s.volume_muted.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        let gen = *signals.playback.read();
                        signals.playback.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            let muted = s.volume_muted.load(std::sync::atomic::Ordering::Relaxed);
                            if muted {
                                let prev_vol = f64::from_bits(s.volume_before_mute.load(std::sync::atomic::Ordering::Relaxed));
                                s.set_volume(prev_vol);
                                s.volume_muted.store(false, std::sync::atomic::Ordering::Relaxed);
                            } else {
                                let current_vol = s.volume();
                                s.volume_before_mute.store(current_vol.to_bits(), std::sync::atomic::Ordering::Relaxed);
                                s.set_volume(0.0);
                                s.volume_muted.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            let gen = *signals.playback.read();
                            signals.playback.set(gen.wrapping_add(1));
                        }
                    },
                    if is_muted || volume == 0.0 { "🔇" }
                    else if volume < 0.3 { "🔈" }
                    else if volume < 0.7 { "🔉" }
                    else { "🔊" }
                }
                input {
                    r#type: "range",
                    class: "pb-volume-slider",
                    min: "0",
                    max: "100",
                    value: "{(volume * 100.0) as i32}",
                    aria_label: "Volume",
                    oninput: move |e| {
                        let vol: f64 = e.value().parse().unwrap_or(volume * 100.0) / 100.0;
                        let gen = *volume_gen.read() + 1;
                        volume_gen.set(gen);
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(SLIDER_DEBOUNCE_MS)).await;
                            if *volume_gen.read() == gen {
                                let s = state.read().clone();
                                s.set_volume(vol);
                                s.volume_muted.store(false, std::sync::atomic::Ordering::Relaxed);
                            }
                        });
                    },
                }
                button {
                    class: "pb-ctrl-btn",
                    aria_label: "Toggle queue panel",
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        let visible = s.queue_visible.load(std::sync::atomic::Ordering::Relaxed);
                        s.queue_visible.store(!visible, std::sync::atomic::Ordering::Relaxed);
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            let visible = s.queue_visible.load(std::sync::atomic::Ordering::Relaxed);
                            s.queue_visible.store(!visible, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "☰"
                }
            }
        }
    }
}
