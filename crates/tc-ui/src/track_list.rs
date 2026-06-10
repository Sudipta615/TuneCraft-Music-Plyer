//! Track list view — table with #, Title+Art, Album, Duration, Mood columns
//! Matches the reference design with proper toolbar and real track data.
//! Responsive: columns collapse on narrow viewports, grid adapts card count.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, TextureHandle, Ui, Vec2};

use crate::{app::TuneCraftApp, theme::TuneCraftColors};

fn truncate_text(ui: &Ui, text: &str, font: &FontId, max_width: f32) -> String {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    if galley.size().x <= max_width {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_galley =
        ui.painter()
            .layout_no_wrap(ellipsis.to_string(), font.clone(), Color32::WHITE);
    let target_width = max_width - ellipsis_galley.size().x;
    if target_width <= 0.0 {
        return ellipsis.to_string();
    }
    let char_offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_count = char_offsets.len();
    let mut lo = 0usize;
    let mut hi = char_count;
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let end_byte = if mid < char_count {
            char_offsets[mid]
        } else {
            text.len()
        };
        let prefix = &text[..end_byte];
        let prefix_galley =
            ui.painter()
                .layout_no_wrap(prefix.to_string(), font.clone(), Color32::WHITE);
        if prefix_galley.size().x <= target_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let end_byte = if lo < char_count {
        char_offsets[lo]
    } else {
        text.len()
    };
    format!("{}{}", &text[..end_byte], ellipsis)
}

const COL_NUM_FRAC: f32 = 0.06;
const COL_ART_W: f32 = 44.0; // fixed px for album art thumbnail
const COL_ART_GAP: f32 = 4.0; // gap after art column
const COL_TITLE_FRAC: f32 = 0.34;
const COL_ALBUM_FRAC: f32 = 0.24;

/// Responsive breakpoints for the track list
const BREAKPOINT_NARROW: f32 = 500.0; // hide album columns
const BREAKPOINT_MEDIUM: f32 = 700.0;

/// Draw the top search bar + notification + theme toggle
/// Responsive: hides Add Music button on narrow widths, scales search bar
pub fn draw_topbar(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();
    let total_w = ui.available_width();
    let bar_h = if total_w < BREAKPOINT_NARROW {
        52.0
    } else {
        64.0
    };

    let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, bar_h), Sense::hover());
    ui.painter().rect_filled(bar_rect, 0.0, colors.bg);

    ui.scope_builder(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
        ui.horizontal(|ui| {
            // Search bar — scales width proportionally
            let search_w = if total_w < BREAKPOINT_NARROW {
                (total_w * 0.70).min(300.0)
            } else {
                (total_w * 0.50).min(600.0)
            };

            // Center horizontally in the top bar
            ui.add_space((total_w - search_w) / 2.0);
            let search_h = if total_w < BREAKPOINT_NARROW {
                32.0
            } else {
                40.0
            };
            let search_y = (bar_h - search_h) / 2.0;
            let search_rect = Rect::from_min_size(
                Pos2::new(
                    ui.available_rect_before_wrap().left(),
                    bar_rect.top() + search_y,
                ),
                Vec2::new(search_w, search_h),
            );
            let (_, _) = ui.allocate_exact_size(Vec2::new(search_w, search_h), Sense::hover());
            ui.painter()
                .rect_filled(search_rect, search_h / 2.0, colors.search_bg);

            // Search TextEdit
            let text_edit_x = search_rect.left() + 16.0;
            let text_edit_w = search_rect.width() - 32.0;
            let text_edit_h = if total_w < BREAKPOINT_NARROW {
                14.0
            } else {
                16.0
            };
            let text_edit_y = search_rect.top() + (search_h - text_edit_h) / 2.0 - 1.0;
            let search_resp = ui.put(
                Rect::from_min_size(
                    Pos2::new(text_edit_x, text_edit_y),
                    Vec2::new(text_edit_w, text_edit_h),
                ),
                egui::TextEdit::singleline(&mut app.search_query)
                    .hint_text("Search songs, artists, albums...")
                    .font(FontId::proportional(if total_w < BREAKPOINT_NARROW {
                        12.0
                    } else {
                        14.0
                    }))
                    .text_color(colors.text),
            );

            if app.focus_search {
                search_resp.request_focus();
                app.focus_search = false;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(if total_w < BREAKPOINT_NARROW {
                    8.0
                } else {
                    16.0
                });
            });
        });
    });
}

