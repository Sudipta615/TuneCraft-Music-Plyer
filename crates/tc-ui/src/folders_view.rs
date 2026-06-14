//! Folders view — browse music by the folders you've added.
//!
//! Two states:
//! - **Folder list**: Shows all added folders (from `config.library.watch_dirs`)
//!   with track counts. Click a folder to open it.
//! - **Folder contents**: Shows tracks inside a specific folder with a back
//!   button. Click a track to play it.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::{app::TuneCraftApp, theme::TuneCraftColors, track_list};

/// Draw the Folders content area.
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    ui.vertical(|ui| {
        // Search/topbar (shared with track list)
        track_list::draw_topbar(app, ui);

        ui.add_space(8.0);

        if app.folder_view_path.is_some() {
            draw_folder_contents(app, ui, &colors);
        } else {
            draw_folder_list(app, ui, &colors);
        }
    });
}

/// Draw the folder list (State 1): all added folders as styled cards.
fn draw_folder_list(app: &mut TuneCraftApp, ui: &mut Ui, colors: &TuneCraftColors) {
    let content_w = ui.available_width();
    let is_narrow = content_w < 500.0;

    // Title row with "Add Music" and "Add Folders" buttons
    ui.horizontal(|ui| {
        ui.add_space(if is_narrow { 12.0 } else { 24.0 });

        ui.vertical(|ui| {
            let heading_size = if is_narrow { 20.0 } else { 28.0 };
            ui.label(
                RichText::new("Folders")
                    .font(FontId::proportional(heading_size))
                    .color(colors.text)
                    .strong(),
            );

            let watch_dirs = app
                .ctx
                .config
                .read(|c| c.library.watch_dirs.clone())
                .unwrap_or_default();
            let sub_font = if is_narrow { 11.0 } else { 14.0 };
            ui.label(
                RichText::new(format!(
                    "{} folder{}",
                    watch_dirs.len(),
                    if watch_dirs.len() == 1 { "" } else { "s" }
                ))
                .font(FontId::proportional(sub_font))
                .color(colors.text_dim),
            );
        });

        // Right-aligned toolbar buttons
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(if is_narrow { 8.0 } else { 16.0 });

            // Add Folders button
            let btn_w = if is_narrow { 100.0 } else { 130.0 };
            let btn_h = 36.0;
            let (folders_rect, folders_resp) =
                ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());
            let folders_bg = if folders_resp.hovered() {
                colors.accent_dark
            } else {
                colors.accent
            };
            ui.painter().rect_filled(folders_rect, 8.0, folders_bg);
            ui.painter().text(
                folders_rect.center(),
                Align2::CENTER_CENTER,
                format!("{} Add Folders", egui_phosphor::regular::FOLDER_PLUS),
                FontId::proportional(if is_narrow { 11.0 } else { 13.0 }),
                Color32::WHITE,
            );
            if folders_resp.clicked() {
                let folder = rfd::FileDialog::new()
                    .set_title("Select Music Folders")
                    .pick_folder();
                if let Some(path) = folder {
                    app.add_music_folders(vec![path]);
                }
            }

            ui.add_space(8.0);

            // Add Music button
            let (files_rect, files_resp) =
                ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());
            let files_bg = if files_resp.hovered() {
                colors.accent_dark
            } else {
                colors.accent
            };
            ui.painter().rect_filled(files_rect, 8.0, files_bg);
            ui.painter().text(
                files_rect.center(),
                Align2::CENTER_CENTER,
                format!("{} Add Music", egui_phosphor::regular::MUSIC_NOTES_PLUS),
                FontId::proportional(if is_narrow { 11.0 } else { 13.0 }),
                Color32::WHITE,
            );
            if files_resp.clicked() {
                let files = rfd::FileDialog::new()
                    .set_title("Select Music Files")
                    .add_filter(
                        "Audio Files",
                        &[
                            "mp3", "flac", "ogg", "wav", "aac", "m4a", "opus", "wma", "aiff", "ape",
                        ],
                    )
                    .pick_files();
                if let Some(paths) = files {
                    app.add_music_files(paths);
                }
            }
        });
    });

    ui.add_space(16.0);

    // Folder cards
    let watch_dirs: Vec<PathBuf> = app
        .ctx
        .config
        .read(|c| c.library.watch_dirs.clone())
        .unwrap_or_default();

    if watch_dirs.is_empty() {
        // Empty state
        ui.add_space(80.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(egui_phosphor::regular::FOLDER_DASHED)
                    .font(FontId::proportional(64.0))
                    .color(colors.text_muted),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new("No folders added yet")
                    .font(FontId::proportional(20.0))
                    .color(colors.text_dim),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Use the buttons above to add music files or folders")
                    .font(FontId::proportional(14.0))
                    .color(colors.text_muted),
            );
        });
        return;
    }

    let card_h = 72.0;
    let pad_x = if is_narrow { 12.0 } else { 24.0 };

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, dir) in watch_dirs.iter().enumerate() {
                let (card_rect, card_resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), card_h), Sense::click());

                // Inner card rect with horizontal padding
                let inner_rect = Rect::from_min_max(
                    Pos2::new(card_rect.left() + pad_x, card_rect.top()),
                    Pos2::new(card_rect.right() - pad_x, card_rect.bottom()),
                );

                // Background
                let bg = if card_resp.hovered() {
                    colors.table_row_hover
                } else if i % 2 == 0 {
                    colors.bg
                } else {
                    if colors.dark_mode {
                        Color32::from_rgba_premultiplied(
                            colors.bg.r().saturating_add(5),
                            colors.bg.g().saturating_add(5),
                            colors.bg.b().saturating_add(5),
                            255,
                        )
                    } else {
                        Color32::from_rgba_premultiplied(
                            colors.bg.r().saturating_sub(5),
                            colors.bg.g().saturating_sub(5),
                            colors.bg.b().saturating_sub(5),
                            255,
                        )
                    }
                };
                ui.painter().rect_filled(inner_rect, 8.0, bg);
                ui.painter().rect_stroke(
                    inner_rect,
                    8.0,
                    egui::Stroke::new(1.0, colors.border),
                    egui::StrokeKind::Inside,
                );

                let cy = inner_rect.center().y;

                // Folder icon
                let icon_x = inner_rect.left() + 24.0;
                let icon_size = 28.0;
                let icon_rect =
                    Rect::from_center_size(Pos2::new(icon_x, cy), Vec2::splat(icon_size + 8.0));
                // Icon background circle
                let icon_bg = if colors.dark_mode {
                    Color32::from_rgba_premultiplied(
                        (colors.accent.r() as u16 * 20 / 100 + colors.bg.r() as u16 * 80 / 100)
                            as u8,
                        (colors.accent.g() as u16 * 20 / 100 + colors.bg.g() as u16 * 80 / 100)
                            as u8,
                        (colors.accent.b() as u16 * 20 / 100 + colors.bg.b() as u16 * 80 / 100)
                            as u8,
                        255,
                    )
                } else {
                    colors.active_bg
                };
                ui.painter()
                    .rect_filled(icon_rect, (icon_size + 8.0) / 2.0, icon_bg);
                ui.painter().text(
                    icon_rect.center(),
                    Align2::CENTER_CENTER,
                    egui_phosphor::regular::FOLDER,
                    FontId::proportional(icon_size),
                    colors.accent,
                );

                // Folder name + path
                let text_x = icon_x + icon_size / 2.0 + 20.0;
                let folder_name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| dir.to_string_lossy().into_owned());
                let full_path = dir.to_string_lossy();

                let max_text_w = inner_rect.right() - text_x - 100.0;
                let name_font = FontId::proportional(15.0);
                let path_font = FontId::proportional(12.0);

                // Truncate if needed
                let name_galley = ui.painter().layout_no_wrap(
                    folder_name.clone(),
                    name_font.clone(),
                    Color32::WHITE,
                );
                let display_name = if name_galley.size().x > max_text_w {
                    let chars: Vec<char> = folder_name.chars().collect();
                    let mut end = chars.len();
                    loop {
                        if end == 0 {
                            break;
                        }
                        end -= 1;
                        let candidate: String = chars[..end].iter().collect();
                        let test = format!("{}…", candidate);
                        let g = ui.painter().layout_no_wrap(
                            test.clone(),
                            name_font.clone(),
                            Color32::WHITE,
                        );
                        if g.size().x <= max_text_w {
                            break;
                        }
                    }
                    let candidate: String = chars[..end].iter().collect();
                    format!("{}…", candidate)
                } else {
                    folder_name
                };

                ui.painter().text(
                    Pos2::new(text_x, cy - 10.0),
                    Align2::LEFT_CENTER,
                    &display_name,
                    name_font,
                    colors.text,
                );

                // Path subtitle (truncated from left if too long)
                let path_str = full_path.to_string();
                let path_galley = ui.painter().layout_no_wrap(
                    path_str.clone(),
                    path_font.clone(),
                    Color32::WHITE,
                );
                let display_path = if path_galley.size().x > max_text_w {
                    let chars: Vec<char> = path_str.chars().collect();
                    let total = chars.len();
                    let mut start = 0;
                    loop {
                        if start >= total {
                            break;
                        }
                        start += 1;
                        let candidate: String = chars[start..].iter().collect();
                        let test = format!("…{}", candidate);
                        let g = ui.painter().layout_no_wrap(
                            test.clone(),
                            path_font.clone(),
                            Color32::WHITE,
                        );
                        if g.size().x <= max_text_w {
                            break;
                        }
                    }
                    let candidate: String = chars[start..].iter().collect();
                    format!("…{}", candidate)
                } else {
                    path_str
                };

                ui.painter().text(
                    Pos2::new(text_x, cy + 10.0),
                    Align2::LEFT_CENTER,
                    &display_path,
                    path_font,
                    colors.text_muted,
                );

                // Track count badge
                let track_count = app
                    .ctx
                    .library
                    .count_tracks_in_folder(&dir.to_string_lossy());
                let badge_text = format!("{} tracks", track_count);
                let badge_font = FontId::proportional(11.0);
                let badge_galley = ui.painter().layout_no_wrap(
                    badge_text.clone(),
                    badge_font.clone(),
                    Color32::WHITE,
                );
                let badge_w = badge_galley.size().x + 16.0;
                let badge_h = 22.0;
                let badge_x = inner_rect.right() - badge_w - 16.0;
                let badge_rect = Rect::from_min_size(
                    Pos2::new(badge_x, cy - badge_h / 2.0),
                    Vec2::new(badge_w, badge_h),
                );

                let badge_bg = if colors.dark_mode {
                    Color32::from_rgba_premultiplied(
                        (colors.text_dim.r() as u16 * 30 / 100 + colors.bg.r() as u16 * 70 / 100)
                            as u8,
                        (colors.text_dim.g() as u16 * 30 / 100 + colors.bg.g() as u16 * 70 / 100)
                            as u8,
                        (colors.text_dim.b() as u16 * 30 / 100 + colors.bg.b() as u16 * 70 / 100)
                            as u8,
                        255,
                    )
                } else {
                    Color32::from_rgb(0xF3, 0xF2, 0xFB)
                };
                ui.painter().rect_filled(badge_rect, 11.0, badge_bg);
                ui.painter().text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    &badge_text,
                    badge_font,
                    if colors.dark_mode {
                        Color32::WHITE
                    } else {
                        colors.text_dim
                    },
                );

                // Click → navigate into folder
                if card_resp.clicked() {
                    app.folder_view_path = Some(dir.clone());
                    // Pre-fetch tracks for this folder
                    app.folder_tracks =
                        app.ctx.library.get_tracks_by_folder(&dir.to_string_lossy());
                }

                // Bottom separator
                ui.painter().line_segment(
                    [
                        Pos2::new(inner_rect.left(), inner_rect.bottom()),
                        Pos2::new(inner_rect.right(), inner_rect.bottom()),
                    ],
                    egui::Stroke::new(0.5, colors.border),
                );

                ui.add_space(4.0);
            }
        });
}

