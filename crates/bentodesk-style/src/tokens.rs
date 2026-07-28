//! Tauri 1.2.4 visual-parity design tokens — Wave B SSoT for Wave C/D/E.
//!
//! Source: `bentodesk/src/styles/variables.css` (46 declarations) + Wave A
//! `research/baseline/tokens-snapshot.css` + per-surface measurements from
//! Wave A `research/baseline/<surface>.md`.
//!
//! Mapping table: see `.trellis/tasks/05-20-w-b-design-token-extraction/
//! research/token-mapping.md`.
//!
//! Spec §3.2 (100% self-rolled): every value is a Rust `const`, no parser,
//! no `lazy_static`, no allocation. Spec §10 (`Copy` + small hot-path):
//! every aggregate is `Copy` and read by value — `&'static` references work
//! too but no clone-bomb risks. Spec §8 (dep whitelist): no new crate
//! dependencies; font family is `&'static str` to avoid pulling `SmolStr`
//! down into this leaf crate.
//!
//! ## Layering
//!
//! `bentodesk_style::tokens` is the **single source of truth** for Tauri-
//! aligned visual tokens. The existing `bentodesk-theme::PaletteTokens`
//! (DARK / LIGHT) is the **pre-Wave-B legacy palette** byte-locked to the
//! `BentoCard::default_chrome` shadow tests; Wave C/D/E surfaces consume
//! the new `tokens::*` SSoT, not the legacy palette. Legacy palette is
//! retained for back-compat until a follow-up wave migrates the remaining
//! widget call sites.
//!
//! ## Token gaps from Wave A — resolved
//!
//! - `RADIUS.tooltip = 8` (Wave A: tooltip uses inline 8 px).
//! - `RADIUS.minibar = 14` (Wave A: minibar uses inline 14 px).
//! - `SURFACE_DIALOG` (alpha 0.92) preserved distinct from `SURFACE_EXPANDED`
//!   (alpha 0.82) per Wave A chrome.md observation.
//! - `MINIBAR_GRADIENT_TOP/BOTTOM` for the minibar custom gradient
//!   (Wave A minibar.md: `rgba(18,22,34,0.82) → rgba(14,16,26,0.72)`).
//! - `SPACING.s6 = 6` and `SPACING.s14 = 14` added alongside the
//!   xs/sm/md/lg/xl/2xl scale so Tooltip / Dialog padding round-trips.
//! - Popover family aliases `SURFACE_EXPANDED` + `BLUR_EXPANDED` +
//!   `SHADOW_EXPANDED` + `RADIUS.expanded`; documented as deliberate.

use crate::{Color, Shadow, ShadowStack};

// M6a — the 15 remaining builtin theme palettes live in a submodule so this
// file stays under the §15 800-line cap. Re-exported at the `tokens` root so
// callers keep using `style::tokens::PALETTE_<ID>`.
pub mod theme_palettes;

// M6b — per-theme radius / shadow / font tables live in their own submodules
// for the same §15 reason. `tokens.rs` was 784 lines pre-M6b (16 under the
// 800 cap); the per-theme const tables would have blown it. Re-exported at
// the `tokens` root so callers use `style::tokens::radius_tauri_for_theme(id)`
// etc. the same way they use `palette_tauri_for_theme`.
pub mod theme_radius;
pub mod theme_shadow;
pub mod theme_typography;
// M6c — the per-theme `effect` token family (scanlines/neon/chromatic) lives
// in its own submodule for the same §15 reason (`tokens.rs` is 782 lines).
pub mod control_palette;
pub mod theme_effect;

pub use control_palette::ControlPaletteTauri;
pub use theme_effect::{
    ChromaticEffect, EFFECT_CHROMATIC_EDITORIAL, EFFECT_NEON_CYBERPUNK, EFFECT_SCANLINES_TERMINAL,
    EffectTauri, NeonEffect, ScanlineEffect, effect_tauri_for_theme,
};
pub use theme_palettes::{
    PALETTE_BRUTALISM, PALETTE_CYBERPUNK, PALETTE_EDITORIAL, PALETTE_FLAT, PALETTE_FOREST,
    PALETTE_FOREST_GREEN, PALETTE_FROSTED, PALETTE_MIDNIGHT, PALETTE_NEO, PALETTE_OCEAN_BLUE,
    PALETTE_ORDER, PALETTE_ROSE_GOLD, PALETTE_SOLID, PALETTE_SUNSET, PALETTE_TERMINAL,
};
pub use theme_radius::{
    RADIUS_CYBERPUNK, RADIUS_FLAT, RADIUS_NEO, RADIUS_ORDER, RADIUS_SHARP, RADIUS_TERMINAL,
    radius_tauri_for_theme,
};
pub use theme_shadow::{
    SHADOW_CYBERPUNK, SHADOW_DARK, SHADOW_NEO, SHADOW_NONE, SHADOW_ORDER, SHADOW_TERMINAL,
    shadow_tauri_for_theme,
};
pub use theme_typography::{TYPOGRAPHY_EDITORIAL, TYPOGRAPHY_TERMINAL, typography_tauri_for_theme};

