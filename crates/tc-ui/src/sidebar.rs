//! Sidebar navigation — Library, Playlists, Mood, Settings
//! Matches the reference design: logo + icon list, section headers, badges.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::app::TuneCraftApp;
use crate::theme::{self, TuneCraftColors};

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
            Self::AllTracks => "\u{266B}",   // ♫
            Self::Albums => "\u{25A3}",       // ▣ (album grid)
            Self::Artists => "\u{263A}",      // ☺ (person)
            Self::Favorites => "\u{2605}",    // ★
            Self::RecentlyPlayed => "\u{23F0}", // ⏰
            Self::MostPlayed => "\u{2197}",   // ↗ trending
            Self::MoodDance => "\u{266B}",    // ♫
            Self::MoodRomantic => "\u{2665}", // ♥
            Self::MoodSad => "\u{1F514}",     // 🔔 bell icon
            Self::MoodSufi => "\u{266A}",     // ♪
            Self::MoodChill => "\u{2744}",    // ❄ snowflake
            Self::Settings => "\u{2699}",     // ⚙
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
            Self::MoodDance | Self::MoodRomantic | Self::MoodSad
            | Self::MoodSufi | Self::MoodChill => {
                Some(tracks.iter().filter(|t| {
                    t.mood.as_deref().map_or(false, |m| self.mood_matches(m))
                }).count() as u32)
            }
            Self::Favorites => None,
            Self::RecentlyPlayed => Some(tracks.iter().filter(|t| t.last_played.is_some()).count() as u32),
            Self::MostPlayed => Some(tracks.iter().filter(|t| t.play_count > 0).count() as u32),
            _ => None,
        }
    }
}

/// Draw the sidebar panel
pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    // Sidebar background — #1E1E2E in dark, #FFFFFF in light
    let sidebar_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(sidebar_rect, 0.0, colors.sidebar);

    // Right border separator line
    ui.painter().line_segment(
        [sidebar_rect.right_top(), sidebar_rect.right_bottom()],
        egui::Stroke::new(1.0, colors.border),
    );

    ui.add_space(12.0);

    // Logo row: waveform bars icon + "TuneCraft" text
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        // Draw waveform-like bars icon (matching reference design)
        let icon_size = 22.0;
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(icon_size, icon_size), Sense::hover());
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
    });

    ui.add_space(24.0);

    section_header(ui, "LIBRARY", &colors);
    nav_item(ui, app, NavSection::AllTracks);
    nav_item(ui, app, NavSection::Albums);
    nav_item(ui, app, NavSection::Artists);

    ui.add_space(12.0);

    section_header(ui, "PLAYLISTS", &colors);
    nav_item(ui, app, NavSection::Favorites);
    nav_item(ui, app, NavSection::RecentlyPlayed);
    nav_item(ui, app, NavSection::MostPlayed);

    // User-created playlists
    if !app.playlists_loaded {
        app.reload_playlists();
        app.playlists_loaded = true;
    }
    let playlists_snapshot: Vec<(i64, String)> = app.playlists
        .iter()
        .filter(|p| !p.is_smart)
        .map(|p| (p.id, p.name.clone()))
        .collect();
    for (id, name) in &playlists_snapshot {
        let selected = app.selected_playlist_id == Some(*id);
        let label = egui::RichText::new(format!("  \u{266A}  {}", name))
            .font(egui::FontId::proportional(14.0))
            .color(if selected { colors.accent } else { colors.text_dim });
        if ui.add(egui::Label::new(label).sense(Sense::click())).clicked() {
            app.selected_playlist_id = Some(*id);
            app.nav = NavSection::Favorites;
            let playlist_tracks = app.ctx.library.get_playlist_tracks(*id);
            app.tracks = playlist_tracks;
            app.ctx.playback.set_play_queue(app.tracks.iter().map(|t| t.id).collect());
            app.play_queue = app.ctx.playback.state().play_queue.clone();
            app.compute_badge_counts();
        }
    }

    ui.add_space(4.0);
    if ui.add(
        egui::Button::new(
            egui::RichText::new("+ New playlist")
                .font(egui::FontId::proportional(11.0))
                .color(colors.text_dim),
        ).frame(false),
    ).clicked() {
        app.show_create_playlist_dialog = true;
    }

    ui.add_space(12.0);

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

    // Separator
    let (sep_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 1.0),
        Sense::hover(),
    );
    ui.painter().line_segment(
        [sep_rect.left_top(), sep_rect.right_top()],
        egui::Stroke::new(1.0, colors.border),
    );

    nav_item(ui, app, NavSection::Settings);
}