/// Draw the main track list content area
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    ui.vertical(|ui| {
        // Top search/action bar
        draw_topbar(app, ui);

        ui.add_space(8.0);

        // Title row + toolbar on same line
        let content_w = ui.available_width();
        let is_narrow = content_w < BREAKPOINT_NARROW;
        let is_medium = content_w < BREAKPOINT_MEDIUM;

        ui.horizontal(|ui| {
            ui.add_space(if is_narrow { 12.0 } else { 24.0 });
            ui.vertical(|ui| {
                let heading_size = if is_narrow {
                    20.0
                } else if is_medium {
                    24.0
                } else {
                    28.0
                };
                ui.label(
                    RichText::new(app.nav.label())
                        .font(FontId::proportional(heading_size))
                        .color(colors.text)
                        .strong(),
                );

                let total_tracks = app.tracks.len();
                let total_duration_mins: f32 =
                    app.tracks.iter().map(|t| t.duration_secs / 60.0).sum();
                let hours = (total_duration_mins / 60.0) as u32;
                let mins = (total_duration_mins % 60.0) as u32;
                let sub_font = if is_narrow { 11.0 } else { 14.0 };
                ui.label(
                    RichText::new(format!(
                        "{} tracks \u{2022} {} hours {} minutes",
                        total_tracks, hours, mins
                    ))
                    .font(FontId::proportional(sub_font))
                    .color(colors.text_dim),
                );
            });

            if !app.status_message.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(&app.status_message)
                        .font(FontId::proportional(11.0))
                        .color(colors.accent),
                );
            }

            // Toolbar (right-aligned) — hide some buttons on narrow widths
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(if is_narrow { 8.0 } else { 16.0 });

                // EQ button — hide on narrow
                if !is_narrow {
                    let eq_active = app.show_eq_panel;
                    if styled_toolbar_btn(
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

                if styled_icon_btn(
                    ui,
                    egui_phosphor::regular::SQUARES_FOUR,
                    grid_active,
                    &colors,
                ) {
                    app.list_view = false;
                }
                if styled_icon_btn(ui, egui_phosphor::regular::LIST, list_active, &colors) {
                    app.list_view = true;
                }

                // Sort & Filter — hide on narrow
                if !is_narrow {
                    ui.add_space(4.0);
                    let sort_resp = styled_toolbar_btn(
                        ui,
                        &format!("{} Sort", egui_phosphor::regular::ARROWS_DOWN_UP),
                        app.sort_active,
                        &colors,
                    );
                    let popup_id = ui.make_persistent_id("sort_popup");

                    egui::Popup::from_toggle_button_response(&sort_resp)
                        .id(popup_id)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                        .show(|ui: &mut egui::Ui| {
                            ui.set_min_width(120.0);
                            if ui.selectable_label(!app.sort_active, "Default").clicked() {
                                app.sort_active = false;
                            }
                            if ui
                                .selectable_label(
                                    app.sort_active && app.sort_ascending,
                                    "Ascending",
                                )
                                .clicked()
                            {
                                app.sort_active = true;
                                app.sort_ascending = true;
                            }
                            if ui
                                .selectable_label(
                                    app.sort_active && !app.sort_ascending,
                                    "Descending",
                                )
                                .clicked()
                            {
                                app.sort_active = true;
                                app.sort_ascending = false;
                            }
                        });

                    ui.add_space(4.0);
                    let filter_resp = styled_toolbar_btn(
                        ui,
                        &format!("{} Filter", egui_phosphor::regular::FUNNEL),
                        app.filter_favorites,
                        &colors,
                    );
                    let filter_popup_id = ui.make_persistent_id("filter_popup");

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
                            if ui.button("Release Year").clicked() {
                                egui::Popup::close_id(ui.ctx(), filter_popup_id);
                            }
                            if ui.button("Album Type").clicked() {
                                egui::Popup::close_id(ui.ctx(), filter_popup_id);
                            }
                            if ui.button("File Size").clicked() {
                                egui::Popup::close_id(ui.ctx(), filter_popup_id);
                            }
                        });
                }

                // Pagination
                let total = app.total_track_count;
                let per_page = app.tracks_per_page;
                let current_page = app.track_page;
                let max_page = if total == 0 { 0 } else { total / per_page };
                if max_page > 0 {
                    ui.add_space(8.0);
                    if current_page < max_page
                        && ui
                            .button(
                                RichText::new(egui_phosphor::regular::CARET_RIGHT)
                                    .font(FontId::proportional(11.0))
                                    .color(colors.text_dim),
                            )
                            .clicked()
                    {
                        app.track_page = current_page + 1;
                        app.ctx.library.next_page();
                        app.refresh_tracks();
                    }
                    ui.label(
                        RichText::new(format!("{}/{}", current_page + 1, max_page + 1))
                            .font(FontId::proportional(10.0))
                            .color(colors.text_dim),
                    );
                    if current_page > 0
                        && ui
                            .button(
                                RichText::new(egui_phosphor::regular::CARET_LEFT)
                                    .font(FontId::proportional(11.0))
                                    .color(colors.text_dim),
                            )
                            .clicked()
                    {
                        app.track_page = current_page - 1;
                        app.ctx.library.prev_page();
                        app.refresh_tracks();
                    }
                }
            });
        });

        ui.add_space(16.0);

        let mut filtered_indices: Vec<usize> = if app.search_query.is_empty() {
            let snapshot = app.ctx.library.snapshot();
            if app.tracks.len() != snapshot.tracks.len()
                || app
                    .tracks
                    .iter()
                    .zip(snapshot.tracks.iter())
                    .any(|(a, b)| a.id != b.id)
            {
                app.tracks = snapshot.tracks.clone();
            }
            app.tracks
                .iter()
                .enumerate()
                .filter(|(_, track)| {
                    if app.filter_favorites && !app.cached_favorite_ids.contains(&track.id) {
                        return false;
                    }
                    match app.nav {
                        crate::sidebar::NavSection::AllTracks => true,
                        crate::sidebar::NavSection::Albums => track.album.is_some(),
                        crate::sidebar::NavSection::Artists => track.artist.is_some(),
                        crate::sidebar::NavSection::Favorites => {
                            app.cached_favorite_ids.contains(&track.id)
                        },
                        crate::sidebar::NavSection::RecentlyPlayed => track.last_played.is_some(),
                        crate::sidebar::NavSection::MostPlayed => track.play_count > 0,
                        crate::sidebar::NavSection::Settings => false,
                    }
                })
                .map(|(i, _)| i)
                .collect()
        } else {
            let q = app.search_query.to_lowercase();
            if let Ok(db_tracks) = app.ctx.library.search_tracks(&app.search_query, 200) {
                for db_track in db_tracks {
                    if !app.tracks.iter().any(|t| t.id == db_track.id) {
                        app.tracks.push(db_track);
                    }
                }
            }
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

        if app.list_view {
            draw_list_view(app, ui, &filtered_indices, &colors);
        } else {
            draw_grid_view(app, ui, &filtered_indices, &colors);
        }
    });
}

