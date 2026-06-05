//! TuneCraft theming — dark/light modes with purple accent (#4231F1)
//!
//! Provides consistent Visuals for the entire UI, matching the reference design:
//! - Dark: #0B1220 bg, #0D1421 sidebar, #4231F1 accent
//! - Light: #FAFAFC bg, #FFFFFF sidebar, #4231F1 accent

use egui::{Color32, Rounding, Stroke, Visuals};

// ── Primary purple palette ──

/// Primary purple accent color
pub const ACCENT: Color32 = Color32::from_rgb(0x42, 0x31, 0xF1);
/// Purple variant — slightly darker, for pressed states
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x35, 0x2D, 0xD9);
/// Deep purple — even darker variant
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x2F, 0x22, 0xA4);
/// Violet tint — lighter, for hover states
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0x8A, 0x5E, 0xED);
/// Light lavender tint — very light, for backgrounds / selections
pub const ACCENT_LAVENDER: Color32 = Color32::from_rgb(0xAF, 0xAE, 0xD6);
/// Indigo accent — another dark variant
pub const ACCENT_INDIGO: Color32 = Color32::from_rgb(0x32, 0x2A, 0xA9);

// ── Dark theme: navy-tinted dark colors matching reference design ──

pub const DARK_BG: Color32 = Color32::from_rgb(0x0B, 0x12, 0x20);
pub const DARK_SIDEBAR: Color32 = Color32::from_rgb(0x0D, 0x14, 0x21);
pub const DARK_SURFACE: Color32 = Color32::from_rgb(0x0D, 0x14, 0x21);
pub const DARK_CARD: Color32 = Color32::from_rgb(0x17, 0x1E, 0x2B);
pub const DARK_TEXT: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xEC);
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(0x9A, 0x9B, 0xA5);
pub const DARK_TEXT_MUTED: Color32 = Color32::from_rgb(0x6C, 0x72, 0x8B);
pub const DARK_BORDER: Color32 = Color32::from_rgb(0x2D, 0x34, 0x4C);
pub const DARK_HOVER: Color32 = Color32::from_rgb(0x17, 0x1E, 0x2B);
pub const DARK_ACTIVE: Color32 = Color32::from_rgb(0x22, 0x21, 0x51);

// ── Light theme matching reference design ──

pub const LIGHT_BG: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xFC);
pub const LIGHT_SIDEBAR: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const LIGHT_SURFACE: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xFC);
pub const LIGHT_CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(0x16, 0x13, 0x18);
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x66);
pub const LIGHT_TEXT_MUTED: Color32 = Color32::from_rgb(0x9A, 0x9C, 0xA8);
pub const LIGHT_BORDER: Color32 = Color32::from_rgb(0xDF, 0xDF, 0xE5);
pub const LIGHT_HOVER: Color32 = Color32::from_rgb(0xF3, 0xF2, 0xFB);
pub const LIGHT_ACTIVE: Color32 = Color32::from_rgb(0xF3, 0xF2, 0xFB);

/// Light sidebar selected tint — distinct from general selected surface
pub const LIGHT_SIDEBAR_ACTIVE: Color32 = Color32::from_rgb(0xD9, 0xD7, 0xE5);

// ── Mood colors (shared across themes) ──

pub const MOOD_ROMANTIC: Color32 = Color32::from_rgb(0x8A, 0x5E, 0xED);
pub const MOOD_DANCE: Color32 = Color32::from_rgb(0xDD, 0x6E, 0x3E);
pub const MOOD_SAD: Color32 = Color32::from_rgb(0x4C, 0x6F, 0xD6);
pub const MOOD_SUFI: Color32 = Color32::from_rgb(0x9C, 0x56, 0x2B);
pub const MOOD_CHILL: Color32 = Color32::from_rgb(0x1F, 0x8A, 0x78);

// Light mode mood pill tints (background colors)
pub const LIGHT_MOOD_ROMANTIC_BG: Color32 = Color32::from_rgb(0xED, 0xE5, 0xFB);
pub const LIGHT_MOOD_DANCE_BG: Color32 = Color32::from_rgb(0xFC, 0xEE, 0xE6);
pub const LIGHT_MOOD_SAD_BG: Color32 = Color32::from_rgb(0xDF, 0xE7, 0xF8);
pub const LIGHT_MOOD_SUFI_BG: Color32 = Color32::from_rgb(0xF2, 0xE6, 0xD9);
pub const LIGHT_MOOD_CHILL_BG: Color32 = Color32::from_rgb(0xD6, 0xF0, 0xEC);

