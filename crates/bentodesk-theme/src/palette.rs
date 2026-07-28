//! Semantic colour tokens.
//!
//! 16 named slots cover background layers, surface layers, text tones, accents,
//! status colours, and one scrim. Every field is `Color`; constants are built
//! via `Color::from_u8` so the file reads like a design-token table.

use bentodesk_style::Color;

/// Palette layer. Field order is by visual hierarchy: backgrounds → surfaces →
/// borders → text → accents → status → scrim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteTokens {
    /// Window background — sits behind every surface.
    pub bg: Color,
    /// Primary surface (cards, popups). Slightly lighter than `bg`.
    pub surface: Color,
    /// Alternative surface tone for nested groupings.
    pub surface_alt: Color,
    /// Hairline divider / outline colour.
    pub border: Color,
    /// Body text on `surface` / `bg`.
    pub text: Color,
    /// De-emphasised text (captions, placeholders).
    pub text_muted: Color,
    /// Brand accent — primary CTA fill, focus ring.
    pub accent: Color,
    /// Hover variant of `accent`.
    pub accent_hover: Color,
    /// Destructive / error state.
    pub danger: Color,
    /// Success state.
    pub success: Color,
    /// Warning state.
    pub warning: Color,
    /// Informational state.
    pub info: Color,
    /// Modal scrim — overlays full window when popups open.
    pub scrim: Color,
    /// Hover background for icon buttons (semi-transparent over `surface`).
    pub hover_overlay: Color,
    /// Active / pressed background overlay.
    pub active_overlay: Color,
    /// Selection background (lists, text fields).
    pub selection: Color,
}

/// Dark palette — matches the React design tokens 1:1 with the existing
/// inline literals in `bentodesk-widget` (BentoCard background `0x18181CCC`,
/// IconButton tint `0xE0E0E6FF`, hover overlay `0xFFFFFF14`).
pub const DARK: PaletteTokens = PaletteTokens {
    bg: Color::from_u8(0x10, 0x10, 0x14, 0xFF),
    surface: Color::from_u8(0x18, 0x18, 0x1C, 0xCC),
    surface_alt: Color::from_u8(0x22, 0x22, 0x28, 0xFF),
    border: Color::from_u8(0x33, 0x33, 0x3A, 0xFF),
    text: Color::from_u8(0xE0, 0xE0, 0xE6, 0xFF),
    text_muted: Color::from_u8(0x9A, 0x9A, 0xA4, 0xFF),
    accent: Color::from_u8(0x33, 0x66, 0xCC, 0xFF),
    accent_hover: Color::from_u8(0x4D, 0x80, 0xE6, 0xFF),
    danger: Color::from_u8(0xE5, 0x4B, 0x4B, 0xFF),
    success: Color::from_u8(0x3F, 0xB9, 0x50, 0xFF),
    warning: Color::from_u8(0xE3, 0xA0, 0x08, 0xFF),
    info: Color::from_u8(0x33, 0x99, 0xCC, 0xFF),
    scrim: Color::from_u8(0x00, 0x00, 0x00, 0x80),
    hover_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x14),
    active_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x29),
    selection: Color::from_u8(0x33, 0x66, 0xCC, 0x66),
};

/// Light palette — inverted polarity, same accent identity.
pub const LIGHT: PaletteTokens = PaletteTokens {
    bg: Color::from_u8(0xF7, 0xF7, 0xF8, 0xFF),
    surface: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF),
    surface_alt: Color::from_u8(0xF0, 0xF0, 0xF2, 0xFF),
    border: Color::from_u8(0xD8, 0xD8, 0xDD, 0xFF),
    text: Color::from_u8(0x18, 0x18, 0x1C, 0xFF),
    text_muted: Color::from_u8(0x6E, 0x6E, 0x78, 0xFF),
    accent: Color::from_u8(0x33, 0x66, 0xCC, 0xFF),
    accent_hover: Color::from_u8(0x29, 0x52, 0xA3, 0xFF),
    danger: Color::from_u8(0xCC, 0x33, 0x33, 0xFF),
    success: Color::from_u8(0x2F, 0x9F, 0x40, 0xFF),
    warning: Color::from_u8(0xCC, 0x88, 0x00, 0xFF),
    info: Color::from_u8(0x29, 0x83, 0xB8, 0xFF),
    scrim: Color::from_u8(0x00, 0x00, 0x00, 0x40),
    hover_overlay: Color::from_u8(0x00, 0x00, 0x00, 0x0A),
    active_overlay: Color::from_u8(0x00, 0x00, 0x00, 0x14),
    selection: Color::from_u8(0x33, 0x66, 0xCC, 0x40),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_surface_matches_existing_widget_literal() {
        // BentoCard::default_chrome ships background = 0x18181CCC. The DARK
        // palette MUST preserve that byte-for-byte so T-004 migration is
        // visually neutral.
        assert_eq!(DARK.surface, Color::from_u8(0x18, 0x18, 0x1C, 0xCC));
    }

    #[test]
    fn dark_text_matches_existing_icon_button_tint() {
        // IconButton ships tint = 0xE0E0E6FF.
        assert_eq!(DARK.text, Color::from_u8(0xE0, 0xE0, 0xE6, 0xFF));
    }

    #[test]
    fn dark_hover_overlay_matches_existing_icon_button_hover() {
        // IconButton ships hover_background = 0xFFFFFF14.
        assert_eq!(DARK.hover_overlay, Color::from_u8(0xFF, 0xFF, 0xFF, 0x14));
    }

    #[test]
    fn dark_accent_matches_existing_button_background() {
        // ButtonNode::new ships background = 0x3366CCFF.
        assert_eq!(DARK.accent, Color::from_u8(0x33, 0x66, 0xCC, 0xFF));
    }
}