/// Renders a pill-shaped toolbar button matching the reference design
fn styled_toolbar_btn(
    ui: &mut Ui,
    label: &str,
    active: bool,
    colors: &TuneCraftColors,
) -> egui::Response {
    let font = FontId::proportional(14.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE);
    let btn_w = galley.size().x + 24.0;
    let btn_h = 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(btn_w, btn_h), Sense::click());

    let bg = if active {
        colors.accent
    } else if resp.hovered() {
        colors.hover
    } else {
        Color32::TRANSPARENT
    };

    let border_color = if active { colors.accent } else { colors.border };
    let text_color = if active {
        Color32::WHITE
    } else {
        colors.text_dim
    };

    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        text_color,
    );

    resp
}

/// Icon-only square toggle button for grid/list view
fn styled_icon_btn(ui: &mut Ui, icon: &str, active: bool, colors: &TuneCraftColors) -> bool {
    let btn_h = 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(btn_h, btn_h), Sense::click());

    let bg = if active {
        if colors.dark_mode {
            Color32::from_rgba_premultiplied(
                (colors.accent.r() as u16 * 20 / 100 + colors.bg.r() as u16 * 80 / 100) as u8,
                (colors.accent.g() as u16 * 20 / 100 + colors.bg.g() as u16 * 80 / 100) as u8,
                (colors.accent.b() as u16 * 20 / 100 + colors.bg.b() as u16 * 80 / 100) as u8,
                255,
            )
        } else {
            colors.active_bg
        }
    } else if resp.hovered() {
        colors.hover
    } else {
        Color32::TRANSPARENT
    };

    let border_color = if active { colors.accent } else { colors.border };
    let icon_color = if active {
        colors.accent
    } else {
        colors.text_dim
    };

    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        icon_color,
    );

    resp.clicked()
}

