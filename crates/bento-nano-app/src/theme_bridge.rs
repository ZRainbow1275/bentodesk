//! M6a — `PaletteTokens` → `PaletteTauri` bridge for non-builtin themes.
//!
//! The 17 builtin themes resolve to byte-exact `PaletteTauri` consts via
//! `bento_nano_style::tokens::palette_tauri_for_theme(id)`. Custom JSON
//! themes (whose ids are unknown to that lookup) only carry the 16-slot
//! `PaletteTokens`; this module derives a surface-oriented `PaletteTauri`
//! from them so the renderer can still re-skin to a custom theme.
//!
//! ## Layering (§8 — no new crate dep)
//!
//! `PaletteTokens` lives in `bento-nano-theme`; `PaletteTauri` lives in
//! `bento-nano-style`. The style crate is a leaf and must NOT depend on
//! `bento-nano-theme`, so the bridge cannot live there. The app crate
//! already depends on both, so the derivation lives here. No new crate, no
//! new cross-crate dep.
//!
//! ## Derivation ratios (m6-migration.md TASK B)
//!
//! The alpha ratios (0.82 / 0.92 / 0.03 / 0.75 / 0.12 …) are calibrated so a
//! `DARK_DEFAULT`-shaped `PaletteTokens` reproduces `PALETTE_DARK` as closely
//! as the ratios allow. The 6 `accent_*` hues are fixed brand identity (not
//! theme-tinted) — copied through from the dark/light const by polarity, so
//! the zone-accent picker palette never desyncs.

use bento_nano_style::Color;
use bento_nano_style::tokens::{PALETTE_DARK, PALETTE_LIGHT, PaletteTauri};
use bento_nano_theme::PaletteTokens;

/// Straight-alpha override (mirrors `render.rs::with_alpha`). Pure, §10.
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// Derive a `PaletteTauri` from a custom-theme `PaletteTokens`.
///
/// Used ONLY for non-builtin (custom JSON) themes — the 17 builtins always
/// resolve to their authored const first. Pure + allocation-free (§10),
/// panic-free (§11).
pub fn derive_palette_tauri(tokens: &PaletteTokens, is_dark: bool) -> PaletteTauri {
    // Accent hues are fixed brand identity per TASK B; copy them through from
    // the matching-polarity const so a custom theme keeps the standard
    // green-success / red-danger / etc. zone hues.
    let brand = if is_dark { PALETTE_DARK } else { PALETTE_LIGHT };

    PaletteTauri {
        is_dark,

        // Surfaces — `surface` is the glass card tone; denser variants are
        // alpha-lifts that reproduce the dark-default 0xD1 / 0xEB bytes.
        surface_zen: tokens.surface,
        surface_expanded: with_alpha(tokens.surface, 0.82),
        surface_dialog: with_alpha(tokens.surface, 0.92),
        surface_hover: tokens.hover_overlay,
        surface_active: tokens.active_overlay,
        // No 0.03 overlay in PaletteTokens — derive a faint text-tinted wash.
        surface_subtle: with_alpha(tokens.text, 0.03),

        // Borders — `border` direct + two text-tinted lifts.
        border_zen: tokens.border,
        border_expanded: with_alpha(tokens.text, 0.12),
        border_hover: with_alpha(tokens.text, 0.20),

        // Text — primary + muted direct; mid tone derived.
        text_primary: tokens.text,
        text_secondary: with_alpha(tokens.text, 0.75),
        text_muted: tokens.text_muted,

        // Accent ramp — fixed brand identity (NOT theme-tinted) per TASK B,
        // except the live accent maps to `accent_blue` so a custom accent
        // actually tints zone affordances.
        accent_blue: tokens.accent,
        accent_purple: brand.accent_purple,
        accent_green: brand.accent_green,
        accent_orange: brand.accent_orange,
        accent_pink: brand.accent_pink,
        accent_red: brand.accent_red,

        // Badge bg — text-tinted wash (white @0.12 on dark).
        badge_bg: with_alpha(tokens.text, 0.12),

        // Minibar gradient + tooltip — niche global chrome; keep the
        // matching-polarity brand const (not custom-theme-tinted).
        minibar_gradient_top: brand.minibar_gradient_top,
        minibar_gradient_bottom: brand.minibar_gradient_bottom,
        tooltip_bg: with_alpha(tokens.surface, 0.90),
    }
}

/// Determine theme polarity from a palette. Returns `true` (dark) when the
/// window background luminance is below the mid-point. Used by the apply path
/// when a builtin polarity flag is unavailable (custom JSON themes).
pub fn palette_is_dark(tokens: &PaletteTokens) -> bool {
    // Rec. 601 relative luminance on the straight (non-premultiplied) bg RGB.
    let bg = tokens.bg;
    let luma = 0.299 * bg.r + 0.587 * bg.g + 0.114 * bg.b;
    luma < 0.5
}

/// M6a — resolve ANY theme id to a `PaletteTauri`. Builtin ids hit the
/// byte-exact `palette_tauri_for_theme` const; unknown (custom) ids fall back
/// to `derive_palette_tauri` off the live tokens. Panic-free (§11).
pub fn resolve_palette_tauri(theme_id: &str, tokens: &PaletteTokens) -> PaletteTauri {
    bento_nano_style::tokens::palette_tauri_for_theme(theme_id)
        .unwrap_or_else(|| derive_palette_tauri(tokens, palette_is_dark(tokens)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_theme::{DARK_DEFAULT, LIGHT_DEFAULT};

    #[test]
    fn builtin_dark_resolves_to_exact_const() {
        // Applying "dark" via the resolver yields the byte-exact PALETTE_DARK.
        let p = resolve_palette_tauri("dark", &DARK_DEFAULT.palette);
        assert_eq!(p, PALETTE_DARK);
    }

    #[test]
    fn builtin_ocean_blue_resolves_to_exact_const() {
        // The id wins even when the live tokens are the dark default.
        let p = resolve_palette_tauri("ocean-blue", &DARK_DEFAULT.palette);
        assert_eq!(
            p,
            bento_nano_style::tokens::PALETTE_OCEAN_BLUE,
        );
    }

    #[test]
    fn custom_theme_falls_back_to_derivation() {
        // An unknown id derives from the live tokens; accents stay brand id.
        let p = resolve_palette_tauri("shell-purple", &DARK_DEFAULT.palette);
        assert_eq!(p.is_dark, true);
        assert_eq!(p.surface_zen, DARK_DEFAULT.palette.surface);
        assert_eq!(p.accent_blue, DARK_DEFAULT.palette.accent);
        assert_eq!(p.accent_green, PALETTE_DARK.accent_green);
    }

    #[test]
    fn palette_is_dark_classifies_defaults() {
        assert!(palette_is_dark(&DARK_DEFAULT.palette));
        assert!(!palette_is_dark(&LIGHT_DEFAULT.palette));
    }

    #[test]
    fn derive_surface_dialog_denser_than_expanded() {
        // The dark-default derivation must keep the 0.92 > 0.82 alpha order.
        let p = derive_palette_tauri(&DARK_DEFAULT.palette, true);
        assert!(p.surface_dialog.a > p.surface_expanded.a);
    }
}