// Light mode mood pill text/border colors
pub const LIGHT_MOOD_ROMANTIC_FG: Color32 = Color32::from_rgb(0x6D, 0x48, 0xCC);
pub const LIGHT_MOOD_DANCE_FG: Color32 = Color32::from_rgb(0xC0, 0x5A, 0x30);
pub const LIGHT_MOOD_SAD_FG: Color32 = Color32::from_rgb(0x3B, 0x5A, 0xB0);
pub const LIGHT_MOOD_SUFI_FG: Color32 = Color32::from_rgb(0x7D, 0x45, 0x22);
pub const LIGHT_MOOD_CHILL_FG: Color32 = Color32::from_rgb(0x16, 0x6E, 0x60);

/// Collection of colors for the current theme
#[derive(Debug, Clone, Copy)]
pub struct TuneCraftColors {
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub card: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_muted: Color32,
    pub border: Color32,
    pub hover: Color32,
    pub active_bg: Color32,
    pub sidebar_active_bg: Color32,
    pub accent: Color32,
    pub accent_light: Color32,
    pub accent_dark: Color32,
    pub player_bar: Color32,
    pub player_bar_border: Color32,
    pub slider_track: Color32,
    pub slider_fill: Color32,
    pub toggle_bg_on: Color32,
    pub toggle_bg_off: Color32,
    pub table_header_bg: Color32,
    pub table_row_even: Color32,
    pub table_row_odd: Color32,
    pub table_row_hover: Color32,
    pub search_bg: Color32,
    pub search_border: Color32,
    pub dark_mode: bool,
}

impl TuneCraftColors {
    pub fn dark() -> Self {
        Self {
            bg: DARK_BG,
            sidebar: DARK_SIDEBAR,
            surface: DARK_SURFACE,
            card: DARK_CARD,
            text: DARK_TEXT,
            text_dim: DARK_TEXT_DIM,
            text_muted: DARK_TEXT_MUTED,
            border: DARK_BORDER,
            hover: DARK_HOVER,
            active_bg: DARK_ACTIVE,
            sidebar_active_bg: DARK_ACTIVE, // #222151
            accent: ACCENT,                 // #4231F1
            accent_light: ACCENT_LIGHT,     // #8A5EED
            accent_dark: ACCENT_DARK,       // #352DD9
            player_bar: DARK_SIDEBAR,       // #0D1421
            player_bar_border: DARK_BORDER, // #2D344C
            slider_track: Color32::from_rgb(0x3A, 0x42, 0x58), // navy-tinted track
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: Color32::from_rgb(0x3A, 0x42, 0x58),
            table_header_bg: DARK_SIDEBAR,                  // #0D1421
            table_row_even: DARK_BG,                        // #0B1220
            table_row_odd: DARK_CARD,                       // #171E2B
            table_row_hover: DARK_CARD,                     // #171E2B
            search_bg: Color32::from_rgb(0x17, 0x1E, 0x2B), // card bg for input
            search_border: DARK_BORDER,                     // #2D344C
            dark_mode: true,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: LIGHT_BG,
            sidebar: LIGHT_SIDEBAR,
            surface: LIGHT_SURFACE,
            card: LIGHT_CARD,
            text: LIGHT_TEXT,
            text_dim: LIGHT_TEXT_DIM,
            text_muted: LIGHT_TEXT_MUTED,
            border: LIGHT_BORDER,
            hover: LIGHT_HOVER,
            active_bg: LIGHT_ACTIVE,                 // #F3F2FB
            sidebar_active_bg: LIGHT_SIDEBAR_ACTIVE, // #D9D7E5
            accent: ACCENT,                          // #4231F1
            accent_light: ACCENT_LIGHT,              // #8A5EED
            accent_dark: ACCENT_DARK,                // #352DD9
            player_bar: LIGHT_CARD,                  // #FFFFFF
            player_bar_border: LIGHT_BORDER,         // #DFDFE5
            slider_track: LIGHT_BORDER,              // #DFDFE5
            slider_fill: ACCENT,                     // #4231F1
            toggle_bg_on: ACCENT,
            toggle_bg_off: Color32::from_rgb(0xDF, 0xDF, 0xE5), // matches border
            table_header_bg: LIGHT_SURFACE,                     // #FAFAFC
            table_row_even: LIGHT_CARD,                         // #FFFFFF
            table_row_odd: LIGHT_BG,                            // #FAFAFC
            table_row_hover: LIGHT_HOVER,                       // #F3F2FB
            search_bg: LIGHT_SURFACE,                           // #FAFAFC
            search_border: Color32::from_rgb(0xDF, 0xDF, 0xE5), // matches border
            dark_mode: false,
        }
    }
}