/// Draw the folder contents (State 2): tracks inside a specific folder.
fn draw_folder_contents(app: &mut TuneCraftApp, ui: &mut Ui, colors: &TuneCraftColors) {
    let content_w = ui.available_width();
    let is_narrow = content_w < 500.0;
    let pad_x = if is_narrow { 12.0 } else { 24.0 };

    // Get folder info
    let folder_path = app.folder_view_path.clone().unwrap_or_default();
    let folder_name = folder_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder_path.to_string_lossy().into_owned());

    // Header row: back button + folder name
    ui.horizontal(|ui| {
        ui.add_space(pad_x);

        // Back button
        let back_h = 32.0;
        let back_w = if is_narrow { 70.0 } else { 90.0 };
        let (back_rect, back_resp) =
            ui.allocate_exact_size(Vec2::new(back_w, back_h), Sense::click());
        let back_bg = if back_resp.hovered() {
            colors.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(back_rect, 6.0, back_bg);
        ui.painter().rect_stroke(
            back_rect,
            6.0,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            back_rect.center(),
            Align2::CENTER_CENTER,
            format!("{} Back", egui_phosphor::regular::ARROW_LEFT),
            FontId::proportional(if is_narrow { 11.0 } else { 13.0 }),
            colors.text_dim,
        );
        if back_resp.clicked() {
            app.folder_view_path = None;
            app.folder_tracks.clear();
        }

        ui.add_space(12.0);

        // Folder name + track info
        ui.vertical(|ui| {
            let heading_size = if is_narrow { 18.0 } else { 24.0 };
            ui.label(
                RichText::new(&folder_name)
                    .font(FontId::proportional(heading_size))
                    .color(colors.text)
                    .strong(),
            );

            let total_tracks = app.folder_tracks.len();
            let total_duration_mins: f32 = app
                .folder_tracks
                .iter()
                .map(|t| t.duration_secs / 60.0)
                .sum();
            let hours = (total_duration_mins / 60.0) as u32;
            let mins = (total_duration_mins % 60.0) as u32;
            ui.label(
                RichText::new(format!(
                    "{} tracks \u{2022} {} hours {} minutes",
                    total_tracks, hours, mins
                ))
                .font(FontId::proportional(if is_narrow { 11.0 } else { 13.0 }))
                .color(colors.text_dim),
            );
        });
    });

    ui.add_space(16.0);

    // Track list — reuse the same list rendering style
    let track_row_h = 64.0;
    let folder_tracks_snapshot: Vec<(i64, String, Option<String>, Option<String>, f32)> = app
        .folder_tracks
        .iter()
        .map(|t| {
            (
                t.id,
                t.title.clone(),
                t.artist.clone(),
                t.album.clone(),
                t.duration_secs,
            )
        })
        .collect();

    if folder_tracks_snapshot.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(egui_phosphor::regular::MUSIC_NOTES_SIMPLE)
                    .font(FontId::proportional(48.0))
                    .color(colors.text_muted),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("No tracks found in this folder")
                    .font(FontId::proportional(16.0))
                    .color(colors.text_dim),
            );
        });
        return;
    }

    // Table header
    let width = ui.available_width();
    let (header_rect, _) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::hover());
    ui.painter()
        .rect_filled(header_rect, 0.0, colors.table_header_bg);

    let header_y = header_rect.center().y;
    let header_font = FontId::proportional(12.0);
    let header_color = colors.text_muted;
    let lx = header_rect.left();

    ui.painter().text(
        Pos2::new(lx + pad_x, header_y),
        Align2::LEFT_CENTER,
        "#",
        header_font.clone(),
        header_color,
    );
    ui.painter().text(
        Pos2::new(lx + pad_x + 40.0, header_y),
        Align2::LEFT_CENTER,
        "TITLE",
        header_font.clone(),
        header_color,
    );
    ui.painter().text(
        Pos2::new(lx + width - 48.0, header_y),
        Align2::RIGHT_CENTER,
        "DURATION",
        header_font,
        header_color,
    );

    ui.painter().line_segment(
        [header_rect.left_bottom(), header_rect.right_bottom()],
        egui::Stroke::new(1.0, colors.border),
    );

    // Scrollable track rows
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(
            ui,
            track_row_h,
            folder_tracks_snapshot.len(),
            |ui, row_range| {
                for (i, (track_id, title, artist, _album, duration)) in
                    folder_tracks_snapshot[row_range.clone()].iter().enumerate()
                {
                    let display_num = row_range.start + i + 1;
                    let is_playing = app.current_track_id == Some(*track_id);

                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), track_row_h),
                        Sense::click(),
                    );

                    let bg = if row_resp.hovered() {
                        colors.table_row_hover
                    } else if is_playing {
                        if colors.dark_mode {
                            Color32::from_rgba_premultiplied(
                                (colors.accent.r() as u16 * 12 / 100
                                    + colors.bg.r() as u16 * 88 / 100)
                                    as u8,
                                (colors.accent.g() as u16 * 12 / 100
                                    + colors.bg.g() as u16 * 88 / 100)
                                    as u8,
                                (colors.accent.b() as u16 * 12 / 100
                                    + colors.bg.b() as u16 * 88 / 100)
                                    as u8,
                                255,
                            )
                        } else {
                            colors.active_bg
                        }
                    } else {
                        colors.bg
                    };
                    ui.painter().rect_filled(row_rect, 0.0, bg);

                    // Left accent bar for playing track
                    if is_playing {
                        let bar =
                            Rect::from_min_size(row_rect.left_top(), Vec2::new(3.0, track_row_h));
                        ui.painter().rect_filled(bar, 1.5, colors.accent);
                    }

                    let rlx = row_rect.left();
                    let cy = row_rect.center().y;

                    // Track number
                    let num_color = if is_playing {
                        colors.accent
                    } else {
                        colors.text_dim
                    };
                    if is_playing && !row_resp.hovered() {
                        let circle_center = Pos2::new(rlx + pad_x + 12.0, cy);
                        ui.painter()
                            .circle_filled(circle_center, 12.0, colors.accent);
                        ui.painter().text(
                            circle_center,
                            Align2::CENTER_CENTER,
                            egui_phosphor::regular::PLAY,
                            FontId::proportional(10.0),
                            Color32::WHITE,
                        );
                    } else {
                        ui.painter().text(
                            Pos2::new(rlx + pad_x + 12.0, cy),
                            Align2::CENTER_CENTER,
                            &display_num.to_string(),
                            FontId::proportional(12.0),
                            num_color,
                        );
                    }

                    // Title + Artist
                    let title_x = rlx + pad_x + 40.0;
                    let title_max_w = row_rect.right() - title_x - 80.0;
                    let title_color = if is_playing {
                        if colors.dark_mode {
                            colors.accent_light
                        } else {
                            colors.accent
                        }
                    } else {
                        colors.text
                    };

                    let title_font = FontId::proportional(14.0);
                    let artist_font = FontId::proportional(13.0);

                    // Truncate title
                    let title_galley = ui.painter().layout_no_wrap(
                        title.clone(),
                        title_font.clone(),
                        Color32::WHITE,
                    );
                    let display_title = if title_galley.size().x > title_max_w {
                        let chars: Vec<char> = title.chars().collect();
                        let mut end = chars.len();
                        while end > 0 {
                            end -= 1;
                            let candidate: String = chars[..end].iter().collect();
                            let test = format!("{}...", candidate);
                            let g = ui.painter().layout_no_wrap(
                                test.clone(),
                                title_font.clone(),
                                Color32::WHITE,
                            );
                            if g.size().x <= title_max_w {
                                break;
                            }
                        }
                        let candidate: String = chars[..end].iter().collect();
                        format!("{}...", candidate)
                    } else {
                        title.clone()
                    };

                    ui.painter().text(
                        Pos2::new(title_x, cy - 9.0),
                        Align2::LEFT_CENTER,
                        &display_title,
                        title_font,
                        title_color,
                    );

                    let artist_str = artist.as_deref().unwrap_or("Unknown Artist");
                    ui.painter().text(
                        Pos2::new(title_x, cy + 9.0),
                        Align2::LEFT_CENTER,
                        artist_str,
                        artist_font,
                        colors.text_dim,
                    );

                    // Duration
                    let dur_secs = *duration as u32;
                    let dur_str = format!("{}:{:02}", dur_secs / 60, dur_secs % 60);
                    ui.painter().text(
                        Pos2::new(row_rect.right() - 48.0, cy),
                        Align2::RIGHT_CENTER,
                        &dur_str,
                        FontId::monospace(13.0),
                        colors.text_dim,
                    );

                    // Click to play
                    if row_resp.clicked() {
                        let queue: Vec<i64> =
                            folder_tracks_snapshot.iter().map(|(id, ..)| *id).collect();
                        app.ctx.playback.set_play_queue(queue.clone());
                        app.play_queue = queue;
                        app.play_track(*track_id);
                        app.selected_track_id = Some(*track_id);
                    }

                    // Bottom separator
                    ui.painter().line_segment(
                        [row_rect.left_bottom(), row_rect.right_bottom()],
                        egui::Stroke::new(1.0, colors.border),
                    );
                }
            },
        );
}