// =============================================================================
// Palette — surface / border / text / accent / badge / scrim.
//
// `SURFACE_*` and `BORDER_*` are alpha-premultiplied at the brush boundary
// (see `bentodesk_platform::d2d::solid_brush`); we store straight-alpha here
// to match the CSS source.
// =============================================================================

/// Dark + light palette — distinguished by `is_dark` so the renderer can
/// pick at runtime without two type families.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteTauri {
    pub is_dark: bool,

    // Surface stack
    pub surface_zen: Color,
    pub surface_expanded: Color,
    pub surface_dialog: Color,
    pub surface_hover: Color,
    pub surface_active: Color,
    pub surface_subtle: Color,

    // Border stack
    pub border_zen: Color,
    pub border_expanded: Color,
    pub border_hover: Color,

    // Text stack
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,

    // Accent (zone tinting)
    pub accent_blue: Color,
    pub accent_purple: Color,
    pub accent_green: Color,
    pub accent_orange: Color,
    pub accent_pink: Color,
    pub accent_red: Color,

    // Badge background (count pill on collapsed zone)
    pub badge_bg: Color,

    // MiniBar custom gradient — distinct from `surface_zen`; Wave A flagged.
    pub minibar_gradient_top: Color,
    pub minibar_gradient_bottom: Color,

    // Tooltip — Wave A: rgba(18,18,24,0.9), a denser variant of surface_zen.
    pub tooltip_bg: Color,
}

/// Dark palette — matches `bentodesk/src/styles/variables.css` `:root,[data-theme="dark"]`.
pub const PALETTE_DARK: PaletteTauri = PaletteTauri {
    is_dark: true,

    // rgba(18,18,24,0.55) → 0x1212_18_8C  (0x8C ≈ 0.55 * 255 = 140.25)
    surface_zen: Color::from_u8(0x12, 0x12, 0x18, 0x8C),
    // rgba(12,12,18,0.82) → 0x0C0C_12_D1
    surface_expanded: Color::from_u8(0x0C, 0x0C, 0x12, 0xD1),
    // rgba(12,12,18,0.92) — Wave A chrome.md flagged divergence preserved.
    surface_dialog: Color::from_u8(0x0C, 0x0C, 0x12, 0xEB),
    // rgba(255,255,255,0.08) → 0xFFFF_FF_14
    surface_hover: Color::from_u8(0xFF, 0xFF, 0xFF, 0x14),
    // rgba(255,255,255,0.05) → 0xFFFF_FF_0D
    surface_active: Color::from_u8(0xFF, 0xFF, 0xFF, 0x0D),
    // rgba(255,255,255,0.03) → 0xFFFF_FF_08
    surface_subtle: Color::from_u8(0xFF, 0xFF, 0xFF, 0x08),

    // rgba(255,255,255,0.10) → 0xFFFF_FF_1A
    border_zen: Color::from_u8(0xFF, 0xFF, 0xFF, 0x1A),
    // rgba(255,255,255,0.12) → 0xFFFF_FF_1F
    border_expanded: Color::from_u8(0xFF, 0xFF, 0xFF, 0x1F),
    // rgba(255,255,255,0.20) → 0xFFFF_FF_33
    border_hover: Color::from_u8(0xFF, 0xFF, 0xFF, 0x33),

    text_primary: Color::from_u8(0xF0, 0xF0, 0xF5, 0xFF),
    text_secondary: Color::from_u8(0xC0, 0xC0, 0xCC, 0xFF),
    text_muted: Color::from_u8(0x78, 0x78, 0x8A, 0xFF),

    accent_blue: Color::from_u8(0x3B, 0x82, 0xF6, 0xFF),
    accent_purple: Color::from_u8(0x8B, 0x5C, 0xF6, 0xFF),
    accent_green: Color::from_u8(0x22, 0xC5, 0x5E, 0xFF),
    accent_orange: Color::from_u8(0xF9, 0x73, 0x16, 0xFF),
    accent_pink: Color::from_u8(0xEC, 0x48, 0x99, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),

    // rgba(255,255,255,0.12)
    badge_bg: Color::from_u8(0xFF, 0xFF, 0xFF, 0x1F),

    // rgba(18,22,34,0.82) → 0x1216_22_D1
    minibar_gradient_top: Color::from_u8(0x12, 0x16, 0x22, 0xD1),
    // rgba(14,16,26,0.72) → 0x0E10_1A_B8
    minibar_gradient_bottom: Color::from_u8(0x0E, 0x10, 0x1A, 0xB8),

    // rgba(18,18,24,0.9) → 0x1212_18_E6
    tooltip_bg: Color::from_u8(0x12, 0x12, 0x18, 0xE6),
};

