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

    ui.vertical(|ui| {
        // ── Header row: ← EQ   [Enable toggle]  [Custom ▾]  [⋮] ──
        ui.horizontal(|ui| {
            ui.add_space(14.0);

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

                // Close button (X icon)
                let arrow_resp = ui.add(
                    egui::Button::new(
                        RichText::new(egui_phosphor::regular::X)
                            .font(FontId::proportional(16.0))
                            .color(colors.text_dim),
                    )
                    .frame(false),
                );
                if arrow_resp.clicked() {
                    app.show_eq_panel = false;
                    app.ctx.eq.state_mut().show_panel = false;
                }

                ui.add_space(8.0);

                // Three-dot menu button (phosphor icon)
                ui.add(
                    egui::Button::new(
                        RichText::new(egui_phosphor::regular::DOTS_THREE_VERTICAL)
                            .font(FontId::proportional(16.0))
                            .color(colors.text_dim),
                    )
                    .frame(false),
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

        ui.add_enabled_ui(app.eq_enabled, |ui| {
            // ── 10-band EQ sliders ──
            let eq_area_h = 220.0;
            let available_w = ui.available_width();
            let (eq_rect, _) =
                ui.allocate_exact_size(Vec2::new(available_w, eq_area_h), Sense::hover());

            // Background card for slider area
            ui.painter().rect_filled(eq_rect, 12.0, colors.card);
            ui.painter().rect_stroke(
                eq_rect,
                12.0,
                egui::Stroke::new(1.0, colors.border),
                egui::StrokeKind::Inside,
            );

            draw_eq_sliders(ui, app, eq_rect, &colors);

            ui.add_space(10.0);

            // ── Secondary controls: Bass Shelf | Treble Shelf | Stereo Width | Balance | Dither + M/S
            // ──
            let secondary_h = 110.0;
            let spacing = 8.0;
            let total_spacing = spacing * 1.0;
            let available_w = ui.available_width();
            let card_w = (available_w - total_spacing) / 2.0;

            let old_bass = app.eq_bass_shelf;
            let old_treble = app.eq_treble_shelf;
            let old_width_pct = (app.eq_stereo_width * 100.0).clamp(0.0, 200.0);
            let old_bal = app.eq_balance;

            let mut new_bass = old_bass;
            let mut new_treble = old_treble;
            let mut new_width_pct = old_width_pct;
            let mut new_bal = old_bal;

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.x = spacing;

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = spacing;

                    secondary_slider_horizontal_card(
                        ui,
                        "Bass",
                        "Shelf",
                        &mut new_bass,
                        -12.0,
                        12.0,
                        "dB",
                        "-12dB",
                        "+12dB",
                        card_w,
                        secondary_h,
                        &colors,
                    );
                    secondary_slider_horizontal_card(
                        ui,
                        "Treble",
                        "Shelf",
                        &mut new_treble,
                        -12.0,
                        12.0,
                        "dB",
                        "-12dB",
                        "+12dB",
                        card_w,
                        secondary_h,
                        &colors,
                    );
                });
                ui.add_space(spacing);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = spacing;

                    secondary_slider_horizontal_card(
                        ui,
                        "Stereo Width",
                        "",
                        &mut new_width_pct,
                        0.0,
                        200.0,
                        "%",
                        "0 %",
                        "200 %",
                        card_w,
                        secondary_h,
                        &colors,
                    );
                    secondary_slider_horizontal_card(
                        ui,
                        "Balance",
                        "",
                        &mut new_bal,
                        -1.0,
                        1.0,
                        "",
                        "-1.00",
                        "+1.00",
                        card_w,
                        secondary_h,
                        &colors,
                    );
                });
            });

            // Apply changes after the UI closure
            if (new_bass - old_bass).abs() > 0.001 {
                app.eq_bass_shelf = new_bass;
                app.ctx.eq.set_bass_shelf(new_bass);
            }
            if (new_treble - old_treble).abs() > 0.001 {
                app.eq_treble_shelf = new_treble;
                app.ctx.eq.set_treble_shelf(new_treble);
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
            ui.add_space(10.0);

            // ── Preamp row ──
            let preamp_h = 44.0;
            let preamp_w = ui.available_width();
            let (preamp_rect, _) =
                ui.allocate_exact_size(Vec2::new(preamp_w, preamp_h), Sense::hover());

            ui.painter().rect_filled(preamp_rect, 12.0, colors.card);
            ui.painter().rect_stroke(
                preamp_rect,
                12.0,
                egui::Stroke::new(1.0, colors.border),
                egui::StrokeKind::Inside,
            );

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

                let norm = ((app.eq_preamp + 12.0) / 24.0).clamp(0.0, 1.0);
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
                let outer_r = 8.0;
                let inner_r = 4.0;
                ui.painter()
                    .circle_filled(Pos2::new(fill_x, slider_y), outer_r, colors.accent);
                ui.painter()
                    .circle_filled(Pos2::new(fill_x, slider_y), inner_r, colors.card);

                // Drag area
                let drag_rect = Rect::from_min_size(
                    Pos2::new(slider_x_start - 8.0, preamp_rect.top()),
                    Vec2::new(slider_w + 16.0, preamp_h),
                );
                let drag_resp =
                    ui.interact(drag_rect, egui::Id::new("preamp_slider"), Sense::drag());
                if drag_resp.dragged() {
                    if let Some(ptr) = drag_resp.interact_pointer_pos() {
                        let t = ((ptr.x - slider_x_start) / slider_w).clamp(0.0, 1.0);
                        let new_preamp = t * 24.0 - 12.0;
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
                let reset_resp =
                    ui.interact(reset_rect, egui::Id::new("eq_reset_btn"), Sense::click());
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
        });
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
        let norm = ((gain + 12.0) / 24.0).clamp(0.0, 1.0);
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

        // Knob circle (hollow ring style matching reference)
        let outer_r = 9.0;
        let inner_r = 4.0;
        ui.painter()
            .circle_filled(Pos2::new(band_cx, knob_y), outer_r, colors.accent);
        ui.painter()
            .circle_filled(Pos2::new(band_cx, knob_y), inner_r, colors.card);

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
                let new_gain = new_norm * 24.0 - 12.0;
                app.eq_bands[i] = new_gain.clamp(-12.0, 12.0);
                app.eq_preset = "Custom".to_string();
                app.ctx.eq.set_band_with_params(
                    i,
                    freq_values[i],
                    new_gain,
                    1.4,
                    true, // Master enabled switch handles global on/off
                );
                // Persist band gain to config
                let band_idx = i;
                let gain_db = new_gain;
                let enabled = gain_db != 0.0;
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
fn secondary_slider_horizontal_card(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    unit: &str,
    min_label: &str,
    max_label: &str,
    width: f32,
    height: f32,
    colors: &TuneCraftColors,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let cx = rect.center().x;

    ui.painter().rect_filled(rect, 12.0, colors.card);
    ui.painter().rect_stroke(
        rect,
        12.0,
        egui::Stroke::new(1.0, colors.border),
        egui::StrokeKind::Inside,
    );

    // Title
    ui.painter().text(
        Pos2::new(cx, rect.top() + 16.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(12.0),
        colors.text,
    );
    if !subtitle.is_empty() {
        ui.painter().text(
            Pos2::new(cx, rect.top() + 32.0),
            Align2::CENTER_CENTER,
            subtitle,
            FontId::proportional(10.0),
            colors.text_dim,
        );
    }

    // Horizontal slider track
    let track_left = rect.left() + 16.0;
    let track_right = rect.right() - 16.0;
    let track_y = rect.top() + if subtitle.is_empty() { 64.0 } else { 72.0 };
    let track_w = track_right - track_left;

    if track_w > 0.0 {
        // Draw min / max labels above track ends
        ui.painter().text(
            Pos2::new(track_left, track_y - 12.0),
            Align2::LEFT_CENTER,
            min_label,
            FontId::proportional(9.0),
            colors.text_dim,
        );
        ui.painter().text(
            Pos2::new(track_right, track_y - 12.0),
            Align2::RIGHT_CENTER,
            max_label,
            FontId::proportional(9.0),
            colors.text_dim,
        );

        // Draw track
        ui.painter().line_segment(
            [
                Pos2::new(track_left, track_y),
                Pos2::new(track_right, track_y),
            ],
            egui::Stroke::new(3.0, colors.slider_track),
        );

        let norm = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let knob_x = track_left + track_w * norm;
        let zero_norm = (-min / (max - min)).clamp(0.0, 1.0);
        let zero_x = track_left + track_w * zero_norm;

        // Fill from zero
        if *value >= 0.0 {
            ui.painter().line_segment(
                [Pos2::new(zero_x, track_y), Pos2::new(knob_x, track_y)],
                egui::Stroke::new(3.0, colors.accent),
            );
        } else {
            ui.painter().line_segment(
                [Pos2::new(knob_x, track_y), Pos2::new(zero_x, track_y)],
                egui::Stroke::new(3.0, colors.accent),
            );
        }

        // Knob (hollow ring style)
        let outer_r = 9.0;
        let inner_r = 4.0;
        ui.painter()
            .circle_filled(Pos2::new(knob_x, track_y), outer_r, colors.accent);
        ui.painter()
            .circle_filled(Pos2::new(knob_x, track_y), inner_r, colors.card);

        // Value label
        let val_str = if unit == "%" {
            format!("{:.0} %", *value)
        } else if unit.is_empty() {
            format!("{:.2}", *value)
        } else {
            format!("{:.1} {}", *value, unit)
        };
        ui.painter().text(
            Pos2::new(cx, rect.bottom() - 20.0),
            Align2::CENTER_CENTER,
            &val_str,
            FontId::proportional(11.0),
            colors.text,
        );

        // Drag interaction
        let drag_resp = ui.interact(
            Rect::from_min_size(
                Pos2::new(track_left - 10.0, track_y - 10.0),
                Vec2::new(track_w + 20.0, 20.0),
            ),
            egui::Id::new(format!("sec_slider_horiz_{}_{}", title, subtitle)),
            Sense::drag(),
        );
        if drag_resp.dragged() {
            if let Some(ptr) = drag_resp.interact_pointer_pos() {
                let t = (ptr.x - track_left) / track_w;
                let new_norm = t.clamp(0.0, 1.0);
                *value = (min + (max - min) * new_norm).clamp(min, max);
            }
        }
    }
}

fn apply_preset(app: &mut TuneCraftApp, preset: &str) {
    match preset {
        "Flat" => {
            app.eq_bands = [0.0; 10];
        },
        "Bass Boost" => {
            app.eq_bands = [8.0, 6.0, 4.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        },
        "Treble Boost" => {
            app.eq_bands = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 6.0, 8.0];
        },
        "V-Shape" => {
            app.eq_bands = [6.0, 4.0, 2.0, 0.0, -1.0, -1.0, 0.0, 2.0, 4.0, 6.0];
        },
        "Vocal" => {
            app.eq_bands = [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0];
        },
        _ => {},
    }
    // Note: We no longer tie bass/treble shelves to graphic bands 0 and 9.
    let freq_values: [f32; 10] = [
        31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];
    for (i, &gain) in app.eq_bands.iter().enumerate() {
        app.ctx.eq.set_band_with_params(
            i,
            freq_values[i],
            gain,
            1.4,
            true, // Always pass true from UI, master switch will handle EQ bypass
        );
    }
    app.ctx.eq.set_preamp(app.eq_preamp);
    app.ctx.eq.set_bass_shelf(app.eq_bass_shelf);
    app.ctx.eq.set_treble_shelf(app.eq_treble_shelf);
    {
        let mut eq_state = app.ctx.eq.state_mut();
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
            c.engine.eq.bands[i].enabled = gain != 0.0;
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
    app.ctx.eq.set_preamp(0.0);
    app.ctx.eq.set_stereo_width(1.0);
    app.ctx.eq.set_balance(0.0);
    app.ctx.eq.set_dither(true);
    app.ctx.eq.set_midside(false);
    app.ctx.eq.set_bass_shelf(0.0);
    app.ctx.eq.set_treble_shelf(0.0);
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
    let eq_enabled = app.eq_enabled;
    app.ctx.config.write(|c| {
        c.engine.eq.enabled = eq_enabled;
        c.engine.eq.preamp_db = 0.0;
        for band in c.engine.eq.bands.iter_mut() {
            band.gain_db = 0.0;
            band.enabled = false;
        }
        c.engine.dither_enabled = true;
    });
}
