//! M6a — the 15 remaining builtin theme palettes (`PaletteTauri`).
//!
//! `dark` + `light` live in the parent `tokens` module (the byte-parity
//! anchor + authoring template). This submodule holds the other 15 builtin
//! themes so the parent file stays under the §15 800-line cap.
//!
//! ## Source of truth
//!
//! Every RGB + alpha byte is transcribed 1:1 from the Tauri 1.x
//! `bentodesk/src/themes/presets.ts` `BentoTheme` literals, via the value
//! tables in `.trellis/tasks/05-29-nano-tauri-parity-plan/research/
//! m6-theme-tokens.md`. Alpha byte = `round(alpha * 255)` (e.g. 0.55 → 0x8C,
//! 0.82 → 0xD1) — round, never truncate, to match the existing `PALETTE_DARK`
//! comment math.
//!
//! ## The 4 nano-only slots (no Tauri source field)
//!
//! Tauri has no `surface_dialog`, `tooltip_bg`, or `minibar_gradient_*`
//! fields. Per the research derivation rules:
//!   - `surface_dialog` = that theme's `surface_expanded` with alpha raised
//!     ~0.10 (clamped to 1.0).
//!   - `tooltip_bg` = that theme's `surface_zen` RGB at alpha 0.90 (0xE6).
//!   - `minibar_gradient_top/bottom` = the global dark/light constants
//!     (minibar is global chrome, not per-theme-tinted in current nano).
//!
//! ## §8 / §10
//!
//! Every entry is a `pub const PaletteTauri` (`Copy`, no allocation, no new
//! crate dep). The leaf style crate stays free of `bento-nano-theme`.

use super::PaletteTauri;
use crate::Color;

// Dark-polarity global minibar gradient (mirror of `PALETTE_DARK`).
const MINIBAR_TOP_DARK: Color = Color::from_u8(0x12, 0x16, 0x22, 0xD1);
const MINIBAR_BOTTOM_DARK: Color = Color::from_u8(0x0E, 0x10, 0x1A, 0xB8);
// Light-polarity global minibar gradient (mirror of `PALETTE_LIGHT`).
const MINIBAR_TOP_LIGHT: Color = Color::from_u8(0xF5, 0xF5, 0xF8, 0xD1);
const MINIBAR_BOTTOM_LIGHT: Color = Color::from_u8(0xFA, 0xFA, 0xFC, 0xB8);

/// `midnight` — deep indigo, dark polarity.
pub const PALETTE_MIDNIGHT: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x0F, 0x17, 0x2A, 0x99),
    surface_expanded: Color::from_u8(0x0F, 0x17, 0x2A, 0xE0),
    surface_dialog: Color::from_u8(0x0F, 0x17, 0x2A, 0xF5),
    surface_hover: Color::from_u8(0x63, 0x66, 0xF1, 0x1A),
    surface_active: Color::from_u8(0x63, 0x66, 0xF1, 0x0F),
    surface_subtle: Color::from_u8(0x63, 0x66, 0xF1, 0x08),
    border_zen: Color::from_u8(0x63, 0x66, 0xF1, 0x26),
    border_expanded: Color::from_u8(0x63, 0x66, 0xF1, 0x33),
    border_hover: Color::from_u8(0x63, 0x66, 0xF1, 0x59),
    text_primary: Color::from_u8(0xE2, 0xE8, 0xF0, 0xFF),
    text_secondary: Color::from_u8(0x94, 0xA3, 0xB8, 0xFF),
    text_muted: Color::from_u8(0x64, 0x74, 0x8B, 0xFF),
    accent_blue: Color::from_u8(0x63, 0x66, 0xF1, 0xFF),
    accent_purple: Color::from_u8(0xA7, 0x8B, 0xFA, 0xFF),
    accent_green: Color::from_u8(0x34, 0xD3, 0x99, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0x92, 0x3C, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xF8, 0x71, 0x71, 0xFF),
    badge_bg: Color::from_u8(0x63, 0x66, 0xF1, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x0F, 0x17, 0x2A, 0xE6),
};

