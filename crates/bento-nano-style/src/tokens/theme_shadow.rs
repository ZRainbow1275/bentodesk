//! M6b — per-theme `ShadowTauri` (multi-layer `ShadowStack`) +
//! `shadow_tauri_for_theme(id)`.
//!
//! Each theme's `zen` / `expanded` / `item_hover` token is a multi-layer
//! `ShadowStack` (back-to-front: `layers()[0]` inner lift, `.outer()` dominant
//! drop). The `ink_*` widget-legacy tints are global (single `Shadow`), so the
//! per-theme `ShadowTauri` copies them unchanged from the global `SHADOW`.
//!
//! ## Source of truth
//!
//! `zen` stacks are transcribed 1:1 from `bentodesk/src/themes/presets.ts`
//! `shadow_zen` (design doc §3.4); `expanded` / `item_hover` follow each
//! theme's documented shape class (§1.3): the Rounded group is a 2-layer
//! drop whose outer (L2) carries the per-theme tint colour at the dark
//! geometry; `solid` is the heavy variant; the Angular themes are `order`
//! (1 flat layer) and `flat`/`brutalism`/`editorial` (`ShadowStack::NONE`);
//! the Personality themes are `neo` (dual opposite-offset extrude),
//! `terminal` (ring + green glow), `cyberpunk` (cyan + magenta neon glow).
//! Alpha byte = `round(α × 255)`.
//!
//! ## §8 / §10 / §11
//!
//! Every entry is a `pub const ShadowTauri` (`Copy`, no allocation, no new
//! crate dep — `ShadowStack` is a `[Shadow;2]`, not a `SmallVec`).
//! `shadow_tauri_for_theme` is a panic-free `match`→`const` lookup returning
//! `None` for unknown ids.

use super::{SHADOW, ShadowTauri};
use crate::{Color, Shadow, ShadowStack};

// =============================================================================
// Rounded group + solid — 2-layer drop. Inner = `0 2 8`; outer = `0 8/16/* *`,
// the outer carries the per-theme L2 tint colour (dark uses #000). The `zen`
// outer alpha follows the presets.ts table; expanded/item_hover follow the
// dark geometry with the theme tint.
// =============================================================================

/// Build a Rounded-group `ShadowTauri` from the theme's L2 tint colour and the
/// `zen`/`expanded`/`item_hover` outer alpha bytes. Inner layers reuse the dark
/// geometry (`0 2 8` / `0 4 16` / `0 2 8`) at the per-theme inner alpha.
const fn rounded(
    tint_r: u8,
    tint_g: u8,
    tint_b: u8,
    zen_inner_a: u8,
    zen_outer_a: u8,
    exp_inner_a: u8,
    exp_outer_a: u8,
    hov_inner_a: u8,
    hov_outer_a: u8,
) -> ShadowTauri {
    ShadowTauri {
        zen: ShadowStack::two(
            Shadow::drop(0.0, 2.0, 8.0, Color::from_u8(0x00, 0x00, 0x00, zen_inner_a)),
            Shadow::drop(0.0, 8.0, 32.0, Color::from_u8(tint_r, tint_g, tint_b, zen_outer_a)),
        ),
        expanded: ShadowStack::two(
            Shadow::drop(0.0, 4.0, 16.0, Color::from_u8(0x00, 0x00, 0x00, exp_inner_a)),
            Shadow::drop(0.0, 16.0, 48.0, Color::from_u8(tint_r, tint_g, tint_b, exp_outer_a)),
        ),
        item_hover: ShadowStack::two(
            Shadow::drop(0.0, 2.0, 8.0, Color::from_u8(0x00, 0x00, 0x00, hov_inner_a)),
            Shadow::drop(0.0, 8.0, 24.0, Color::from_u8(tint_r, tint_g, tint_b, hov_outer_a)),
        ),
        ink_card: SHADOW.ink_card,
        ink_popup: SHADOW.ink_popup,
        ink_modal: SHADOW.ink_modal,
    }
}

/// `dark` — the global baseline (== `SHADOW`). Kept explicit so the §5.3
/// byte-parity contract (`shadow_tauri_for_theme("dark") == SHADOW`) reads
/// directly from the global const.
pub const SHADOW_DARK: ShadowTauri = SHADOW;

/// `light` — faint black drop (`0 2 8 #000@.04`, `0 8 32 #000@.06`).
pub const SHADOW_LIGHT: ShadowTauri =
    rounded(0x00, 0x00, 0x00, 0x0A, 0x0F, 0x14, 0x1A, 0x08, 0x0A);

