//! TuneCraft theming — palette definitions for all 11 themes.
//!
//! In the egui version this module held `egui::Color32` and `egui::Visuals`
//! builders. In the Slint version, the actual color values are duplicated
//! in `ui/theme/colors.slint` (because Slint needs them at compile time
//! for the `ThemeColorsProvider` global).
//!
//! This Rust file is kept for backwards-compatibility with code that
//! references `TuneCraftColors` (e.g. for image processing decisions
//! based on `dark_mode`). It is intentionally egui-free.

/// Plain RGB color (no egui dependency).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Collection of colors for the current theme.
///
/// This struct is preserved from the egui version for code that needs
/// runtime access to the current palette (e.g. for procedural image
/// generation, cover-art placeholder rendering).
#[derive(Debug, Clone, Copy)]
pub struct TuneCraftColors {
    pub bg: RgbColor,
    pub sidebar: RgbColor,
    pub surface: RgbColor,
    pub card: RgbColor,
    pub text: RgbColor,
    pub text_dim: RgbColor,
    pub text_muted: RgbColor,
    pub border: RgbColor,
    pub hover: RgbColor,
    pub active_bg: RgbColor,
    pub sidebar_active_bg: RgbColor,
    pub accent: RgbColor,
    pub accent_light: RgbColor,
    pub accent_dark: RgbColor,
    pub player_bar: RgbColor,
    pub player_bar_border: RgbColor,
    pub slider_track: RgbColor,
    pub slider_fill: RgbColor,
    pub toggle_bg_on: RgbColor,
    pub toggle_bg_off: RgbColor,
    pub table_header_bg: RgbColor,
    pub table_row_even: RgbColor,
    pub table_row_odd: RgbColor,
    pub table_row_hover: RgbColor,
    pub search_bg: RgbColor,
    pub search_border: RgbColor,
    pub dark_mode: bool,
}

// Accent colors (cyan family).
pub const ACCENT: RgbColor = RgbColor::rgb(0x35, 0xC8, 0xE1);
pub const ACCENT_DARK: RgbColor = RgbColor::rgb(0x2A, 0xA0, 0xB4);
pub const ACCENT_LIGHT: RgbColor = RgbColor::rgb(0x5E, 0xD3, 0xE7);

