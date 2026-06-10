//! TuneCraft theming — dark/light modes with purple accent
//!
//! Provides consistent Visuals for the entire UI, matching the reference design.

use egui::{Color32, CornerRadius, Stroke, Visuals};

// ── Primary cyan palette ──

/// Primary accent color (button color)
pub const ACCENT: Color32 = Color32::from_rgb(0x35, 0xC8, 0xE1); // #35c8e1
/// Variant — slightly darker, for pressed states
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x2A, 0xA0, 0xB4);
/// Deep accent — even darker variant
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x20, 0x78, 0x87);
/// Light tint — for hover states
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0x5E, 0xD3, 0xE7);
/// Very light tint — for backgrounds / selections
pub const ACCENT_LAVENDER: Color32 = Color32::from_rgb(0xE0, 0xF7, 0xFA);
/// Dark accent
pub const ACCENT_INDIGO: Color32 = Color32::from_rgb(0x1A, 0x64, 0x70);

// ── Dark theme: updated matching custom design ──

pub const DARK_BG: Color32 = Color32::from_rgb(0x0A, 0x11, 0x1E); // #0a111e
pub const DARK_SIDEBAR: Color32 = Color32::from_rgb(0x0F, 0x15, 0x23); // #0f1523
pub const DARK_SURFACE: Color32 = Color32::from_rgb(0x0A, 0x11, 0x1E); // #0a111e
pub const DARK_CARD: Color32 = Color32::from_rgb(0x0F, 0x15, 0x23); // #0f1523
pub const DARK_TEXT: Color32 = Color32::from_rgb(0xE6, 0xE7, 0xE7); // #e6e7e7
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(0xBA, 0xBF, 0xC8); // #babfc8
pub const DARK_TEXT_MUTED: Color32 = Color32::from_rgb(0xBA, 0xBF, 0xC8); // #babfc8
pub const DARK_BORDER: Color32 = Color32::from_rgb(0x1C, 0x23, 0x33);
pub const DARK_HOVER: Color32 = Color32::from_rgb(0x14, 0x1B, 0x2B); // #141b2b
pub const DARK_ACTIVE: Color32 = Color32::from_rgb(0x14, 0x1B, 0x2B); // #141b2b

// ── Light theme matching reference design ──

pub const LIGHT_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const LIGHT_SIDEBAR: Color32 = Color32::from_rgb(0xF8, 0xF7, 0xFF);
pub const LIGHT_SURFACE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const LIGHT_CARD: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xFC);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x2A);
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(0x88, 0x88, 0x99);
pub const LIGHT_TEXT_MUTED: Color32 = Color32::from_rgb(0x88, 0x88, 0x99);
pub const LIGHT_BORDER: Color32 = Color32::from_rgb(0xE5, 0xE4, 0xF0);
pub const LIGHT_HOVER: Color32 = Color32::from_rgb(0xF3, 0xF2, 0xFB);
pub const LIGHT_ACTIVE: Color32 = Color32::from_rgb(0xEE, 0xED, 0xFD);

/// Light sidebar selected tint — distinct from general selected surface
pub const LIGHT_SIDEBAR_ACTIVE: Color32 = Color32::from_rgb(0xE0, 0xF7, 0xFA);

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
            surface: DARK_BG,
            card: DARK_CARD,
            text: DARK_TEXT,
            text_dim: DARK_TEXT_DIM,
            text_muted: DARK_TEXT_MUTED,
            border: DARK_BORDER,
            hover: DARK_HOVER,
            active_bg: DARK_ACTIVE,
            sidebar_active_bg: DARK_ACTIVE,
            accent: ACCENT,
            accent_light: ACCENT_LIGHT,
            accent_dark: ACCENT_DARK,
            player_bar: DARK_CARD, // matching deep navy
            player_bar_border: DARK_BORDER,
            slider_track: DARK_BORDER,
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: DARK_BORDER,
            table_header_bg: DARK_BG,
            table_row_even: DARK_BG,
            table_row_odd: DARK_BG,
            table_row_hover: DARK_HOVER,
            search_bg: DARK_CARD,
            search_border: DARK_BORDER,
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
            active_bg: LIGHT_ACTIVE,
            sidebar_active_bg: LIGHT_SIDEBAR_ACTIVE,
            accent: ACCENT,
            accent_light: ACCENT_LIGHT,
            accent_dark: ACCENT_DARK,
            player_bar: LIGHT_BG, // white
            player_bar_border: LIGHT_BORDER,
            slider_track: LIGHT_BORDER,
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: LIGHT_BORDER,
            table_header_bg: LIGHT_BG,
            table_row_even: LIGHT_BG,
            table_row_odd: LIGHT_BG,
            table_row_hover: LIGHT_HOVER,
            search_bg: LIGHT_CARD, // light gray
            search_border: LIGHT_BORDER,
            dark_mode: false,
        }
    }
}

/// Build the egui Visuals for a dark theme matching the reference design
pub fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.extreme_bg_color = DARK_BG;
    v.panel_fill = DARK_BG;
    v.window_fill = DARK_BG;
    v.faint_bg_color = DARK_CARD;
    v.widgets.noninteractive.bg_fill = DARK_BG;
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
    v.window_corner_radius = CornerRadius::same(12);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.slider_trailing_fill = true;
    v
}

/// Build the egui Visuals for a light theme matching the reference design
pub fn light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.extreme_bg_color = LIGHT_BG;
    v.panel_fill = LIGHT_BG;
    v.window_fill = LIGHT_BG;
    v.faint_bg_color = LIGHT_CARD;
    v.widgets.noninteractive.bg_fill = LIGHT_BG;
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
    v.window_corner_radius = CornerRadius::same(12);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.slider_trailing_fill = true;
    v
}