/// `midnight` — indigo-tinted L2 (`#0f172a@.40`).
pub const SHADOW_MIDNIGHT: ShadowTauri =
    rounded(0x0F, 0x17, 0x2A, 0x33, 0x66, 0x40, 0x80, 0x1F, 0x33);

/// `forest` — green-tinted L2 (`#0a1e0a@.30`).
pub const SHADOW_FOREST: ShadowTauri =
    rounded(0x0A, 0x1E, 0x0A, 0x26, 0x4C, 0x33, 0x66, 0x1A, 0x29);

/// `sunset` — warm-tinted L2 (`#1e0f00@.30`).
pub const SHADOW_SUNSET: ShadowTauri =
    rounded(0x1E, 0x0F, 0x00, 0x26, 0x4C, 0x33, 0x66, 0x1A, 0x29);

/// `frosted` — faint black drop (`0 2 8 #000@.08`, `0 8 32 #000@.12`).
pub const SHADOW_FROSTED: ShadowTauri =
    rounded(0x00, 0x00, 0x00, 0x14, 0x1F, 0x1A, 0x29, 0x0D, 0x14);

/// `solid` — heavy black drop (`0 2 8 #000@.20`, `0 8 32 #000@.30`).
pub const SHADOW_SOLID: ShadowTauri =
    rounded(0x00, 0x00, 0x00, 0x33, 0x4C, 0x40, 0x73, 0x29, 0x40);

/// `ocean-blue` — blue-tinted L2 (`#082f49@.40`).
pub const SHADOW_OCEAN_BLUE: ShadowTauri =
    rounded(0x08, 0x2F, 0x49, 0x33, 0x66, 0x40, 0x80, 0x1F, 0x33);

/// `rose-gold` — rose-tinted L2 (`#4c1d27@.40`).
pub const SHADOW_ROSE_GOLD: ShadowTauri =
    rounded(0x4C, 0x1D, 0x27, 0x33, 0x66, 0x40, 0x80, 0x1F, 0x33);

/// `forest-green` — green-tinted L2 (`#142e1a@.40`).
pub const SHADOW_FOREST_GREEN: ShadowTauri =
    rounded(0x14, 0x2E, 0x1A, 0x33, 0x66, 0x40, 0x80, 0x1F, 0x33);

// =============================================================================
// Angular group.
// =============================================================================

/// `order` — single flat layer (`0 1px 3px #000@.08`); expanded/item_hover use
/// the same flat shape at slightly heavier / lighter alpha (`presets.ts:339`).
pub const SHADOW_ORDER: ShadowTauri = ShadowTauri {
    zen: ShadowStack::one(Shadow::drop(0.0, 1.0, 3.0, Color::from_u8(0x00, 0x00, 0x00, 0x14))),
    expanded: ShadowStack::one(Shadow::drop(0.0, 2.0, 6.0, Color::from_u8(0x00, 0x00, 0x00, 0x1F))),
    item_hover: ShadowStack::one(Shadow::drop(0.0, 1.0, 2.0, Color::from_u8(0x00, 0x00, 0x00, 0x0F))),
    ink_card: SHADOW.ink_card,
    ink_popup: SHADOW.ink_popup,
    ink_modal: SHADOW.ink_modal,
};

/// `flat` / `brutalism` / `editorial` — `none`: hard borders carry the depth,
/// no drop shadow (`presets.ts:423` / `:591` / `:729`). All three stacks empty.
pub const SHADOW_NONE: ShadowTauri = ShadowTauri {
    zen: ShadowStack::NONE,
    expanded: ShadowStack::NONE,
    item_hover: ShadowStack::NONE,
    ink_card: SHADOW.ink_card,
    ink_popup: SHADOW.ink_popup,
    ink_modal: SHADOW.ink_modal,
};

// =============================================================================
// Personality group — the reason M6b enriches the Shadow model.
// =============================================================================

