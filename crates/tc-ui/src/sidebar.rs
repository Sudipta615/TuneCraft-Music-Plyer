//! Sidebar navigation — Library, Playlists, Mood, Settings
//! Matches the reference design: logo + icon list, section headers, badges.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::{app::TuneCraftApp, theme::TuneCraftColors};

/// Which navigation section is active
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavSection {
    AllTracks,
    Albums,
    Artists,
    Folders,
    Favorites,
    RecentlyPlayed,
    MostPlayed,
    Settings,
}

impl NavSection {
    pub fn label(&self) -> &str {
        match self {
            Self::AllTracks => "All Tracks",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Folders => "Folders",
            Self::Favorites => "Favorites",
            Self::RecentlyPlayed => "Recently Played",
            Self::MostPlayed => "Most Played",
            Self::Settings => "Settings",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::AllTracks => egui_phosphor::regular::MUSIC_NOTES,
            Self::Albums => egui_phosphor::regular::SQUARES_FOUR,
            Self::Artists => egui_phosphor::regular::USERS,
            Self::Folders => egui_phosphor::regular::FOLDER,
            Self::Favorites => egui_phosphor::regular::STAR,
            Self::RecentlyPlayed => egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
            Self::MostPlayed => egui_phosphor::regular::CHART_BAR,
            Self::Settings => egui_phosphor::regular::GEAR,
        }
    }

    pub fn badge_count(&self, tracks: &[tc_db::Track]) -> Option<u32> {
        match self {
            Self::AllTracks => Some(tracks.len() as u32),
            Self::Favorites => None,
            Self::RecentlyPlayed => {
                let now = chrono::Utc::now().naive_utc();
                Some(tracks.iter().filter(|t| t.last_played.map_or(false, |dt| (now - dt).num_hours() <= 48)).count() as u32)
            }
            Self::MostPlayed => Some(tracks.iter().filter(|t| t.play_count > 3).count().min(30) as u32),
            _ => None,
        }
    }
}