/// Load or retrieve cached album art texture for a track.
///
/// Lazily decodes cover art bytes from the database into an egui TextureHandle,
/// caching the result in `app.album_art_cache` keyed by `track_id`.
/// Returns `None` if no cover art exists for this track.
fn get_or_load_album_art(app: &mut TuneCraftApp, ui: &Ui, track_id: i64) -> Option<TextureHandle> {
    if let Some(handle) = app.album_art_cache.get(&track_id) {
        return Some(handle.clone());
    }

    // Try loading from DB
    let (bytes, _mime) = app.ctx.library.get_cover_art_by_track_id(track_id)?;

    // Decode image bytes → RGBA
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());

    let handle = ui.ctx().load_texture(
        format!("cover_{}", track_id),
        color_image,
        egui::TextureOptions::LINEAR,
    );

    app.album_art_cache.insert(track_id, handle.clone());
    Some(handle)
}

/// Draw album art (real texture or placeholder) into the given rect.
fn draw_album_art(
    ui: &mut Ui,
    rect: egui::Rect,
    colors: &TuneCraftColors,
    is_playing: bool,
    texture: Option<&TextureHandle>,
) {
    if let Some(tex) = texture {
        // Real cover art — paint with rounded corners
        let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        ui.painter().add(egui::epaint::RectShape::filled(
            rect,
            egui::CornerRadius::same(6),
            colors.card,
        ));
        // Draw the texture as a mesh with rounded clip
        let mut mesh = egui::Mesh::with_texture(tex.id());
        mesh.add_rect_with_uv(rect, uv, Color32::WHITE);
        ui.painter().add(egui::Shape::mesh(mesh));
    } else {
        // Placeholder fallback
        let art_color = if is_playing {
            if colors.dark_mode {
                Color32::from_rgba_premultiplied(
                    (colors.accent.r() as u16 * 30 / 100 + colors.card.r() as u16 * 70 / 100) as u8,
                    (colors.accent.g() as u16 * 30 / 100 + colors.card.g() as u16 * 70 / 100) as u8,
                    (colors.accent.b() as u16 * 30 / 100 + colors.card.b() as u16 * 70 / 100) as u8,
                    255,
                )
            } else {
                colors.active_bg
            }
        } else {
            colors.card
        };
        ui.painter().rect_filled(rect, 6.0, art_color);
        if is_playing {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                egui_phosphor::regular::PLAY,
                FontId::proportional(14.0),
                Color32::WHITE,
            );
        } else {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                egui_phosphor::regular::MUSIC_NOTES,
                FontId::proportional(14.0),
                colors.text_dim,
            );
        }
    }
}