/// `neo` — NEUMORPHIC dual: `+offset` dark blue-grey + `−offset` white extrude
/// (`6 6 12 #a3b1c6@.6`, `-6 -6 12 #fff@.8`; `presets.ts:381`). Layer order is
/// inner=dark / outer=light so the light extrude sits on top. expanded scales
/// the offsets/blur up (8/16), item_hover down (3/6).
pub const SHADOW_NEO: ShadowTauri = ShadowTauri {
    zen: ShadowStack::two(
        Shadow::drop(6.0, 6.0, 12.0, Color::from_u8(0xA3, 0xB1, 0xC6, 0x99)),
        Shadow::drop(-6.0, -6.0, 12.0, Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC)),
    ),
    expanded: ShadowStack::two(
        Shadow::drop(8.0, 8.0, 16.0, Color::from_u8(0xA3, 0xB1, 0xC6, 0x99)),
        Shadow::drop(-8.0, -8.0, 16.0, Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC)),
    ),
    item_hover: ShadowStack::two(
        Shadow::drop(3.0, 3.0, 6.0, Color::from_u8(0xA3, 0xB1, 0xC6, 0x80)),
        Shadow::drop(-3.0, -3.0, 6.0, Color::from_u8(0xFF, 0xFF, 0xFF, 0xB2)),
    ),
    ink_card: SHADOW.ink_card,
    ink_popup: SHADOW.ink_popup,
    ink_modal: SHADOW.ink_modal,
};

/// `terminal` — GLOW: a `0 0 0 1px` phosphor-green ring (spread=1, blur=0) +
/// a `0 0 16px` green glow (`#00ff9c@.25` ring, `@.15` glow; `presets.ts:637`).
/// Layer order is inner=ring / outer=glow. expanded widens the glow, item_hover
/// narrows it; the ring stays a hairline at every level.
pub const SHADOW_TERMINAL: ShadowTauri = ShadowTauri {
    zen: ShadowStack::two(
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::from_u8(0x00, 0xFF, 0x9C, 0x40),
        },
        Shadow::drop(0.0, 0.0, 16.0, Color::from_u8(0x00, 0xFF, 0x9C, 0x26)),
    ),
    expanded: ShadowStack::two(
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::from_u8(0x00, 0xFF, 0x9C, 0x59),
        },
        Shadow::drop(0.0, 0.0, 24.0, Color::from_u8(0x00, 0xFF, 0x9C, 0x33)),
    ),
    item_hover: ShadowStack::two(
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::from_u8(0x00, 0xFF, 0x9C, 0x33),
        },
        Shadow::drop(0.0, 0.0, 12.0, Color::from_u8(0x00, 0xFF, 0x9C, 0x1F)),
    ),
    ink_card: SHADOW.ink_card,
    ink_popup: SHADOW.ink_popup,
    ink_modal: SHADOW.ink_modal,
};

/// `cyberpunk` — NEON GLOW: 2 colored 0-offset glow layers, cyan inner +
/// magenta outer (`0 0 16px #00f0ff@.35`, `0 0 32px #ff2e93@.20`;
/// `presets.ts:683`). expanded widens both, item_hover narrows both.
pub const SHADOW_CYBERPUNK: ShadowTauri = ShadowTauri {
    zen: ShadowStack::two(
        Shadow::drop(0.0, 0.0, 16.0, Color::from_u8(0x00, 0xF0, 0xFF, 0x59)),
        Shadow::drop(0.0, 0.0, 32.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x33)),
    ),
    expanded: ShadowStack::two(
        Shadow::drop(0.0, 0.0, 24.0, Color::from_u8(0x00, 0xF0, 0xFF, 0x73)),
        Shadow::drop(0.0, 0.0, 48.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x4C)),
    ),
    item_hover: ShadowStack::two(
        Shadow::drop(0.0, 0.0, 12.0, Color::from_u8(0x00, 0xF0, 0xFF, 0x40)),
        Shadow::drop(0.0, 0.0, 24.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x26)),
    ),
    ink_card: SHADOW.ink_card,
    ink_popup: SHADOW.ink_popup,
    ink_modal: SHADOW.ink_modal,
};

