//! TuneCraft theming — dark/light/custom themes with proper chromatic palettes
//! and glassmorphism surface system.
//!
//! Each custom theme has its own fully saturated chromatic base rather than
//! just accent-swapped near-black. Glassmorphism is achieved by painting a
//! vivid gradient wallpaper behind all panels, then using semi-transparent
//! surface fills so the wallpaper bleeds through.

use egui::{Color32, CornerRadius, Stroke, Visuals};

// ── Primary cyan palette (Light / Dark defaults) ──

pub const ACCENT: Color32 = Color32::from_rgb(0x35, 0xC8, 0xE1);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x2A, 0xA0, 0xB4);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x20, 0x78, 0x87);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0x5E, 0xD3, 0xE7);
pub const ACCENT_LAVENDER: Color32 = Color32::from_rgb(0xE0, 0xF7, 0xFA);
pub const ACCENT_INDIGO: Color32 = Color32::from_rgb(0x1A, 0x64, 0x70);

// ── Dark theme base colors ──

pub const DARK_BG: Color32 = Color32::from_rgba_premultiplied(8, 14, 25, 240);
pub const DARK_SIDEBAR: Color32 = Color32::from_rgba_premultiplied(12, 18, 30, 240);
pub const DARK_SURFACE: Color32 = Color32::from_rgba_premultiplied(8, 14, 25, 240);
pub const DARK_CARD: Color32 = Color32::from_rgba_premultiplied(12, 18, 30, 240);
pub const DARK_TEXT: Color32 = Color32::from_rgb(0xE6, 0xE7, 0xE7);
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(0xBA, 0xBF, 0xC8);
pub const DARK_TEXT_MUTED: Color32 = Color32::from_rgb(0xBA, 0xBF, 0xC8);
pub const DARK_BORDER: Color32 = Color32::from_rgb(0x1C, 0x23, 0x33);
pub const DARK_HOVER: Color32 = Color32::from_rgb(0x14, 0x1B, 0x2B);
pub const DARK_ACTIVE: Color32 = Color32::from_rgb(0x14, 0x1B, 0x2B);

// ── Light theme base colors ──