/// Light palette — matches `[data-theme="light"]` overrides.
pub const PALETTE_LIGHT: PaletteTauri = PaletteTauri {
    is_dark: false,

    // rgba(250,250,252,0.60)
    surface_zen: Color::from_u8(0xFA, 0xFA, 0xFC, 0x99),
    // rgba(255,255,255,0.85)
    surface_expanded: Color::from_u8(0xFF, 0xFF, 0xFF, 0xD9),
    // 0.85 + 0.07 → 0.92 lift to preserve dark/light parity on the dialog
    // contrast gap (Wave A chrome.md flagged 0.82 vs 0.92 split).
    surface_dialog: Color::from_u8(0xFF, 0xFF, 0xFF, 0xEB),
    surface_hover: Color::from_u8(0x00, 0x00, 0x00, 0x0D),
    surface_active: Color::from_u8(0x00, 0x00, 0x00, 0x08),
    surface_subtle: Color::from_u8(0x00, 0x00, 0x00, 0x05),

    border_zen: Color::from_u8(0x00, 0x00, 0x00, 0x14),
    border_expanded: Color::from_u8(0x00, 0x00, 0x00, 0x1A),
    border_hover: Color::from_u8(0x00, 0x00, 0x00, 0x29),

    text_primary: Color::from_u8(0x11, 0x11, 0x18, 0xFF),
    text_secondary: Color::from_u8(0x3C, 0x3C, 0x46, 0xFF),
    text_muted: Color::from_u8(0x8E, 0x8E, 0x9A, 0xFF),

    // Light theme keeps accent identity (same hue, Wave A confirmed via
    // tokens-snapshot.css — only neutrals invert).
    accent_blue: Color::from_u8(0x3B, 0x82, 0xF6, 0xFF),
    accent_purple: Color::from_u8(0x8B, 0x5C, 0xF6, 0xFF),
    accent_green: Color::from_u8(0x22, 0xC5, 0x5E, 0xFF),
    accent_orange: Color::from_u8(0xF9, 0x73, 0x16, 0xFF),
    accent_pink: Color::from_u8(0xEC, 0x48, 0x99, 0xFF),
    accent_red: Color::from_u8(0xEF, 0x44, 0x44, 0xFF),

    // rgba(0,0,0,0.06)
    badge_bg: Color::from_u8(0x00, 0x00, 0x00, 0x0F),

    // Light variant of minibar gradient — derived to preserve depth gradient
    // direction (top slightly darker than bottom in the dark theme; in light
    // we mirror toward a frosted-white gradient).
    minibar_gradient_top: Color::from_u8(0xF5, 0xF5, 0xF8, 0xD1),
    minibar_gradient_bottom: Color::from_u8(0xFA, 0xFA, 0xFC, 0xB8),

    // Light tooltip — white at 0.9 alpha; mirrors dark behaviour.
    tooltip_bg: Color::from_u8(0xFF, 0xFF, 0xFF, 0xE6),
};

