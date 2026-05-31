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

use bento_nano_style::tokens::{
    PALETTE_DARK, PALETTE_LIGHT, PaletteTauri, RadiusTauri, ShadowTauri, TypographyTauri,
};
use bento_nano_style::{BorderRadius, Color};
use bento_nano_theme::{
    PaletteTokens, RadiusTokens, ShadowTokens, ThemeTokens, radius, shadow,
};
use smol_str::SmolStr;

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

// =============================================================================
// M6b — Family-2 bridge: RadiusTauri/ShadowTauri (style) -> RadiusTokens/
// ShadowTokens (theme-crate) so the renderer-ready `ThemeTokens` carries the
// active theme's per-theme radius/shadow/font. The app crate is the only place
// allowed to convert between the style + theme token shapes (§8 — `theme` is a
// leaf that must not depend on the style Tauri-token shapes). Pure, §10, §11.
// =============================================================================

/// The default `RadiusTauri.expanded` (`dark`/Rounded baseline = 16). The
/// per-theme `RadiusTokens` scale is derived proportionally to this so a theme
/// whose corners are sharper/softer scales the whole 5-stop ramp coherently.
const DEFAULT_TAURI_EXPANDED: f32 = 16.0;

/// Map a style-crate [`RadiusTauri`] onto the theme-crate [`RadiusTokens`]
/// 5-stop scale by scaling `radius::DEFAULT` proportionally to the theme's
/// `expanded` corner vs the default 16. For `dark`/Rounded (`expanded == 16`)
/// the factor is exactly `1.0`, so the result is **byte-identical** to
/// `radius::DEFAULT` (the §5.2 regression net). `flat` (4) scales every stop to
/// 0.25×; `brutalism`/`editorial` (0) collapse all stops to 0; `terminal` (2)
/// to 0.125×. `full` keeps its pill role: scaled `9999` is still effectively a
/// pill, but a 0-radius theme collapses it too (brutalism pills go square,
/// matching Tauri). Monotonicity is preserved (a positive scale of a monotonic
/// ramp stays monotonic). Pure, allocation-free (§10).
pub fn radius_tokens_from_tauri(rt: RadiusTauri) -> RadiusTokens {
    let factor = (rt.expanded / DEFAULT_TAURI_EXPANDED).max(0.0);
    let base = radius::DEFAULT;
    let scale = |br: BorderRadius| BorderRadius::all(br.top_left * factor);
    RadiusTokens {
        sm: scale(base.sm),
        md: scale(base.md),
        lg: scale(base.lg),
        xl: scale(base.xl),
        full: scale(base.full),
    }
}

/// Map a style-crate [`ShadowTauri`] onto the theme-crate [`ShadowTokens`]
/// 3-step scale used by widget/picker chrome.
///
/// The visible per-theme shadow stacks (neo dual / terminal-cyberpunk glow /
/// tinted Rounded drops) render through the Family-1 `draw_shadow_stack` path;
/// the Family-2 `ShadowTokens` only feeds the legacy widget `from_tokens`
/// chrome. To keep the §5.2 byte-parity net intact, drop-shaped themes (`dark`,
/// `light`, the Rounded group, `solid`, the Personality themes whose glow is
/// rendered elsewhere) keep `shadow::DEFAULT` byte-for-byte; only the Angular
/// `none` themes (whose Tauri `zen` stack is empty) flatten the widget chrome
/// shadow to transparent so e.g. a `brutalism` picker reads flat. Pure, §10.
pub fn shadow_tokens_from_tauri(st: ShadowTauri) -> ShadowTokens {
    if st.zen.is_empty() {
        // Angular `none` themes: no widget-chrome drop shadow.
        ShadowTokens {
            sm: bento_nano_style::Shadow::NONE,
            md: bento_nano_style::Shadow::NONE,
            lg: bento_nano_style::Shadow::NONE,
        }
    } else {
        // Drop / glow themes: keep the proven widget-chrome scale byte-for-byte
        // (the per-theme drop/glow lands via the Family-1 `ShadowStack` path).
        shadow::DEFAULT
    }
}

/// Build the renderer-ready [`ThemeTokens`] for a builtin id, with per-theme
/// radius / shadow / font folded in. Starts from `base` (the polarity default,
/// which carries the palette/spacing/line-heights), then overrides
/// radius/shadow from the per-theme Tauri tokens and the single theme-keyed
/// font family. Mirrors [`resolve_palette_tauri`]; the app crate is the only
/// place allowed to bridge the style <-> theme token shapes (§8). For an
/// unknown (custom JSON) id the per-theme lookups return `None` and `base` is
/// returned unchanged. Panic-free (§11).
pub fn theme_tokens_for_theme(id: &str, base: &ThemeTokens) -> ThemeTokens {
    let mut t = base.clone();
    if let Some(rt) = bento_nano_style::tokens::radius_tauri_for_theme(id) {
        t.radius = radius_tokens_from_tauri(rt);
    }
    if let Some(st) = bento_nano_style::tokens::shadow_tauri_for_theme(id) {
        t.shadow = shadow_tokens_from_tauri(st);
    }
    if let Some(tt) = bento_nano_style::tokens::typography_tauri_for_theme(id) {
        // ONLY the family is theme-keyed (sizes/weights stay the global scale,
        // already carried by `base.typo`). `new_static` keeps the literal
        // inline (no heap) — every per-theme family is a `&'static str`.
        t.typo.font_family = SmolStr::new_static(tauri_font_family_static(tt));
    }
    t
}

