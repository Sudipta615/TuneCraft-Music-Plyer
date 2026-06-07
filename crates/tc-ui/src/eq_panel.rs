//! EQ Panel — floating window with 10-band EQ, shelves, stereo controls, presets
//! Matches the reference design: back arrow + title, enable toggle, preset dropdown,
//! vertical band sliders, secondary sliders for Bass/Treble/Stereo/Balance, Dither/M-S toggles,
//! Preamp.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};
#[allow(unused_imports)]
use tc_config;

use crate::{app::TuneCraftApp, theme::TuneCraftColors};

const EQ_FREQUENCIES: [&str; 10] = [
    "32", "64", "125", "250", "500", "1000", "2000", "4000", "8000", "16000",
];

const EQ_PRESETS: [&str; 6] = [
    "Custom",
    "Flat",
    "Bass Boost",
    "Treble Boost",
    "V-Shape",
    "Vocal",
];

/// Draw the EQ panel (intended to be rendered inside an egui::Window)
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    // Fill the whole panel background
    let panel_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(panel_rect, 8.0, colors.surface);

    ui.vertical(|ui| {
        ui.add_space(12.0);

        // ── Header row: ← EQ   [Enable toggle]  [Custom ▾]  [⋮] ──
        ui.horizontal(|ui| {
            ui.add_space(14.0);

            // Back arrow / close — must update EqService so sync_from_eq_service() stays in sync
            let arrow_resp = ui.add(
                egui::Button::new(
                    RichText::new("\u{2190}")
                        .font(FontId::proportional(16.0))
                        .color(colors.text),
                )
                .frame(false),
            );
            if arrow_resp.clicked() {
                app.show_eq_panel = false;
                app.ctx.eq.state_mut().show_panel = false;
            }

            ui.add_space(8.0);

            // "EQ" title
            ui.label(
                RichText::new("EQ")
                    .font(FontId::proportional(18.0))
                    .color(colors.text)
                    .strong(),
            );

            ui.add_space(16.0);

            // Enable Equalizer toggle
            let (toggle_rect, toggle_resp) =
                ui.allocate_exact_size(Vec2::new(46.0, 24.0), Sense::click());
            let toggle_bg = if app.eq_enabled {
                colors.accent
            } else {
                colors.toggle_bg_off
            };
            ui.painter().rect_filled(toggle_rect, 12.0, toggle_bg);
            let knob_x = if app.eq_enabled {
                toggle_rect.right() - 12.0
            } else {
                toggle_rect.left() + 12.0
            };
            ui.painter().circle_filled(
                Pos2::new(knob_x, toggle_rect.center().y),
                9.0,
                Color32::WHITE,
            );
            if toggle_resp.clicked() {
                let new_enabled = !app.eq_enabled;
                app.ctx.eq.set_enabled(new_enabled);
                app.eq_enabled = new_enabled;
                // Persist to config
                app.ctx.config.write(|c| {
                    c.engine.eq.enabled = new_enabled;
                });
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("Enable Equalizer")
                    .font(FontId::proportional(13.0))
                    .color(colors.text),
            );

            // Spacer
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(14.0);

                // Three-dot menu placeholder
                ui.label(
                    RichText::new("\u{22EE}")
                        .font(FontId::proportional(16.0))
                        .color(colors.text_dim),
                );

                ui.add_space(8.0);

                // Preset dropdown (styled to match reference)
                egui::ComboBox::from_id_salt("eq_preset_combo")
                    .selected_text(
                        RichText::new(&app.eq_preset)
                            .font(FontId::proportional(12.0))
                            .color(colors.accent),
                    )
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for preset in EQ_PRESETS {
                            let is_selected = app.eq_preset == preset;
                            if ui.selectable_label(is_selected, preset).clicked() {
                                app.eq_preset = preset.to_string();
                                apply_preset(app, preset);
                            }
                        }
                    });
            });
        });

        ui.add_space(10.0);

        // ── 10-band EQ sliders ──
        let eq_area_h = 220.0;
        let available_w = ui.available_width();
        let (eq_rect, _) =
            ui.allocate_exact_size(Vec2::new(available_w, eq_area_h), Sense::hover());

        // Background card for slider area
        ui.painter().rect_filled(eq_rect, 6.0, colors.card);
        ui.painter()
            .rect_stroke(eq_rect, 6.0, egui::Stroke::new(1.0, colors.border));

        draw_eq_sliders(ui, app, eq_rect, &colors);

        ui.add_space(10.0);

        // ── Secondary controls: Bass Shelf | Treble Shelf | Stereo Width | Balance | Dither + M/S
        // ──
        let secondary_h = 130.0;
        let sec_available_w = ui.available_width();
        let (sec_rect, _) =
            ui.allocate_exact_size(Vec2::new(sec_available_w, secondary_h), Sense::hover());

        ui.painter().rect_filled(sec_rect, 6.0, colors.card);
        ui.painter()
            .rect_stroke(sec_rect, 6.0, egui::Stroke::new(1.0, colors.border));

        // Render secondary sliders INSIDE the sec_rect using allocate_new_ui
        {
            let toggles_w = 130.0;
            let slider_area_w = sec_rect.width() - toggles_w;
            let col_w = slider_area_w / 4.0;

            // Save values before the closure borrows app
            let old_bass = app.eq_bass_shelf;
            let old_treble = app.eq_treble_shelf;
            let old_width_pct = (app.eq_stereo_width * 100.0).clamp(0.0, 200.0);
            let old_bal = app.eq_balance;
            let old_dither = app.eq_dither;
            let old_midside = app.eq_midside;

            let mut new_bass = old_bass;
            let mut new_treble = old_treble;
            let mut new_width_pct = old_width_pct;
            let mut new_bal = old_bal;
            let mut new_dither = old_dither;
            let mut new_midside = old_midside;

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(sec_rect), |ui| {
                ui.horizontal(|ui| {
                    // Bass Shelf
                    secondary_slider_vertical(
                        ui,
                        "Bass",
                        "Shelf",
                        &mut new_bass,
                        -12.0,
                        12.0,
                        "dB",
                        col_w,
                        secondary_h - 20.0,
                        &colors,
                        |_| {},
                    );

                    // Vertical divider
                    let (d_rect, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, secondary_h - 20.0), Sense::hover());
                    ui.painter().rect_filled(d_rect, 0.0, colors.border);

                    // Treble Shelf
                    secondary_slider_vertical(
                        ui,
                        "Treble",
                        "Shelf",
                        &mut new_treble,
                        -12.0,
                        12.0,
                        "dB",
                        col_w,
                        secondary_h - 20.0,
                        &colors,
                        |_| {},
                    );

                    // Vertical divider
                    let (d_rect, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, secondary_h - 20.0), Sense::hover());
                    ui.painter().rect_filled(d_rect, 0.0, colors.border);

                    // Stereo Width
                    secondary_slider_vertical(
                        ui,
                        "Stereo Width",
                        "",
                        &mut new_width_pct,
                        0.0,
                        200.0,
                        "%",
                        col_w,
                        secondary_h - 20.0,
                        &colors,
                        |_| {},
                    );

                    // Vertical divider
                    let (d_rect, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, secondary_h - 20.0), Sense::hover());
                    ui.painter().rect_filled(d_rect, 0.0, colors.border);

                    // Balance
                    secondary_slider_vertical(
                        ui,
                        "Balance",
                        "",
                        &mut new_bal,
                        -1.0,
                        1.0,
                        "",
                        col_w,
                        secondary_h - 20.0,
                        &colors,
                        |_| {},
                    );

                    // Vertical divider
                    let (d_rect, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, secondary_h - 20.0), Sense::hover());
                    ui.painter().rect_filled(d_rect, 0.0, colors.border);

                    // Toggles column: Dither + Mid/Side EQ
                    ui.vertical(|ui| {
                        ui.add_space(16.0);
                        draw_labeled_toggle(ui, "Dither", &mut new_dither, &colors);
                        ui.add_space(16.0);
                        draw_labeled_toggle(ui, "Mid/Side EQ", &mut new_midside, &colors);
                    });
                });
            });

            // Apply changes after the UI closure
            if (new_bass - old_bass).abs() > 0.001 {
                app.eq_bass_shelf = new_bass;
                app.eq_bands[0] = new_bass;
                app.ctx
                    .eq
                    .set_band_with_params(0, 31.25, new_bass, 1.4, app.eq_enabled);
            }
            if (new_treble - old_treble).abs() > 0.001 {
                app.eq_treble_shelf = new_treble;
                app.eq_bands[9] = new_treble;
                app.ctx
                    .eq
                    .set_band_with_params(9, 16000.0, new_treble, 1.4, app.eq_enabled);
            }
            if (new_width_pct - old_width_pct).abs() > 0.5 {
                let new_w = (new_width_pct / 100.0).clamp(0.0, 2.0);
                app.eq_stereo_width = new_w;
                app.ctx.eq.set_stereo_width(new_w);
            }
            if (new_bal - old_bal).abs() > 0.005 {
                app.eq_balance = new_bal;
                app.ctx.eq.set_balance(new_bal);
            }
            if new_dither != old_dither {
                app.eq_dither = new_dither;
                app.cached_dither_enabled = new_dither;
                app.ctx.eq.set_dither(new_dither);
                app.ctx.config.write(|c| {
                    c.engine.dither_enabled = new_dither;
                });
            }
            if new_midside != old_midside {
                app.eq_midside = new_midside;
                app.cached_midside_enabled = new_midside;
                app.ctx.eq.set_midside(new_midside);
            }
        }
        ui.add_space(10.0);

        // ── Preamp row ──
        let preamp_h = 44.0;
        let preamp_w = ui.available_width();
        let (preamp_rect, _) =
            ui.allocate_exact_size(Vec2::new(preamp_w, preamp_h), Sense::hover());

        ui.painter().rect_filled(preamp_rect, 6.0, colors.card);
        ui.painter()
            .rect_stroke(preamp_rect, 6.0, egui::Stroke::new(1.0, colors.border));

        // Draw preamp content directly using painter (no child UI needed)
        {
            ui.painter().text(
                Pos2::new(preamp_rect.left() + 12.0, preamp_rect.center().y - 6.0),
                Align2::LEFT_CENTER,
                "Preamp",
                FontId::proportional(11.0),
                colors.text_dim,
            );
            ui.painter().text(
                Pos2::new(preamp_rect.left() + 12.0, preamp_rect.center().y + 8.0),
                Align2::LEFT_CENTER,
                "-12dB",
                FontId::proportional(9.0),
                colors.text_dim,
            );

            // Slider
            let slider_x_start = preamp_rect.left() + 76.0;
            let slider_x_end = preamp_rect.right() - 120.0;
            let slider_y = preamp_rect.center().y;
            let slider_w = slider_x_end - slider_x_start;

            let norm = ((app.eq_preamp + 12.0) / 24.0).clamp(0.0, 1.0) as f32;
            let fill_x = slider_x_start + slider_w * norm;

            // Track
            ui.painter().line_segment(
                [
                    Pos2::new(slider_x_start, slider_y),
                    Pos2::new(slider_x_end, slider_y),
                ],
                egui::Stroke::new(3.0, colors.slider_track),
            );
            // Fill
            ui.painter().line_segment(
                [
                    Pos2::new(slider_x_start, slider_y),
                    Pos2::new(fill_x, slider_y),
                ],
                egui::Stroke::new(3.0, colors.accent),
            );
            // Knob
            ui.painter()
                .circle_filled(Pos2::new(fill_x, slider_y), 7.0, colors.accent);
            ui.painter().circle_stroke(
                Pos2::new(fill_x, slider_y),
                7.0,
                egui::Stroke::new(1.5, colors.card),
            );

            // Drag area
            let drag_rect = Rect::from_min_size(
                Pos2::new(slider_x_start - 8.0, preamp_rect.top()),
                Vec2::new(slider_w + 16.0, preamp_h),
            );
            let drag_resp = ui.interact(drag_rect, egui::Id::new("preamp_slider"), Sense::drag());
            if drag_resp.dragged() {
                if let Some(ptr) = drag_resp.interact_pointer_pos() {
                    let t = ((ptr.x - slider_x_start) / slider_w).clamp(0.0, 1.0);
                    let new_preamp = (t * 24.0 - 12.0) as f32;
                    app.eq_preamp = new_preamp;
                    app.ctx.eq.set_preamp(new_preamp);
                }
            }

            // Current value label below knob
            ui.painter().text(
                Pos2::new(fill_x, slider_y + 12.0),
                Align2::CENTER_CENTER,
                format!("{:.1} dB", app.eq_preamp),
                FontId::proportional(9.0),
                colors.text_dim,
            );

            // "+12dB" on right
            ui.painter().text(
                Pos2::new(slider_x_end + 8.0, preamp_rect.center().y),
                Align2::LEFT_CENTER,
                "+12dB",
                FontId::proportional(9.0),
                colors.text_dim,
            );

            // Reset button (right side)
            let reset_x = preamp_rect.right() - 70.0;
            let reset_rect = Rect::from_min_size(
                Pos2::new(reset_x, preamp_rect.top() + 8.0),
                Vec2::new(50.0, 28.0),
            );
            let reset_resp = ui.interact(reset_rect, egui::Id::new("eq_reset_btn"), Sense::click());
            ui.painter().text(
                reset_rect.center(),
                Align2::CENTER_CENTER,
                "Reset",
                FontId::proportional(12.0),
                colors.accent,
            );
            // Refresh icon
            ui.painter().text(
                Pos2::new(reset_rect.right() + 14.0, reset_rect.center().y),
                Align2::CENTER_CENTER,
                "\u{21BA}",
                FontId::proportional(14.0),
                colors.accent,
            );
            if reset_resp.clicked() {
                reset_eq(app);
            }
        }

        ui.add_space(8.0);
    });
}