/// M6a — resolve a builtin theme id to its authored `PaletteTauri`.
///
/// Matches all 17 builtin ids 1:1 to their byte-exact const. Unknown ids
/// (custom JSON themes) return `None`; the app crate then derives a palette
/// from the live `PaletteTokens` instead. Returns `Copy`, no allocation
/// (§10); panic-free (§11).
pub fn palette_tauri_for_theme(theme_id: &str) -> Option<PaletteTauri> {
    let palette = match theme_id {
        "dark" => PALETTE_DARK,
        "light" => PALETTE_LIGHT,
        "midnight" => PALETTE_MIDNIGHT,
        "forest" => PALETTE_FOREST,
        "sunset" => PALETTE_SUNSET,
        "frosted" => PALETTE_FROSTED,
        "ocean-blue" => PALETTE_OCEAN_BLUE,
        "rose-gold" => PALETTE_ROSE_GOLD,
        "forest-green" => PALETTE_FOREST_GREEN,
        "solid" => PALETTE_SOLID,
        "order" => PALETTE_ORDER,
        "flat" => PALETTE_FLAT,
        "brutalism" => PALETTE_BRUTALISM,
        "editorial" => PALETTE_EDITORIAL,
        "neo" => PALETTE_NEO,
        "terminal" => PALETTE_TERMINAL,
        "cyberpunk" => PALETTE_CYBERPUNK,
        _ => return None,
    };
    Some(palette)
}

/// Acrylic fallback per parent PRD D5 — solid `rgba(20,20,20,0.55)` glass tint
/// when Mica/Acrylic runtime is unavailable. Sits at module root so the
/// platform code can sample it without picking a theme polarity first.
pub const ACRYLIC_FALLBACK: Color = Color::from_u8(0x14, 0x14, 0x14, 0x8C);

// =============================================================================
// Radius — 4-stop scale + capsule + tooltip + minibar gaps.
// =============================================================================

/// Border-radius scale in DIPs. Symmetric (per-corner equal); per-corner
/// variants live with the widget that needs them (e.g. tab lozenge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiusTauri {
    /// 10 px — card / badge / item.
    pub card: f32,
    /// 10 px — count badge on collapsed zone.
    pub badge: f32,
    /// 16 px — expanded zone body / popover / dialog / context menu.
    pub expanded: f32,
    /// 24 px — collapsed zone pill / stack capsule.
    pub capsule: f32,
    /// 8 px — tooltip (Wave A flagged gap, resolved here).
    pub tooltip: f32,
    /// 14 px — minibar (Wave A flagged gap, resolved here).
    pub minibar: f32,
}

pub const RADIUS: RadiusTauri = RadiusTauri {
    card: 10.0,
    badge: 10.0,
    expanded: 16.0,
    capsule: 24.0,
    tooltip: 8.0,
    minibar: 14.0,
};

// =============================================================================
// Spacing — 6-stop scale (xs/sm/md/lg/xl/2xl) + Wave A inline 6 px and 14 px.
// =============================================================================

/// Spacing scale in DIPs. Tailwind-shaped but Tauri-anchored (4 / 8 / 12 / 16 /
/// 20 / 28 — note the 12 step replaces Tailwind's 10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacingTauri {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    /// 6 px — Wave A tooltip vertical padding (inline use).
    pub s6: f32,
    /// 14 px — Wave A dialog header / popover horizontal padding.
    pub s14: f32,
}

pub const SPACING: SpacingTauri = SpacingTauri {
    xs: 4.0,
    sm: 8.0,
    md: 12.0,
    lg: 16.0,
    xl: 20.0,
    xxl: 28.0,
    s6: 6.0,
    s14: 14.0,
};

// =============================================================================
// Shadow — 3 multi-layer stacks (zen / expanded / item-hover) + 3 ink presets.
//
// M6b — each of the three CSS box-shadow tokens (`--shadow-zen` etc.) is a
// comma-separated multi-layer shadow; native now expresses that as a
// `ShadowStack`. Layer order is back-to-front: `layers()[0]` is the inner
// surface lift, `layers()[len-1]` (`.outer()`) the dominant drop. The
// per-theme variants live in `theme_shadow.rs`; this global `SHADOW` is the
// `dark`/Rounded baseline (`shadow_tauri_for_theme("dark") == SHADOW`,
// keeping every Wave-B byte-parity test green via `.outer()`).
//
// The `ink_*` widget-legacy tints stay single-layer `Shadow` (consumed by
// `bentodesk-widget` card/popup/modal chrome — never stacked).
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowTauri {
    /// `--shadow-zen`: inner `0 2px 8px #000@.15` + outer `0 8px 32px #000@.25`.
    pub zen: ShadowStack,
    /// `--shadow-expanded`: inner `0 4px 16px #000@.20` + outer `0 16px 48px #000@.40`.
    pub expanded: ShadowStack,
    /// `--shadow-item-hover`: inner `0 2px 8px #000@.12` + outer `0 8px 24px #000@.08`.
    pub item_hover: ShadowStack,
    /// Drop-shadow ink for the BentoCard chrome (legacy widget literal).
    pub ink_card: Shadow,
    /// Drop-shadow ink for the Popup primitive (legacy widget literal).
    pub ink_popup: Shadow,
    /// Drop-shadow ink for the Modal primitive (legacy widget literal).
    pub ink_modal: Shadow,
}