/// `forest` — dark mossy green, dark polarity.
pub const PALETTE_FOREST: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x1A, 0x2E, 0x1A, 0x8C),
    surface_expanded: Color::from_u8(0x14, 0x26, 0x14, 0xD9),
    surface_dialog: Color::from_u8(0x14, 0x26, 0x14, 0xF0),
    surface_hover: Color::from_u8(0x22, 0xC5, 0x5E, 0x1A),
    surface_active: Color::from_u8(0x22, 0xC5, 0x5E, 0x0F),
    surface_subtle: Color::from_u8(0x22, 0xC5, 0x5E, 0x08),
    border_zen: Color::from_u8(0x22, 0xC5, 0x5E, 0x1F),
    border_expanded: Color::from_u8(0x22, 0xC5, 0x5E, 0x2E),
    border_hover: Color::from_u8(0x22, 0xC5, 0x5E, 0x4C),
    text_primary: Color::from_u8(0xE8, 0xF5, 0xE9, 0xFF),
    text_secondary: Color::from_u8(0xA5, 0xD6, 0xA7, 0xFF),
    text_muted: Color::from_u8(0x6B, 0x8E, 0x6B, 0xFF),
    accent_blue: Color::from_u8(0x4A, 0xDE, 0x80, 0xFF),
    accent_purple: Color::from_u8(0xA7, 0x8B, 0xFA, 0xFF),
    accent_green: Color::from_u8(0x22, 0xC5, 0x5E, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0xBF, 0x24, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
    badge_bg: Color::from_u8(0x22, 0xC5, 0x5E, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x1A, 0x2E, 0x1A, 0xE6),
};

/// `sunset` — warm amber on near-black brown, dark polarity.
pub const PALETTE_SUNSET: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x2A, 0x1A, 0x0A, 0x8C),
    surface_expanded: Color::from_u8(0x23, 0x14, 0x08, 0xD9),
    surface_dialog: Color::from_u8(0x23, 0x14, 0x08, 0xF0),
    surface_hover: Color::from_u8(0xF5, 0x9E, 0x0B, 0x1A),
    surface_active: Color::from_u8(0xF5, 0x9E, 0x0B, 0x0F),
    surface_subtle: Color::from_u8(0xF5, 0x9E, 0x0B, 0x08),
    border_zen: Color::from_u8(0xF5, 0x9E, 0x0B, 0x24),
    border_expanded: Color::from_u8(0xF5, 0x9E, 0x0B, 0x33),
    border_hover: Color::from_u8(0xF5, 0x9E, 0x0B, 0x52),
    text_primary: Color::from_u8(0xFE, 0xF3, 0xC7, 0xFF),
    text_secondary: Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF),
    text_muted: Color::from_u8(0xA8, 0x8A, 0x4A, 0xFF),
    accent_blue: Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF),
    accent_purple: Color::from_u8(0xC0, 0x84, 0xFC, 0xFF),
    accent_green: Color::from_u8(0x84, 0xCC, 0x16, 0xFF),
    accent_orange: Color::from_u8(0xF9, 0x73, 0x16, 0xFF),
    accent_pink: Color::from_u8(0xFB, 0x71, 0x85, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
    badge_bg: Color::from_u8(0xF5, 0x9E, 0x0B, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x2A, 0x1A, 0x0A, 0xE6),
};

/// `frosted` — readable cool glass, light polarity.
///
/// The Tauri token transcription used near-transparent white surfaces with
/// near-white text.  That only worked when CSS happened to composite over a
/// dark page and became white-on-white in a native auxiliary HWND.  Nano keeps
/// the glass identity while making the semantic light polarity self-sufficient.
pub const PALETTE_FROSTED: PaletteTauri = PaletteTauri {
    is_dark: false,
    surface_zen: Color::from_u8(0xF8, 0xFA, 0xFC, 0x9E),
    surface_expanded: Color::from_u8(0xF8, 0xFA, 0xFC, 0xD9),
    surface_dialog: Color::from_u8(0xF8, 0xFA, 0xFC, 0xF2),
    surface_hover: Color::from_u8(0x0F, 0x17, 0x2A, 0x0F),
    surface_active: Color::from_u8(0x0F, 0x17, 0x2A, 0x0A),
    surface_subtle: Color::from_u8(0x0F, 0x17, 0x2A, 0x06),
    border_zen: Color::from_u8(0x64, 0x74, 0x8B, 0x29),
    border_expanded: Color::from_u8(0x64, 0x74, 0x8B, 0x3D),
    border_hover: Color::from_u8(0x47, 0x55, 0x69, 0x66),
    text_primary: Color::from_u8(0x17, 0x20, 0x33, 0xFF),
    text_secondary: Color::from_u8(0x47, 0x55, 0x69, 0xFF),
    text_muted: Color::from_u8(0x78, 0x83, 0x97, 0xFF),
    accent_blue: Color::from_u8(0x3B, 0x82, 0xF6, 0xFF),
    accent_purple: Color::from_u8(0xA7, 0x8B, 0xFA, 0xFF),
    accent_green: Color::from_u8(0x4A, 0xDE, 0x80, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0xBF, 0x24, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xF8, 0x71, 0x71, 0xFF),
    badge_bg: Color::from_u8(0x3B, 0x82, 0xF6, 0x24),
    minibar_gradient_top: MINIBAR_TOP_LIGHT,
    minibar_gradient_bottom: MINIBAR_BOTTOM_LIGHT,
    tooltip_bg: Color::from_u8(0xF8, 0xFA, 0xFC, 0xF2),
};