/// Map a per-theme [`TypographyTauri`] family back to a `&'static str` literal
/// so `SmolStr::new_static` keeps it inline. The three builtin families are the
/// only ones M6b emits.
fn tauri_font_family_static(tt: TypographyTauri) -> &'static str {
    match tt.font_family {
        "Consolas" => "Consolas",
        "Georgia" => "Georgia",
        _ => "Microsoft YaHei UI",
    }
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

    // --- M6b Family-2 bridge ---

    #[test]
    fn radius_dark_maps_byte_identical_to_default() {
        // §5.2 regression net: dark (expanded=16) -> radius::DEFAULT exactly.
        let rt = bento_nano_style::tokens::radius_tauri_for_theme("dark").unwrap();
        assert_eq!(radius_tokens_from_tauri(rt), radius::DEFAULT);
    }

    #[test]
    fn radius_flat_collapses_all_stops_toward_4() {
        // flat (expanded=4) -> factor 0.25 -> every DEFAULT stop scaled 0.25×.
        let rt = bento_nano_style::tokens::radius_tauri_for_theme("flat").unwrap();
        let r = radius_tokens_from_tauri(rt);
        assert_eq!(r.sm.top_left, radius::DEFAULT.sm.top_left * 0.25);
        assert_eq!(r.xl.top_left, radius::DEFAULT.xl.top_left * 0.25);
    }

    #[test]
    fn radius_brutalism_collapses_to_zero() {
        let rt = bento_nano_style::tokens::radius_tauri_for_theme("brutalism").unwrap();
        let r = radius_tokens_from_tauri(rt);
        assert_eq!(r.sm.top_left, 0.0);
        assert_eq!(r.lg.top_left, 0.0);
        assert_eq!(r.full.top_left, 0.0);
    }

    #[test]
    fn radius_scale_stays_monotonic() {
        // A positive scale of the monotonic DEFAULT ramp stays monotonic.
        for id in ["dark", "order", "flat", "neo", "terminal", "cyberpunk"] {
            let rt = bento_nano_style::tokens::radius_tauri_for_theme(id).unwrap();
            let r = radius_tokens_from_tauri(rt);
            assert!(r.sm.top_left <= r.md.top_left, "{id} sm>md");
            assert!(r.md.top_left <= r.lg.top_left, "{id} md>lg");
            assert!(r.lg.top_left <= r.xl.top_left, "{id} lg>xl");
            assert!(r.xl.top_left <= r.full.top_left, "{id} xl>full");
        }
    }

    #[test]
    fn shadow_drop_themes_keep_default_byte_for_byte() {
        // §5.2 net: dark/Rounded keep shadow::DEFAULT.
        for id in ["dark", "light", "midnight", "ocean-blue", "solid"] {
            let st = bento_nano_style::tokens::shadow_tauri_for_theme(id).unwrap();
            assert_eq!(shadow_tokens_from_tauri(st), shadow::DEFAULT, "{id} shadow drifted");
        }
    }

    #[test]
    fn shadow_angular_none_themes_flatten_widget_chrome() {
        for id in ["flat", "brutalism", "editorial"] {
            let st = bento_nano_style::tokens::shadow_tauri_for_theme(id).unwrap();
            let s = shadow_tokens_from_tauri(st);
            assert_eq!(s.md, bento_nano_style::Shadow::NONE, "{id} shadow not flat");
        }
    }

    #[test]
    fn theme_tokens_dark_is_byte_identical_to_base() {
        // dark must round-trip to DARK_DEFAULT (radius/shadow/typo unchanged).
        let t = theme_tokens_for_theme("dark", &DARK_DEFAULT);
        assert_eq!(t.radius, DARK_DEFAULT.radius);
        assert_eq!(t.shadow, DARK_DEFAULT.shadow);
        assert_eq!(t.typo.font_family, DARK_DEFAULT.typo.font_family);
    }

    #[test]
    fn theme_tokens_terminal_uses_consolas() {
        let t = theme_tokens_for_theme("terminal", &DARK_DEFAULT);
        assert_eq!(t.typo.font_family.as_str(), "Consolas");
        // terminal radius collapses toward 2 (factor 2/16 = 0.125).
        assert_eq!(t.radius.sm.top_left, radius::DEFAULT.sm.top_left * 0.125);
    }

    #[test]
    fn theme_tokens_editorial_uses_georgia_and_sharp_corners() {
        let t = theme_tokens_for_theme("editorial", &LIGHT_DEFAULT);
        assert_eq!(t.typo.font_family.as_str(), "Georgia");
        assert_eq!(t.radius.xl.top_left, 0.0); // sharp magazine corners
        assert_eq!(t.shadow.md, bento_nano_style::Shadow::NONE); // no drop
    }

    #[test]
    fn theme_tokens_order_sharp_radius_keeps_yahei_font() {
        let t = theme_tokens_for_theme("order", &LIGHT_DEFAULT);
        // order expanded=8 -> factor 0.5.
        assert_eq!(t.radius.xl.top_left, radius::DEFAULT.xl.top_left * 0.5);
        assert_eq!(t.typo.font_family.as_str(), "Microsoft YaHei UI");
    }

    #[test]
    fn theme_tokens_unknown_id_returns_base_unchanged() {
        let t = theme_tokens_for_theme("shell-purple", &DARK_DEFAULT);
        assert_eq!(t.radius, DARK_DEFAULT.radius);
        assert_eq!(t.shadow, DARK_DEFAULT.shadow);
        assert_eq!(t.typo.font_family, DARK_DEFAULT.typo.font_family);
    }
}