pub const SHADOW: ShadowTauri = ShadowTauri {
    // zen — inner (surface lift) then outer (dominant drop). `.outer()` == the
    // pre-M6b single `SHADOW.zen`; `.inner()` == the pre-M6b `SHADOW.zen_inner`.
    zen: ShadowStack::two(
        Shadow::drop(0.0, 2.0, 8.0, Color::from_u8(0x00, 0x00, 0x00, 0x26)),
        Shadow::drop(0.0, 8.0, 32.0, Color::from_u8(0x00, 0x00, 0x00, 0x40)),
    ),
    expanded: ShadowStack::two(
        Shadow::drop(0.0, 4.0, 16.0, Color::from_u8(0x00, 0x00, 0x00, 0x33)),
        Shadow::drop(0.0, 16.0, 48.0, Color::from_u8(0x00, 0x00, 0x00, 0x66)),
    ),
    item_hover: ShadowStack::two(
        Shadow::drop(0.0, 2.0, 8.0, Color::from_u8(0x00, 0x00, 0x00, 0x1F)),
        Shadow::drop(0.0, 8.0, 24.0, Color::from_u8(0x00, 0x00, 0x00, 0x14)),
    ),
    // Widget-legacy ink tints — preserved byte-for-byte from the pre-Wave-B
    // hardcoded literals in `bentodesk-widget` so the migration is visually
    // neutral. See `research/token-mapping.md` for the call-site mapping.
    ink_card: Shadow::drop(0.0, 2.0, 12.0, Color::from_u8(0x00, 0x00, 0x00, 0x40)),
    ink_popup: Shadow::drop(0.0, 4.0, 16.0, Color::from_u8(0x00, 0x00, 0x00, 0x66)),
    ink_modal: Shadow::drop(0.0, 8.0, 32.0, Color::from_u8(0x00, 0x00, 0x00, 0x99)),
};

// =============================================================================
// Typography — 4 roles (xs / sm / md / lg) with weight + line-height.
// =============================================================================

/// Typography role bundle. `font_family` is `&'static str` — string literal,
/// no allocation, no `SmolStr` dep (style is a leaf crate per spec §15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypographyRole {
    pub size_px: f32,
    pub weight: u16,
    pub line_height: f32,
}

/// Typography token bundle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypographyTauri {
    pub font_family: &'static str,

    pub xs: TypographyRole,
    pub sm: TypographyRole,
    pub md: TypographyRole,
    pub lg: TypographyRole,

    pub weight_normal: u16,
    pub weight_medium: u16,
    pub weight_semibold: u16,
    pub weight_bold: u16,
}

pub const TYPOGRAPHY: TypographyTauri = TypographyTauri {
    // V21-T4 (2026-06-23): Tauri's CSS stack starts at `"Segoe UI"`.
    // DirectWrite keeps using system fallback for CJK glyphs, while Latin zone
    // titles and right-rail labels now share the same primary metrics as the
    // WebView baseline. System fonts only; no bundled font assets.
    font_family: "Segoe UI",

    xs: TypographyRole {
        size_px: 11.0,
        weight: 400,
        line_height: 1.4,
    },
    sm: TypographyRole {
        size_px: 13.0,
        weight: 400,
        line_height: 1.4,
    },
    md: TypographyRole {
        size_px: 14.0,
        weight: 500,
        line_height: 1.4,
    },
    lg: TypographyRole {
        size_px: 16.0,
        weight: 600,
        line_height: 1.3,
    },

    weight_normal: 400,
    weight_medium: 500,
    weight_semibold: 600,
    weight_bold: 700,
};

// =============================================================================
// Motion — durations + easings for 3 categories (fast / normal / spring).
// =============================================================================

/// Easing curve identity — paired with `bentodesk-tree::Easing` downstream;
/// the leaf style crate keeps an inert tag instead of depending on tree state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingTag {
    /// CSS `ease-out` — fastest decel curve, default for fade/tooltip.
    EaseOut,
    /// Spring-style decel — used for capsule enter / stack-tray expand.
    Spring,
    /// Linear — used for the auto-stack glow pulse.
    Linear,
}