/// `ocean-blue` — deep ocean teal, dark polarity.
pub const PALETTE_OCEAN_BLUE: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x08, 0x2F, 0x49, 0x99),
    surface_expanded: Color::from_u8(0x08, 0x2F, 0x49, 0xD9),
    surface_dialog: Color::from_u8(0x08, 0x2F, 0x49, 0xF0),
    surface_hover: Color::from_u8(0x0E, 0xA5, 0xE9, 0x1A),
    surface_active: Color::from_u8(0x0E, 0xA5, 0xE9, 0x0F),
    surface_subtle: Color::from_u8(0x0E, 0xA5, 0xE9, 0x08),
    border_zen: Color::from_u8(0x0E, 0xA5, 0xE9, 0x26),
    border_expanded: Color::from_u8(0x0E, 0xA5, 0xE9, 0x33),
    border_hover: Color::from_u8(0x0E, 0xA5, 0xE9, 0x59),
    text_primary: Color::from_u8(0xE0, 0xF2, 0xFE, 0xFF),
    text_secondary: Color::from_u8(0x7D, 0xD3, 0xFC, 0xFF),
    text_muted: Color::from_u8(0x03, 0x69, 0xA1, 0xFF),
    accent_blue: Color::from_u8(0x0E, 0xA5, 0xE9, 0xFF),
    accent_purple: Color::from_u8(0xA7, 0x8B, 0xFA, 0xFF),
    accent_green: Color::from_u8(0x34, 0xD3, 0x99, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0x92, 0x3C, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xF8, 0x71, 0x71, 0xFF),
    badge_bg: Color::from_u8(0x0E, 0xA5, 0xE9, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x08, 0x2F, 0x49, 0xE6),
};

/// `rose-gold` — wine-rose, dark polarity.
pub const PALETTE_ROSE_GOLD: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x4C, 0x1D, 0x27, 0x99),
    surface_expanded: Color::from_u8(0x4C, 0x1D, 0x27, 0xD9),
    surface_dialog: Color::from_u8(0x4C, 0x1D, 0x27, 0xF0),
    surface_hover: Color::from_u8(0xF4, 0x3F, 0x5E, 0x1A),
    surface_active: Color::from_u8(0xF4, 0x3F, 0x5E, 0x0F),
    surface_subtle: Color::from_u8(0xF4, 0x3F, 0x5E, 0x08),
    border_zen: Color::from_u8(0xF4, 0x3F, 0x5E, 0x26),
    border_expanded: Color::from_u8(0xF4, 0x3F, 0x5E, 0x33),
    border_hover: Color::from_u8(0xF4, 0x3F, 0x5E, 0x59),
    text_primary: Color::from_u8(0xFF, 0xF1, 0xF2, 0xFF),
    text_secondary: Color::from_u8(0xFD, 0xA4, 0xAF, 0xFF),
    text_muted: Color::from_u8(0x9F, 0x12, 0x39, 0xFF),
    accent_blue: Color::from_u8(0xF4, 0x3F, 0x5E, 0xFF),
    accent_purple: Color::from_u8(0xC0, 0x84, 0xFC, 0xFF),
    accent_green: Color::from_u8(0x4A, 0xDE, 0x80, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0xBF, 0x24, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
    badge_bg: Color::from_u8(0xF4, 0x3F, 0x5E, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x4C, 0x1D, 0x27, 0xE6),
};