impl TuneCraftColors {
    pub fn dark() -> Self {
        Self {
            bg: RgbColor::rgba(8, 14, 25, 240),
            sidebar: RgbColor::rgba(12, 18, 30, 240),
            surface: RgbColor::rgba(8, 14, 25, 240),
            card: RgbColor::rgba(12, 18, 30, 240),
            text: RgbColor::rgb(0xE6, 0xE7, 0xE7),
            text_dim: RgbColor::rgb(0xBA, 0xBF, 0xC8),
            text_muted: RgbColor::rgb(0xBA, 0xBF, 0xC8),
            border: RgbColor::rgb(0x1C, 0x23, 0x33),
            hover: RgbColor::rgb(0x14, 0x1B, 0x2B),
            active_bg: RgbColor::rgb(0x14, 0x1B, 0x2B),
            sidebar_active_bg: RgbColor::rgb(0x14, 0x1B, 0x2B),
            accent: ACCENT,
            accent_light: ACCENT_LIGHT,
            accent_dark: ACCENT_DARK,
            player_bar: RgbColor::rgba(12, 18, 30, 240),
            player_bar_border: RgbColor::rgb(0x1C, 0x23, 0x33),
            slider_track: RgbColor::rgb(0x1C, 0x23, 0x33),
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: RgbColor::rgb(0x1C, 0x23, 0x33),
            table_header_bg: RgbColor::rgba(8, 14, 25, 240),
            table_row_even: RgbColor::rgba(8, 14, 25, 240),
            table_row_odd: RgbColor::rgba(8, 14, 25, 240),
            table_row_hover: RgbColor::rgb(0x14, 0x1B, 0x2B),
            search_bg: RgbColor::rgba(12, 18, 30, 240),
            search_border: RgbColor::rgb(0x1C, 0x23, 0x33),
            dark_mode: true,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: RgbColor::rgba(224, 225, 227, 245),
            sidebar: RgbColor::rgba(232, 232, 233, 245),
            surface: RgbColor::rgba(224, 225, 227, 245),
            card: RgbColor::rgba(235, 235, 235, 245),
            text: RgbColor::rgb(0x11, 0x18, 0x27),
            text_dim: RgbColor::rgb(0x6B, 0x72, 0x80),
            text_muted: RgbColor::rgb(0x9C, 0xA3, 0xAF),
            border: RgbColor::rgb(0xE5, 0xE7, 0xEB),
            hover: RgbColor::rgb(0xF9, 0xFA, 0xFB),
            active_bg: RgbColor::rgb(0xEE, 0xF2, 0xFF),
            sidebar_active_bg: RgbColor::rgb(0xE0, 0xF2, 0xFE),
            accent: ACCENT,
            accent_light: ACCENT_LIGHT,
            accent_dark: ACCENT_DARK,
            player_bar: RgbColor::rgba(224, 225, 227, 245),
            player_bar_border: RgbColor::rgb(0xE5, 0xE7, 0xEB),
            slider_track: RgbColor::rgb(0xE5, 0xE7, 0xEB),
            slider_fill: ACCENT,
            toggle_bg_on: ACCENT,
            toggle_bg_off: RgbColor::rgb(0xE5, 0xE7, 0xEB),
            table_header_bg: RgbColor::rgba(224, 225, 227, 245),
            table_row_even: RgbColor::rgba(224, 225, 227, 245),
            table_row_odd: RgbColor::rgba(224, 225, 227, 245),
            table_row_hover: RgbColor::rgb(0xF9, 0xFA, 0xFB),
            search_bg: RgbColor::rgba(235, 235, 235, 245),
            search_border: RgbColor::rgb(0xE5, 0xE7, 0xEB),
            dark_mode: false,
        }
    }

    pub fn ocean() -> Self {
        Self {
            bg: RgbColor::rgb(0x03, 0x10, 0x2E),
            sidebar: RgbColor::rgb(0x07, 0x1A, 0x40),
            surface: RgbColor::rgb(0x03, 0x10, 0x2E),
            card: RgbColor::rgb(0x07, 0x1A, 0x40),
            text: RgbColor::rgb(0xF0, 0xF2, 0xF4),
            text_dim: RgbColor::rgb(0xD0, 0xD5, 0xDF),
            text_muted: RgbColor::rgb(0xA0, 0xA8, 0xB8),
            border: RgbColor::rgb(0x1A, 0x32, 0x61),
            hover: RgbColor::rgb(0x0B, 0x1F, 0x47),
            active_bg: RgbColor::rgb(0x0B, 0x1F, 0x47),
            sidebar_active_bg: RgbColor::rgb(0x0D, 0x26, 0x52),
            accent: RgbColor::rgb(0x00, 0xE5, 0xFF),
            accent_light: RgbColor::rgb(0x5E, 0xEC, 0xFF),
            accent_dark: RgbColor::rgb(0x00, 0xA0, 0xB4),
            player_bar: RgbColor::rgb(0x03, 0x10, 0x2E),
            player_bar_border: RgbColor::rgb(0x1A, 0x32, 0x61),
            slider_track: RgbColor::rgb(0x1A, 0x32, 0x61),
            slider_fill: RgbColor::rgb(0x00, 0xE5, 0xFF),
            toggle_bg_on: RgbColor::rgb(0x00, 0xE5, 0xFF),
            toggle_bg_off: RgbColor::rgb(0x1A, 0x32, 0x61),
            table_header_bg: RgbColor::rgb(0x03, 0x10, 0x2E),
            table_row_even: RgbColor::rgb(0x03, 0x10, 0x2E),
            table_row_odd: RgbColor::rgb(0x02, 0x0A, 0x20),
            table_row_hover: RgbColor::rgb(0x0B, 0x1F, 0x47),
            search_bg: RgbColor::rgb(0x07, 0x1A, 0x40),
            search_border: RgbColor::rgb(0x1A, 0x32, 0x61),
            dark_mode: true,
        }
    }