fn draw_eq_sliders(ui: &mut Ui, app: &mut TuneCraftApp, rect: Rect, colors: &TuneCraftColors) {
    let band_count = 10;
    let left_margin = 36.0; // for dB scale labels
    let bottom_margin = 22.0; // for frequency labels
    let slider_area = Rect::from_min_max(
        Pos2::new(rect.left() + left_margin, rect.top() + 8.0),
        Pos2::new(rect.right() - 8.0, rect.bottom() - bottom_margin),
    );
    let freq_values: [f32; 10] = [
        31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];

    // dB scale labels + grid lines
    for &db in &[12i32, 6, 0, -6, -12] {
        let norm = (db as f32 + 12.0) / 24.0;
        let y = slider_area.bottom() - slider_area.height() * norm;
        let label = if db == 0 {
            "0dB".to_string()
        } else {
            format!(
                "{}dB",
                if db > 0 {
                    format!("+{}", db)
                } else {
                    db.to_string()
                }
            )
        };
        ui.painter().text(
            Pos2::new(rect.left() + 4.0, y),
            Align2::LEFT_CENTER,
            &label,
            FontId::proportional(8.5),
            colors.text_dim,
        );
        // Grid line (subtle)
        let line_color = if db == 0 {
            Color32::from_rgba_premultiplied(
                colors.border.r(),
                colors.border.g(),
                colors.border.b(),
                150,
            )
        } else {
            Color32::from_rgba_premultiplied(
                colors.border.r(),
                colors.border.g(),
                colors.border.b(),
                60,
            )
        };
        ui.painter().line_segment(
            [
                Pos2::new(slider_area.left(), y),
                Pos2::new(slider_area.right(), y),
            ],
            egui::Stroke::new(if db == 0 { 1.0 } else { 0.5 }, line_color),
        );
    }

    let band_w = slider_area.width() / band_count as f32;

    for i in 0..band_count {
        let band_cx = slider_area.left() + band_w * i as f32 + band_w / 2.0;
        let track_top = slider_area.top();
        let track_bot = slider_area.bottom();

        // Track line (vertical, thin)
        ui.painter().line_segment(
            [Pos2::new(band_cx, track_top), Pos2::new(band_cx, track_bot)],
            egui::Stroke::new(3.0, colors.slider_track),
        );

        let gain = app.eq_bands[i];
        let zero_y = slider_area.top() + slider_area.height() * (1.0 - 0.5); // 0 dB line
        let norm = ((gain + 12.0) / 24.0).clamp(0.0, 1.0) as f32;
        let knob_y = slider_area.bottom() - slider_area.height() * norm;

        // Colored fill from 0dB to current
        let fill_color = colors.accent;
        if gain > 0.05 {
            ui.painter().line_segment(
                [Pos2::new(band_cx, knob_y), Pos2::new(band_cx, zero_y)],
                egui::Stroke::new(3.0, fill_color),
            );
        } else if gain < -0.05 {
            ui.painter().line_segment(
                [Pos2::new(band_cx, zero_y), Pos2::new(band_cx, knob_y)],
                egui::Stroke::new(3.0, fill_color),
            );
        }

        // Knob circle (with border ring matching dark/light theme)
        let knob_r = 8.0;
        let ring_color = if colors.dark_mode {
            Color32::from_rgb(0x2D, 0x34, 0x4C)
        } else {
            Color32::from_rgb(0xDF, 0xDF, 0xE5)
        };
        ui.painter()
            .circle_filled(Pos2::new(band_cx, knob_y), knob_r + 3.0, ring_color);
        ui.painter()
            .circle_filled(Pos2::new(band_cx, knob_y), knob_r, colors.accent);

        // Frequency label — scale font on narrow panels
        let freq_font_size = if rect.width() < 400.0 { 8.0 } else { 9.5 };
        ui.painter().text(
            Pos2::new(band_cx, rect.bottom() - bottom_margin / 2.0),
            Align2::CENTER_CENTER,
            EQ_FREQUENCIES[i],
            FontId::proportional(freq_font_size),
            colors.text_dim,
        );

        // Drag interaction
        let drag_rect = Rect::from_min_size(
            Pos2::new(band_cx - band_w / 2.0, track_top),
            Vec2::new(band_w, track_bot - track_top),
        );
        let drag_resp = ui.interact(
            drag_rect,
            egui::Id::new(format!("eq_band_{}", i)),
            Sense::drag(),
        );
        if drag_resp.dragged() {
            if let Some(ptr) = drag_resp.interact_pointer_pos() {
                let t = (ptr.y - track_top) / (track_bot - track_top);
                let new_norm = (1.0 - t).clamp(0.0, 1.0);
                let new_gain = (new_norm * 24.0 - 12.0) as f32;
                app.eq_bands[i] = new_gain.clamp(-12.0, 12.0);
                app.eq_preset = "Custom".to_string();
                if i == 0 {
                    app.eq_bass_shelf = new_gain;
                }
                if i == 9 {
                    app.eq_treble_shelf = new_gain;
                }
                app.ctx.eq.set_band_with_params(
                    i,
                    freq_values[i],
                    new_gain,
                    1.4,
                    app.eq_enabled && new_gain != 0.0,
                );
                // Persist band gain to config
                let band_idx = i;
                let gain_db = new_gain;
                let enabled = app.eq_enabled && gain_db != 0.0;
                let freq = freq_values[i];
                app.ctx.config.write(|c| {
                    while c.engine.eq.bands.len() <= band_idx {
                        c.engine.eq.bands.push(tc_config::EqBand::default());
                    }
                    c.engine.eq.bands[band_idx].gain_db = gain_db;
                    c.engine.eq.bands[band_idx].frequency = freq;
                    c.engine.eq.bands[band_idx].enabled = enabled;
                });
            }
        }
    }
}

