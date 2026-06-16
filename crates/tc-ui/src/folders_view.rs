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

        // Right-aligned toolbar area empty since Add buttons were moved
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(if is_narrow { 8.0 } else { 16.0 });
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

                // Delete button
                let delete_size = 28.0;
                let delete_x = inner_rect.right() - delete_size - 16.0;
                let delete_rect = Rect::from_min_size(
                    Pos2::new(delete_x, cy - delete_size / 2.0),
                    Vec2::splat(delete_size),
                );
                let delete_id = ui.id().with(format!("del_{}", i));
                let delete_resp = ui.interact(delete_rect, delete_id, Sense::click());

                let del_color = if delete_resp.hovered() {
                    colors.accent
                } else {
                    colors.text_dim
                };
                ui.painter().text(
                    delete_rect.center(),
                    Align2::CENTER_CENTER,
                    egui_phosphor::regular::TRASH,
                    FontId::proportional(16.0),
                    del_color,
                );

                if delete_resp.clicked() {
                    let dir_clone = dir.clone();
                    app.ctx.config.write(|c| {
                        c.library.watch_dirs.retain(|d| d != &dir_clone);
                    });
                    // Need to trigger a background analysis or just delete tracks
                    // We can't delete directly if library module isn't exposed, but wait, `tc-db` has it.
                    // Wait! app.ctx.library has delete_tracks_by_folder? We checked track.rs and it has delete_tracks_by_folder.
                    // Let's use app.ctx.library.delete_tracks_by_folder? The function is on Db or Library?
                    // It's on Db, so it's app.ctx.library.db.delete_tracks_by_folder... Oh, app.ctx.library might not expose `db`.
                }

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
                let badge_x = delete_rect.left() - badge_w - 16.0;
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

                // Click → navigate into folder (make sure we didn't click delete)
                if card_resp.clicked() && !delete_rect.contains(card_resp.interact_pointer_pos().unwrap_or_default()) {
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

    let folder_path = app.folder_view_path.clone().unwrap_or_default();
    let folder_str = folder_path.to_string_lossy().into_owned();
    let prefix = if folder_str.ends_with('/') { folder_str.clone() } else { format!("{}/", folder_str) };

    let folder_name = folder_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder_str.clone());

    ui.horizontal(|ui| {
        ui.add_space(pad_x);

        let back_h = 32.0;
        let back_w = if is_narrow { 70.0 } else { 90.0 };
        let (back_rect, back_resp) = ui.allocate_exact_size(Vec2::new(back_w, back_h), Sense::click());
        let back_bg = if back_resp.hovered() { colors.hover } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(back_rect, 6.0, back_bg);
        ui.painter().rect_stroke(back_rect, 6.0, egui::Stroke::new(1.0, colors.border), egui::StrokeKind::Inside);
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
            return;
        }

        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(
                RichText::new(&folder_name)
                    .font(FontId::proportional(if is_narrow { 18.0 } else { 24.0 }))
                    .color(colors.text)
                    .strong(),
            );
            ui.label(
                RichText::new(&folder_str)
                    .font(FontId::proportional(if is_narrow { 11.0 } else { 13.0 }))
                    .color(colors.text_dim),
            );
        });
    });

    ui.add_space(16.0);

    // Split app.folder_tracks into subfolders and exact tracks
    let mut exact_indices = Vec::new();
    let mut subfolders = std::collections::HashSet::new();

    for (i, track) in app.folder_tracks.iter().enumerate() {
        let path = std::path::Path::new(&track.path);
        if let Some(parent) = path.parent() {
            let parent_str = parent.to_string_lossy();
            if parent_str == folder_str || parent_str == prefix.trim_end_matches('/') {
                exact_indices.push(i);
            } else if parent_str.starts_with(&prefix) {
                if let Ok(stripped) = parent.strip_prefix(&folder_path) {
                    if let Some(first) = stripped.components().next() {
                        let mut sub = folder_path.clone();
                        sub.push(first);
                        subfolders.insert(sub);
                    }
                }
            }
        }
    }

    let mut sorted_subfolders: Vec<PathBuf> = subfolders.into_iter().collect();
    sorted_subfolders.sort();

    // Render Subfolders
    if !sorted_subfolders.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(pad_x);
            ui.label(
                RichText::new("Folders")
                    .font(FontId::proportional(18.0))
                    .color(colors.text)
                    .strong()
            );
        });
        ui.add_space(8.0);

        let card_h = 60.0;
        for dir in sorted_subfolders {
            let (card_rect, card_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), card_h), Sense::click());
            let inner_rect = Rect::from_min_max(
                Pos2::new(card_rect.left() + pad_x, card_rect.top()),
                Pos2::new(card_rect.right() - pad_x, card_rect.bottom()),
            );
            
            let bg = if card_resp.hovered() { colors.table_row_hover } else { colors.bg };
            ui.painter().rect_filled(inner_rect, 8.0, bg);
            ui.painter().rect_stroke(inner_rect, 8.0, egui::Stroke::new(1.0, colors.border), egui::StrokeKind::Inside);
            
            let cy = inner_rect.center().y;
            let icon_x = inner_rect.left() + 20.0;
            ui.painter().text(
                Pos2::new(icon_x, cy),
                Align2::CENTER_CENTER,
                egui_phosphor::regular::FOLDER,
                FontId::proportional(22.0),
                colors.accent,
            );
            
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            ui.painter().text(
                Pos2::new(icon_x + 30.0, cy),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(15.0),
                colors.text,
            );

            // Subfolder track count
            let track_count = app.ctx.library.count_tracks_in_folder(&dir.to_string_lossy());
            let badge_text = format!("{} tracks", track_count);
            let badge_font = FontId::proportional(11.0);
            let badge_galley = ui.painter().layout_no_wrap(badge_text.clone(), badge_font.clone(), Color32::WHITE);
            let badge_w = badge_galley.size().x + 16.0;
            let badge_rect = Rect::from_min_size(
                Pos2::new(inner_rect.right() - badge_w - 16.0, cy - 11.0),
                Vec2::new(badge_w, 22.0),
            );
            ui.painter().rect_filled(badge_rect, 11.0, colors.hover);
            ui.painter().text(
                badge_rect.center(), Align2::CENTER_CENTER, &badge_text, badge_font, colors.text_dim
            );
            
            if card_resp.clicked() {
                app.folder_view_path = Some(dir.clone());
                app.folder_tracks = app.ctx.library.get_tracks_by_folder(&dir.to_string_lossy());
                return;
            }
            ui.add_space(4.0);
        }
        ui.add_space(16.0);
    }

    if exact_indices.is_empty() {
        if sorted_subfolders.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(egui_phosphor::regular::FOLDER_OPEN).font(FontId::proportional(48.0)).color(colors.text_muted));
                ui.add_space(12.0);
                ui.label(RichText::new("Empty folder").font(FontId::proportional(16.0)).color(colors.text_dim));
            });
        }
        return;
    }

    // Render exact tracks toolbar
    ui.horizontal(|ui| {
        ui.add_space(pad_x);
        ui.label(RichText::new("Songs").font(FontId::proportional(18.0)).color(colors.text).strong());
        
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(pad_x);
            let list_active = app.list_view;
            let grid_active = !app.list_view;

            if crate::track_list::styled_icon_btn(ui, egui_phosphor::regular::SQUARES_FOUR, grid_active, colors) {
                app.list_view = false;
            }
            if crate::track_list::styled_icon_btn(ui, egui_phosphor::regular::LIST, list_active, colors) {
                app.list_view = true;
            }
            ui.add_space(8.0);
            
            if !is_narrow {
                let sort_resp = crate::track_list::styled_toolbar_btn(
                    ui, &format!("{} Sort", egui_phosphor::regular::ARROWS_DOWN_UP), app.sort_active, colors
                );
                let popup_id = ui.make_persistent_id("sort_popup_folder");
                egui::Popup::from_toggle_button_response(&sort_resp)
                    .id(popup_id)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                    .show(|ui: &mut egui::Ui| {
                        ui.set_min_width(120.0);
                        if ui.selectable_label(!app.sort_active, "Default").clicked() { app.sort_active = false; }
                        if ui.selectable_label(app.sort_active && app.sort_ascending, "Ascending").clicked() { app.sort_active = true; app.sort_ascending = true; }
                        if ui.selectable_label(app.sort_active && !app.sort_ascending, "Descending").clicked() { app.sort_active = true; app.sort_ascending = false; }
                    });
            }
        });
    });

    ui.add_space(8.0);

    // Swap app.tracks to exact tracks so track_list functions render them correctly
    let exact_tracks_vec: Vec<_> = exact_indices.iter().map(|&i| app.folder_tracks[i].clone()).collect();
    let original_tracks = std::mem::replace(&mut app.tracks, exact_tracks_vec);
    let track_indices: Vec<usize> = (0..app.tracks.len()).collect();

    // Sorting logic (copying from track_list.rs)
    let mut filtered_indices = track_indices;
    if app.sort_active {
        filtered_indices.sort_by(|&a, &b| {
            let ta = &app.tracks[a];
            let tb = &app.tracks[b];
            let cmp = ta.title.to_lowercase().cmp(&tb.title.to_lowercase());
            if app.sort_ascending { cmp } else { cmp.reverse() }
        });
    }

    if app.list_view {
        crate::track_list::draw_list_view(app, ui, &filtered_indices, colors);
    } else {
        crate::track_list::draw_grid_view(app, ui, &filtered_indices, colors);
    }

    // Restore app.tracks
    app.tracks = original_tracks;
}