/// Motion role: duration + easing tag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionRole {
    pub duration_ms: u32,
    pub easing: EasingTag,
}

/// Motion token bundle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionTauri {
    /// 150 ms ease-out — hover, tooltip fade-in.
    pub fast: MotionRole,
    /// 250 ms ease-out — capsule enter, dialog scale-in.
    pub normal: MotionRole,
    /// 250 ms spring — stack-tray expand / collapse.
    pub spring: MotionRole,
}

pub const MOTION: MotionTauri = MotionTauri {
    fast: MotionRole {
        duration_ms: 150,
        easing: EasingTag::EaseOut,
    },
    normal: MotionRole {
        duration_ms: 250,
        easing: EasingTag::EaseOut,
    },
    spring: MotionRole {
        duration_ms: 250,
        easing: EasingTag::Spring,
    },
};

// =============================================================================
// Convenience: 4 stops radius / 6 stops spacing as arrays (for tests +
// future enum-driven access).
// =============================================================================

/// 4-stop core radius array — card, badge, expanded, capsule.
/// Acceptance: must be monotonically non-decreasing.
pub const RADIUS_STOPS: [f32; 4] = [RADIUS.card, RADIUS.badge, RADIUS.expanded, RADIUS.capsule];

/// 6-stop core spacing array — xs/sm/md/lg/xl/2xl.
/// Acceptance: must be strictly monotonically increasing.
pub const SPACING_STOPS: [f32; 6] = [
    SPACING.xs,
    SPACING.sm,
    SPACING.md,
    SPACING.lg,
    SPACING.xl,
    SPACING.xxl,
];

// =============================================================================
// Tests — hex parity, monotonicity, alpha order, motion order.
// =============================================================================

#[cfg(test)]
mod tests {
    // These tests intentionally assert over `const` design-token values to
    // guard compile-time invariants (monotonic radius/spacing scales, alpha
    // orderings). Clippy's `assertions_on_constants` fires on every one; the
    // assertions are deliberate regression guards, so allow it module-wide.
    #![allow(clippy::assertions_on_constants)]
    use super::*;

    // --- Palette hex parity (dark) ---

    #[test]
    fn dark_surface_zen_matches_css() {
        // rgba(18,18,24,0.55) — Wave A `tokens-snapshot.css` line 30.
        assert_eq!(
            PALETTE_DARK.surface_zen,
            Color::from_u8(0x12, 0x12, 0x18, 0x8C),
        );
    }

    #[test]
    fn dark_text_primary_matches_css() {
        // #f0f0f5 — variables.css line 17.
        assert_eq!(
            PALETTE_DARK.text_primary,
            Color::from_u8(0xF0, 0xF0, 0xF5, 0xFF),
        );
    }

    #[test]
    fn dark_accent_blue_matches_css() {
        // #3b82f6 — variables.css line 21.
        assert_eq!(
            PALETTE_DARK.accent_blue,
            Color::from_u8(0x3B, 0x82, 0xF6, 0xFF),
        );
    }

    #[test]
    fn dark_accent_red_matches_css() {
        // #ef4444 — used by Dialog danger button (Wave A chrome.md).
        assert_eq!(
            PALETTE_DARK.accent_red,
            Color::from_u8(0xEF, 0x44, 0x44, 0xFF),
        );
    }

    #[test]
    fn light_text_primary_matches_css() {
        // #111118 — variables.css line 84.
        assert_eq!(
            PALETTE_LIGHT.text_primary,
            Color::from_u8(0x11, 0x11, 0x18, 0xFF),
        );
    }

    #[test]
    fn accent_identity_dark_vs_light() {
        // Per Wave A: only neutrals invert, accents share identity across themes.
        assert_eq!(PALETTE_DARK.accent_blue, PALETTE_LIGHT.accent_blue);
        assert_eq!(PALETTE_DARK.accent_red, PALETTE_LIGHT.accent_red);
        assert_eq!(PALETTE_DARK.accent_green, PALETTE_LIGHT.accent_green);
    }

    #[test]
    fn surface_dialog_denser_than_expanded() {
        // Wave A chrome.md flagged 0.92 vs 0.82 divergence — preserve it.
        assert!(PALETTE_DARK.surface_dialog.a > PALETTE_DARK.surface_expanded.a);
    }