/// Compute responsive column visibility based on width
struct ColumnVisibility {
    show_album: bool,
    show_art: bool,
}

fn column_visibility(width: f32) -> ColumnVisibility {
    ColumnVisibility {
        show_album: width >= BREAKPOINT_MEDIUM,
        show_art: width >= BREAKPOINT_NARROW,
    }
}

/// Compute column offsets given width and visibility
fn compute_col_offsets(width: f32, vis: &ColumnVisibility) -> [f32; 5] {
    // ...
    // compute_col_offsets was edited above but we still need to fix array size
    let art_w = if vis.show_art { COL_ART_W } else { 0.0 };
    let art_gap = if vis.show_art { COL_ART_GAP } else { 0.0 };
    let album_frac = if vis.show_album { COL_ALBUM_FRAC } else { 0.0 };
    let rest_frac = 1.0 - COL_NUM_FRAC - COL_TITLE_FRAC - album_frac;
    let title_frac = if !vis.show_album {
        COL_TITLE_FRAC + COL_ALBUM_FRAC
    } else {
        COL_TITLE_FRAC
    };
    let _dur_frac = rest_frac;
    let usable_w = width - art_w - art_gap;
    let col_num_w = usable_w * COL_NUM_FRAC;
    let col_title_w = usable_w * title_frac;
    let col_album_w = usable_w * album_frac;

    [
        0.0f32,
        col_num_w,
        col_num_w + art_w + art_gap,
        col_num_w + art_w + art_gap + col_title_w,
        col_num_w + art_w + art_gap + col_title_w + col_album_w,
    ]
}

