//! M6c — per-theme `EffectTauri` descriptor + `effect_tauri_for_theme(id)`.
//!
//! Tauri ships a 4th per-theme visual axis beyond palette/radius/shadow/font:
//! a single optional `effect` channel (`bentodesk/src/themes/types.ts:82`:
//! `effect?: "scanlines" | "chromatic" | "neon" | "none"`), applied as a global
//! `data-theme-effect` attribute on `<html>` and keyed by
//! `bentodesk/src/styles/theme-effects.css` (the entire 41-line effect spec).
//!
//! ## Source of truth
//!
//! Exactly 3 of the 17 builtin themes set a non-`none` effect
//! (`bentodesk/src/themes/presets.ts`):
//!
//! | theme id | `effect` | preset line |
//! |---|---|---|
//! | `terminal` | `"scanlines"` | `:653` |
//! | `cyberpunk` | `"neon"` | `:699` |
//! | `editorial` | `"chromatic"` | `:749` |
//!
//! All 14 others (incl. `brutalism`'s explicit `effect: "none"` at `:607`)
//! return [`EffectTauri::None`]. Every literal is transcribed 1:1 from
//! `theme-effects.css` via the design doc
//! `.trellis/tasks/05-29-nano-tauri-parity-plan/research/
//! m6c-effect-primitives-design.md` §1.
//!
//! ## §8 / §10 / §11 / §15
//!
//! Lives in its own submodule (mirrors `theme_radius.rs`) so the parent
//! `tokens.rs` stays under the §15 800-line cap (it is 782 today). Every
//! aggregate is a `pub const` `Copy` value — no allocation, no new crate dep
//! (the sub-structs reuse the existing `Color` / `Shadow` primitives).
//! `effect_tauri_for_theme` is a panic-free `match`→`const` lookup returning
//! `None` for unknown (custom JSON) ids; the caller falls back to
//! `EffectTauri::None`.

use crate::{Color, Shadow};

/// One scanline period: a `lit` band of `band_dip` height + a transparent gap,
/// repeating every `period_dip`. Tauri terminal: a 1-DIP green band on a 3-DIP
/// period (`theme-effects.css:6-21`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanlineEffect {
    /// Repeat period in DIP (Tauri: 3.0 — 1-DIP band + 2-DIP gap).
    pub period_dip: f32,
    /// Lit portion of each period in DIP (Tauri: 1.0).
    pub band_dip: f32,
    /// Band colour (Tauri: `#00FF9C` @ alpha 0.06).
    pub color: Color,
}

/// A two-layer additive bloom glow (the `filter: drop-shadow` stack). Each
/// layer reuses M6b's `Shadow` primitive (offset 0,0; `blur` = the CSS blur
/// DIP; `spread` = 0; `color` = the glow colour). Drawn as concentric grown
/// fills — the same alpha-graded grow-and-fill idiom as `draw_shadow_stack`.
///
/// Authored in CSS reading order `[cyan_inner, magenta_outer]`; the render
/// primitive iterates `.rev()` so the wider magenta bloom paints first and the
/// tighter brighter cyan sits on top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeonEffect {
    /// `.bento-zone` (collapsed pill) glow: `[#00F0FF@1 blur6, #FF2E93@.4 blur12]`.
    pub collapsed: [Shadow; 2],
    /// `.bento-zone-expanded` (expanded panel) glow: `[#00F0FF@1 blur8, #FF2E93@.35 blur20]`.
    pub expanded: [Shadow; 2],
}

/// RGB channel split on heading glyph runs (`text-shadow`). Two offset copies
/// (red at `+dx`, cyan at `-dx`) drawn behind the primary text fill. Tauri
/// editorial: ±1 DIP, blur 0 (`theme-effects.css:34-40`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaticEffect {
    /// Horizontal offset in DIP for each channel copy (Tauri: 1.0).
    pub dx_dip: f32,
    /// Red channel colour (Tauri: `rgba(255,0,80,0.45)`).
    pub red: Color,
    /// Cyan channel colour (Tauri: `rgba(0,200,255,0.45)`).
    pub cyan: Color,
}

/// The single per-theme effect channel. The `None`-equivalent is the
/// [`EffectTauri::None`] variant (matches Tauri's `effect: "none"` / unset).
/// `Copy`, zero-alloc (§10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectTauri {
    None,
    Scanlines(ScanlineEffect),
    Neon(NeonEffect),
    Chromatic(ChromaticEffect),
}

/// `terminal` → scanlines: a 1-DIP `#00FF9C` band on a 3-DIP period at alpha
/// 0.06 (`theme-effects.css:6-21`). 0.06 × 255 = 15.3 → `0x0F`.
pub const EFFECT_SCANLINES_TERMINAL: EffectTauri = EffectTauri::Scanlines(ScanlineEffect {
    period_dip: 3.0,
    band_dip: 1.0,
    color: Color::from_u8(0x00, 0xFF, 0x9C, 0x0F), // #00FF9C @ .06
});

/// `cyberpunk` → neon: a two-layer `filter: drop-shadow` bloom on the zone
/// surface (`theme-effects.css:23-32`).
///
/// **1:1-INTENT divergence (LOCK)**: Tauri's L1 uses `var(--accent-blue)`,
/// which resolves to cyberpunk's `accent_blue: "#00F0FF"` (`presets.ts:676`).
/// Cyberpunk is the only neon theme, so the `var()` indirection collapses to
/// the literal `#00F0FF` here — identical value, no runtime palette coupling.
///
/// This effect glow is ADDITIVE to the M6b `SHADOW_CYBERPUNK` box-shadow stack
/// (different blur radii / alphas); both composite in Tauri (box-shadow +
/// filter are independent CSS properties). Authored `[cyan_inner, magenta_outer]`.
pub const EFFECT_NEON_CYBERPUNK: EffectTauri = EffectTauri::Neon(NeonEffect {
    collapsed: [
        Shadow::drop(0.0, 0.0, 6.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF)), // #00F0FF @ 1.0
        Shadow::drop(0.0, 0.0, 12.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x66)), // #FF2E93 @ .4 (0x66)
    ],
    expanded: [
        Shadow::drop(0.0, 0.0, 8.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF)), // #00F0FF @ 1.0
        Shadow::drop(0.0, 0.0, 20.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x59)), // #FF2E93 @ .35 (0x59)
    ],
});