/// Draw the sidebar panel
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    // Sidebar background — either solid or glass over the wallpaper
    let sidebar_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(sidebar_rect, 0.0, colors.sidebar);
    // Right border separator line
    ui.painter().line_segment(
        [sidebar_rect.right_top(), sidebar_rect.right_bottom()],
        egui::Stroke::new(1.0, colors.border),
    );

    ui.add_space(16.0);

    // Logo row: waveform bars icon + "TuneCraft" text
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        // Draw waveform-like bars icon (matching reference design)
        let icon_size = 22.0;
        let (icon_rect, _) =
            ui.allocate_exact_size(Vec2::new(icon_size, icon_size), Sense::hover());
        let bar_heights = [0.5_f32, 1.0, 0.7, 1.0, 0.5];
        let bar_w = 2.5_f32;
        let bar_gap = 1.5_f32;
        let total_w = bar_heights.len() as f32 * bar_w + (bar_heights.len() - 1) as f32 * bar_gap;
        let start_x = icon_rect.center().x - total_w / 2.0;
        for (i, &frac) in bar_heights.iter().enumerate() {
            let bx = start_x + i as f32 * (bar_w + bar_gap);
            let bh = icon_rect.height() * frac;
            let by = icon_rect.center().y - bh / 2.0;
            let bar_r = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bar_w, bh));
            ui.painter().rect_filled(bar_r, 1.0, colors.accent);
        }

        if !app.sidebar_collapsed {
            ui.add_space(8.0);
            ui.label(
                RichText::new("TuneCraft")
                    .font(FontId::proportional(18.0))
                    .color(colors.text)
                    .strong(),
            );
        }

        // Push collapse button to the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            let chevron = if app.sidebar_collapsed {
                "\u{00BB}" // »
            } else {
                "\u{00AB}" // «
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(chevron)
                            .font(FontId::proportional(14.0))
                            .color(colors.text_dim),
                    )
                    .frame(false),
                )
                .clicked()
            {
                app.sidebar_collapsed = !app.sidebar_collapsed;
            }
        });
    });

    ui.add_space(24.0);

    section_header(ui, "LIBRARY", &colors, app.sidebar_collapsed);
    nav_item(ui, app, NavSection::AllTracks);
    nav_item(ui, app, NavSection::Albums);
    nav_item(ui, app, NavSection::Artists);
    nav_item(ui, app, NavSection::Folders);

    ui.add_space(16.0);

    section_header(ui, "PLAYLISTS", &colors, app.sidebar_collapsed);
    nav_item(ui, app, NavSection::Favorites);
    nav_item(ui, app, NavSection::RecentlyPlayed);
    nav_item(ui, app, NavSection::MostPlayed);

    // User-created playlists
    if !app.playlists_loaded {
        app.reload_playlists();
        app.playlists_loaded = true;
    }
    let playlists_snapshot: Vec<(i64, String)> = app
        .playlists
        .iter()
        .filter(|p| !p.is_smart)
        .map(|p| (p.id, p.name.clone()))
        .collect();
    for (id, name) in &playlists_snapshot {
        let selected = app.selected_playlist_id == Some(*id);
        let text_color = if selected {
            colors.accent
        } else {
            colors.text_dim
        };
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
        let pad_x = 12.0;
        let pill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + pad_x, rect.top()),
            Pos2::new(rect.right() - pad_x, rect.bottom()),
        );

        if selected {
            ui.painter()
                .rect_filled(pill_rect, 6.0, colors.sidebar_active_bg);
        } else if resp.hovered() {
            ui.painter().rect_filled(pill_rect, 6.0, colors.hover);
        }

        let icon_x = if app.sidebar_collapsed {
            pill_rect.center().x
        } else {
            pill_rect.left() + 12.0
        };
        let align = if app.sidebar_collapsed {
            Align2::CENTER_CENTER
        } else {
            Align2::LEFT_CENTER
        };

        ui.painter().text(
            Pos2::new(icon_x, pill_rect.center().y),
            align,
            egui_phosphor::regular::PLAYLIST,
            FontId::proportional(16.0),
            text_color,
        );

        if !app.sidebar_collapsed {
            ui.painter().text(
                Pos2::new(pill_rect.left() + 36.0, pill_rect.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(14.0),
                if selected { colors.accent } else { colors.text },
            );
        }

        if resp.clicked() {
            app.selected_playlist_id = Some(*id);
            app.nav = NavSection::Favorites;
            let playlist_tracks = app.ctx.library.get_playlist_tracks(*id);
            app.tracks = playlist_tracks;
            app.ctx
                .playback
                .set_play_queue(app.tracks.iter().map(|t| t.id).collect());
            app.play_queue = app.ctx.playback.state().play_queue.clone();
            app.compute_badge_counts();
        }
    }

    if !app.sidebar_collapsed {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            let btn_h = 28.0;

            // Create button
            let (create_rect, create_resp) = ui.allocate_exact_size(egui::Vec2::new(70.0, btn_h), Sense::click());
            let create_bg = if create_resp.hovered() { colors.hover } else { colors.card };
            ui.painter().rect_filled(create_rect, 6.0, create_bg);
            ui.painter().rect_stroke(create_rect, 6.0, egui::Stroke::new(1.0, colors.border));
            ui.painter().text(
                create_rect.center(),
                Align2::CENTER_CENTER,
                "Create",
                FontId::proportional(13.0),
                colors.text,
            );
            if create_resp.clicked() {
                app.show_create_playlist_dialog = true;
            }

            ui.add_space(8.0);

            // Remove button
            let (remove_rect, remove_resp) = ui.allocate_exact_size(egui::Vec2::new(70.0, btn_h), Sense::click());
            let remove_bg = if remove_resp.hovered() { colors.hover } else { colors.card };
            ui.painter().rect_filled(remove_rect, 6.0, remove_bg);
            ui.painter().rect_stroke(remove_rect, 6.0, egui::Stroke::new(1.0, colors.border));
            ui.painter().text(
                remove_rect.center(),
                Align2::CENTER_CENTER,
                "Remove",
                FontId::proportional(13.0),
                colors.text,
            );

            let popup_id = ui.make_persistent_id("remove_playlist_popup");

            egui::Popup::from_toggle_button_response(&remove_resp)
                .id(popup_id)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                .show(|ui: &mut egui::Ui| {
                    ui.set_min_width(160.0);
                    if playlists_snapshot.is_empty() {
                        ui.label(egui::RichText::new("No playlists").color(colors.text_dim));
                    } else {
                        for (pid, pname) in &playlists_snapshot {
                            ui.horizontal(|ui| {
                                if ui.button(egui_phosphor::regular::TRASH).clicked() {
                                    if let Err(e) = app.ctx.library.db().delete_playlist(*pid) {
                                        log::warn!("Failed to delete playlist: {}", e);
                                    }
                                    app.playlists_loaded = false;
                                    if app.selected_playlist_id == Some(*pid) {
                                        app.selected_playlist_id = None;
                                    }
                                    egui::Popup::close_id(ui.ctx(), popup_id);
                                }
                                ui.label(pname);
                            });
                        }
                    }
                });
        });

        ui.add_space(16.0);
    }

    // Push Settings to the bottom
    let remaining = ui.available_height();
    if remaining > 48.0 {
        ui.add_space(remaining - 48.0);
    }

    nav_item(ui, app, NavSection::Settings);
}