/// `forest-green` — emerald on near-black, dark polarity.
pub const PALETTE_FOREST_GREEN: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x14, 0x2E, 0x1A, 0x99),
    surface_expanded: Color::from_u8(0x14, 0x2E, 0x1A, 0xD9),
    surface_dialog: Color::from_u8(0x14, 0x2E, 0x1A, 0xF0),
    surface_hover: Color::from_u8(0x22, 0xC5, 0x5E, 0x1A),
    surface_active: Color::from_u8(0x22, 0xC5, 0x5E, 0x0F),
    surface_subtle: Color::from_u8(0x22, 0xC5, 0x5E, 0x08),
    border_zen: Color::from_u8(0x22, 0xC5, 0x5E, 0x26),
    border_expanded: Color::from_u8(0x22, 0xC5, 0x5E, 0x33),
    border_hover: Color::from_u8(0x22, 0xC5, 0x5E, 0x59),
    text_primary: Color::from_u8(0xDC, 0xFC, 0xE7, 0xFF),
    text_secondary: Color::from_u8(0x86, 0xEF, 0xAC, 0xFF),
    text_muted: Color::from_u8(0x16, 0x65, 0x34, 0xFF),
    accent_blue: Color::from_u8(0x22, 0xC5, 0x5E, 0xFF),
    accent_purple: Color::from_u8(0xA7, 0x8B, 0xFA, 0xFF),
    accent_green: Color::from_u8(0x4A, 0xDE, 0x80, 0xFF),
    accent_orange: Color::from_u8(0xFB, 0xBF, 0x24, 0xFF),
    accent_pink: Color::from_u8(0xF4, 0x72, 0xB6, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
    badge_bg: Color::from_u8(0x22, 0xC5, 0x5E, 0x26),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x14, 0x2E, 0x1A, 0xE6),
};

/// `solid` — Catppuccin-style opaque dark (every surface alpha = 1.0),
/// dark polarity.
pub const PALETTE_SOLID: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x1E, 0x1E, 0x2E, 0xFF),
    surface_expanded: Color::from_u8(0x18, 0x18, 0x25, 0xFF),
    surface_dialog: Color::from_u8(0x18, 0x18, 0x25, 0xFF),
    surface_hover: Color::from_u8(0x31, 0x32, 0x44, 0xFF),
    surface_active: Color::from_u8(0x2D, 0x2D, 0x3C, 0xFF),
    surface_subtle: Color::from_u8(0x23, 0x23, 0x32, 0xFF),
    border_zen: Color::from_u8(0x45, 0x47, 0x5A, 0xFF),
    border_expanded: Color::from_u8(0x58, 0x5B, 0x70, 0xFF),
    border_hover: Color::from_u8(0x6C, 0x70, 0x86, 0xFF),
    text_primary: Color::from_u8(0xCD, 0xD6, 0xF4, 0xFF),
    text_secondary: Color::from_u8(0xA6, 0xAD, 0xC8, 0xFF),
    text_muted: Color::from_u8(0x6C, 0x70, 0x86, 0xFF),
    accent_blue: Color::from_u8(0x89, 0xB4, 0xFA, 0xFF),
    accent_purple: Color::from_u8(0xCB, 0xA6, 0xF7, 0xFF),
    accent_green: Color::from_u8(0xA6, 0xE3, 0xA1, 0xFF),
    accent_orange: Color::from_u8(0xFA, 0xB3, 0x87, 0xFF),
    accent_pink: Color::from_u8(0xF5, 0xC2, 0xE7, 0xFF),
    accent_red: Color::from_u8(0xF3, 0x8B, 0xA8, 0xFF),
    badge_bg: Color::from_u8(0x45, 0x47, 0x5A, 0x99),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x1E, 0x1E, 0x2E, 0xE6),
};