    pub fn forest() -> Self {
        Self {
            bg: RgbColor::rgb(0xEB, 0xF4, 0xEE),
            sidebar: RgbColor::rgb(0xF2, 0xF9, 0xF5),
            surface: RgbColor::rgb(0xEB, 0xF4, 0xEE),
            card: RgbColor::rgb(0xF2, 0xF9, 0xF5),
            text: RgbColor::rgb(0x11, 0x18, 0x27),
            text_dim: RgbColor::rgb(0x4B, 0x55, 0x63),
            text_muted: RgbColor::rgb(0x6B, 0x72, 0x80),
            border: RgbColor::rgb(0xC9, 0xDD, 0xD2),
            hover: RgbColor::rgb(0xDE, 0xEA, 0xE2),
            active_bg: RgbColor::rgb(0xD5, 0xF5, 0xE3),
            sidebar_active_bg: RgbColor::rgb(0xD5, 0xF5, 0xE3),
            accent: RgbColor::rgb(0x10, 0xB9, 0x81),
            accent_light: RgbColor::rgb(0x4F, 0xCF, 0xA1),
            accent_dark: RgbColor::rgb(0x0B, 0x82, 0x5E),
            player_bar: RgbColor::rgb(0xEB, 0xF4, 0xEE),
            player_bar_border: RgbColor::rgb(0xC9, 0xDD, 0xD2),
            slider_track: RgbColor::rgb(0xC9, 0xDD, 0xD2),
            slider_fill: RgbColor::rgb(0x10, 0xB9, 0x81),
            toggle_bg_on: RgbColor::rgb(0x10, 0xB9, 0x81),
            toggle_bg_off: RgbColor::rgb(0xC9, 0xDD, 0xD2),
            table_header_bg: RgbColor::rgb(0xEB, 0xF4, 0xEE),
            table_row_even: RgbColor::rgb(0xEB, 0xF4, 0xEE),
            table_row_odd: RgbColor::rgb(0xE2, 0xED, 0xE6),
            table_row_hover: RgbColor::rgb(0xDE, 0xEA, 0xE2),
            search_bg: RgbColor::rgb(0xF2, 0xF9, 0xF5),
            search_border: RgbColor::rgb(0xC9, 0xDD, 0xD2),
            dark_mode: false,
        }
    }

    pub fn sunset() -> Self {
        Self {
            bg: RgbColor::rgb(0xFD, 0xF3, 0xEA),
            sidebar: RgbColor::rgb(0xFF, 0xFA, 0xF5),
            surface: RgbColor::rgb(0xFD, 0xF3, 0xEA),
            card: RgbColor::rgb(0xFF, 0xFA, 0xF5),
            text: RgbColor::rgb(0x11, 0x18, 0x27),
            text_dim: RgbColor::rgb(0x4B, 0x55, 0x63),
            text_muted: RgbColor::rgb(0x6B, 0x72, 0x80),
            border: RgbColor::rgb(0xF4, 0xD9, 0xC0),
            hover: RgbColor::rgb(0xF8, 0xE6, 0xD2),
            active_bg: RgbColor::rgb(0xFF, 0xE4, 0xD1),
            sidebar_active_bg: RgbColor::rgb(0xFF, 0xE4, 0xD1),
            accent: RgbColor::rgb(0xF9, 0x73, 0x16),
            accent_light: RgbColor::rgb(0xFB, 0x96, 0x48),
            accent_dark: RgbColor::rgb(0xC4, 0x52, 0x0B),
            player_bar: RgbColor::rgb(0xFD, 0xF3, 0xEA),
            player_bar_border: RgbColor::rgb(0xF4, 0xD9, 0xC0),
            slider_track: RgbColor::rgb(0xF4, 0xD9, 0xC0),
            slider_fill: RgbColor::rgb(0xF9, 0x73, 0x16),
            toggle_bg_on: RgbColor::rgb(0xF9, 0x73, 0x16),
            toggle_bg_off: RgbColor::rgb(0xF4, 0xD9, 0xC0),
            table_header_bg: RgbColor::rgb(0xFD, 0xF3, 0xEA),
            table_row_even: RgbColor::rgb(0xFD, 0xF3, 0xEA),
            table_row_odd: RgbColor::rgb(0xFB, 0xE9, 0xD7),
            table_row_hover: RgbColor::rgb(0xF8, 0xE6, 0xD2),
            search_bg: RgbColor::rgb(0xFF, 0xFA, 0xF5),
            search_border: RgbColor::rgb(0xF4, 0xD9, 0xC0),
            dark_mode: false,
        }
    }

