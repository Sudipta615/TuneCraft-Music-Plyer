//! Sidebar navigation — Library, Playlists, Mood, Settings
//! Matches the reference design: logo + icon list, section headers, badges.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::{
    app::TuneCraftApp,
    theme::{self, TuneCraftColors},
};

/// Which navigation section is active
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavSection {
    AllTracks,
    Albums,
    Artists,
    Favorites,
    RecentlyPlayed,
    MostPlayed,
    MoodDance,
    MoodRomantic,
    MoodSad,
    MoodSufi,
    MoodChill,
    Settings,
}

impl NavSection {
    pub fn label(&self) -> &str {
        match self {
            Self::AllTracks => "All Tracks",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Favorites => "Favorites",
            Self::RecentlyPlayed => "Recently Played",
            Self::MostPlayed => "Most Played",
            Self::MoodDance => "Dance",
            Self::MoodRomantic => "Romantic",
            Self::MoodSad => "Sad",
            Self::MoodSufi => "Sufi",
            Self::MoodChill => "Chill",
            Self::Settings => "Settings",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::AllTracks => "\u{266B}",      // ♫
            Self::Albums => "\u{25A3}",         // ▣ (album grid)
            Self::Artists => "\u{263A}",        // ☺ (person)
            Self::Favorites => "\u{2605}",      // ★
            Self::RecentlyPlayed => "\u{23F0}", // ⏰
            Self::MostPlayed => "\u{2197}",     // ↗ trending
            Self::MoodDance => "\u{266B}",      // ♫
            Self::MoodRomantic => "\u{2665}",   // ♥
            Self::MoodSad => "\u{1F3B5}",       // 🎵 musical note
            Self::MoodSufi => "\u{266A}",       // ♪
            Self::MoodChill => "\u{2744}",      // ❄ snowflake
            Self::Settings => "\u{2699}",       // ⚙
        }
    }

    pub fn mood_matches(&self, mood: &str) -> bool {
        match self {
            Self::MoodDance => mood == "Dance" || mood == "Energetic",
            Self::MoodRomantic => mood == "Romantic",
            Self::MoodSad => mood == "Sad" || mood == "Melancholic",
            Self::MoodSufi => mood == "Sufi",
            Self::MoodChill => mood == "Chill" || mood == "Calm",
            _ => false,
        }
    }

    pub fn mood_tag(&self) -> Option<&str> {
        match self {
            Self::MoodDance => Some("Dance"),
            Self::MoodRomantic => Some("Romantic"),
            Self::MoodSad => Some("Sad"),
            Self::MoodSufi => Some("Sufi"),
            Self::MoodChill => Some("Chill"),
            _ => None,
        }
    }

    pub fn mood_color(&self) -> Option<Color32> {
        self.mood_tag().map(theme::mood_color)
    }

    pub fn badge_count(&self, tracks: &[tc_db::Track]) -> Option<u32> {
        match self {
            Self::AllTracks => Some(tracks.len() as u32),
            Self::MoodDance
            | Self::MoodRomantic
            | Self::MoodSad
            | Self::MoodSufi
            | Self::MoodChill => Some(
                tracks
                    .iter()
                    .filter(|t| t.mood.as_deref().is_some_and(|m| self.mood_matches(m)))
                    .count() as u32,
            ),
            Self::Favorites => None,
            Self::RecentlyPlayed => {
                Some(tracks.iter().filter(|t| t.last_played.is_some()).count() as u32)
            },
            Self::MostPlayed => Some(tracks.iter().filter(|t| t.play_count > 0).count() as u32),
            _ => None,
        }
    }
}

/// Draw the sidebar panel
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    // Sidebar background
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

        ui.add_space(8.0);
        ui.label(
            RichText::new("TuneCraft")
                .font(FontId::proportional(18.0))
                .color(colors.text)
                .strong(),
        );

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

    section_header(ui, "LIBRARY", &colors);
    nav_item(ui, app, NavSection::AllTracks);
    nav_item(ui, app, NavSection::Albums);
    nav_item(ui, app, NavSection::Artists);

    ui.add_space(16.0);

    section_header(ui, "PLAYLISTS", &colors);
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

        ui.painter().text(
            Pos2::new(pill_rect.left() + 12.0, pill_rect.center().y),
            Align2::LEFT_CENTER,
            "\u{266A}",
            FontId::proportional(16.0),
            text_color,
        );

        ui.painter().text(
            Pos2::new(pill_rect.left() + 36.0, pill_rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(14.0),
            if selected { colors.accent } else { colors.text },
        );

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

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("+ New playlist")
                        .font(egui::FontId::proportional(12.0))
                        .color(colors.text_dim),
                )
                .frame(false),
            )
            .clicked()
        {
            app.show_create_playlist_dialog = true;
        }
    });

    ui.add_space(16.0);

    section_header(ui, "MOOD", &colors);
    for nav in &[
        NavSection::MoodDance,
        NavSection::MoodRomantic,
        NavSection::MoodSad,
        NavSection::MoodSufi,
        NavSection::MoodChill,
    ] {
        nav_item(ui, app, *nav);
    }

    // Push Settings to the bottom
    let remaining = ui.available_height();
    if remaining > 48.0 {
        ui.add_space(remaining - 48.0);
    }

    nav_item(ui, app, NavSection::Settings);
}

fn section_header(ui: &mut Ui, label: &str, colors: &TuneCraftColors) {
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
    } else if let Some(c) = nav.mood_color() {
        c
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
    ui.painter().text(
        Pos2::new(pill_rect.left() + 12.0, cy),
        Align2::LEFT_CENTER,
        nav.icon(),
        FontId::proportional(18.0),
        icon_color,
    );

    // Label — 14px
    let label_x = pill_rect.left() + 40.0;
    // For bold text simulation, use label or just paint. We'll put a label to support `strong()`
    ui.put(
        Rect::from_min_max(
            Pos2::new(label_x, rect.top()),
            Pos2::new(pill_rect.right() - 40.0, rect.bottom()),
        ),
        egui::Label::new(
            RichText::new(nav.label())
                .font(FontId::proportional(14.0))
                .color(text_color)
                .strong(),
        ),
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
        if let Some(mc) = nav.mood_color() {
            if colors.dark_mode {
                // Dark: solid mood color bg, white text
                ui.painter().rect_filled(badge_rect, 10.0, mc);
                ui.painter().text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    &badge_text,
                    badge_font,
                    Color32::WHITE,
                );
            } else {
                // Light: pastel tinted bg, mood-colored text
                let mood_bg = crate::theme::mood_color_light_bg(nav.mood_tag().unwrap_or(""));
                let mood_fg = crate::theme::mood_color_light_fg(nav.mood_tag().unwrap_or(""));
                ui.painter().rect_filled(badge_rect, 10.0, mood_bg);
                ui.painter().text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    &badge_text,
                    badge_font,
                    mood_fg,
                );
            }
        } else if is_active {
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
                    (colors.text_dim.r() as u16 * 40 / 100 + colors.sidebar.r() as u16 * 60 / 100)
                        as u8,
                    (colors.text_dim.g() as u16 * 40 / 100 + colors.sidebar.g() as u16 * 60 / 100)
                        as u8,
                    (colors.text_dim.b() as u16 * 40 / 100 + colors.sidebar.b() as u16 * 60 / 100)
                        as u8,
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
            _ => app.refresh_tracks(),
        }
    }
}