/// Build the egui Visuals for a dark theme matching the reference design
pub fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.extreme_bg_color = DARK_BG;
    v.panel_fill = DARK_SIDEBAR;
    v.window_fill = DARK_SURFACE;
    v.faint_bg_color = DARK_CARD;
    v.widgets.noninteractive.bg_fill = DARK_SURFACE;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, DARK_TEXT_DIM);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DARK_BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, DARK_TEXT);
    v.widgets.inactive.bg_fill = DARK_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, DARK_BORDER);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, DARK_TEXT);
    v.widgets.hovered.bg_fill = DARK_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_LIGHT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, DARK_TEXT);
    v.widgets.active.bg_fill = ACCENT_DARK;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.selection.bg_fill = ACCENT_DARK;
    v.selection.stroke = Stroke::new(1.0, DARK_TEXT);
    v.override_text_color = Some(DARK_TEXT);
    v.window_rounding = Rounding::same(8.0);
    v.widgets.noninteractive.rounding = Rounding::same(6.0);
    v.widgets.inactive.rounding = Rounding::same(6.0);
    v.widgets.hovered.rounding = Rounding::same(6.0);
    v.widgets.active.rounding = Rounding::same(6.0);
    v.slider_trailing_fill = true;
    v
}

/// Build the egui Visuals for a light theme matching the reference design
pub fn light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.extreme_bg_color = LIGHT_BG;
    v.panel_fill = LIGHT_SIDEBAR;
    v.window_fill = LIGHT_SURFACE;
    v.faint_bg_color = LIGHT_CARD;
    v.widgets.noninteractive.bg_fill = LIGHT_SURFACE;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, LIGHT_TEXT_DIM);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LIGHT_BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    v.widgets.inactive.bg_fill = LIGHT_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LIGHT_BORDER);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    v.widgets.hovered.bg_fill = LIGHT_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_LIGHT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
    v.selection.bg_fill = ACCENT_LAVENDER;
    v.selection.stroke = Stroke::new(1.0, LIGHT_TEXT);
    v.override_text_color = Some(LIGHT_TEXT);
    v.window_rounding = Rounding::same(8.0);
    v.widgets.noninteractive.rounding = Rounding::same(6.0);
    v.widgets.inactive.rounding = Rounding::same(6.0);
    v.widgets.hovered.rounding = Rounding::same(6.0);
    v.widgets.active.rounding = Rounding::same(6.0);
    v.slider_trailing_fill = true;
    v
}

/// Mood tag color lookup
pub fn mood_color(mood: &str) -> Color32 {
    match mood.to_lowercase().as_str() {
        "romantic" => MOOD_ROMANTIC,
        "dance" => MOOD_DANCE,
        "sad" => MOOD_SAD,
        "sufi" => MOOD_SUFI,
        "chill" => MOOD_CHILL,
        _ => Color32::from_rgb(0x6C, 0x72, 0x8B),
    }
}

/// Mood tag background tint color for light mode pills
pub fn mood_color_light_bg(mood: &str) -> Color32 {
    match mood.to_lowercase().as_str() {
        "romantic" => LIGHT_MOOD_ROMANTIC_BG,
        "dance" => LIGHT_MOOD_DANCE_BG,
        "sad" => LIGHT_MOOD_SAD_BG,
        "sufi" => LIGHT_MOOD_SUFI_BG,
        "chill" => LIGHT_MOOD_CHILL_BG,
        _ => Color32::from_rgb(0xF3, 0xF2, 0xFB),
    }
}

/// Mood tag foreground/text color for light mode pills
pub fn mood_color_light_fg(mood: &str) -> Color32 {
    match mood.to_lowercase().as_str() {
        "romantic" => LIGHT_MOOD_ROMANTIC_FG,
        "dance" => LIGHT_MOOD_DANCE_FG,
        "sad" => LIGHT_MOOD_SAD_FG,
        "sufi" => LIGHT_MOOD_SUFI_FG,
        "chill" => LIGHT_MOOD_CHILL_FG,
        _ => Color32::from_rgb(0x5A, 0x5A, 0x66),
    }
}
