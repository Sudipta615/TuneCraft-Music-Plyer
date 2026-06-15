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
                RichText::new("Add folders from Settings to see them here")
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

                let max_text_w = inner_rect.right() - text_x - 140.0;
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
                // Delete button
                let del_size = 32.0;
                let del_rect = Rect::from_center_size(
                    Pos2::new(inner_rect.right() - pad_x - del_size / 2.0, cy),
                    Vec2::splat(del_size),
                );
                let del_resp = ui.put(
                    del_rect,
                    egui::Button::new(
                        RichText::new(egui_phosphor::regular::TRASH)
                            .font(FontId::proportional(16.0))
                            .color(if ui.rect_contains_pointer(del_rect) {
                                colors.accent
                            } else {
                                colors.text_dim
                            }),
                    )
                    .frame(false),
                );

                if del_resp.clicked() {
                    app.remove_music_folder(&dir.to_string_lossy());
                }

                let badge_x = del_rect.left() - badge_w - 16.0;
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
                if card_resp.clicked() && !del_resp.clicked() {
                    app.folder_view_path = Some(dir.clone());
                    // Pre-fetch tracks for this folder
                    app.folder_tracks =
                        app.ctx.library.get_tracks_by_folder(&dir.to_string_lossy());

                    // Extract direct tracks into app.tracks for drawing
                    let mut direct_tracks = Vec::new();
                    for track in &app.folder_tracks {
                        let track_path = std::path::Path::new(&track.path);
                        if let Some(track_dir) = track_path.parent() {
                            if track_dir == dir {
                                direct_tracks.push(track.clone());
                            }
                        }
                    }
                    app.tracks = direct_tracks;
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
fn process_folder_contents(
    folder_path: &std::path::Path,
    tracks: &[crate::app::Track],
) -> Vec<PathBuf> {
    let mut subfolders = std::collections::HashSet::new();
    for track in tracks {
        let track_path = std::path::Path::new(&track.path);
        if let Some(track_dir) = track_path.parent() {
            if track_dir.starts_with(folder_path) && track_dir != folder_path {
                if let Ok(relative) = track_dir.strip_prefix(folder_path) {
                    if let Some(first_comp) = relative.components().next() {
                        subfolders.insert(folder_path.join(first_comp));
                    }
                }
            }
        }
    }
    let mut subfolders_vec: Vec<PathBuf> = subfolders.into_iter().collect();
    subfolders_vec.sort();
    subfolders_vec
}

/// Draw the folder contents (State 2): tracks and subfolders inside a specific folder.
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

    // Compute subfolders dynamically
    let subfolders = process_folder_contents(&folder_path, &app.folder_tracks);

    // Header row: back button + folder name + toolbar
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
            // Traverse up one level or go back to root folder list
            if let Some(parent) = folder_path.parent() {
                let watch_dirs = app
                    .ctx
                    .config
                    .read(|c| c.library.watch_dirs.clone())
                    .unwrap_or_default();
                if watch_dirs.contains(&folder_path) {
                    // It was a root folder, go back to main list
                    app.folder_view_path = None;
                    app.folder_tracks.clear();
                    app.tracks.clear();
                } else {
                    app.folder_view_path = Some(parent.to_path_buf());
                    app.folder_tracks = app
                        .ctx
                        .library
                        .get_tracks_by_folder(&parent.to_string_lossy());
                    let mut direct_tracks = Vec::new();
                    for track in &app.folder_tracks {
                        let track_path = std::path::Path::new(&track.path);
                        if let Some(track_dir) = track_path.parent() {
                            if track_dir == parent {
                                direct_tracks.push(track.clone());
                            }
                        }
                    }
                    app.tracks = direct_tracks;
                }
            } else {
                app.folder_view_path = None;
                app.folder_tracks.clear();
                app.tracks.clear();
            }
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

            let direct_track_count = app.tracks.len();
            let total_duration_mins: f32 = app.tracks.iter().map(|t| t.duration_secs / 60.0).sum();
            let hours = (total_duration_mins / 60.0) as u32;
            let mins = (total_duration_mins % 60.0) as u32;
            ui.label(
                RichText::new(format!(
                    "{} tracks \u{2022} {} hours {} minutes",
                    direct_track_count, hours, mins
                ))
                .font(FontId::proportional(if is_narrow { 11.0 } else { 13.0 }))
                .color(colors.text_dim),
            );
        });

        // Toolbar (right-aligned) — hide some buttons on narrow widths
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(if is_narrow { 8.0 } else { 16.0 });

            // EQ button — hide on narrow
            if !is_narrow {
                let eq_active = app.show_eq_panel;
                if crate::track_list::styled_toolbar_btn(
                    ui,
                    &format!("{} EQ", egui_phosphor::regular::SLIDERS_HORIZONTAL),
                    eq_active,
                    &colors,
                )
                .clicked()
                {
                    app.ctx.eq.toggle_panel();
                    app.show_eq_panel = !eq_active;
                }
                ui.add_space(4.0);
            }

            // Grid / List toggle buttons side by side
            let list_active = app.list_view;
            let grid_active = !app.list_view;

            if crate::track_list::styled_icon_btn(
                ui,
                egui_phosphor::regular::SQUARES_FOUR,
                grid_active,
                &colors,
            ) {
                app.list_view = false;
            }
            if crate::track_list::styled_icon_btn(
                ui,
                egui_phosphor::regular::LIST,
                list_active,
                &colors,
            ) {
                app.list_view = true;
            }

            // Sort & Filter — hide on narrow
            if !is_narrow {
                ui.add_space(4.0);
                let sort_resp = crate::track_list::styled_toolbar_btn(
                    ui,
                    &format!("{} Sort", egui_phosphor::regular::ARROWS_DOWN_UP),
                    app.sort_active,
                    &colors,
                );
                let popup_id = ui.make_persistent_id("sort_popup_folders");

                egui::Popup::from_toggle_button_response(&sort_resp)
                    .id(popup_id)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                    .show(|ui: &mut egui::Ui| {
                        ui.set_min_width(120.0);
                        if ui.selectable_label(!app.sort_active, "Default").clicked() {
                            app.sort_active = false;
                        }
                        if ui
                            .selectable_label(app.sort_active && app.sort_ascending, "Ascending")
                            .clicked()
                        {
                            app.sort_active = true;
                            app.sort_ascending = true;
                        }
                        if ui
                            .selectable_label(app.sort_active && !app.sort_ascending, "Descending")
                            .clicked()
                        {
                            app.sort_active = true;
                            app.sort_ascending = false;
                        }
                    });

                ui.add_space(4.0);
                let filter_resp = crate::track_list::styled_toolbar_btn(
                    ui,
                    &format!("{} Filter", egui_phosphor::regular::FUNNEL),
                    app.filter_favorites,
                    &colors,
                );
                let filter_popup_id = ui.make_persistent_id("filter_popup_folders");

                egui::Popup::from_toggle_button_response(&filter_resp)
                    .id(filter_popup_id)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                    .show(|ui: &mut egui::Ui| {
                        ui.set_min_width(120.0);
                        if ui
                            .selectable_label(app.filter_favorites, "Favorites Only")
                            .clicked()
                        {
                            app.filter_favorites = !app.filter_favorites;
                            egui::Popup::close_id(ui.ctx(), filter_popup_id);
                        }
                    });
            }
        });
    });

    ui.add_space(16.0);

    // Draw Subfolders
    if !subfolders.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(pad_x);
            ui.label(
                RichText::new("Folders")
                    .font(FontId::proportional(14.0))
                    .color(colors.text_dim),
            );
        });
        ui.add_space(8.0);

        // We use a small scroll area for subfolders if there are many, or just wrap
        egui::ScrollArea::vertical()
            .id_source("subfolders_scroll")
            .auto_shrink([false, true])
            .max_height(200.0)
            .show(ui, |ui| {
                for subfolder in subfolders {
                    let (row_rect, row_resp) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::click());
                    let bg = if row_resp.hovered() {
                        colors.table_row_hover
                    } else {
                        colors.bg
                    };
                    ui.painter().rect_filled(row_rect, 0.0, bg);

                    let cy = row_rect.center().y;
                    let icon_x = row_rect.left() + pad_x + 12.0;

                    // Folder icon
                    ui.painter().text(
                        Pos2::new(icon_x, cy),
                        Align2::CENTER_CENTER,
                        egui_phosphor::regular::FOLDER,
                        FontId::proportional(20.0),
                        colors.accent,
                    );

                    let subfolder_name = subfolder
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    ui.painter().text(
                        Pos2::new(icon_x + 24.0, cy),
                        Align2::LEFT_CENTER,
                        &subfolder_name,
                        FontId::proportional(14.0),
                        colors.text,
                    );

                    ui.painter().line_segment(
                        [row_rect.left_bottom(), row_rect.right_bottom()],
                        egui::Stroke::new(1.0, colors.border),
                    );

                    if row_resp.clicked() {
                        app.folder_view_path = Some(subfolder.clone());
                        app.folder_tracks = app
                            .ctx
                            .library
                            .get_tracks_by_folder(&subfolder.to_string_lossy());
                        let mut direct_tracks = Vec::new();
                        for track in &app.folder_tracks {
                            let track_path = std::path::Path::new(&track.path);
                            if let Some(track_dir) = track_path.parent() {
                                if track_dir == subfolder {
                                    direct_tracks.push(track.clone());
                                }
                            }
                        }
                        app.tracks = direct_tracks;
                        app.search_query.clear(); // Reset search when entering a new folder
                    }
                }
            });
        ui.add_space(16.0);
    }

    if app.tracks.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(egui_phosphor::regular::MUSIC_NOTES_SIMPLE)
                    .font(FontId::proportional(48.0))
                    .color(colors.text_muted),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("No tracks directly in this folder")
                    .font(FontId::proportional(16.0))
                    .color(colors.text_dim),
            );
        });
        return;
    }

    // Filter app.tracks for the list view
    let mut filtered_indices: Vec<usize> = if app.search_query.is_empty() {
        app.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                if app.filter_favorites && !app.cached_favorite_ids.contains(&track.id) {
                    return false;
                }
                true
            })
            .map(|(i, _)| i)
            .collect()
    } else {
        let q = app.search_query.to_lowercase();
        app.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                if app.filter_favorites && !app.cached_favorite_ids.contains(&track.id) {
                    return false;
                }
                track.title.to_lowercase().contains(&q)
                    || track
                        .artist
                        .as_ref()
                        .is_some_and(|a| a.to_lowercase().contains(&q))
                    || track
                        .album
                        .as_ref()
                        .is_some_and(|a| a.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    };

    if app.sort_active {
        if app.sort_ascending {
            filtered_indices.sort_by(|&a, &b| app.tracks[a].title.cmp(&app.tracks[b].title));
        } else {
            filtered_indices.sort_by(|&a, &b| app.tracks[b].title.cmp(&app.tracks[a].title));
        }
    }

    // Track list — reuse the same list rendering style from track_list
    if app.list_view {
        crate::track_list::draw_list_view(app, ui, &filtered_indices, colors);
    } else {
        crate::track_list::draw_grid_view(app, ui, &filtered_indices, colors);
    }
}