pub const LIGHT_BG: Color32 = Color32::from_rgba_premultiplied(224, 225, 227, 245);
pub const LIGHT_SIDEBAR: Color32 = Color32::from_rgba_premultiplied(232, 232, 233, 245);
pub const LIGHT_SURFACE: Color32 = Color32::from_rgba_premultiplied(224, 225, 227, 245);
pub const LIGHT_CARD: Color32 = Color32::from_rgba_premultiplied(235, 235, 235, 245);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(0x11, 0x18, 0x27);
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
pub const LIGHT_TEXT_MUTED: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);
pub const LIGHT_BORDER: Color32 = Color32::from_rgb(0xE5, 0xE7, 0xEB);
pub const LIGHT_HOVER: Color32 = Color32::from_rgb(0xF9, 0xFA, 0xFB);
pub const LIGHT_ACTIVE: Color32 = Color32::from_rgb(0xEE, 0xF2, 0xFF);
pub const LIGHT_SIDEBAR_ACTIVE: Color32 = Color32::from_rgb(0xE0, 0xF2, 0xFE);

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
    // ── Glass helpers ─────────────────────────────────────────────────────────

    /// Mix `base` toward white by `t` (0 = base, 1 = white).
    fn lighten(base: Color32, t: f32) -> Color32 {
        let lerp = |a: u8, b: u8, t: f32| -> u8 { (a as f32 + (b as f32 - a as f32) * t) as u8 };
        Color32::from_rgb(
            lerp(base.r(), 255, t),
            lerp(base.g(), 255, t),
            lerp(base.b(), 255, t),
        )
    }

    /// Mix `base` toward black by `t`.
    fn darken(base: Color32, t: f32) -> Color32 {
        let lerp = |a: u8, t: f32| -> u8 { (a as f32 * (1.0 - t)) as u8 };
        Color32::from_rgb(lerp(base.r(), t), lerp(base.g(), t), lerp(base.b(), t))
    }

    // ── Core constructor for every chromatic theme ────────────────────────────

    /// Build a fully chromatic glassmorphism theme.
    ///
    /// `hue_dark`  — the deep/saturated base hue for bg surfaces  
    /// `hue_mid`   — a lighter mid-tone of the same hue  
    /// `hue_light` — the lightest/most-washed variant  
    /// `accent`    — the primary interactive color  
    fn chromatic(
        hue_dark: Color32,   // deep bg
        hue_mid: Color32,    // card / sidebar
        _hue_light: Color32, // surface highlight
        accent: Color32,
    ) -> Self {
        let accent_light = Self::lighten(accent, 0.25);
        let accent_dark = Self::darken(accent, 0.30);

        let bg = hue_dark;
        let card = hue_mid;
        let sidebar = hue_mid;
        let hover = Self::lighten(hue_dark, 0.12);
        let border = Self::lighten(hue_mid, 0.30);
        let player = hue_dark;

        Self {
            bg,
            sidebar,
            surface: bg,
            card,
            text: Color32::from_rgb(0xF0, 0xF2, 0xF4),
            text_dim: Color32::from_rgba_unmultiplied(0xD0, 0xD5, 0xDF, 210),
            text_muted: Color32::from_rgba_unmultiplied(0xA0, 0xA8, 0xB8, 180),
            border,
            hover,
            active_bg: hover,
            sidebar_active_bg: Self::lighten(hue_mid, 0.18),
            accent,
            accent_light,
            accent_dark,
            player_bar: player,
            player_bar_border: border,
            slider_track: border,
            slider_fill: accent,
            toggle_bg_on: accent,
            toggle_bg_off: border,
            table_header_bg: bg,
            table_row_even: bg,
            table_row_odd: Self::darken(hue_dark, 0.10),
            table_row_hover: hover,
            search_bg: card,
            search_border: border,
            dark_mode: true,
        }
    }

    /// Build a fully chromatic glassmorphism light theme.
    fn chromatic_light(
        hue_light: Color32, // base light bg
        hue_card: Color32,  // slightly lighter card
        accent: Color32,
    ) -> Self {
        let accent_light = Self::lighten(accent, 0.25);
        let accent_dark = Self::darken(accent, 0.30);

        let bg = hue_light;
        let card = hue_card;
        let sidebar = hue_card;
        let hover = Self::darken(hue_light, 0.05);
        let border = Self::darken(hue_card, 0.15);

        Self {
            bg,
            sidebar,
            surface: bg,
            card,
            text: Color32::from_rgb(0x11, 0x18, 0x27), // LIGHT_TEXT
            text_dim: Color32::from_rgb(0x4B, 0x55, 0x63),
            text_muted: Color32::from_rgb(0x6B, 0x72, 0x80),
            border,
            hover,
            active_bg: Self::lighten(accent, 0.85),
            sidebar_active_bg: Self::lighten(accent, 0.85),
            accent,
            accent_light,
            accent_dark,
            player_bar: bg,
            player_bar_border: border,
            slider_track: border,
            slider_fill: accent,
            toggle_bg_on: accent,
            toggle_bg_off: border,
            table_header_bg: bg,
            table_row_even: bg,
            table_row_odd: Self::darken(hue_light, 0.02),
            table_row_hover: hover,
            search_bg: card,
            search_border: border,
            dark_mode: false,
        }
    }

    // ── Public theme constructors ─────────────────────────────────────────────

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
            player_bar: DARK_CARD,
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
            player_bar: LIGHT_BG,
            player_bar_border: LIGHT_BORDER,
            slider_track: LIGHT_BORDER,
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: LIGHT_BORDER,
            table_header_bg: LIGHT_BG,
            table_row_even: LIGHT_BG,
            table_row_odd: LIGHT_BG,
            table_row_hover: LIGHT_HOVER,
            search_bg: LIGHT_CARD,
            search_border: LIGHT_BORDER,
            dark_mode: false,
        }
    }

    // ── Chromatic themes — each has a fully distinct hue base ─────────────────

    /// Deep ocean blues with electric cyan accent
    pub fn ocean() -> Self {
        Self::chromatic(
            Color32::from_rgb(0x03, 0x10, 0x2E), // deep navy
            Color32::from_rgb(0x07, 0x1A, 0x40), // mid navy
            Color32::from_rgb(0x0D, 0x26, 0x52), // lighter navy
            Color32::from_rgb(0x00, 0xE5, 0xFF), // electric cyan
        )
    }

    /// Light emerald greens with forest accent
    pub fn forest() -> Self {
        Self::chromatic_light(
            Color32::from_rgb(0xEB, 0xF4, 0xEE), // pale green bg
            Color32::from_rgb(0xF2, 0xF9, 0xF5), // lighter green card
            Color32::from_rgb(0x10, 0xB9, 0x81), // emerald accent
        )
    }

    /// Soft warm amber/orange sunset
    pub fn sunset() -> Self {
        Self::chromatic_light(
            Color32::from_rgb(0xFD, 0xF3, 0xEA), // pale orange bg
            Color32::from_rgb(0xFF, 0xFA, 0xF5), // lighter orange card
            Color32::from_rgb(0xF9, 0x73, 0x16), // vivid orange accent
        )
    }

    /// Deep berry purples with vivid magenta accent
    pub fn berry() -> Self {
        Self::chromatic(
            Color32::from_rgb(0x18, 0x04, 0x22), // deep plum
            Color32::from_rgb(0x24, 0x07, 0x32), // mid plum
            Color32::from_rgb(0x32, 0x0B, 0x44), // lighter plum
            Color32::from_rgb(0xE8, 0x43, 0x93), // hot pink
        )
    }

    /// Pure black with sapphire blue accent
    pub fn midnight() -> Self {
        Self::chromatic(
            Color32::from_rgb(0x02, 0x02, 0x0E), // near black blue
            Color32::from_rgb(0x05, 0x05, 0x18), // deep blue-black
            Color32::from_rgb(0x08, 0x08, 0x24), // dark blue
            Color32::from_rgb(0x60, 0xA5, 0xFA), // bright blue
        )
    }

    /// Soft pinks with rose accent
    pub fn rose() -> Self {
        Self::chromatic_light(
            Color32::from_rgb(0xFC, 0xED, 0xF0), // pale pink bg
            Color32::from_rgb(0xFF, 0xF5, 0xF7), // lighter pink card
            Color32::from_rgb(0xF4, 0x3F, 0x5E), // rose red accent
        )
    }

    /// Warm espresso browns with amber accent
    pub fn coffee() -> Self {
        Self::chromatic(
            Color32::from_rgb(0x18, 0x0E, 0x05), // espresso
            Color32::from_rgb(0x24, 0x16, 0x08), // dark caramel
            Color32::from_rgb(0x32, 0x20, 0x0C), // caramel
            Color32::from_rgb(0xFB, 0xBF, 0x24), // golden amber
        )
    }

    /// Fresh crisp light mint
    pub fn mint() -> Self {
        Self::chromatic_light(
            Color32::from_rgb(0xEC, 0xF8, 0xF6), // pale mint bg
            Color32::from_rgb(0xF3, 0xFB, 0xFA), // lighter mint card
            Color32::from_rgb(0x0D, 0x94, 0x88), // deep teal accent
        )
    }
}