    pub fn berry() -> Self {
        Self {
            bg: RgbColor::rgb(0x18, 0x04, 0x22),
            sidebar: RgbColor::rgb(0x24, 0x07, 0x32),
            surface: RgbColor::rgb(0x18, 0x04, 0x22),
            card: RgbColor::rgb(0x24, 0x07, 0x32),
            text: RgbColor::rgb(0xF0, 0xF2, 0xF4),
            text_dim: RgbColor::rgb(0xD0, 0xD5, 0xDF),
            text_muted: RgbColor::rgb(0xA0, 0xA8, 0xB8),
            border: RgbColor::rgb(0x3A, 0x0E, 0x54),
            hover: RgbColor::rgb(0x2A, 0x09, 0x38),
            active_bg: RgbColor::rgb(0x2A, 0x09, 0x38),
            sidebar_active_bg: RgbColor::rgb(0x32, 0x0B, 0x44),
            accent: RgbColor::rgb(0xE8, 0x43, 0x93),
            accent_light: RgbColor::rgb(0xEF, 0x72, 0xAE),
            accent_dark: RgbColor::rgb(0xB2, 0x2E, 0x6E),
            player_bar: RgbColor::rgb(0x18, 0x04, 0x22),
            player_bar_border: RgbColor::rgb(0x3A, 0x0E, 0x54),
            slider_track: RgbColor::rgb(0x3A, 0x0E, 0x54),
            slider_fill: RgbColor::rgb(0xE8, 0x43, 0x93),
            toggle_bg_on: RgbColor::rgb(0xE8, 0x43, 0x93),
            toggle_bg_off: RgbColor::rgb(0x3A, 0x0E, 0x54),
            table_header_bg: RgbColor::rgb(0x18, 0x04, 0x22),
            table_row_even: RgbColor::rgb(0x18, 0x04, 0x22),
            table_row_odd: RgbColor::rgb(0x13, 0x03, 0x1C),
            table_row_hover: RgbColor::rgb(0x2A, 0x09, 0x38),
            search_bg: RgbColor::rgb(0x24, 0x07, 0x32),
            search_border: RgbColor::rgb(0x3A, 0x0E, 0x54),
            dark_mode: true,
        }
    }