fn section_header(ui: &mut Ui, label: &str, colors: &TuneCraftColors, collapsed: bool) {
    if collapsed {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(
            RichText::new(label.to_uppercase())
                .font(FontId::proportional(11.0))
                .color(colors.text_muted)
                .strong(),
        );
    });
    ui.add_space(8.0);
}

fn nav_item(ui: &mut Ui, app: &mut TuneCraftApp, nav: NavSection) {
    let colors = app.colors();
    let is_active = app.nav == nav;
    let badge = app
        .badge_cache
        .get(&format!("{:?}", nav))
        .copied()
        .or_else(|| nav.badge_count(&app.tracks));
    let height = 36.0;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());

    let pad_x = 12.0;
    let pill_rect = Rect::from_min_max(
        Pos2::new(rect.left() + pad_x, rect.top()),
        Pos2::new(rect.right() - pad_x, rect.bottom()),
    );

    // Active background
    if is_active {
        ui.painter()
            .rect_filled(pill_rect, 6.0, colors.sidebar_active_bg);
    } else if response.hovered() {
        ui.painter().rect_filled(pill_rect, 6.0, colors.hover);
    }

    let cy = rect.center().y;
    let is_settings = nav == NavSection::Settings;

    let icon_color = if is_settings {
        colors.text_dim
    } else if is_active {
        colors.accent
    } else {
        colors.text_dim
    };

    let text_color = if is_settings {
        colors.text_dim
    } else if is_active {
        colors.accent
    } else {
        colors.text
    };

    // Icon — 18px
    let icon_x = if app.sidebar_collapsed {
        pill_rect.center().x
    } else {
        pill_rect.left() + 12.0
    };
    let align = if app.sidebar_collapsed {
        Align2::CENTER_CENTER
    } else {
        Align2::LEFT_CENTER
    };

    ui.painter().text(
        Pos2::new(icon_x, cy),
        align,
        nav.icon(),
        FontId::proportional(18.0),
        icon_color,
    );

    if !app.sidebar_collapsed {
        // Label — 14px
        let label_x = pill_rect.left() + 40.0;
        ui.painter().text(
            Pos2::new(label_x, cy),
            Align2::LEFT_CENTER,
            nav.label(),
            FontId::proportional(14.0),
            text_color,
        );

        // Badge
        if let Some(count) = badge {
            let badge_text = count.to_string();
            let badge_font = FontId::proportional(11.0);
            let badge_galley =
                ui.painter()
                    .layout_no_wrap(badge_text.clone(), badge_font.clone(), Color32::WHITE);
            let badge_w = (badge_galley.size().x + 16.0).max(24.0);
            let badge_h = 20.0;
            let badge_x = pill_rect.right() - badge_w - 8.0;
            let badge_y = cy - badge_h / 2.0;
            let badge_rect =
                Rect::from_min_size(Pos2::new(badge_x, badge_y), Vec2::new(badge_w, badge_h));

            // Badge colors
            if is_active {
                // Active non-mood badge: accent bg, white text
                ui.painter().rect_filled(badge_rect, 10.0, colors.accent);
                ui.painter().text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    &badge_text,
                    badge_font,
                    Color32::WHITE,
                );
            } else {
                // Inactive non-mood badge: muted bg, dim text
                let muted_bg = if colors.dark_mode {
                    Color32::from_rgba_premultiplied(
                        (colors.text_dim.r() as u16 * 40 / 100
                            + colors.sidebar.r() as u16 * 60 / 100) as u8,
                        (colors.text_dim.g() as u16 * 40 / 100
                            + colors.sidebar.g() as u16 * 60 / 100) as u8,
                        (colors.text_dim.b() as u16 * 40 / 100
                            + colors.sidebar.b() as u16 * 60 / 100) as u8,
                        255,
                    )
                } else {
                    Color32::from_rgb(0xF3, 0xF2, 0xFB)
                };
                let muted_fg = if colors.dark_mode {
                    Color32::WHITE
                } else {
                    colors.text_dim
                };
                ui.painter().rect_filled(badge_rect, 10.0, muted_bg);
                ui.painter().text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    &badge_text,
                    badge_font,
                    muted_fg,
                );
            }
        }
    }

    // Handle click
    if response.clicked() {
        app.nav = nav;
        app.search_query.clear();
        app.selected_playlist_id = None;
        match nav {
            NavSection::AllTracks => app.refresh_tracks(),
            NavSection::Favorites => {
                app.refresh_favorite_ids();
                app.refresh_tracks();
            },
            NavSection::Folders => {
                // Reset to folder list view
                app.folder_view_path = None;
                app.folder_tracks.clear();
            },
            _ => app.refresh_tracks(),
        }
    }
}
