//! Bottom player bar — current track info, playback controls, progress, volume
//! Matches the reference design: album art | track info + heart | controls | progress | volume |
//! queue Responsive: adapts layout at different viewport widths.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::{app::TuneCraftApp, theme::TuneCraftColors};

pub const PLAYER_BAR_HEIGHT: f32 = 80.0;

/// Responsive breakpoint: below this width, use compact layout
const COMPACT_BREAKPOINT: f32 = 600.0;

fn truncate(ui: &Ui, text: &str, font: &FontId, max_width: f32) -> String {
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    if g.size().x <= max_width {
        return text.to_string();
    }
    let eg = ui
        .painter()
        .layout_no_wrap("...".to_string(), font.clone(), Color32::WHITE);
    let target = max_width - eg.size().x;
    if target <= 0.0 {
        return "...".to_string();
    }
    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let n = offsets.len();
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        let end = if mid < n { offsets[mid] } else { text.len() };
        let pg = ui
            .painter()
            .layout_no_wrap(text[..end].to_string(), font.clone(), Color32::WHITE);
        if pg.size().x <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let end = if lo < n { offsets[lo] } else { text.len() };
    format!("{}...", &text[..end])
}

pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();
    let total_w = ui.available_width();

    let bar_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(bar_rect, 0.0, colors.player_bar);
    ui.painter().line_segment(
        [bar_rect.left_top(), bar_rect.right_top()],
        egui::Stroke::new(1.0, colors.player_bar_border),
    );

    // Responsive layout selection
    if total_w < COMPACT_BREAKPOINT {
        draw_compact(app, ui, &colors, bar_rect, total_w);
    } else {
        draw_full(app, ui, &colors, bar_rect, total_w);
    }
}