/// Draw the traditional list/table view of tracks
fn draw_list_view(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    filtered_indices: &[usize],
    colors: &TuneCraftColors,
) {
    let width = ui.available_width();
    let vis = column_visibility(width);
    let col_offsets = compute_col_offsets(width, &vis);
    let art_w = if vis.show_art { COL_ART_W } else { 0.0 };

    // Table header
    let (header_rect, _) = ui.allocate_exact_size(Vec2::new(width, 48.0), Sense::hover());
    ui.painter()
        .rect_filled(header_rect, 0.0, colors.table_header_bg);

    let header_y = header_rect.center().y;
    let header_font = FontId::proportional(12.0);
    let header_color = colors.text_muted;
    let lx = header_rect.left();

    ui.painter().text(
        Pos2::new(lx + col_offsets[0] + 12.0, header_y),
        Align2::LEFT_CENTER,
        "#",
        header_font.clone(),
        header_color,
    );
    if vis.show_art {
        ui.painter().text(
            Pos2::new(lx + col_offsets[1] + art_w + 8.0, header_y),
            Align2::LEFT_CENTER,
            "TITLE",
            header_font.clone(),
            header_color,
        );
    } else {
        ui.painter().text(
            Pos2::new(lx + col_offsets[1] + 4.0, header_y),
            Align2::LEFT_CENTER,
            "TITLE",
            header_font.clone(),
            header_color,
        );
    }
    if vis.show_album {
        ui.painter().text(
            Pos2::new(lx + col_offsets[3] + 4.0, header_y),
            Align2::LEFT_CENTER,
            "ALBUM",
            header_font.clone(),
            header_color,
        );
    }
    ui.painter().text(
        Pos2::new(lx + width - 48.0, header_y),
        Align2::RIGHT_CENTER,
        "DURATION",
        header_font,
        header_color,
    );

    // Separator line
    ui.painter().line_segment(
        [header_rect.left_bottom(), header_rect.right_bottom()],
        egui::Stroke::new(1.0, colors.border),
    );

    let track_row_h = 64.0;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, track_row_h, filtered_indices.len(), |ui, row_range| {
            for (i, &idx) in filtered_indices[row_range.clone()].iter().enumerate() {
                // Copy track fields upfront to avoid borrow conflicts with album art loading
                let track_id = app.tracks[idx].id;
                let track_title = app.tracks[idx].title.clone();
                let track_artist = app.tracks[idx].artist.clone();
                let track_album = app.tracks[idx].album.clone();
                let track_duration = app.tracks[idx].duration_secs;
                let is_playing = app.current_track_id == Some(track_id);
                let display_num = row_range.start + i + 1;

                let (row_rect, row_resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), track_row_h),
                    Sense::click(),
                );

                let bg = if row_resp.hovered() {
                    colors.table_row_hover
                } else if is_playing {
                    if colors.dark_mode {
                        Color32::from_rgba_premultiplied(
                            (colors.accent.r() as u16 * 12 / 100 + colors.bg.r() as u16 * 88 / 100)
                                as u8,
                            (colors.accent.g() as u16 * 12 / 100 + colors.bg.g() as u16 * 88 / 100)
                                as u8,
                            (colors.accent.b() as u16 * 12 / 100 + colors.bg.b() as u16 * 88 / 100)
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

                // Left accent bar for playing track — full height
                if is_playing {
                    let bar =
                        egui::Rect::from_min_size(row_rect.left_top(), Vec2::new(3.0, track_row_h));
                    ui.painter().rect_filled(bar, 1.5, colors.accent);
                }

                let lx = row_rect.left();
                let cy = row_rect.center().y;
                let rw = row_rect.width();

                // Recompute column offsets for this row's actual width
                let row_vis = column_visibility(rw);
                let cx = compute_col_offsets(rw, &row_vis);
                // Shift by row's left edge
                let cx: [f32; 5] = [lx + cx[0], lx + cx[1], lx + cx[2], lx + cx[3], lx + cx[4]];

                // Track number / play indicator (circled ▶ for playing track)
                let num_font = FontId::proportional(12.0);
                let num_color = if is_playing {
                    colors.accent
                } else {
                    colors.text_dim
                };
                let num_str = display_num.to_string();
                if is_playing && !row_resp.hovered() {
                    // Draw a filled purple circle with ▶ inside
                    let circle_center = Pos2::new(cx[0] + 12.0, cy);
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
                        Pos2::new(cx[0] + 12.0, cy),
                        Align2::CENTER_CENTER,
                        &num_str,
                        num_font,
                        num_color,
                    );
                }

                // Album art — real cover art or placeholder — only if visible
                let row_art_tex = get_or_load_album_art(app, ui, track_id);
                if row_vis.show_art {
                    let art_size = 44.0;
                    let art_rect = egui::Rect::from_center_size(
                        Pos2::new(cx[1] + art_w / 2.0, cy),
                        Vec2::new(art_size, art_size),
                    );
                    draw_album_art(ui, art_rect, colors, is_playing, row_art_tex.as_ref());
                }

                // Title + Artist (stacked)
                let title_end_x = if row_vis.show_album { cx[3] } else { cx[4] };
                let title_max_w = title_end_x - cx[2] - 8.0;
                let title_font = FontId::proportional(14.0);
                let artist_font = FontId::proportional(13.0);
                let title_color = if is_playing {
                    if colors.dark_mode {
                        colors.accent_light
                    } else {
                        colors.accent
                    }
                } else {
                    colors.text
                };
                let truncated_title = truncate_text(ui, &track_title, &title_font, title_max_w);
                ui.painter().text(
                    Pos2::new(cx[2], cy - 9.0),
                    Align2::LEFT_CENTER,
                    &truncated_title,
                    title_font,
                    title_color,
                );
                let artist = track_artist.as_deref().unwrap_or("Unknown Artist");
                let truncated_artist = truncate_text(ui, artist, &artist_font, title_max_w);
                ui.painter().text(
                    Pos2::new(cx[2], cy + 9.0),
                    Align2::LEFT_CENTER,
                    &truncated_artist,
                    artist_font,
                    colors.text_dim,
                );

                // Album — only if column is visible
                if row_vis.show_album {
                    let album_max_w = cx[4] - cx[3] - 8.0;
                    let album_font = FontId::proportional(14.0);
                    let album = track_album.as_deref().unwrap_or("");
                    let truncated_album = truncate_text(ui, album, &album_font, album_max_w);
                    ui.painter().text(
                        Pos2::new(cx[3] + 4.0, cy),
                        Align2::LEFT_CENTER,
                        &truncated_album,
                        album_font,
                        colors.text_dim,
                    );
                }

                // Duration
                let dur_secs = track_duration as u32;
                let dur_str = format!("{}:{:02}", dur_secs / 60, dur_secs % 60);
                let dur_x = row_rect.right() - 48.0;
                ui.painter().text(
                    Pos2::new(dur_x, cy),
                    Align2::RIGHT_CENTER,
                    &dur_str,
                    FontId::monospace(13.0),
                    colors.text_dim,
                );

                // Three-dot context menu — always visible (matching reference image)
                let dots_rect = egui::Rect::from_center_size(
                    Pos2::new(row_rect.right() - 16.0, cy),
                    Vec2::new(20.0, 20.0),
                );
                let dots_color = if row_resp.hovered() {
                    colors.text
                } else {
                    colors.text_dim
                };
                ui.painter().text(
                    dots_rect.center(),
                    Align2::CENTER_CENTER,
                    egui_phosphor::regular::DOTS_THREE_VERTICAL,
                    FontId::proportional(18.0),
                    dots_color,
                );

                // Interactions
                if row_resp.clicked() {
                    let new_queue: Vec<i64> = filtered_indices
                        .iter()
                        .filter_map(|&idx| app.tracks.get(idx).map(|t| t.id))
                        .collect();
                    app.ctx.playback.set_play_queue(new_queue.clone());
                    app.play_queue = new_queue;
                    app.play_track(track_id);
                    app.selected_track_id = Some(track_id);
                }

                // Bottom row separator
                ui.painter().line_segment(
                    [row_rect.left_bottom(), row_rect.right_bottom()],
                    egui::Stroke::new(1.0, colors.border),
                );
            }
        });
}