    pub fn midnight() -> Self {
        Self {
            bg: RgbColor::rgb(0x02, 0x02, 0x0E),
            sidebar: RgbColor::rgb(0x05, 0x05, 0x18),
            surface: RgbColor::rgb(0x02, 0x02, 0x0E),
            card: RgbColor::rgb(0x05, 0x05, 0x18),
            text: RgbColor::rgb(0xF0, 0xF2, 0xF4),
            text_dim: RgbColor::rgb(0xD0, 0xD5, 0xDF),
            text_muted: RgbColor::rgb(0xA0, 0xA8, 0xB8),
            border: RgbColor::rgb(0x1A, 0x1A, 0x38),
            hover: RgbColor::rgb(0x0A, 0x0A, 0x1F),
            active_bg: RgbColor::rgb(0x0A, 0x0A, 0x1F),
            sidebar_active_bg: RgbColor::rgb(0x08, 0x08, 0x24),
            accent: RgbColor::rgb(0x60, 0xA5, 0xFA),
            accent_light: RgbColor::rgb(0x93, 0xBF, 0xFB),
            accent_dark: RgbColor::rgb(0x3B, 0x82, 0xF6),
            player_bar: RgbColor::rgb(0x02, 0x02, 0x0E),
            player_bar_border: RgbColor::rgb(0x1A, 0x1A, 0x38),
            slider_track: RgbColor::rgb(0x1A, 0x1A, 0x38),
            slider_fill: RgbColor::rgb(0x60, 0xA5, 0xFA),
            toggle_bg_on: RgbColor::rgb(0x60, 0xA5, 0xFA),
            toggle_bg_off: RgbColor::rgb(0x1A, 0x1A, 0x38),
            table_header_bg: RgbColor::rgb(0x02, 0x02, 0x0E),
            table_row_even: RgbColor::rgb(0x02, 0x02, 0x0E),
            table_row_odd: RgbColor::rgb(0x01, 0x01, 0x0A),
            table_row_hover: RgbColor::rgb(0x0A, 0x0A, 0x1F),
            search_bg: RgbColor::rgb(0x05, 0x05, 0x18),
            search_border: RgbColor::rgb(0x1A, 0x1A, 0x38),
            dark_mode: true,
        }
    }

    pub fn rose() -> Self {
        Self {
            bg: RgbColor::rgb(0xFC, 0xED, 0xF0),
            sidebar: RgbColor::rgb(0xFF, 0xF5, 0xF7),
            surface: RgbColor::rgb(0xFC, 0xED, 0xF0),
            card: RgbColor::rgb(0xFF, 0xF5, 0xF7),
            text: RgbColor::rgb(0x11, 0x18, 0x27),
            text_dim: RgbColor::rgb(0x4B, 0x55, 0x63),
            text_muted: RgbColor::rgb(0x6B, 0x72, 0x80),
            border: RgbColor::rgb(0xF4, 0xC9, 0xD2),
            hover: RgbColor::rgb(0xF8, 0xDD, 0xE3),
            active_bg: RgbColor::rgb(0xFC, 0xD5, 0xDC),
            sidebar_active_bg: RgbColor::rgb(0xFC, 0xD5, 0xDC),
            accent: RgbColor::rgb(0xF4, 0x3F, 0x5E),
            accent_light: RgbColor::rgb(0xF7, 0x6C, 0x85),
            accent_dark: RgbColor::rgb(0xC2, 0x2E, 0x47),
            player_bar: RgbColor::rgb(0xFC, 0xED, 0xF0),
            player_bar_border: RgbColor::rgb(0xF4, 0xC9, 0xD2),
            slider_track: RgbColor::rgb(0xF4, 0xC9, 0xD2),
            slider_fill: RgbColor::rgb(0xF4, 0x3F, 0x5E),
            toggle_bg_on: RgbColor::rgb(0xF4, 0x3F, 0x5E),
            toggle_bg_off: RgbColor::rgb(0xF4, 0xC9, 0xD2),
            table_header_bg: RgbColor::rgb(0xFC, 0xED, 0xF0),
            table_row_even: RgbColor::rgb(0xFC, 0xED, 0xF0),
            table_row_odd: RgbColor::rgb(0xF8, 0xDD, 0xE3),
            table_row_hover: RgbColor::rgb(0xF8, 0xDD, 0xE3),
            search_bg: RgbColor::rgb(0xFF, 0xF5, 0xF7),
            search_border: RgbColor::rgb(0xF4, 0xC9, 0xD2),
            dark_mode: false,
        }
    }