/// Full layout: [track info ~22%] [controls+progress ~52%] [volume+queue ~26%]
fn draw_full(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    colors: &TuneCraftColors,
    bar_rect: Rect,
    total_w: f32,
) {
    // Proportional section widths — scale with available space
    let info_w = (total_w * 0.22).min(260.0).max(140.0);
    let right_w = (total_w * 0.22).min(220.0).max(140.0);

    ui.horizontal(|ui| {
        ui.add_space(16.0);

        // ── Left: Album art + track info + heart ──
        // Scale art size slightly with width
        let art_size = if total_w > 900.0 { 48.0 } else { 40.0 };
        let (art_alloc, _) = ui.allocate_exact_size(Vec2::new(art_size, art_size), Sense::hover());
        let art_r = Rect::from_center_size(art_alloc.center(), Vec2::new(art_size, art_size));

        ui.painter().rect_filled(art_r, 4.0, colors.card);
        ui.painter()
            .rect_stroke(art_r, 4.0, egui::Stroke::new(1.0, colors.border));
        ui.painter().text(
            art_r.center(),
            Align2::CENTER_CENTER,
            "\u{266A}",
            FontId::proportional(20.0),
            colors.text_dim,
        );

        ui.add_space(8.0);

        let track_info_w = (info_w - art_size - 40.0).max(60.0);
        let (info_rect, _) =
            ui.allocate_exact_size(Vec2::new(track_info_w, PLAYER_BAR_HEIGHT), Sense::hover());
        let cy = info_rect.center().y;

        if let Some(track) = app.current_track() {
            let title_font = FontId::proportional(14.0);
            let max_title_w = info_rect.width() - 4.0;
            let title = truncate(ui, &track.title, &title_font, max_title_w);
            ui.painter().text(
                Pos2::new(info_rect.left(), cy - 10.0),
                Align2::LEFT_CENTER,
                &title,
                title_font,
                colors.text,
            );

            let artist_font = FontId::proportional(12.0);
            let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
            let artist_t = truncate(ui, artist, &artist_font, max_title_w);
            ui.painter().text(
                Pos2::new(info_rect.left(), cy + 10.0),
                Align2::LEFT_CENTER,
                &artist_t,
                artist_font,
                colors.text_dim,
            );
        } else {
            ui.painter().text(
                Pos2::new(info_rect.left(), cy),
                Align2::LEFT_CENTER,
                "No track playing",
                FontId::proportional(14.0),
                colors.text_dim,
            );
        }

        // Heart button
        let heart_rect = Rect::from_center_size(
            Pos2::new(info_rect.right() + 12.0, cy),
            Vec2::new(24.0, 24.0),
        );
        let heart_resp = ui.interact(heart_rect, egui::Id::new("fav_heart"), Sense::click());
        let heart_color = if app.is_favorited {
            Color32::from_rgb(0xEF, 0x44, 0x44)
        } else {
            colors.text_dim
        };
        ui.painter().text(
            heart_rect.center(),
            Align2::CENTER_CENTER,
            "\u{2665}",
            FontId::proportional(20.0),
            heart_color,
        );
        if heart_resp.clicked() {
            app.toggle_favorite();
        }

        // ── Center: Controls + progress bar ──
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.vertical(|ui| {
                    ui.add_space(6.0);

                    // Controls row — scale play button with available width
                    let play_btn_size = if total_w > 900.0 { 48.0 } else { 36.0 };
                    let play_radius = play_btn_size / 2.0;
                    let icon_size = if total_w > 900.0 { 20.0 } else { 16.0 };

                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                let shuffle_color = if app.shuffle {
                                    colors.accent
                                } else {
                                    colors.text_dim
                                };
                                if icon_btn(ui, "\u{21C4}", icon_size, shuffle_color) {
                                    app.toggle_shuffle();
                                }

                                if icon_btn(ui, "\u{23EE}", icon_size, colors.text) {
                                    app.play_prev();
                                }

                                // Play/Pause circle button
                                let play_label = if app.is_playing {
                                    "\u{23F8}"
                                } else {
                                    "\u{25B6}"
                                };
                                let (pb_rect, pb_resp) = ui.allocate_exact_size(
                                    Vec2::new(play_btn_size, play_btn_size),
                                    Sense::click(),
                                );
                                let pb_bg = if pb_resp.hovered() {
                                    colors.accent_dark
                                } else {
                                    colors.accent
                                };
                                ui.painter()
                                    .circle_filled(pb_rect.center(), play_radius, pb_bg);
                                ui.painter().text(
                                    pb_rect.center(),
                                    Align2::CENTER_CENTER,
                                    play_label,
                                    FontId::proportional(play_btn_size * 0.42),
                                    Color32::WHITE,
                                );
                                if pb_resp.clicked() {
                                    app.toggle_playback();
                                }

                                if icon_btn(ui, "\u{23ED}", icon_size, colors.text) {
                                    app.play_next();
                                }

                                let (repeat_color, repeat_icon) = match app.repeat {
                                    tc_config::RepeatMode::Off => (colors.text_dim, "\u{1F501}"),
                                    tc_config::RepeatMode::All => (colors.accent, "\u{1F501}"),
                                    tc_config::RepeatMode::One => (colors.accent, "\u{1F502}"),
                                };
                                if icon_btn(ui, repeat_icon, icon_size * 0.9, repeat_color) {
                                    let new_repeat = match app.repeat {
                                        tc_config::RepeatMode::Off => tc_config::RepeatMode::All,
                                        tc_config::RepeatMode::All => tc_config::RepeatMode::One,
                                        tc_config::RepeatMode::One => tc_config::RepeatMode::Off,
                                    };
                                    app.set_repeat(new_repeat);
                                }
                            },
                        );
                    });

                    // Progress bar row — 4px bar
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let track_duration = if app.duration_secs > 0.0 {
                            app.duration_secs
                        } else {
                            1.0
                        };
                        let progress = (app.position_secs / track_duration).clamp(0.0, 1.0) as f32;

                        let pos_s = (app.position_secs % 60.0) as u32;
                        let pos_m = (app.position_secs / 60.0) as u32;
                        ui.label(
                            RichText::new(format!("{}:{:02}", pos_m, pos_s))
                                .font(FontId::proportional(10.0))
                                .color(colors.text_dim),
                        );

                        // Progress bar uses remaining width
                        let prog_w = ui.available_width() - 36.0;
                        let prog_h = 14.0;
                        let (prog_rect, prog_resp) =
                            ui.allocate_exact_size(Vec2::new(prog_w, prog_h), Sense::click());
                        let py = prog_rect.center().y;

                        ui.painter().line_segment(
                            [
                                Pos2::new(prog_rect.left(), py),
                                Pos2::new(prog_rect.right(), py),
                            ],
                            egui::Stroke::new(4.0, colors.slider_track),
                        );
                        let fill_x = prog_rect.left() + prog_rect.width() * progress;
                        ui.painter().line_segment(
                            [Pos2::new(prog_rect.left(), py), Pos2::new(fill_x, py)],
                            egui::Stroke::new(4.0, colors.slider_fill),
                        );
                        ui.painter()
                            .circle_filled(Pos2::new(fill_x, py), 6.0, colors.accent);

                        if prog_resp.clicked() || prog_resp.dragged() {
                            if let Some(ptr) = prog_resp.interact_pointer_pos() {
                                let t = ((ptr.x - prog_rect.left()) / prog_rect.width())
                                    .clamp(0.0, 1.0);
                                let new_pos = t as f64 * track_duration;
                                app.position_secs = new_pos;
                                app.seek(new_pos);
                            }
                        }

                        let dur_s = (track_duration % 60.0) as u32;
                        let dur_m = (track_duration / 60.0) as u32;
                        ui.label(
                            RichText::new(format!("{}:{:02}", dur_m, dur_s))
                                .font(FontId::proportional(10.0))
                                .color(colors.text_dim),
                        );
                    });
                });
            },
        );

        // ── Right: Volume + Queue ──
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let vol_icon = if app.volume < 0.01 {
                    "\u{1F507}"
                } else if app.volume < 0.5 {
                    "\u{1F508}"
                } else {
                    "\u{1F509}"
                };
                ui.label(
                    RichText::new(vol_icon)
                        .font(FontId::proportional(14.0))
                        .color(colors.text_dim),
                );

                // Custom-drawn volume slider — width adapts to available space
                let vol_w = ui.available_width().min(120.0).max(40.0);
                let vol_h = 14.0;
                let (vol_rect, vol_resp) =
                    ui.allocate_exact_size(Vec2::new(vol_w, vol_h), Sense::click_and_drag());
                let vy = vol_rect.center().y;
                let vol = app.volume as f32;

                ui.painter().line_segment(
                    [
                        Pos2::new(vol_rect.left(), vy),
                        Pos2::new(vol_rect.right(), vy),
                    ],
                    egui::Stroke::new(4.0, colors.slider_track),
                );
                let fill_x = vol_rect.left() + vol_rect.width() * vol;
                ui.painter().line_segment(
                    [Pos2::new(vol_rect.left(), vy), Pos2::new(fill_x, vy)],
                    egui::Stroke::new(4.0, colors.slider_fill),
                );
                ui.painter()
                    .circle_filled(Pos2::new(fill_x, vy), 5.0, colors.accent);

                if vol_resp.clicked() || vol_resp.dragged() {
                    if let Some(ptr) = vol_resp.interact_pointer_pos() {
                        let t = ((ptr.x - vol_rect.left()) / vol_rect.width()).clamp(0.0, 1.0);
                        app.set_volume(t as f64);
                    }
                }
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let lyrics_color = if app.show_lyrics {
                    colors.accent
                } else {
                    colors.text_dim
                };
                if icon_btn(ui, "\u{2630}", 12.0, lyrics_color) {
                    app.show_lyrics = !app.show_lyrics;
                    app.ctx.lyrics.toggle_panel();
                }

                // Speed control — hide individual speed buttons on narrow widths, show only current
                // speed label
                if total_w > 800.0 {
                    let speed_label = if (app.speed - 1.0).abs() < 0.01 {
                        "1x".to_string()
                    } else {
                        format!("{:.1}x", app.speed)
                    };
                    let speed_color = if (app.speed - 1.0).abs() < 0.01 {
                        colors.text_dim
                    } else {
                        colors.accent
                    };
                    ui.label(
                        RichText::new(&speed_label)
                            .font(FontId::proportional(10.0))
                            .color(speed_color),
                    );

                    for &s in &[0.5f64, 0.75, 1.0, 1.25, 1.5, 2.0] {
                        let is_cur = (app.speed - s).abs() < 0.01;
                        let label = if s == s.round() {
                            format!("{}x", s as u32)
                        } else {
                            format!("{:.1}x", s)
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(&label).font(FontId::proportional(9.0)).color(
                                        if is_cur {
                                            colors.accent
                                        } else {
                                            colors.text_dim
                                        },
                                    ),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            app.set_speed(s);
                        }
                    }
                } else {
                    // Compact: just show current speed as clickable toggle
                    let speed_label = if (app.speed - 1.0).abs() < 0.01 {
                        "1x".to_string()
                    } else {
                        format!("{:.1}x", app.speed)
                    };
                    let speed_color = if (app.speed - 1.0).abs() < 0.01 {
                        colors.text_dim
                    } else {
                        colors.accent
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(&speed_label)
                                    .font(FontId::proportional(10.0))
                                    .color(speed_color),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        // Cycle through speeds
                        let speeds = [0.5f64, 0.75, 1.0, 1.25, 1.5, 2.0];
                        let cur_idx = speeds
                            .iter()
                            .position(|&s| (app.speed - s).abs() < 0.01)
                            .unwrap_or(2);
                        let next_idx = (cur_idx + 1) % speeds.len();
                        app.set_speed(speeds[next_idx]);
                    }
                }
            });
        });

        ui.add_space(12.0);
    });
}