/// Vertical slider with title + subtitle above, value label below
#[allow(clippy::too_many_arguments)]
fn secondary_slider_vertical(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    unit: &str,
    width: f32,
    height: f32,
    colors: &TuneCraftColors,
    _on_change: impl Fn(f32),
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let cx = rect.center().x;

    // Title
    ui.painter().text(
        Pos2::new(cx, rect.top() + 12.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(11.0),
        colors.text,
    );
    if !subtitle.is_empty() {
        ui.painter().text(
            Pos2::new(cx, rect.top() + 24.0),
            Align2::CENTER_CENTER,
            subtitle,
            FontId::proportional(9.0),
            colors.text_dim,
        );
    }

    // Vertical slider track
    let track_top = rect.top() + if subtitle.is_empty() { 32.0 } else { 38.0 };
    let track_bot = rect.bottom() - 22.0;
    let track_h = track_bot - track_top;

    if track_h <= 0.0 {
        return;
    }

    // Draw track
    ui.painter().line_segment(
        [Pos2::new(cx, track_top), Pos2::new(cx, track_bot)],
        egui::Stroke::new(3.0, colors.slider_track),
    );

    let norm = ((*value - min) / (max - min)).clamp(0.0, 1.0) as f32;
    let knob_y = track_bot - track_h * norm;
    let zero_y = track_bot - track_h * ((-min / (max - min)).clamp(0.0, 1.0) as f32);

    // Fill from zero
    if *value >= 0.0 {
        ui.painter().line_segment(
            [Pos2::new(cx, knob_y), Pos2::new(cx, zero_y)],
            egui::Stroke::new(3.0, colors.accent),
        );
    } else {
        ui.painter().line_segment(
            [Pos2::new(cx, zero_y), Pos2::new(cx, knob_y)],
            egui::Stroke::new(3.0, colors.accent),
        );
    }

    // Knob
    let ring_color = if colors.dark_mode {
        Color32::from_rgb(0x2D, 0x34, 0x4C)
    } else {
        Color32::from_rgb(0xDF, 0xDF, 0xE5)
    };
    ui.painter()
        .circle_filled(Pos2::new(cx, knob_y), 10.0, ring_color);
    ui.painter()
        .circle_filled(Pos2::new(cx, knob_y), 7.0, colors.accent);

    // Value label
    let val_str = if unit == "%" {
        format!("{:.0} %", *value)
    } else if unit.is_empty() {
        format!("{:.2}", *value)
    } else {
        format!("{:.1} {}", *value, unit)
    };
    ui.painter().text(
        Pos2::new(cx, rect.bottom() - 10.0),
        Align2::CENTER_CENTER,
        &val_str,
        FontId::proportional(9.5),
        colors.text_dim,
    );

    // Drag interaction
    let drag_resp = ui.interact(
        rect,
        egui::Id::new(format!("sec_slider_{}_{}", title, subtitle)),
        Sense::drag(),
    );
    if drag_resp.dragged() {
        if let Some(ptr) = drag_resp.interact_pointer_pos() {
            let t = (ptr.y - track_top) / track_h;
            let new_norm = (1.0 - t).clamp(0.0, 1.0);
            *value = (min + (max - min) * new_norm as f32).clamp(min, max);
        }
    }
}

/// Toggle with label to the right
fn draw_labeled_toggle(ui: &mut Ui, label: &str, enabled: &mut bool, colors: &TuneCraftColors) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(label)
                .font(FontId::proportional(12.0))
                .color(colors.text),
        );

        // Right-align toggle
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            let (toggle_rect, toggle_resp) =
                ui.allocate_exact_size(Vec2::new(42.0, 22.0), Sense::click());
            let bg = if *enabled {
                colors.accent
            } else {
                colors.toggle_bg_off
            };
            ui.painter().rect_filled(toggle_rect, 11.0, bg);
            let kx = if *enabled {
                toggle_rect.right() - 11.0
            } else {
                toggle_rect.left() + 11.0
            };
            ui.painter()
                .circle_filled(Pos2::new(kx, toggle_rect.center().y), 8.0, Color32::WHITE);
            if toggle_resp.clicked() {
                *enabled = !*enabled;
            }
        });
    });
}