    // --- Radius monotonicity ---

    #[test]
    fn radius_4_stops_monotonic() {
        // Core 4-stop scale must be non-decreasing for animation lerps and
        // for the "next size up / down" UX gestures.
        for w in RADIUS_STOPS.windows(2) {
            assert!(w[0] <= w[1], "radius regression: {} > {}", w[0], w[1]);
        }
    }

    #[test]
    fn radius_capsule_largest_in_core() {
        assert_eq!(
            *RADIUS_STOPS
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap(),
            RADIUS.capsule,
        );
    }

    #[test]
    fn radius_tooltip_smaller_than_card() {
        // Tooltip is the densest surface; must read tighter than card.
        assert!(RADIUS.tooltip < RADIUS.card);
    }

    #[test]
    fn radius_minibar_between_card_and_expanded() {
        assert!(RADIUS.card < RADIUS.minibar);
        assert!(RADIUS.minibar < RADIUS.expanded);
    }

    // --- Spacing monotonicity ---

    #[test]
    fn spacing_6_stops_strictly_monotonic() {
        for w in SPACING_STOPS.windows(2) {
            assert!(w[0] < w[1], "spacing regression: {} >= {}", w[0], w[1]);
        }
    }

    #[test]
    fn spacing_extras_fall_between_scale_steps() {
        // Wave A flagged 6 px between xs(4) and sm(8); 14 between md(12) and lg(16).
        assert!(SPACING.xs < SPACING.s6 && SPACING.s6 < SPACING.sm);
        assert!(SPACING.md < SPACING.s14 && SPACING.s14 < SPACING.lg);
    }

    // --- Shadow alpha order ---

    #[test]
    fn shadow_zen_outer_denser_than_inner() {
        // Outer layer carries the visible drop, inner is the surface lift.
        // M6b — `zen` is now a 2-layer `ShadowStack`; `.outer()`/`.inner()`
        // recover the pre-M6b single `zen`/`zen_inner` layers byte-for-byte.
        assert!(SHADOW.zen.outer().color.a > SHADOW.zen.inner().color.a);
    }

    #[test]
    fn shadow_expanded_outer_denser_than_zen_outer() {
        // Expanded surfaces (popover/dialog) sit on a heavier shadow than
        // collapsed pills (Wave A: 0.40 vs 0.25 outer alpha).
        assert!(SHADOW.expanded.outer().color.a > SHADOW.zen.outer().color.a);
    }

    #[test]
    fn shadow_zen_outer_matches_pre_m6b_single_layer() {
        // §5.3 byte-parity contract: the dark `zen` outer layer must equal the
        // pre-M6b `0 8px 32px #000@.25` and the inner the `0 2px 8px #000@.15`.
        assert_eq!(SHADOW.zen.outer().offset_y, 8.0);
        assert_eq!(SHADOW.zen.outer().blur, 32.0);
        assert_eq!(
            SHADOW.zen.outer().color,
            Color::from_u8(0x00, 0x00, 0x00, 0x40)
        );
        assert_eq!(SHADOW.zen.inner().offset_y, 2.0);
        assert_eq!(SHADOW.zen.inner().blur, 8.0);
        assert_eq!(SHADOW.zen.outer().spread, 0.0);
        assert_eq!(SHADOW.zen.len(), 2);
    }

    #[test]
    fn shadow_ink_card_matches_legacy_widget_literal() {
        // BentoCard::default_chrome ships `Color::from_u8(0x00,0x00,0x00,0x40)`.
        // Migration must be byte-neutral.
        assert_eq!(
            SHADOW.ink_card.color,
            Color::from_u8(0x00, 0x00, 0x00, 0x40),
        );
        assert_eq!(SHADOW.ink_card.offset_y, 2.0);
        assert_eq!(SHADOW.ink_card.blur, 12.0);
    }

    #[test]
    fn shadow_ink_popup_matches_legacy_widget_literal() {
        // popup.rs ships `Color::from_u8(0x00,0x00,0x00,0x66)`.
        assert_eq!(
            SHADOW.ink_popup.color,
            Color::from_u8(0x00, 0x00, 0x00, 0x66),
        );
        assert_eq!(SHADOW.ink_popup.offset_y, 4.0);
        assert_eq!(SHADOW.ink_popup.blur, 16.0);
    }