/// Draw the grid/card view of tracks — responsive card sizing
fn draw_grid_view(
    app: &mut TuneCraftApp,
    ui: &mut Ui,
    filtered_indices: &[usize],
    colors: &TuneCraftColors,
) {
    let available_width = ui.available_width();
    // Adaptive card sizing: smaller cards on narrow viewports
    let (card_width, card_height, card_spacing) = if available_width < BREAKPOINT_NARROW {
        (140.0, 80.0, 6.0)
    } else if available_width < BREAKPOINT_MEDIUM {
        (160.0, 88.0, 8.0)
    } else {
        (180.0, 96.0, 10.0)
    };
    let columns =
        ((available_width + card_spacing) / (card_width + card_spacing)).max(1.0) as usize;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for chunk in filtered_indices.chunks(columns) {
                ui.horizontal(|ui| {
                    for &idx in chunk {
                        // Copy track fields upfront to avoid borrow conflicts
                        let track_id = app.tracks[idx].id;
                        let track_title = app.tracks[idx].title.clone();
                        let track_artist = app.tracks[idx].artist.clone();
                        let track_album = app.tracks[idx].album.clone();
                        let track_duration = app.tracks[idx].duration_secs;
                        let is_playing = app.current_track_id == Some(track_id);

                        let (card_rect, card_resp) = ui.allocate_exact_size(
                            Vec2::new(card_width, card_height),
                            Sense::click(),
                        );

                        let bg = if card_resp.hovered() {
                            colors.table_row_hover
                        } else if is_playing {
                            if colors.dark_mode {
                                Color32::from_rgba_premultiplied(
                                    (colors.accent.r() as u16 * 15 / 100
                                        + colors.card.r() as u16 * 85 / 100)
                                        as u8,
                                    (colors.accent.g() as u16 * 15 / 100
                                        + colors.card.g() as u16 * 85 / 100)
                                        as u8,
                                    (colors.accent.b() as u16 * 15 / 100
                                        + colors.card.b() as u16 * 85 / 100)
                                        as u8,
                                    255,
                                )
                            } else {
                                colors.active_bg
                            }
                        } else {
                            colors.card
                        };
                        ui.painter().rect_filled(card_rect, 8.0, bg);
                        ui.painter().rect_stroke(
                            card_rect,
                            8.0,
                            egui::Stroke::new(1.0, colors.border),
                            egui::StrokeKind::Inside,
                        );

                        // Album art on left (real or placeholder)
                        let grid_art_tex = get_or_load_album_art(app, ui, track_id);
                        let art_size = card_height - 16.0;
                        let art_rect = egui::Rect::from_min_size(
                            Pos2::new(card_rect.left() + 8.0, card_rect.top() + 8.0),
                            Vec2::new(art_size, art_size),
                        );
                        draw_album_art(ui, art_rect, colors, is_playing, grid_art_tex.as_ref());

                        // Left accent bar if playing
                        if is_playing {
                            let bar = egui::Rect::from_min_size(
                                card_rect.left_top(),
                                Vec2::new(3.0, card_height),
                            );
                            ui.painter().rect_filled(bar, 8.0, colors.accent);
                        }

                        let text_x = art_rect.right() + 8.0;
                        let max_text_w = card_rect.right() - text_x - 6.0;
                        let title_color = if is_playing {
                            colors.accent
                        } else {
                            colors.text
                        };

                        let title_font = FontId::proportional(12.0);
                        let truncated_title =
                            truncate_text(ui, &track_title, &title_font, max_text_w);
                        ui.painter().text(
                            Pos2::new(text_x, card_rect.top() + 18.0),
                            Align2::LEFT_CENTER,
                            &truncated_title,
                            title_font,
                            title_color,
                        );

                        let artist_font = FontId::proportional(10.0);
                        let artist = track_artist.as_deref().unwrap_or("Unknown");
                        let truncated_artist = truncate_text(ui, artist, &artist_font, max_text_w);
                        ui.painter().text(
                            Pos2::new(text_x, card_rect.top() + 34.0),
                            Align2::LEFT_CENTER,
                            &truncated_artist,
                            artist_font,
                            colors.text_dim,
                        );

                        let album_font = FontId::proportional(9.5);
                        let album = track_album.as_deref().unwrap_or("");
                        let truncated_album = truncate_text(ui, album, &album_font, max_text_w);
                        ui.painter().text(
                            Pos2::new(text_x, card_rect.top() + 50.0),
                            Align2::LEFT_CENTER,
                            &truncated_album,
                            album_font,
                            colors.text_dim,
                        );

                        let dur_secs = track_duration as u32;
                        let dur_str = format!("{}:{:02}", dur_secs / 60, dur_secs % 60);
                        ui.painter().text(
                            Pos2::new(text_x, card_rect.top() + 66.0),
                            Align2::LEFT_CENTER,
                            &dur_str,
                            FontId::proportional(9.5),
                            colors.text_dim,
                        );

                        if card_resp.clicked() {
                            let new_queue: Vec<i64> = filtered_indices
                                .iter()
                                .filter_map(|&idx| app.tracks.get(idx).map(|t| t.id))
                                .collect();
                            app.ctx.playback.set_play_queue(new_queue.clone());
                            app.play_queue = new_queue;
                            app.play_track(track_id);
                            app.selected_track_id = Some(track_id);
                        }

                        ui.add_space(card_spacing);
                    }
                });
                ui.add_space(card_spacing);
            }
        });
}