fn section_header(ui: &mut Ui, label: &str, colors: &TuneCraftColors) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(
            RichText::new(label)
                .font(FontId::proportional(12.0))
                .color(colors.text_muted)
                .strong(),
        );
    });
    ui.add_space(4.0);
}

fn nav_item(ui: &mut Ui, app: &mut TuneCraftApp, nav: NavSection) {
    let colors = app.colors();
    let is_active = app.nav == nav;
    let badge = app.badge_cache.get(&format!("{:?}", nav)).copied().or_else(|| nav.badge_count(&app.tracks));
    let height = 40.0;

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::click(),
    );

    // Active background — navy purple in dark, sidebar selected tint in light
    if is_active {
        ui.painter().rect_filled(rect, 6.0, colors.sidebar_active_bg);
        // Left accent bar — full height, rounded
        let bar = Rect::from_min_size(rect.left_top(), Vec2::new(3.0, height));
        ui.painter().rect_filled(bar, 1.5, colors.accent);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, colors.hover);
    }

    let cy = rect.center().y;
    let icon_color = if is_active { colors.accent } else { colors.text_dim };
    let text_color = if is_active { colors.accent } else { colors.text };

    // Icon — 18px
    ui.painter().text(
        Pos2::new(rect.left() + 24.0, cy),
        Align2::LEFT_CENTER,
        nav.icon(),
        FontId::proportional(18.0),
        icon_color,
    );

    // Label — 14px
    ui.painter().text(
        Pos2::new(rect.left() + 48.0, cy),
        Align2::LEFT_CENTER,
        nav.label(),
        FontId::proportional(14.0),
        text_color,
    );

    // Badge
    if let Some(count) = badge {
        let badge_text = count.to_string();
        let badge_font = FontId::proportional(10.0);
        let badge_galley = ui.painter().layout_no_wrap(badge_text.clone(), badge_font.clone(), Color32::WHITE);
        let badge_w = (badge_galley.size().x + 12.0).max(22.0);
        let badge_h = 20.0;
        let badge_x = rect.right() - badge_w - 16.0;
        let badge_y = cy - badge_h / 2.0;
        let badge_rect = Rect::from_min_size(Pos2::new(badge_x, badge_y), Vec2::new(badge_w, badge_h));

        // Badge colors: match reference design
        // Dark mode: mood badges use mood color bg with white text; non-mood use accent or muted bg
        // Light mode: mood badges use pastel bg with colored text; non-mood use accent or muted bg
        if let Some(mc) = nav.mood_color() {
            if colors.dark_mode {
                // Dark: solid mood color bg, white text
                ui.painter().rect_filled(badge_rect, 8.0, mc);
                ui.painter().text(badge_rect.center(), Align2::CENTER_CENTER, &badge_text, badge_font, Color32::WHITE);
            } else {
                // Light: pastel tinted bg, mood-colored text + border
                let mood_bg = crate::theme::mood_color_light_bg(nav.mood_tag().unwrap_or(""));
                let mood_fg = crate::theme::mood_color_light_fg(nav.mood_tag().unwrap_or(""));
                ui.painter().rect_filled(badge_rect, 8.0, mood_bg);
                ui.painter().rect_stroke(badge_rect, 8.0, egui::Stroke::new(1.0, mood_fg));
                ui.painter().text(badge_rect.center(), Align2::CENTER_CENTER, &badge_text, badge_font, mood_fg);
            }
        } else if is_active {
            // Active non-mood badge: accent bg, white text
            ui.painter().rect_filled(badge_rect, 8.0, colors.accent);
            ui.painter().text(badge_rect.center(), Align2::CENTER_CENTER, &badge_text, badge_font, Color32::WHITE);
        } else {
            // Inactive non-mood badge: muted bg, dim text
            let muted_bg = if colors.dark_mode {
                Color32::from_rgba_premultiplied(
                    (colors.text_dim.r() as u16 * 40 / 100 + colors.sidebar.r() as u16 * 60 / 100) as u8,
                    (colors.text_dim.g() as u16 * 40 / 100 + colors.sidebar.g() as u16 * 60 / 100) as u8,
                    (colors.text_dim.b() as u16 * 40 / 100 + colors.sidebar.b() as u16 * 60 / 100) as u8,
                    255,
                )
            } else {
                Color32::from_rgb(0xF3, 0xF2, 0xFB)
            };
            let muted_fg = if colors.dark_mode { Color32::WHITE } else { colors.text_dim };
            ui.painter().rect_filled(badge_rect, 8.0, muted_bg);
            ui.painter().text(badge_rect.center(), Align2::CENTER_CENTER, &badge_text, badge_font, muted_fg);
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
            }
            _ => app.refresh_tracks(),
        }
    }
}