    pub fn coffee() -> Self {
        Self {
            bg: RgbColor::rgb(0x18, 0x0E, 0x05),
            sidebar: RgbColor::rgb(0x24, 0x16, 0x08),
            surface: RgbColor::rgb(0x18, 0x0E, 0x05),
            card: RgbColor::rgb(0x24, 0x16, 0x08),
            text: RgbColor::rgb(0xF0, 0xF2, 0xF4),
            text_dim: RgbColor::rgb(0xD0, 0xD5, 0xDF),
            text_muted: RgbColor::rgb(0xA0, 0xA8, 0xB8),
            border: RgbColor::rgb(0x3D, 0x2A, 0x14),
            hover: RgbColor::rgb(0x22, 0x19, 0x0C),
            active_bg: RgbColor::rgb(0x22, 0x19, 0x0C),
            sidebar_active_bg: RgbColor::rgb(0x32, 0x20, 0x0C),
            accent: RgbColor::rgb(0xFB, 0xBF, 0x24),
            accent_light: RgbColor::rgb(0xFC, 0xD3, 0x4D),
            accent_dark: RgbColor::rgb(0xD9, 0x77, 0x06),
            player_bar: RgbColor::rgb(0x18, 0x0E, 0x05),
            player_bar_border: RgbColor::rgb(0x3D, 0x2A, 0x14),
            slider_track: RgbColor::rgb(0x3D, 0x2A, 0x14),
            slider_fill: RgbColor::rgb(0xFB, 0xBF, 0x24),
            toggle_bg_on: RgbColor::rgb(0xFB, 0xBF, 0x24),
            toggle_bg_off: RgbColor::rgb(0x3D, 0x2A, 0x14),
            table_header_bg: RgbColor::rgb(0x18, 0x0E, 0x05),
            table_row_even: RgbColor::rgb(0x18, 0x0E, 0x05),
            table_row_odd: RgbColor::rgb(0x12, 0x0B, 0x04),
            table_row_hover: RgbColor::rgb(0x22, 0x19, 0x0C),
            search_bg: RgbColor::rgb(0x24, 0x16, 0x08),
            search_border: RgbColor::rgb(0x3D, 0x2A, 0x14),
            dark_mode: true,
        }
    }

    pub fn mint() -> Self {
        Self {
            bg: RgbColor::rgb(0xEC, 0xF8, 0xF6),
            sidebar: RgbColor::rgb(0xF3, 0xFB, 0xFA),
            surface: RgbColor::rgb(0xEC, 0xF8, 0xF6),
            card: RgbColor::rgb(0xF3, 0xFB, 0xFA),
            text: RgbColor::rgb(0x11, 0x18, 0x27),
            text_dim: RgbColor::rgb(0x4B, 0x55, 0x63),
            text_muted: RgbColor::rgb(0x6B, 0x72, 0x80),
            border: RgbColor::rgb(0xC5, 0xE5, 0xE0),
            hover: RgbColor::rgb(0xD9, 0xEE, 0xEB),
            active_bg: RgbColor::rgb(0xC9, 0xF2, 0xEE),
            sidebar_active_bg: RgbColor::rgb(0xC9, 0xF2, 0xEE),
            accent: RgbColor::rgb(0x0D, 0x94, 0x88),
            accent_light: RgbColor::rgb(0x14, 0xB8, 0xA6),
            accent_dark: RgbColor::rgb(0x0F, 0x76, 0x6E),
            player_bar: RgbColor::rgb(0xEC, 0xF8, 0xF6),
            player_bar_border: RgbColor::rgb(0xC5, 0xE5, 0xE0),
            slider_track: RgbColor::rgb(0xC5, 0xE5, 0xE0),
            slider_fill: RgbColor::rgb(0x0D, 0x94, 0x88),
            toggle_bg_on: RgbColor::rgb(0x0D, 0x94, 0x88),
            toggle_bg_off: RgbColor::rgb(0xC5, 0xE5, 0xE0),
            table_header_bg: RgbColor::rgb(0xEC, 0xF8, 0xF6),
            table_row_even: RgbColor::rgb(0xEC, 0xF8, 0xF6),
            table_row_odd: RgbColor::rgb(0xDF, 0xF2, 0xEF),
            table_row_hover: RgbColor::rgb(0xD9, 0xEE, 0xEB),
            search_bg: RgbColor::rgb(0xF3, 0xFB, 0xFA),
            search_border: RgbColor::rgb(0xC5, 0xE5, 0xE0),
            dark_mode: false,
        }
    }
}