    #[test]
    fn shadow_ink_modal_matches_legacy_widget_literal() {
        // modal.rs ships `Color::from_u8(0x00,0x00,0x00,0x99)`.
        assert_eq!(
            SHADOW.ink_modal.color,
            Color::from_u8(0x00, 0x00, 0x00, 0x99),
        );
        assert_eq!(SHADOW.ink_modal.offset_y, 8.0);
        assert_eq!(SHADOW.ink_modal.blur, 32.0);
    }

    // --- Typography ---

    #[test]
    fn typography_sizes_monotonic() {
        assert!(TYPOGRAPHY.xs.size_px < TYPOGRAPHY.sm.size_px);
        assert!(TYPOGRAPHY.sm.size_px < TYPOGRAPHY.md.size_px);
        assert!(TYPOGRAPHY.md.size_px < TYPOGRAPHY.lg.size_px);
    }

    #[test]
    fn typography_weights_monotonic() {
        assert!(TYPOGRAPHY.weight_normal < TYPOGRAPHY.weight_medium);
        assert!(TYPOGRAPHY.weight_medium < TYPOGRAPHY.weight_semibold);
        assert!(TYPOGRAPHY.weight_semibold < TYPOGRAPHY.weight_bold);
    }

    #[test]
    fn typography_xs_size_matches_css() {
        // variables.css: --font-size-xs: 11px.
        assert_eq!(TYPOGRAPHY.xs.size_px, 11.0);
    }

    #[test]
    fn typography_font_family_matches_tauri_css_primary() {
        // variables.css: --font-family-primary starts with "Segoe UI".
        assert_eq!(TYPOGRAPHY.font_family, "Segoe UI");
    }

    // --- Motion ---

    #[test]
    fn motion_fast_shorter_than_normal() {
        assert!(MOTION.fast.duration_ms < MOTION.normal.duration_ms);
    }

    #[test]
    fn motion_fast_matches_css_150ms() {
        assert_eq!(MOTION.fast.duration_ms, 150);
        assert_eq!(MOTION.fast.easing, EasingTag::EaseOut);
    }

    #[test]
    fn motion_spring_uses_spring_tag() {
        assert_eq!(MOTION.spring.easing, EasingTag::Spring);
    }

    // --- Acrylic fallback ---

    #[test]
    fn acrylic_fallback_matches_prd_d5() {
        // Parent PRD D5: solid rgba(20,20,20,0.55) glass tint.
        // 0x14 = 20, 0x8C ≈ 0.55 * 255.
        assert_eq!(ACRYLIC_FALLBACK, Color::from_u8(0x14, 0x14, 0x14, 0x8C));
    }

    // --- Minibar gradient ---

    #[test]
    fn minibar_gradient_top_denser_than_bottom() {
        // Wave A minibar.md: 0.82 → 0.72 gradient direction.
        assert!(PALETTE_DARK.minibar_gradient_top.a > PALETTE_DARK.minibar_gradient_bottom.a,);
    }

    #[test]
    fn minibar_gradient_distinct_from_surface_zen() {
        // Wave A flagged: minibar gradient must NOT collapse to surface_zen
        // (different blue tint, denser alpha).
        assert_ne!(PALETTE_DARK.minibar_gradient_top, PALETTE_DARK.surface_zen,);
    }

    // --- M6a: byte-parity anchors for the lookup fn. The full 17-id lookup +
    //         polarity coverage lives in `theme_palettes::tests` (keeps this
    //         module under the §15 800-line cap). ---

    #[test]
    fn lookup_dark_is_byte_identical() {
        // The dark theme must stay BYTE-IDENTICAL — this is the safety net.
        assert_eq!(palette_tauri_for_theme("dark"), Some(PALETTE_DARK));
    }

    #[test]
    fn lookup_light_is_byte_identical() {
        assert_eq!(palette_tauri_for_theme("light"), Some(PALETTE_LIGHT));
    }

    // --- Coverage marker for Wave A ledger ---

    #[test]
    fn wave_a_gap_tokens_present() {
        // The five gap tokens flagged by Wave A index.md must all resolve.
        let _ = RADIUS.tooltip;
        let _ = RADIUS.minibar;
        let _ = PALETTE_DARK.surface_dialog;
        let _ = PALETTE_DARK.minibar_gradient_top;
        let _ = PALETTE_DARK.minibar_gradient_bottom;
        let _ = SPACING.s6;
        let _ = SPACING.s14;
    }
}
