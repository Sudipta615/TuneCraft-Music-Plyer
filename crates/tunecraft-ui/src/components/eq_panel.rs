//! Equalizer panel component with 10-band EQ, presets, and advanced controls.

use dioxus::prelude::*;
use std::sync::Arc;

use crate::app::ReactivitySignals;
use crate::app_state::{AppState, EqPreset};
use crate::i18n::tr;

/// EQ Panel overlay component.
pub fn EqPanel() -> Element {
    let state: Signal<Arc<AppState>> = use_context();
    let signals: ReactivitySignals = use_context();
    let _ = *signals.ui.read();

    let dark = state
        .read()
        .dark_mode
        .load(std::sync::atomic::Ordering::Relaxed);
    let eq_enabled = state
        .read()
        .eq_enabled
        .load(std::sync::atomic::Ordering::Relaxed);
    let bands = *state
        .read()
        .eq_bands
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let current_preset = *state
        .read()
        .eq_preset
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let bass_db = *state
        .read()
        .eq_bass_db
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let treble_db = *state
        .read()
        .eq_treble_db
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stereo_width = *state
        .read()
        .eq_stereo_width
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let balance = *state
        .read()
        .eq_balance
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dither_enabled = state
        .read()
        .eq_dither_enabled
        .load(std::sync::atomic::Ordering::Relaxed);
    let ms_enabled = state
        .read()
        .eq_ms_enabled
        .load(std::sync::atomic::Ordering::Relaxed);
    let preamp = *state
        .read()
        .eq_preamp
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let freq_labels = [
        "32", "64", "125", "250", "500", "1K", "2K", "4K", "8K", "16K",
    ];
    let preset_options = EqPreset::all();

    rsx! {
        div { class: "overlay-panel eq-panel",
            class: if dark { "dark" } else { "light" },
            role: "dialog",
            aria_label: "Equalizer panel",

            div { class: "eq-header",
                h3 { class: "eq-title", "EQ" }

                label { class: "eq-toggle",
                    input {
                        r#type: "checkbox",
                        checked: eq_enabled,
                        aria_label: "Enable equalizer",
                        onchange: move |_| {
                            let s = state.read().clone();
                            let enabled = s.eq_enabled.load(std::sync::atomic::Ordering::Relaxed);
                            let new_enabled = !enabled;
                            s.eq_enabled.store(new_enabled, std::sync::atomic::Ordering::Relaxed);
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_eq_enabled(new_enabled);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{tr(\"Enable Equalizer\")}" }
                }

                select {
                    class: "eq-preset-select",
                    aria_label: "EQ preset",
                    value: current_preset.label(),
                    onchange: move |e| {
                        let name = e.value().clone();
                        let s = state.read().clone();
                        let preset = match name.as_str() {
                            "Flat" => EqPreset::Flat,
                            "Bass Boost" => EqPreset::BassBoost,
                            "Treble Boost" => EqPreset::TrebleBoost,
                            "Vocal" => EqPreset::Vocal,
                            "Rock" => EqPreset::Rock,
                            "Pop" => EqPreset::Pop,
                            "Jazz" => EqPreset::Jazz,
                            "Classical" => EqPreset::Classical,
                            "Electronic" => EqPreset::Electronic,
                            _ => EqPreset::Custom,
                        };
                        let gains = preset.gains();
                        *s.eq_bands.lock().unwrap_or_else(|e| e.into_inner()) = gains;
                        *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = preset;
                        if let Ok(mut engine) = s.engine.lock() {
                            if let Some(ref engine) = *engine {
                                for (i, &g) in gains.iter().enumerate() {
                                    engine.set_eq_band_gain(i, g as f64);
                                }
                            }
                        }
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    for preset in preset_options {
                        option {
                            value: "{preset.label()}",
                            selected: *preset == current_preset,
                            "{preset.label()}"
                        }
                    }
                }

                button {
                    class: "eq-close-btn",
                    aria_label: "Close equalizer panel",
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            s.eq_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "✕"
                }
            }

            div { class: "eq-bands",
                div { class: "eq-db-scale",
                    span { "+12dB" }
                    span { "0dB" }
                    span { "-12dB" }
                }

                div { class: "eq-sliders-container",
                    for (band_idx, freq) in freq_labels.iter().enumerate() {
                        {
                            let gain = bands[band_idx];
                            let band_idx_for_closure = band_idx;
                            rsx! {
                                div { class: "eq-band", key: "{band_idx}",
                                    div { class: "eq-band-value", "{gain:.1}" }
                                    input {
                                        r#type: "range",
                                        class: "eq-band-slider vertical",
                                        min: "-12",
                                        max: "12",
                                        step: "0.5",
                                        value: "{gain}",
                                        aria_label: "{freq} Hz band, {gain:.1} dB",
                                        onchange: move |e| {
                                            let gain: f32 = e.value().parse().unwrap_or(0.0);
                                            let s = state.read().clone();
                                            {
                                                let mut bands = s.eq_bands.lock().unwrap_or_else(|e| e.into_inner());
                                                if band_idx_for_closure < 10 {
                                                    bands[band_idx_for_closure] = gain;
                                                }
                                                *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = EqPreset::Custom;
                                            }
                                            if let Ok(mut engine) = s.engine.lock() {
                                                if let Some(ref engine) = *engine {
                                                    engine.set_eq_band_gain(band_idx_for_closure, gain as f64);
                                                }
                                            }
                                            let gen = *signals.ui.read();
                                            signals.ui.set(gen.wrapping_add(1));
                                        },
                                    }
                                    span { class: "eq-band-label", "{freq}" }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "eq-secondary",
                div { class: "eq-secondary-item",
                    label { "{tr(\"Bass\")}" }
                    small { "{tr(\"Shelf\")}" }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        step: "0.5",
                        value: "{bass_db}",
                        aria_label: "Bass shelf, {bass_db:.1} dB",
                        onchange: move |e| {
                            let gain: f32 = e.value().parse().unwrap_or(0.0);
                            let s = state.read().clone();
                            *s.eq_bass_db.lock().unwrap_or_else(|e| e.into_inner()) = gain;
                            *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = EqPreset::Custom;
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_bass(gain as f64);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{bass_db:.1} dB" }
                }

                div { class: "eq-secondary-item",
                    label { "{tr(\"Treble\")}" }
                    small { "{tr(\"Shelf\")}" }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        step: "0.5",
                        value: "{treble_db}",
                        aria_label: "Treble shelf, {treble_db:.1} dB",
                        onchange: move |e| {
                            let gain: f32 = e.value().parse().unwrap_or(0.0);
                            let s = state.read().clone();
                            *s.eq_treble_db.lock().unwrap_or_else(|e| e.into_inner()) = gain;
                            *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = EqPreset::Custom;
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_treble(gain as f64);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{treble_db:.1} dB" }
                }

                div { class: "eq-secondary-item",
                    label { "{tr(\"Stereo Width\")}" }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "300",
                        step: "1",
                        value: "{(stereo_width * 100.0).clamp(0.0, 300.0) as i32}",
                        aria_label: "Stereo width, {(stereo_width * 100.0).clamp(0.0, 300.0) as i32} percent",
                        onchange: move |e| {
                            let width: f32 = e.value().parse::<f32>().unwrap_or(stereo_width * 100.0).clamp(0.0, 300.0) / 100.0;
                            let s = state.read().clone();
                            *s.eq_stereo_width.lock().unwrap_or_else(|e| e.into_inner()) = width;
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_stereo_width(width as f64);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{(stereo_width * 100.0).clamp(0.0, 300.0) as i32}%" }
                }

                div { class: "eq-secondary-item",
                    label { "{tr(\"Balance\")}" }
                    input {
                        r#type: "range",
                        min: "-100",
                        max: "100",
                        step: "1",
                        value: "{(balance * 100.0).clamp(-100.0, 100.0) as i32}",
                        aria_label: "Balance, {balance:.2}",
                        onchange: move |e| {
                            let bal: f32 = e.value().parse::<f32>().unwrap_or(balance * 100.0).clamp(-100.0, 100.0) / 100.0;
                            let s = state.read().clone();
                            *s.eq_balance.lock().unwrap_or_else(|e| e.into_inner()) = bal;
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_balance(bal as f64);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{balance:.2}" }
                }

                div { class: "eq-toggles",
                    label { class: "eq-toggle",
                        input {
                            r#type: "checkbox",
                            checked: dither_enabled,
                            aria_label: "Dither toggle",
                            onchange: move |_| {
                                let s = state.read().clone();
                                let enabled = s.eq_dither_enabled.load(std::sync::atomic::Ordering::Relaxed);
                                let new_enabled = !enabled;
                                s.eq_dither_enabled.store(new_enabled, std::sync::atomic::Ordering::Relaxed);
                                if let Ok(mut engine) = s.engine.lock() {
                                    if let Some(ref engine) = *engine {
                                        engine.set_dither_enabled(new_enabled);
                                    }
                                }
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            },
                        }
                        span { "{tr(\"Dither\")}" }
                    }

                    label { class: "eq-toggle",
                        input {
                            r#type: "checkbox",
                            checked: ms_enabled,
                            aria_label: "Mid/Side EQ toggle",
                            onchange: move |_| {
                                let s = state.read().clone();
                                let enabled = s.eq_ms_enabled.load(std::sync::atomic::Ordering::Relaxed);
                                let new_enabled = !enabled;
                                s.eq_ms_enabled.store(new_enabled, std::sync::atomic::Ordering::Relaxed);
                                if let Ok(mut engine) = s.engine.lock() {
                                    if let Some(ref engine) = *engine {
                                        let mut eq_state = engine.eq_state();
                                        eq_state.ms_eq_enabled = new_enabled;
                                        engine.set_eq_state(eq_state);
                                    }
                                }
                                let gen = *signals.ui.read();
                                signals.ui.set(gen.wrapping_add(1));
                            },
                        }
                        span { "{tr(\"Mid/Side EQ\")}" }
                    }
                }
            }

            div { class: "eq-footer",
                div { class: "eq-preamp",
                    label { "{tr(\"Preamp\")}" }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        step: "0.5",
                        value: "{preamp}",
                        aria_label: "Preamp, {preamp:.1} dB",
                        onchange: move |e| {
                            let gain: f32 = e.value().parse().unwrap_or(0.0);
                            let s = state.read().clone();
                            *s.eq_preamp.lock().unwrap_or_else(|e| e.into_inner()) = gain;
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    engine.set_replaygain_preamp_db(gain as f64);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        },
                    }
                    span { "{preamp:.1} dB" }
                }

                button {
                    class: "eq-reset-btn",
                    aria_label: "Reset equalizer to defaults",
                    tabindex: "0",
                    onclick: move |_| {
                        let s = state.read().clone();
                        *s.eq_bands.lock().unwrap_or_else(|e| e.into_inner()) = [0.0; 10];
                        *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = EqPreset::Flat;
                        *s.eq_bass_db.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                        *s.eq_treble_db.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                        *s.eq_stereo_width.lock().unwrap_or_else(|e| e.into_inner()) = 1.0;
                        *s.eq_balance.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                        *s.eq_preamp.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                        s.eq_dither_enabled.store(false, std::sync::atomic::Ordering::Relaxed);
                        s.eq_ms_enabled.store(false, std::sync::atomic::Ordering::Relaxed);
                        if let Ok(mut engine) = s.engine.lock() {
                            if let Some(ref engine) = *engine {
                                for i in 0..10 { engine.set_eq_band_gain(i, 0.0); }
                                engine.set_bass(0.0);
                                engine.set_treble(0.0);
                                engine.set_stereo_width(1.0);
                                engine.set_balance(0.0);
                                engine.set_dither_enabled(false);
                                engine.set_replaygain_preamp_db(0.0);
                                let mut eq_state = engine.eq_state();
                                eq_state.ms_eq_enabled = false;
                                engine.set_eq_state(eq_state);
                            }
                        }
                        let gen = *signals.ui.read();
                        signals.ui.set(gen.wrapping_add(1));
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter || e.key() == Key::Character(" ".into()) {
                            let s = state.read().clone();
                            *s.eq_bands.lock().unwrap_or_else(|e| e.into_inner()) = [0.0; 10];
                            *s.eq_preset.lock().unwrap_or_else(|e| e.into_inner()) = EqPreset::Flat;
                            *s.eq_bass_db.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                            *s.eq_treble_db.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                            *s.eq_stereo_width.lock().unwrap_or_else(|e| e.into_inner()) = 1.0;
                            *s.eq_balance.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                            *s.eq_preamp.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                            s.eq_dither_enabled.store(false, std::sync::atomic::Ordering::Relaxed);
                            s.eq_ms_enabled.store(false, std::sync::atomic::Ordering::Relaxed);
                            if let Ok(mut engine) = s.engine.lock() {
                                if let Some(ref engine) = *engine {
                                    for i in 0..10 { engine.set_eq_band_gain(i, 0.0); }
                                    engine.set_bass(0.0);
                                    engine.set_treble(0.0);
                                    engine.set_stereo_width(1.0);
                                    engine.set_balance(0.0);
                                    engine.set_dither_enabled(false);
                                    engine.set_replaygain_preamp_db(0.0);
                                    let mut eq_state = engine.eq_state();
                                    eq_state.ms_eq_enabled = false;
                                    engine.set_eq_state(eq_state);
                                }
                            }
                            let gen = *signals.ui.read();
                            signals.ui.set(gen.wrapping_add(1));
                        }
                    },
                    "↺ {tr(\"Reset\")}"
                }
            }
        }
    }
}