/// `order` — clean near-white with slate borders, light polarity.
pub const PALETTE_ORDER: PaletteTauri = PaletteTauri {
    is_dark: false,
    surface_zen: Color::from_u8(0xFA, 0xFA, 0xFA, 0xF2),
    surface_expanded: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFA),
    surface_dialog: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF),
    surface_hover: Color::from_u8(0x00, 0x00, 0x00, 0x0A),
    surface_active: Color::from_u8(0x00, 0x00, 0x00, 0x0F),
    surface_subtle: Color::from_u8(0x00, 0x00, 0x00, 0x05),
    border_zen: Color::from_u8(0xCB, 0xD5, 0xE1, 0x80),
    border_expanded: Color::from_u8(0xCB, 0xD5, 0xE1, 0x99),
    border_hover: Color::from_u8(0xCB, 0xD5, 0xE1, 0xCC),
    text_primary: Color::from_u8(0x1F, 0x29, 0x37, 0xFF),
    text_secondary: Color::from_u8(0x37, 0x41, 0x51, 0xFF),
    text_muted: Color::from_u8(0x94, 0xA3, 0xB8, 0xFF),
    accent_blue: Color::from_u8(0xFF, 0x51, 0x2F, 0xFF),
    accent_purple: Color::from_u8(0xDD, 0x24, 0x76, 0xFF),
    accent_green: Color::from_u8(0x22, 0xC5, 0x5E, 0xFF),
    accent_orange: Color::from_u8(0xF9, 0x73, 0x16, 0xFF),
    accent_pink: Color::from_u8(0xDD, 0x24, 0x76, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
    badge_bg: Color::from_u8(0xFF, 0x51, 0x2F, 0x1F),
    minibar_gradient_top: MINIBAR_TOP_LIGHT,
    minibar_gradient_bottom: MINIBAR_BOTTOM_LIGHT,
    tooltip_bg: Color::from_u8(0xFA, 0xFA, 0xFA, 0xE6),
};

/// `flat` — Flat-UI midnight-blue slab, dark polarity.
///
/// Its opaque surface and light text are unambiguously dark; treating it as a
/// light theme inverted native control overlays and disabled states.
pub const PALETTE_FLAT: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x2C, 0x3E, 0x50, 0xF2),
    surface_expanded: Color::from_u8(0x2C, 0x3E, 0x50, 0xFA),
    surface_dialog: Color::from_u8(0x2C, 0x3E, 0x50, 0xFF),
    surface_hover: Color::from_u8(0xFF, 0xFF, 0xFF, 0x1A),
    surface_active: Color::from_u8(0xFF, 0xFF, 0xFF, 0x0F),
    surface_subtle: Color::from_u8(0xFF, 0xFF, 0xFF, 0x08),
    border_zen: Color::from_u8(0xEC, 0xF0, 0xF1, 0x26),
    border_expanded: Color::from_u8(0xEC, 0xF0, 0xF1, 0x33),
    border_hover: Color::from_u8(0xEC, 0xF0, 0xF1, 0x4C),
    text_primary: Color::from_u8(0xEC, 0xF0, 0xF1, 0xFF),
    text_secondary: Color::from_u8(0xBD, 0xC3, 0xC7, 0xFF),
    text_muted: Color::from_u8(0x7F, 0x8C, 0x8D, 0xFF),
    accent_blue: Color::from_u8(0x34, 0x98, 0xDB, 0xFF),
    accent_purple: Color::from_u8(0x9B, 0x59, 0xB6, 0xFF),
    accent_green: Color::from_u8(0x2E, 0xCC, 0x71, 0xFF),
    accent_orange: Color::from_u8(0xE6, 0x7E, 0x22, 0xFF),
    accent_pink: Color::from_u8(0xE9, 0x1E, 0x8C, 0xFF),
    accent_red: Color::from_u8(0xE7, 0x4C, 0x3C, 0xFF),
    badge_bg: Color::from_u8(0xE7, 0x4C, 0x3C, 0x33),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x2C, 0x3E, 0x50, 0xE6),
};

/// `brutalism` — hard yellow + black borders, light polarity.
pub const PALETTE_BRUTALISM: PaletteTauri = PaletteTauri {
    is_dark: false,
    surface_zen: Color::from_u8(0xFF, 0xD4, 0x00, 0xFF),
    surface_expanded: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFA),
    surface_dialog: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF),
    surface_hover: Color::from_u8(0x00, 0x00, 0x00, 0x14),
    surface_active: Color::from_u8(0x00, 0x00, 0x00, 0x1F),
    surface_subtle: Color::from_u8(0x00, 0x00, 0x00, 0x0A),
    border_zen: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
    border_expanded: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
    border_hover: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
    text_primary: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
    text_secondary: Color::from_u8(0x1A, 0x1A, 0x1A, 0xFF),
    text_muted: Color::from_u8(0x4A, 0x4A, 0x4A, 0xFF),
    accent_blue: Color::from_u8(0x00, 0x66, 0xFF, 0xFF),
    accent_purple: Color::from_u8(0x7C, 0x3A, 0xED, 0xFF),
    accent_green: Color::from_u8(0x00, 0xA8, 0x6B, 0xFF),
    accent_orange: Color::from_u8(0xFF, 0xD4, 0x00, 0xFF),
    accent_pink: Color::from_u8(0xFF, 0x2E, 0x93, 0xFF),
    accent_red: Color::from_u8(0xE6, 0x39, 0x46, 0xFF),
    badge_bg: Color::from_u8(0x00, 0x00, 0x00, 0x1F),
    minibar_gradient_top: MINIBAR_TOP_LIGHT,
    minibar_gradient_bottom: MINIBAR_BOTTOM_LIGHT,
    tooltip_bg: Color::from_u8(0xFF, 0xD4, 0x00, 0xE6),
};