/// M6b — resolve a builtin theme id to its authored `ShadowTauri`. Unknown ids
/// (custom JSON themes) return `None`; the caller falls back to the global
/// `SHADOW`. Returns `Copy`, no allocation (§10); panic-free (§11).
pub fn shadow_tauri_for_theme(theme_id: &str) -> Option<ShadowTauri> {
    let s = match theme_id {
        "dark" => SHADOW_DARK,
        "light" => SHADOW_LIGHT,
        "midnight" => SHADOW_MIDNIGHT,
        "forest" => SHADOW_FOREST,
        "sunset" => SHADOW_SUNSET,
        "frosted" => SHADOW_FROSTED,
        "solid" => SHADOW_SOLID,
        "ocean-blue" => SHADOW_OCEAN_BLUE,
        "rose-gold" => SHADOW_ROSE_GOLD,
        "forest-green" => SHADOW_FOREST_GREEN,
        "order" => SHADOW_ORDER,
        "flat" | "brutalism" | "editorial" => SHADOW_NONE,
        "neo" => SHADOW_NEO,
        "terminal" => SHADOW_TERMINAL,
        "cyberpunk" => SHADOW_CYBERPUNK,
        _ => return None,
    };
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_resolves_all_17_builtin_ids() {
        for id in [
            "dark", "light", "midnight", "forest", "sunset", "frosted", "ocean-blue",
            "rose-gold", "forest-green", "solid", "order", "flat", "brutalism",
            "editorial", "neo", "terminal", "cyberpunk",
        ] {
            assert!(
                shadow_tauri_for_theme(id).is_some(),
                "builtin id {id} did not resolve shadow",
            );
        }
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        assert_eq!(shadow_tauri_for_theme("shell-purple"), None);
        assert_eq!(shadow_tauri_for_theme(""), None);
    }

    #[test]
    fn dark_shadow_is_byte_identical_to_global() {
        // §5.3 byte-parity contract.
        assert_eq!(shadow_tauri_for_theme("dark"), Some(SHADOW));
    }

    #[test]
    fn angular_none_themes_have_empty_stacks() {
        for id in ["flat", "brutalism", "editorial"] {
            let s = shadow_tauri_for_theme(id).unwrap();
            assert!(s.zen.is_empty(), "{id} zen should be empty");
            assert!(s.expanded.is_empty(), "{id} expanded should be empty");
            assert!(s.item_hover.is_empty(), "{id} item_hover should be empty");
        }
    }

    #[test]
    fn order_zen_is_single_flat_layer() {
        let s = shadow_tauri_for_theme("order").unwrap();
        assert_eq!(s.zen.len(), 1);
        assert_eq!(s.zen.outer().offset_y, 1.0);
        assert_eq!(s.zen.outer().blur, 3.0);
        assert_eq!(s.zen.outer().color, Color::from_u8(0x00, 0x00, 0x00, 0x14));
    }

    #[test]
    fn neo_zen_is_dual_opposite_offset() {
        let s = shadow_tauri_for_theme("neo").unwrap();
        assert_eq!(s.zen.len(), 2);
        // inner = +offset dark blue-grey extrude.
        assert_eq!(s.zen.inner().offset_x, 6.0);
        assert_eq!(s.zen.inner().offset_y, 6.0);
        assert_eq!(s.zen.inner().color, Color::from_u8(0xA3, 0xB1, 0xC6, 0x99));
        // outer = −offset white light extrude.
        assert_eq!(s.zen.outer().offset_x, -6.0);
        assert_eq!(s.zen.outer().offset_y, -6.0);
        assert_eq!(s.zen.outer().color, Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC));
    }

    #[test]
    fn terminal_zen_ring_uses_spread_not_blur() {
        let s = shadow_tauri_for_theme("terminal").unwrap();
        assert_eq!(s.zen.len(), 2);
        // inner = the `0 0 0 1px` phosphor ring: spread=1, blur=0.
        assert_eq!(s.zen.inner().spread, 1.0);
        assert_eq!(s.zen.inner().blur, 0.0);
        assert_eq!(s.zen.inner().color, Color::from_u8(0x00, 0xFF, 0x9C, 0x40));
        // outer = the green glow: 0 offset, 16 blur.
        assert_eq!(s.zen.outer().blur, 16.0);
        assert_eq!(s.zen.outer().spread, 0.0);
    }

    #[test]
    fn cyberpunk_zen_is_cyan_then_magenta_glow() {
        let s = shadow_tauri_for_theme("cyberpunk").unwrap();
        assert_eq!(s.zen.len(), 2);
        // inner cyan, outer magenta — both 0-offset glow.
        assert_eq!(s.zen.inner().color, Color::from_u8(0x00, 0xF0, 0xFF, 0x59));
        assert_eq!(s.zen.inner().offset_x, 0.0);
        assert_eq!(s.zen.outer().color, Color::from_u8(0xFF, 0x2E, 0x93, 0x33));
        assert_eq!(s.zen.outer().blur, 32.0);
    }

    #[test]
    fn rounded_tints_carry_per_theme_l2_colour() {
        // ocean-blue's zen outer is blue-tinted (#082f49), not pure black.
        let s = shadow_tauri_for_theme("ocean-blue").unwrap();
        assert_eq!(s.zen.outer().color, Color::from_u8(0x08, 0x2F, 0x49, 0x66));
        // midnight's is indigo (#0f172a).
        let m = shadow_tauri_for_theme("midnight").unwrap();
        assert_eq!(m.zen.outer().color, Color::from_u8(0x0F, 0x17, 0x2A, 0x66));
    }
}
