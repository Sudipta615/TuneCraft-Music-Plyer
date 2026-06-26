//! Bottom player bar — current track info, playback controls, progress, volume
//! Matches the reference design: album art | track info + heart | controls | progress | volume |
//! queue Responsive: adapts layout at different viewport widths.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Ui, Vec2};

use crate::{app::TuneCraftApp, theme::TuneCraftColors};

pub const PLAYER_BAR_HEIGHT: f32 = 104.0;

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
        let mid = lo + (hi - lo).div_ceil(2);
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
    let top_border_color = colors.player_bar_border;
    ui.painter().line_segment(
        [bar_rect.left_top(), bar_rect.right_top()],
        egui::Stroke::new(1.0, top_border_color),
    );

    // Responsive layout selection
    if total_w < COMPACT_BREAKPOINT {
        draw_compact(app, ui, &colors, bar_rect, total_w);
    } else {
        draw_full(app, ui, &colors, bar_rect, total_w);
    }
}

/// Full layout: [album art + track info ~25%] [controls+progress ~50%] [volume+extras ~25%]
fn draw_full(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    colors: &TuneCraftColors,
    bar_rect: Rect,
    total_w: f32,
) {
    let _bar_h = PLAYER_BAR_HEIGHT;

    // ── Left section: Album art + track info + heart ──
    let left_w = (total_w * 0.28).clamp(160.0, 280.0);
    let art_size = if total_w > 900.0 { 56.0 } else { 48.0 };
    let art_margin = 14.0;

    // Album art box
    let art_rect = Rect::from_center_size(
        Pos2::new(
            bar_rect.left() + art_margin + art_size / 2.0,
            bar_rect.center().y,
        ),
        Vec2::new(art_size, art_size),
    );

    // Render real cover art or placeholder
    let art_tex = app
        .current_track_id
        .and_then(|id| app.get_or_load_album_art(ui.ctx(), id));
    if let Some(ref tex) = art_tex {
        // Background fill for rounded corners
        ui.painter().rect_filled(art_rect, 8.0, colors.card);
        let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        let mut mesh = egui::Mesh::with_texture(tex.id());
        mesh.add_rect_with_uv(art_rect, uv, Color32::WHITE);
        ui.painter().add(egui::Shape::mesh(mesh));
    } else {
        ui.painter().rect_filled(art_rect, 8.0, colors.card);
        ui.painter().rect_stroke(
            art_rect,
            8.0,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        // Music note icon
        ui.painter().text(
            art_rect.center(),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::MUSIC_NOTES,
            FontId::proportional(22.0),
            colors.text_dim,
        );
    }

    // Track info area
    let info_x = bar_rect.left() + art_margin + art_size + 10.0;
    let info_w = left_w - art_margin - art_size - 10.0 - 30.0; // 30 for heart
    let cy = bar_rect.center().y;

    if let Some(track) = app.current_track() {
        let title_font = FontId::proportional(14.0);
        let title = truncate(ui, &track.title, &title_font, info_w);
        ui.painter().text(
            Pos2::new(info_x, cy - 10.0),
            Align2::LEFT_CENTER,
            &title,
            title_font,
            colors.text,
        );

        let artist_font = FontId::proportional(13.0);
        let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
        let artist_t = truncate(ui, artist, &artist_font, info_w);
        ui.painter().text(
            Pos2::new(info_x, cy + 10.0),
            Align2::LEFT_CENTER,
            &artist_t,
            artist_font,
            colors.text_dim,
        );
    } else {
        ui.painter().text(
            Pos2::new(info_x, cy),
            Align2::LEFT_CENTER,
            "No track playing",
            FontId::proportional(13.0),
            colors.text_dim,
        );
    }

    // Heart button
    let heart_x = bar_rect.left() + left_w - 20.0;
    let heart_rect = Rect::from_center_size(Pos2::new(heart_x, cy), Vec2::new(28.0, 28.0));
    let heart_resp = ui.interact(heart_rect, egui::Id::new("fav_heart"), Sense::click());
    let heart_color = if app.is_favorited {
        Color32::from_rgb(0xEF, 0x44, 0x44)
    } else {
        colors.text_dim
    };
    ui.painter().text(
        heart_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::HEART,
        FontId::proportional(18.0),
        heart_color,
    );
    if heart_resp.clicked() {
        app.toggle_favorite();
    }

    // ── Right section: Volume + lyrics ──
    let right_w = (total_w * 0.22).clamp(100.0, 200.0);
    let right_x = bar_rect.right() - right_w;

    // Volume icon
    let vol_icon = if app.volume < 0.01 {
        egui_phosphor::regular::SPEAKER_SLASH
    } else if app.volume < 0.5 {
        egui_phosphor::regular::SPEAKER_LOW
    } else {
        egui_phosphor::regular::SPEAKER_HIGH
    };
    let vol_icon_x = right_x + 8.0;
    ui.painter().text(
        Pos2::new(vol_icon_x, cy - 14.0),
        Align2::LEFT_CENTER,
        vol_icon,
        FontId::proportional(16.0),
        colors.text_dim,
    );

    // Volume slider
    let vol_slider_x = vol_icon_x + 24.0;
    let vol_slider_w = (right_w - 70.0).max(40.0);
    let vol_rect = Rect::from_min_size(
        Pos2::new(vol_slider_x, cy - 20.0),
        Vec2::new(vol_slider_w, 14.0),
    );
    let vy = vol_rect.center().y;
    let vol = app.volume;

    ui.painter().line_segment(
        [
            Pos2::new(vol_rect.left(), vy),
            Pos2::new(vol_rect.right(), vy),
        ],
        egui::Stroke::new(2.0, colors.slider_track),
    );
    let vfx = vol_rect.left() + vol_rect.width() * vol;
    ui.painter().line_segment(
        [Pos2::new(vol_rect.left(), vy), Pos2::new(vfx, vy)],
        egui::Stroke::new(2.0, colors.slider_fill),
    );
    ui.painter()
        .circle_filled(Pos2::new(vfx, vy), 5.0, colors.accent);

    let vol_interact_rect = Rect::from_min_size(
        Pos2::new(vol_rect.left() - 4.0, vy - 8.0),
        Vec2::new(vol_rect.width() + 8.0, 16.0),
    );
    let vol_resp = ui
        .interact(
            vol_interact_rect,
            egui::Id::new("vol_slider"),
            Sense::click_and_drag(),
        )
        .on_hover_text(format!("Volume: {:.0}%", app.volume * 100.0));
    if vol_resp.dragged() {
        if let Some(ptr) = vol_resp.interact_pointer_pos() {
            let t = ((ptr.x - vol_rect.left()) / vol_rect.width()).clamp(0.0, 1.0);
            app.set_volume_dsp(t);
        }
    } else if vol_resp.drag_stopped() || vol_resp.clicked() {
        if let Some(ptr) = vol_resp.interact_pointer_pos() {
            let t = ((ptr.x - vol_rect.left()) / vol_rect.width()).clamp(0.0, 1.0);
            app.set_volume(t);
        } else if vol_resp.drag_stopped() {
            let vol = app.volume;
            app.set_volume(vol);
        }
    }

    // Volume percentage text
    let vol_pct_str = format!("{:.0}%", app.volume * 100.0);
    let vol_pct_x = vol_slider_x + vol_slider_w + 8.0;
    ui.painter().text(
        Pos2::new(vol_pct_x, cy - 14.0),
        Align2::LEFT_CENTER,
        &vol_pct_str,
        FontId::proportional(12.0),
        colors.text,
    );

    // ── Center section: Controls + progress ──
    let center_x = bar_rect.left() + left_w;
    let center_w = right_x - center_x;

    // Play/Pause button (large circle)
    let play_btn_size = 44.0;
    let play_radius = play_btn_size / 2.0;
    let play_cx = center_x + center_w / 2.0;

    // Controls row y position (upper part of center)
    let controls_y = cy - 14.0;
    let icon_size = 18.0;

    // Shuffle button
    let shuffle_color = if app.shuffle {
        colors.accent
    } else {
        colors.text_dim
    };
    let shuffle_rect = Rect::from_center_size(
        Pos2::new(play_cx - play_btn_size - icon_size * 2.0 - 20.0, controls_y),
        Vec2::new(28.0, 28.0),
    );
    let shuffle_resp = ui.interact(shuffle_rect, egui::Id::new("shuffle_btn"), Sense::click());
    if shuffle_resp.hovered() {
        ui.painter().rect_filled(shuffle_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        shuffle_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::SHUFFLE,
        FontId::proportional(icon_size * 1.3),
        shuffle_color,
    );
    if shuffle_resp.clicked() {
        app.toggle_shuffle();
    }

    // Previous button
    let prev_rect = Rect::from_center_size(
        Pos2::new(play_cx - play_btn_size - icon_size - 6.0, controls_y),
        Vec2::new(28.0, 28.0),
    );
    let prev_resp = ui.interact(prev_rect, egui::Id::new("prev_btn"), Sense::click());
    if prev_resp.hovered() {
        ui.painter().rect_filled(prev_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        prev_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::SKIP_BACK,
        FontId::proportional(icon_size),
        colors.text,
    );
    if prev_resp.clicked() {
        app.play_prev();
    }

    // Play/Pause circle
    let pb_rect = Rect::from_center_size(
        Pos2::new(play_cx, controls_y),
        Vec2::new(play_btn_size, play_btn_size),
    );
    let pb_resp = ui.interact(pb_rect, egui::Id::new("play_pause_btn"), Sense::click());
    let pb_bg = if pb_resp.hovered() {
        colors.accent_dark
    } else {
        colors.accent
    };
    ui.painter()
        .circle_filled(pb_rect.center(), play_radius, pb_bg);
    let play_label = if app.is_playing {
        egui_phosphor::regular::PAUSE
    } else {
        egui_phosphor::regular::PLAY
    };
    ui.painter().text(
        pb_rect.center(),
        Align2::CENTER_CENTER,
        play_label,
        FontId::proportional(play_btn_size * 0.40),
        Color32::WHITE,
    );
    if pb_resp.clicked() {
        app.toggle_playback();
    }

    // Next button
    let next_rect = Rect::from_center_size(
        Pos2::new(play_cx + play_btn_size + icon_size + 6.0, controls_y),
        Vec2::new(28.0, 28.0),
    );
    let next_resp = ui.interact(next_rect, egui::Id::new("next_btn"), Sense::click());
    if next_resp.hovered() {
        ui.painter().rect_filled(next_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        next_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::SKIP_FORWARD,
        FontId::proportional(icon_size),
        colors.text,
    );
    if next_resp.clicked() {
        app.play_next();
    }

    // Repeat button
    let (repeat_color, repeat_icon) = match app.repeat {
        tc_config::RepeatMode::Off => (colors.text_dim, egui_phosphor::regular::REPEAT),
        tc_config::RepeatMode::All => (colors.accent, egui_phosphor::regular::REPEAT),
        tc_config::RepeatMode::One => (colors.accent, egui_phosphor::regular::REPEAT_ONCE),
    };
    let repeat_rect = Rect::from_center_size(
        Pos2::new(play_cx + play_btn_size + icon_size * 2.0 + 20.0, controls_y),
        Vec2::new(28.0, 28.0),
    );
    let repeat_resp = ui.interact(repeat_rect, egui::Id::new("repeat_btn"), Sense::click());
    if repeat_resp.hovered() {
        ui.painter().rect_filled(repeat_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        repeat_rect.center(),
        Align2::CENTER_CENTER,
        repeat_icon,
        FontId::proportional(icon_size * 1.2),
        repeat_color,
    );
    if repeat_resp.clicked() {
        let new_repeat = match app.repeat {
            tc_config::RepeatMode::Off => tc_config::RepeatMode::All,
            tc_config::RepeatMode::All => tc_config::RepeatMode::One,
            tc_config::RepeatMode::One => tc_config::RepeatMode::Off,
        };
        app.set_repeat(new_repeat);
    }

    // ── Progress bar ──
    let prog_y = cy + 26.0;
    let prog_margin = 14.0;
    let time_label_w = 32.0;

    let track_duration = if app.duration_secs > 0.0 {
        app.duration_secs
    } else {
        1.0
    };
    let progress = (app.position_secs / track_duration).clamp(0.0, 1.0);

    let pos_s = (app.position_secs % 60.0) as u32;
    let pos_m = (app.position_secs / 60.0) as u32;
    let pos_str = format!("{}:{:02}", pos_m, pos_s);
    ui.painter().text(
        Pos2::new(center_x + prog_margin, prog_y),
        Align2::LEFT_CENTER,
        &pos_str,
        FontId::monospace(11.0),
        colors.text_dim,
    );

    let dur_s = (track_duration % 60.0) as u32;
    let dur_m = (track_duration / 60.0) as u32;
    let dur_str = format!("{}:{:02}", dur_m, dur_s);

    let prog_x_start = center_x + prog_margin + time_label_w;
    let prog_x_end = right_x - prog_margin - time_label_w;
    let prog_w = (prog_x_end - prog_x_start).max(1.0);

    ui.painter().text(
        Pos2::new(prog_x_end + 4.0, prog_y),
        Align2::LEFT_CENTER,
        &dur_str,
        FontId::monospace(11.0),
        colors.text_dim,
    );

    // Track line
    ui.painter().line_segment(
        [
            Pos2::new(prog_x_start, prog_y),
            Pos2::new(prog_x_end, prog_y),
        ],
        egui::Stroke::new(4.0, colors.slider_track),
    );
    let fill_x = prog_x_start + prog_w * progress;
    ui.painter().line_segment(
        [Pos2::new(prog_x_start, prog_y), Pos2::new(fill_x, prog_y)],
        egui::Stroke::new(4.0, colors.slider_fill),
    );
    ui.painter()
        .circle_filled(Pos2::new(fill_x, prog_y), 6.0, colors.accent);

    // Progress interaction
    let prog_interact_rect = Rect::from_min_size(
        Pos2::new(prog_x_start - 4.0, prog_y - 8.0),
        Vec2::new(prog_w + 8.0, 16.0),
    );
    let prog_resp = ui.interact(
        prog_interact_rect,
        egui::Id::new("progress_bar"),
        Sense::click_and_drag(),
    );
    if prog_resp.clicked() || prog_resp.dragged() {
        if let Some(ptr) = prog_resp.interact_pointer_pos() {
            let t = ((ptr.x - prog_x_start) / prog_w).clamp(0.0, 1.0);
            let new_pos = t * track_duration;
            app.position_secs = new_pos;
            app.seek(new_pos);
        }
    }

    // Allocate the full bar area so egui knows it's been used
    let _ = ui.interact(bar_rect, egui::Id::new("player_bar_area"), Sense::hover());
}

/// Compact layout: stack controls vertically, hide non-essential elements
fn draw_compact(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    colors: &TuneCraftColors,
    bar_rect: Rect,
    total_w: f32,
) {
    // Row 1: mini art + title + controls + volume
    let row1_y = bar_rect.top() + 22.0;
    let art_size = 32.0;
    let art_x = bar_rect.left() + 8.0;

    let art_rect = Rect::from_min_size(
        Pos2::new(art_x, row1_y - art_size / 2.0),
        Vec2::new(art_size, art_size),
    );

    // Render real cover art or placeholder
    let compact_art_tex = app
        .current_track_id
        .and_then(|id| app.get_or_load_album_art(ui.ctx(), id));
    if let Some(ref tex) = compact_art_tex {
        ui.painter().rect_filled(art_rect, 4.0, colors.card);
        let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        let mut mesh = egui::Mesh::with_texture(tex.id());
        mesh.add_rect_with_uv(art_rect, uv, Color32::WHITE);
        ui.painter().add(egui::Shape::mesh(mesh));
    } else {
        ui.painter().rect_filled(art_rect, 4.0, colors.card);
        ui.painter().rect_stroke(
            art_rect,
            4.0,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            art_rect.center(),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::MUSIC_NOTES,
            FontId::proportional(14.0),
            colors.text_dim,
        );
    }

    // Track info
    let info_x = art_x + art_size + 6.0;
    let info_w = (total_w * 0.25).clamp(60.0, 120.0);
    if let Some(track) = app.current_track() {
        let title_font = FontId::proportional(12.0);
        let title = truncate(ui, &track.title, &title_font, info_w);
        ui.painter().text(
            Pos2::new(info_x, row1_y),
            Align2::LEFT_CENTER,
            &title,
            title_font,
            colors.text,
        );
    } else {
        ui.painter().text(
            Pos2::new(info_x, row1_y),
            Align2::LEFT_CENTER,
            "No track",
            FontId::proportional(12.0),
            colors.text_dim,
        );
    }

    // Controls — centered
    let ctrl_cx = bar_rect.center().x;
    let prev_rect =
        Rect::from_center_size(Pos2::new(ctrl_cx - 32.0, row1_y), Vec2::new(28.0, 28.0));
    let play_rect = Rect::from_center_size(Pos2::new(ctrl_cx, row1_y), Vec2::new(36.0, 36.0));
    let next_rect =
        Rect::from_center_size(Pos2::new(ctrl_cx + 32.0, row1_y), Vec2::new(28.0, 28.0));

    let prev_resp = ui.interact(prev_rect, egui::Id::new("c_prev"), Sense::click());
    if prev_resp.hovered() {
        ui.painter().rect_filled(prev_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        prev_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::SKIP_BACK,
        FontId::proportional(16.0),
        colors.text,
    );
    if prev_resp.clicked() {
        app.play_prev();
    }

    let play_resp = ui.interact(play_rect, egui::Id::new("c_play"), Sense::click());
    let pb_bg = if play_resp.hovered() {
        colors.accent_dark
    } else {
        colors.accent
    };
    ui.painter().circle_filled(play_rect.center(), 18.0, pb_bg);
    let play_label = if app.is_playing {
        egui_phosphor::regular::PAUSE
    } else {
        egui_phosphor::regular::PLAY
    };
    ui.painter().text(
        play_rect.center(),
        Align2::CENTER_CENTER,
        play_label,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    if play_resp.clicked() {
        app.toggle_playback();
    }

    let next_resp = ui.interact(next_rect, egui::Id::new("c_next"), Sense::click());
    if next_resp.hovered() {
        ui.painter().rect_filled(next_rect, 4.0, colors.hover);
    }
    ui.painter().text(
        next_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::SKIP_FORWARD,
        FontId::proportional(16.0),
        colors.text,
    );
    if next_resp.clicked() {
        app.play_next();
    }

    // Volume icon (right side)
    let vol_icon = if app.volume < 0.01 {
        egui_phosphor::regular::SPEAKER_SLASH
    } else if app.volume < 0.5 {
        egui_phosphor::regular::SPEAKER_LOW
    } else {
        egui_phosphor::regular::SPEAKER_HIGH
    };
    let vol_rect = Rect::from_center_size(
        Pos2::new(bar_rect.right() - 20.0, row1_y),
        Vec2::new(24.0, 24.0),
    );
    let vol_resp = ui.interact(vol_rect, egui::Id::new("c_vol"), Sense::click());
    ui.painter().text(
        vol_rect.center(),
        Align2::CENTER_CENTER,
        vol_icon,
        FontId::proportional(14.0),
        colors.text_dim,
    );

    let vol_pct_str = format!("{:.0}%", app.volume * 100.0);
    ui.painter().text(
        Pos2::new(bar_rect.right() - 36.0, row1_y),
        Align2::RIGHT_CENTER,
        &vol_pct_str,
        FontId::proportional(12.0),
        colors.text,
    );

    if vol_resp.clicked() {
        if app.volume > 0.0 {
            app.set_volume(0.0);
        } else {
            app.set_volume(0.7);
        }
    }

    // Row 2: Progress bar
    let row2_y = bar_rect.top() + 52.0;
    let prog_margin = 8.0;
    let time_lw = 28.0;

    let track_duration = if app.duration_secs > 0.0 {
        app.duration_secs
    } else {
        1.0
    };
    let progress = (app.position_secs / track_duration).clamp(0.0, 1.0);

    let pos_s = (app.position_secs % 60.0) as u32;
    let pos_m = (app.position_secs / 60.0) as u32;
    ui.painter().text(
        Pos2::new(bar_rect.left() + prog_margin, row2_y),
        Align2::LEFT_CENTER,
        format!("{}:{:02}", pos_m, pos_s),
        FontId::proportional(9.0),
        colors.text_dim,
    );

    let dur_s = (track_duration % 60.0) as u32;
    let dur_m = (track_duration / 60.0) as u32;
    let prog_x_start = bar_rect.left() + prog_margin + time_lw;
    let prog_x_end = bar_rect.right() - prog_margin - time_lw;
    let prog_w = (prog_x_end - prog_x_start).max(1.0);

    ui.painter().text(
        Pos2::new(prog_x_end + 4.0, row2_y),
        Align2::LEFT_CENTER,
        format!("{}:{:02}", dur_m, dur_s),
        FontId::proportional(9.0),
        colors.text_dim,
    );

    ui.painter().line_segment(
        [
            Pos2::new(prog_x_start, row2_y),
            Pos2::new(prog_x_end, row2_y),
        ],
        egui::Stroke::new(3.0, colors.slider_track),
    );
    let fill_x = prog_x_start + prog_w * progress;
    ui.painter().line_segment(
        [Pos2::new(prog_x_start, row2_y), Pos2::new(fill_x, row2_y)],
        egui::Stroke::new(3.0, colors.slider_fill),
    );
    ui.painter()
        .circle_filled(Pos2::new(fill_x, row2_y), 4.0, colors.accent);

    let prog_interact = Rect::from_min_size(
        Pos2::new(prog_x_start - 4.0, row2_y - 7.0),
        Vec2::new(prog_w + 8.0, 14.0),
    );
    let prog_resp = ui.interact(
        prog_interact,
        egui::Id::new("c_progress"),
        Sense::click_and_drag(),
    );
    if prog_resp.clicked() || prog_resp.dragged() {
        if let Some(ptr) = prog_resp.interact_pointer_pos() {
            let t = ((ptr.x - prog_x_start) / prog_w).clamp(0.0, 1.0);
            let new_pos = t * track_duration;
            app.position_secs = new_pos;
            app.seek(new_pos);
        }
    }

    let _ = ui.interact(bar_rect, egui::Id::new("c_player_bar"), Sense::hover());
}