/// `editorial` — serif print-press, light polarity.
pub const PALETTE_EDITORIAL: PaletteTauri = PaletteTauri {
    is_dark: false,
    surface_zen: Color::from_u8(0xFA, 0xFA, 0xFA, 0xF5),
    surface_expanded: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF),
    surface_dialog: Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF),
    surface_hover: Color::from_u8(0x00, 0x00, 0x00, 0x08),
    surface_active: Color::from_u8(0x00, 0x00, 0x00, 0x0D),
    surface_subtle: Color::from_u8(0x00, 0x00, 0x00, 0x04),
    border_zen: Color::from_u8(0x00, 0x00, 0x00, 0x1A),
    border_expanded: Color::from_u8(0x00, 0x00, 0x00, 0x1F),
    border_hover: Color::from_u8(0x00, 0x00, 0x00, 0x33),
    text_primary: Color::from_u8(0x0A, 0x0A, 0x0A, 0xFF),
    text_secondary: Color::from_u8(0x3A, 0x3A, 0x3A, 0xFF),
    text_muted: Color::from_u8(0x88, 0x88, 0x88, 0xFF),
    accent_blue: Color::from_u8(0x1F, 0x3A, 0x8A, 0xFF),
    accent_purple: Color::from_u8(0x5B, 0x21, 0xB6, 0xFF),
    accent_green: Color::from_u8(0x16, 0x65, 0x34, 0xFF),
    accent_orange: Color::from_u8(0xC2, 0x41, 0x0C, 0xFF),
    accent_pink: Color::from_u8(0xBE, 0x18, 0x5D, 0xFF),
    accent_red: Color::from_u8(0xD7, 0x26, 0x3D, 0xFF),
    badge_bg: Color::from_u8(0xD7, 0x26, 0x3D, 0x1A),
    minibar_gradient_top: MINIBAR_TOP_LIGHT,
    minibar_gradient_bottom: MINIBAR_BOTTOM_LIGHT,
    tooltip_bg: Color::from_u8(0xFA, 0xFA, 0xFA, 0xE6),
};

/// `neo` — neumorphic light grey, light polarity.
pub const PALETTE_NEO: PaletteTauri = PaletteTauri {
    is_dark: false,
    surface_zen: Color::from_u8(0xE6, 0xE8, 0xEE, 0xF2),
    surface_expanded: Color::from_u8(0xE6, 0xE8, 0xEE, 0xFA),
    surface_dialog: Color::from_u8(0xE6, 0xE8, 0xEE, 0xFF),
    surface_hover: Color::from_u8(0xFF, 0xFF, 0xFF, 0x80),
    surface_active: Color::from_u8(0xFF, 0xFF, 0xFF, 0x4C),
    surface_subtle: Color::from_u8(0xFF, 0xFF, 0xFF, 0x33),
    border_zen: Color::from_u8(0x00, 0x00, 0x00, 0x00),
    border_expanded: Color::from_u8(0x00, 0x00, 0x00, 0x00),
    border_hover: Color::from_u8(0xA3, 0xB1, 0xC6, 0x33),
    text_primary: Color::from_u8(0x2D, 0x37, 0x48, 0xFF),
    text_secondary: Color::from_u8(0x4A, 0x55, 0x68, 0xFF),
    text_muted: Color::from_u8(0xA0, 0xAE, 0xC0, 0xFF),
    accent_blue: Color::from_u8(0x66, 0x7E, 0xEA, 0xFF),
    accent_purple: Color::from_u8(0x9F, 0x7A, 0xEA, 0xFF),
    accent_green: Color::from_u8(0x48, 0xBB, 0x78, 0xFF),
    accent_orange: Color::from_u8(0xED, 0x89, 0x36, 0xFF),
    accent_pink: Color::from_u8(0xED, 0x64, 0xA6, 0xFF),
    accent_red: Color::from_u8(0xFC, 0x81, 0x81, 0xFF),
    badge_bg: Color::from_u8(0x66, 0x7E, 0xEA, 0x1F),
    minibar_gradient_top: MINIBAR_TOP_LIGHT,
    minibar_gradient_bottom: MINIBAR_BOTTOM_LIGHT,
    tooltip_bg: Color::from_u8(0xE6, 0xE8, 0xEE, 0xE6),
};

