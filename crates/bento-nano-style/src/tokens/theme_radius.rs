//! M6b — per-theme `RadiusTauri` consts + `radius_tauri_for_theme(id)`.
//!
//! The `dark`/Rounded baseline (24/16/10/10) lives in the parent `tokens`
//! module as the global `RADIUS` const; this submodule holds the Angular +
//! Personality shape overrides so the parent file stays under the §15
//! 800-line cap (mirrors `theme_palettes.rs`).
//!
//! ## Source of truth
//!
//! Every value is transcribed 1:1 from the Tauri 1.x
//! `bentodesk/src/themes/presets.ts` `BentoTheme` radius fields
//! (`radius_capsule / radius_expanded / radius_card / radius_badge`), via the
//! per-theme table in `.trellis/tasks/05-29-nano-tauri-parity-plan/research/
//! m6b-theme-tokens-design.md` §1.2.
//!
//! ## The 2 nano-only slots
//!
//! `tooltip` (8) and `minibar` (14) are GLOBAL chrome, not per-theme in Tauri
//! (the tooltip/minibar live outside the theme's card/panel surface — design
//! doc §1.2). Every per-theme const copies them unchanged from the global
//! `RADIUS`, so only `card/badge/expanded/capsule` vary by theme.
//!
//! ## §8 / §10 / §11
//!
//! Every entry is a `pub const RadiusTauri` (`Copy`, no allocation, no new
//! crate dep). `radius_tauri_for_theme` is a panic-free `match`→`const`
//! lookup returning `None` for unknown (custom JSON) ids.

use super::{RADIUS, RadiusTauri};

/// `order` — Swiss/Bauhaus Angular: 8 capsule/expanded, 6 card/badge (`presets.ts:348-351`).
pub const RADIUS_ORDER: RadiusTauri = RadiusTauri {
    card: 6.0,
    badge: 6.0,
    expanded: 8.0,
    capsule: 8.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// `flat` — uniform 4 across all stops (`presets.ts:432-435`).
pub const RADIUS_FLAT: RadiusTauri = RadiusTauri {
    card: 4.0,
    badge: 4.0,
    expanded: 4.0,
    capsule: 4.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// `brutalism` / `editorial` — sharp 0 corners (`presets.ts:600-603` / `:738-741`).
pub const RADIUS_SHARP: RadiusTauri = RadiusTauri {
    card: 0.0,
    badge: 0.0,
    expanded: 0.0,
    capsule: 0.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// `neo` — soft neumorphic: 16 capsule/expanded, 12 card/badge (`presets.ts:390-393`).
pub const RADIUS_NEO: RadiusTauri = RadiusTauri {
    card: 12.0,
    badge: 12.0,
    expanded: 16.0,
    capsule: 16.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// `terminal` — near-sharp 2 across all stops (`presets.ts:646-649`).
pub const RADIUS_TERMINAL: RadiusTauri = RadiusTauri {
    card: 2.0,
    badge: 2.0,
    expanded: 2.0,
    capsule: 2.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// `cyberpunk` — near-sharp 3 across all stops (`presets.ts:692-695`).
pub const RADIUS_CYBERPUNK: RadiusTauri = RadiusTauri {
    card: 3.0,
    badge: 3.0,
    expanded: 3.0,
    capsule: 3.0,
    tooltip: RADIUS.tooltip,
    minibar: RADIUS.minibar,
};

/// M6b — resolve a builtin theme id to its authored `RadiusTauri`.
///
/// The 10 Rounded themes + `solid` share the 24/16/10/10 baseline (the global
/// `RADIUS`). The Angular + Personality themes return their shape override.
/// Unknown ids (custom JSON themes) return `None`; the caller falls back to
/// the global `RADIUS`. Returns `Copy`, no allocation (§10); panic-free (§11).
pub fn radius_tauri_for_theme(theme_id: &str) -> Option<RadiusTauri> {
    let r = match theme_id {
        // Rounded group (10) + solid all share the 24/16/10/10 baseline.
        "dark" | "light" | "midnight" | "forest" | "sunset" | "frosted" | "solid"
        | "ocean-blue" | "rose-gold" | "forest-green" => RADIUS,
        "order" => RADIUS_ORDER,
        "flat" => RADIUS_FLAT,
        "brutalism" | "editorial" => RADIUS_SHARP,
        "neo" => RADIUS_NEO,
        "terminal" => RADIUS_TERMINAL,
        "cyberpunk" => RADIUS_CYBERPUNK,
        _ => return None,
    };
    Some(r)
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
                radius_tauri_for_theme(id).is_some(),
                "builtin id {id} did not resolve radius",
            );
        }
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        assert_eq!(radius_tauri_for_theme("shell-purple"), None);
        assert_eq!(radius_tauri_for_theme(""), None);
    }

    #[test]
    fn dark_radius_is_byte_identical_to_global() {
        // §5.3 byte-parity contract: dark MUST equal the global RADIUS so the
        // Wave-B chrome contract tests stay green.
        assert_eq!(radius_tauri_for_theme("dark"), Some(RADIUS));
    }

    #[test]
    fn rounded_group_all_share_global_baseline() {
        for id in [
            "light", "midnight", "forest", "sunset", "frosted", "solid", "ocean-blue",
            "rose-gold", "forest-green",
        ] {
            assert_eq!(radius_tauri_for_theme(id), Some(RADIUS), "{id} radius drifted");
        }
    }

    #[test]
    fn angular_personality_radius_literals() {
        // The 1:1 presets.ts shape overrides.
        assert_eq!(radius_tauri_for_theme("order").unwrap().capsule, 8.0);
        assert_eq!(radius_tauri_for_theme("order").unwrap().card, 6.0);
        assert_eq!(radius_tauri_for_theme("flat").unwrap().card, 4.0);
        assert_eq!(radius_tauri_for_theme("brutalism").unwrap().capsule, 0.0);
        assert_eq!(radius_tauri_for_theme("editorial").unwrap().card, 0.0);
        assert_eq!(radius_tauri_for_theme("neo").unwrap().expanded, 16.0);
        assert_eq!(radius_tauri_for_theme("neo").unwrap().card, 12.0);
        assert_eq!(radius_tauri_for_theme("terminal").unwrap().card, 2.0);
        assert_eq!(radius_tauri_for_theme("cyberpunk").unwrap().card, 3.0);
    }

    #[test]
    fn nano_only_slots_stay_global_for_all_themes() {
        // tooltip/minibar are global chrome — never per-theme (design doc §1.2).
        for id in ["order", "flat", "brutalism", "neo", "terminal", "cyberpunk"] {
            let r = radius_tauri_for_theme(id).unwrap();
            assert_eq!(r.tooltip, RADIUS.tooltip, "{id} tooltip drifted");
            assert_eq!(r.minibar, RADIUS.minibar, "{id} minibar drifted");
        }
    }
}