/// `editorial` → chromatic: an RGB channel split on panel-title headings
/// (`theme-effects.css:34-40`). 0.45 × 255 = 114.75 → `0x73`.
pub const EFFECT_CHROMATIC_EDITORIAL: EffectTauri = EffectTauri::Chromatic(ChromaticEffect {
    dx_dip: 1.0,
    red: Color::from_u8(0xFF, 0x00, 0x50, 0x73),  // rgba(255,0,80,.45)
    cyan: Color::from_u8(0x00, 0xC8, 0xFF, 0x73), // rgba(0,200,255,.45)
});

/// M6c — resolve a builtin theme id to its authored effect.
///
/// 3 themes set one (`terminal`/`cyberpunk`/`editorial`); the other 14 (incl.
/// `brutalism`'s explicit `"none"`) return [`EffectTauri::None`]. Unknown
/// (custom JSON) ids return `None` → the caller falls back to
/// `EffectTauri::None`. Returns `Copy`, no allocation (§10); panic-free (§11).
pub fn effect_tauri_for_theme(theme_id: &str) -> Option<EffectTauri> {
    let e = match theme_id {
        "terminal" => EFFECT_SCANLINES_TERMINAL,
        "cyberpunk" => EFFECT_NEON_CYBERPUNK,
        "editorial" => EFFECT_CHROMATIC_EDITORIAL,
        "dark" | "light" | "midnight" | "forest" | "sunset" | "frosted" | "solid"
        | "ocean-blue" | "rose-gold" | "forest-green" | "order" | "flat" | "brutalism"
        | "neo" => EffectTauri::None,
        _ => return None,
    };
    Some(e)
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
                effect_tauri_for_theme(id).is_some(),
                "builtin id {id} did not resolve effect",
            );
        }
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        assert_eq!(effect_tauri_for_theme("shell-purple"), None);
        assert_eq!(effect_tauri_for_theme(""), None);
    }

    #[test]
    fn terminal_is_scanlines() {
        match effect_tauri_for_theme("terminal").unwrap() {
            EffectTauri::Scanlines(s) => {
                assert_eq!(s.period_dip, 3.0);
                assert_eq!(s.band_dip, 1.0);
                assert_eq!(s.color, Color::from_u8(0x00, 0xFF, 0x9C, 0x0F));
            }
            other => panic!("terminal effect not scanlines: {other:?}"),
        }
    }

    #[test]
    fn cyberpunk_is_neon() {
        match effect_tauri_for_theme("cyberpunk").unwrap() {
            EffectTauri::Neon(n) => {
                // collapsed: [#00F0FF@1 blur6, #FF2E93@.4 blur12]
                assert_eq!(n.collapsed[0].blur, 6.0);
                assert_eq!(n.collapsed[0].color, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
                assert_eq!(n.collapsed[1].blur, 12.0);
                assert_eq!(n.collapsed[1].color, Color::from_u8(0xFF, 0x2E, 0x93, 0x66));
                // expanded: [#00F0FF@1 blur8, #FF2E93@.35 blur20]
                assert_eq!(n.expanded[0].blur, 8.0);
                assert_eq!(n.expanded[0].color, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
                assert_eq!(n.expanded[1].blur, 20.0);
                assert_eq!(n.expanded[1].color, Color::from_u8(0xFF, 0x2E, 0x93, 0x59));
                // 0,0 offsets — pure symmetric bloom.
                for layer in n.collapsed.iter().chain(n.expanded.iter()) {
                    assert_eq!(layer.offset_x, 0.0);
                    assert_eq!(layer.offset_y, 0.0);
                    assert_eq!(layer.spread, 0.0);
                }
            }
            other => panic!("cyberpunk effect not neon: {other:?}"),
        }
    }

    #[test]
    fn editorial_is_chromatic() {
        match effect_tauri_for_theme("editorial").unwrap() {
            EffectTauri::Chromatic(c) => {
                assert_eq!(c.dx_dip, 1.0);
                assert_eq!(c.red, Color::from_u8(0xFF, 0x00, 0x50, 0x73));
                assert_eq!(c.cyan, Color::from_u8(0x00, 0xC8, 0xFF, 0x73));
            }
            other => panic!("editorial effect not chromatic: {other:?}"),
        }
    }

    #[test]
    fn fourteen_non_effect_themes_are_none() {
        for id in [
            "dark", "light", "midnight", "forest", "sunset", "frosted", "solid",
            "ocean-blue", "rose-gold", "forest-green", "order", "flat", "brutalism",
            "neo",
        ] {
            assert_eq!(
                effect_tauri_for_theme(id),
                Some(EffectTauri::None),
                "{id} must have no effect",
            );
        }
    }

    #[test]
    fn effect_is_copy() {
        // Compile-time Copy contract: bind, copy, re-read all aggregates.
        let e = EFFECT_NEON_CYBERPUNK;
        let e2 = e;
        assert_eq!(e, e2);
        let s = EFFECT_SCANLINES_TERMINAL;
        let s2 = s;
        assert_eq!(s, s2);
        let c = EFFECT_CHROMATIC_EDITORIAL;
        let c2 = c;
        assert_eq!(c, c2);
    }
}