/// `terminal` — green-on-black phosphor CRT, dark polarity.
pub const PALETTE_TERMINAL: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x0A, 0x0E, 0x0C, 0xEB),
    surface_expanded: Color::from_u8(0x05, 0x07, 0x05, 0xFA),
    surface_dialog: Color::from_u8(0x05, 0x07, 0x05, 0xFF),
    surface_hover: Color::from_u8(0x00, 0xFF, 0x9C, 0x14),
    surface_active: Color::from_u8(0x00, 0xFF, 0x9C, 0x24),
    surface_subtle: Color::from_u8(0x00, 0xFF, 0x9C, 0x0A),
    border_zen: Color::from_u8(0x00, 0xFF, 0x9C, 0x59),
    border_expanded: Color::from_u8(0x00, 0xFF, 0x9C, 0x73),
    border_hover: Color::from_u8(0x00, 0xFF, 0x9C, 0xB2),
    text_primary: Color::from_u8(0x00, 0xFF, 0x9C, 0xFF),
    text_secondary: Color::from_u8(0x00, 0xFF, 0x9C, 0xC7),
    text_muted: Color::from_u8(0x00, 0xFF, 0x9C, 0x80),
    accent_blue: Color::from_u8(0x00, 0xD4, 0xFF, 0xFF),
    accent_purple: Color::from_u8(0xB7, 0x94, 0xFF, 0xFF),
    accent_green: Color::from_u8(0x00, 0xFF, 0x9C, 0xFF),
    accent_orange: Color::from_u8(0xFF, 0xB4, 0x00, 0xFF),
    accent_pink: Color::from_u8(0xFF, 0x6E, 0xC7, 0xFF),
    accent_red: Color::from_u8(0xFF, 0x33, 0x66, 0xFF),
    badge_bg: Color::from_u8(0x00, 0xFF, 0x9C, 0x21),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x0A, 0x0E, 0x0C, 0xE6),
};