/// Compact layout: stack controls vertically, hide non-essential elements
fn draw_compact(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    colors: &TuneCraftColors,
    bar_rect: Rect,
    total_w: f32,
) {
    ui.vertical(|ui| {
        // Row 1: Track info (left) + controls (center) + volume (right)
        ui.horizontal(|ui| {
            ui.add_space(8.0);

            // Mini art + track info
            let art_size = 32.0;
            let (art_alloc, _) =
                ui.allocate_exact_size(Vec2::new(art_size, art_size), Sense::hover());
            let art_r = Rect::from_center_size(art_alloc.center(), Vec2::new(art_size, art_size));
            ui.painter().rect_filled(art_r, 4.0, colors.card);
            ui.painter()
                .rect_stroke(art_r, 4.0, egui::Stroke::new(1.0, colors.border));
            ui.painter().text(
                art_r.center(),
                Align2::CENTER_CENTER,
                "\u{266A}",
                FontId::proportional(14.0),
                colors.text_dim,
            );

            ui.add_space(6.0);

            // Track title only (no artist in compact)
            let info_w = (total_w * 0.25).max(60.0).min(120.0);
            let (info_rect, _) = ui.allocate_exact_size(Vec2::new(info_w, 36.0), Sense::hover());
            if let Some(track) = app.current_track() {
                let title_font = FontId::proportional(12.0);
                let max_w = info_rect.width() - 4.0;
                let title = truncate(ui, &track.title, &title_font, max_w);
                ui.painter().text(
                    Pos2::new(info_rect.left(), info_rect.center().y),
                    Align2::LEFT_CENTER,
                    &title,
                    title_font,
                    colors.text,
                );
            } else {
                ui.painter().text(
                    info_rect.center(),
                    Align2::CENTER_CENTER,
                    "No track",
                    FontId::proportional(12.0),
                    colors.text_dim,
                );
            }

            // Center controls
            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    if icon_btn(ui, "\u{23EE}", 16.0, colors.text) {
                        app.play_prev();
                    }
                    let play_label = if app.is_playing {
                        "\u{23F8}"
                    } else {
                        "\u{25B6}"
                    };
                    let (pb_rect, pb_resp) =
                        ui.allocate_exact_size(Vec2::new(36.0, 36.0), Sense::click());
                    let pb_bg = if pb_resp.hovered() {
                        colors.accent_dark
                    } else {
                        colors.accent
                    };
                    ui.painter().circle_filled(pb_rect.center(), 18.0, pb_bg);
                    ui.painter().text(
                        pb_rect.center(),
                        Align2::CENTER_CENTER,
                        play_label,
                        FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                    if pb_resp.clicked() {
                        app.toggle_playback();
                    }
                    if icon_btn(ui, "\u{23ED}", 16.0, colors.text) {
                        app.play_next();
                    }
                },
            );

            // Volume icon only (no slider in compact)
            let vol_icon = if app.volume < 0.01 {
                "\u{1F507}"
            } else if app.volume < 0.5 {
                "\u{1F508}"
            } else {
                "\u{1F509}"
            };
            if icon_btn(ui, vol_icon, 14.0, colors.text_dim) {
                // Toggle mute on click
                if app.volume > 0.0 {
                    app.set_volume(0.0);
                } else {
                    app.set_volume(0.7);
                }
            }
        });

        // Row 2: Progress bar full width
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let track_duration = if app.duration_secs > 0.0 {
                app.duration_secs
            } else {
                1.0
            };
            let progress = (app.position_secs / track_duration).clamp(0.0, 1.0) as f32;

            let pos_s = (app.position_secs % 60.0) as u32;
            let pos_m = (app.position_secs / 60.0) as u32;
            ui.label(
                RichText::new(format!("{}:{:02}", pos_m, pos_s))
                    .font(FontId::proportional(9.0))
                    .color(colors.text_dim),
            );

            let prog_w = ui.available_width() - 30.0;
            let prog_h = 10.0;
            let (prog_rect, prog_resp) =
                ui.allocate_exact_size(Vec2::new(prog_w, prog_h), Sense::click());
            let py = prog_rect.center().y;

            ui.painter().line_segment(
                [
                    Pos2::new(prog_rect.left(), py),
                    Pos2::new(prog_rect.right(), py),
                ],
                egui::Stroke::new(3.0, colors.slider_track),
            );
            let fill_x = prog_rect.left() + prog_rect.width() * progress;
            ui.painter().line_segment(
                [Pos2::new(prog_rect.left(), py), Pos2::new(fill_x, py)],
                egui::Stroke::new(3.0, colors.slider_fill),
            );
            ui.painter()
                .circle_filled(Pos2::new(fill_x, py), 4.0, colors.accent);

            if prog_resp.clicked() || prog_resp.dragged() {
                if let Some(ptr) = prog_resp.interact_pointer_pos() {
                    let t = ((ptr.x - prog_rect.left()) / prog_rect.width()).clamp(0.0, 1.0);
                    let new_pos = t as f64 * track_duration;
                    app.position_secs = new_pos;
                    app.seek(new_pos);
                }
            }

            let dur_s = (track_duration % 60.0) as u32;
            let dur_m = (track_duration / 60.0) as u32;
            ui.label(
                RichText::new(format!("{}:{:02}", dur_m, dur_s))
                    .font(FontId::proportional(9.0))
                    .color(colors.text_dim),
            );
        });
    });
}

fn icon_btn(ui: &mut Ui, icon: &str, size: f32, color: Color32) -> bool {
    ui.add(
        egui::Button::new(
            RichText::new(icon)
                .font(FontId::proportional(size))
                .color(color),
        )
        .frame(false),
    )
    .clicked()
}