fn apply_preset(app: &mut TuneCraftApp, preset: &str) {
    match preset {
        "Flat" => {
            app.eq_bands = [0.0; 10];
            app.eq_preamp = 0.0;
            app.eq_bass_shelf = 0.0;
            app.eq_treble_shelf = 0.0;
        },
        "Bass Boost" => {
            app.eq_bands = [8.0, 6.0, 4.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
            app.eq_preamp = -2.0;
        },
        "Treble Boost" => {
            app.eq_bands = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 6.0, 8.0];
            app.eq_preamp = -2.0;
        },
        "V-Shape" => {
            app.eq_bands = [6.0, 4.0, 2.0, 0.0, -1.0, -1.0, 0.0, 2.0, 4.0, 6.0];
            app.eq_preamp = -2.0;
        },
        "Vocal" => {
            app.eq_bands = [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0];
            app.eq_preamp = -1.0;
        },
        _ => {},
    }
    app.eq_bass_shelf = app.eq_bands[0];
    app.eq_treble_shelf = app.eq_bands[9];
    let freq_values: [f32; 10] = [
        31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];
    for (i, &gain) in app.eq_bands.iter().enumerate() {
        app.ctx.eq.set_band_with_params(
            i,
            freq_values[i],
            gain,
            1.4,
            app.eq_enabled && gain != 0.0,
        );
    }
    app.ctx.eq.set_preamp(app.eq_preamp);
    {
        let mut eq_state = app.ctx.eq.state_mut();
        eq_state.bass_shelf = app.eq_bass_shelf;
        eq_state.treble_shelf = app.eq_treble_shelf;
        eq_state.preset = preset.to_string();
    }
    // Persist all band changes and preamp to config
    let bands_snapshot = app.eq_bands;
    let preamp_snapshot = app.eq_preamp;
    let eq_enabled = app.eq_enabled;
    app.ctx.config.write(|c| {
        c.engine.eq.enabled = eq_enabled;
        c.engine.eq.preamp_db = preamp_snapshot;
        while c.engine.eq.bands.len() < 10 {
            c.engine.eq.bands.push(tc_config::EqBand::default());
        }
        for (i, &gain) in bands_snapshot.iter().enumerate() {
            c.engine.eq.bands[i].gain_db = gain;
            c.engine.eq.bands[i].enabled = eq_enabled && gain != 0.0;
        }
    });
}