/// `cyberpunk` — neon cyan/magenta on deep violet, dark polarity.
pub const PALETTE_CYBERPUNK: PaletteTauri = PaletteTauri {
    is_dark: true,
    surface_zen: Color::from_u8(0x0C, 0x04, 0x20, 0xC7),
    surface_expanded: Color::from_u8(0x0C, 0x04, 0x20, 0xF0),
    surface_dialog: Color::from_u8(0x0C, 0x04, 0x20, 0xFF),
    surface_hover: Color::from_u8(0x00, 0xF0, 0xFF, 0x14),
    surface_active: Color::from_u8(0xFF, 0x2E, 0x93, 0x1A),
    surface_subtle: Color::from_u8(0x00, 0xF0, 0xFF, 0x0A),
    border_zen: Color::from_u8(0x00, 0xF0, 0xFF, 0x8C),
    border_expanded: Color::from_u8(0xFF, 0x2E, 0x93, 0x99),
    border_hover: Color::from_u8(0x00, 0xF0, 0xFF, 0xD9),
    text_primary: Color::from_u8(0xE7, 0xF7, 0xFF, 0xFF),
    text_secondary: Color::from_u8(0x7F, 0xD3, 0xF7, 0xFF),
    text_muted: Color::from_u8(0x6B, 0x5E, 0x8E, 0xFF),
    accent_blue: Color::from_u8(0x00, 0xF0, 0xFF, 0xFF),
    accent_purple: Color::from_u8(0xB0, 0x26, 0xFF, 0xFF),
    accent_green: Color::from_u8(0x39, 0xFF, 0x14, 0xFF),
    accent_orange: Color::from_u8(0xFF, 0xB4, 0x00, 0xFF),
    accent_pink: Color::from_u8(0xFF, 0x2E, 0x93, 0xFF),
    accent_red: Color::from_u8(0xFF, 0x38, 0x64, 0xFF),
    badge_bg: Color::from_u8(0x00, 0xF0, 0xFF, 0x29),
    minibar_gradient_top: MINIBAR_TOP_DARK,
    minibar_gradient_bottom: MINIBAR_BOTTOM_DARK,
    tooltip_bg: Color::from_u8(0x0C, 0x04, 0x20, 0xE6),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocean_blue_surface_zen_matches_table() {
        // m6-theme-tokens.md: ocean-blue surface_zen = 082F49·99.
        assert_eq!(
            PALETTE_OCEAN_BLUE.surface_zen,
            Color::from_u8(0x08, 0x2F, 0x49, 0x99),
        );
    }

    #[test]
    fn ocean_blue_text_primary_matches_table() {
        // E0F2FE opaque.
        assert_eq!(
            PALETTE_OCEAN_BLUE.text_primary,
            Color::from_u8(0xE0, 0xF2, 0xFE, 0xFF),
        );
    }

    #[test]
    fn ocean_blue_accent_blue_matches_table() {
        // 0EA5E9 opaque — the tinting accent.
        assert_eq!(
            PALETTE_OCEAN_BLUE.accent_blue,
            Color::from_u8(0x0E, 0xA5, 0xE9, 0xFF),
        );
    }

    #[test]
    fn terminal_text_primary_is_phosphor_green() {
        assert_eq!(
            PALETTE_TERMINAL.text_primary,
            Color::from_u8(0x00, 0xFF, 0x9C, 0xFF),
        );
    }

    #[test]
    fn neo_border_zen_is_transparent() {
        // neo border_zen/expanded = CSS transparent.
        assert_eq!(PALETTE_NEO.border_zen, Color::TRANSPARENT);
        assert_eq!(PALETTE_NEO.border_expanded, Color::TRANSPARENT);
    }

    #[test]
    fn solid_surfaces_are_fully_opaque() {
        assert_eq!(PALETTE_SOLID.surface_zen.a, 1.0);
        assert_eq!(PALETTE_SOLID.surface_expanded.a, 1.0);
        assert_eq!(PALETTE_SOLID.surface_dialog.a, 1.0);
    }

    // --- M6a: full lookup-fn coverage (the fn lives in the parent `tokens`
    //          module; tested here so `tokens.rs` stays under the 800-line cap). ---

    use crate::tokens::palette_tauri_for_theme;

    #[test]
    fn lookup_resolves_all_17_builtin_ids() {
        for id in [
            "dark",
            "light",
            "midnight",
            "forest",
            "sunset",
            "frosted",
            "ocean-blue",
            "rose-gold",
            "forest-green",
            "solid",
            "order",
            "flat",
            "brutalism",
            "editorial",
            "neo",
            "terminal",
            "cyberpunk",
        ] {
            assert!(
                palette_tauri_for_theme(id).is_some(),
                "builtin id {id} did not resolve",
            );
        }
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        assert_eq!(palette_tauri_for_theme("shell-purple"), None);
        assert_eq!(palette_tauri_for_theme(""), None);
        assert_eq!(palette_tauri_for_theme("Dark"), None);
    }

    #[test]
    fn lookup_ocean_blue_spot_checks() {
        // m6-theme-tokens.md table values for ocean-blue, via the lookup fn.
        let p = palette_tauri_for_theme("ocean-blue").expect("ocean-blue resolves");
        assert_eq!(p.surface_zen, Color::from_u8(0x08, 0x2F, 0x49, 0x99));
        assert_eq!(p.text_primary, Color::from_u8(0xE0, 0xF2, 0xFE, 0xFF));
        assert_eq!(p.accent_blue, Color::from_u8(0x0E, 0xA5, 0xE9, 0xFF));
    }

    #[test]
    fn builtin_theme_polarity_matches_contract() {
        // Six palettes use light surfaces with dark text.
        let light = ["light", "frosted", "order", "brutalism", "editorial", "neo"];
        // Eleven palettes use dark surfaces with light text.
        let dark = [
            "dark",
            "midnight",
            "forest",
            "sunset",
            "ocean-blue",
            "rose-gold",
            "forest-green",
            "solid",
            "flat",
            "terminal",
            "cyberpunk",
        ];
        assert_eq!(light.len(), 6);
        assert_eq!(dark.len(), 11);
        for id in light {
            let p = palette_tauri_for_theme(id).expect("resolves");
            assert!(!p.is_dark, "{id} should be light-polarity");
        }
        for id in dark {
            let p = palette_tauri_for_theme(id).expect("resolves");
            assert!(p.is_dark, "{id} should be dark-polarity");
        }
    }
}