// ── egui Visuals builders ─────────────────────────────────────────────────────

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

pub fn custom_visuals(colors: &TuneCraftColors) -> Visuals {
    let mut v = if colors.dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.extreme_bg_color = colors.bg;
    v.panel_fill = colors.bg;
    v.window_fill = colors.card;
    v.window_stroke = egui::Stroke::new(1.0, colors.border);
    v.faint_bg_color = colors.card;
    v.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors.text_dim);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors.border);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors.text);
    v.widgets.inactive.bg_fill = colors.card;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, colors.border);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, colors.text);
    v.widgets.hovered.bg_fill = colors.hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, colors.accent_light);
    v.widgets.active.fg_stroke = egui::Stroke::new(
        1.0,
        if colors.dark_mode {
            colors.text
        } else {
            Color32::WHITE
        },
    );
    v.widgets.active.bg_fill = if colors.dark_mode {
        colors.accent_dark
    } else {
        colors.accent
    };
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, colors.accent);
    v.selection.bg_fill = if colors.dark_mode {
        colors.accent_dark
    } else {
        colors.active_bg
    };
    v.selection.stroke = egui::Stroke::new(1.0, colors.text);
    v.override_text_color = Some(colors.text);
    v.window_corner_radius = egui::CornerRadius::same(12);
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
    v.widgets.active.corner_radius = egui::CornerRadius::same(8);
    v.slider_trailing_fill = true;
    v
}

pub fn custom_dark_visuals(colors: &TuneCraftColors) -> Visuals {
    custom_visuals(colors)
}