fn reset_eq(app: &mut TuneCraftApp) {
    app.eq_bands = [0.0; 10];
    app.eq_preamp = 0.0;
    app.eq_bass_shelf = 0.0;
    app.eq_treble_shelf = 0.0;
    app.eq_stereo_width = 1.0;
    app.eq_balance = 0.0;
    app.eq_dither = true;
    app.eq_midside = false;
    app.eq_preset = "Custom".to_string();
    app.ctx.eq.set_enabled(false);
    app.eq_enabled = false;
    app.ctx.eq.set_preamp(0.0);
    app.ctx.eq.set_stereo_width(1.0);
    app.ctx.eq.set_balance(0.0);
    app.ctx.eq.set_dither(true);
    app.ctx.eq.set_midside(false);
    let freq_values: [f32; 10] = [
        31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];
    for (i, &freq) in freq_values.iter().enumerate() {
        app.ctx.eq.set_band_with_params(i, freq, 0.0, 1.4, false);
    }
    {
        let mut eq_state = app.ctx.eq.state_mut();
        eq_state.bass_shelf = 0.0;
        eq_state.treble_shelf = 0.0;
        eq_state.preset = "Custom".to_string();
    }
    // Persist reset state to config
    app.ctx.config.write(|c| {
        c.engine.eq.enabled = false;
        c.engine.eq.preamp_db = 0.0;
        for band in c.engine.eq.bands.iter_mut() {
            band.gain_db = 0.0;
            band.enabled = false;
        }
        c.engine.dither_enabled = true;
    });
}
