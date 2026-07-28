//! M6b — per-theme `TypographyTauri` + `typography_tauri_for_theme(id)`.
//!
//! Only `font_family` is theme-keyed (Tauri font sizes/weights live in the
//! global `variables.css`, not per-theme — design doc §1.4). Two builtin
//! themes set a non-default family:
//!
//! | theme    | Tauri CSS stack (`presets.ts`)                         | native family |
//! |----------|--------------------------------------------------------|-------------|
//! | terminal | `"JetBrains Mono", "Consolas", ui-monospace, monospace`| `"Consolas"`|
//! | editorial| `"Playfair Display", Georgia, "Times New Roman", serif`| `"Georgia"` |
//! | all 15   | *(unset)* → system UI font                             | `"Segoe UI"` |
//!
//! ## Font-family resolution caveat (LOCK — design doc §1.4 / §7)
//!
//! native renders via DirectWrite `text_format_from_family_name_with_metrics`,
//! which takes ONE family name and does NOT fall through a CSS comma-stack.
//! `"JetBrains Mono"` / `"Playfair Display"` are NOT guaranteed installed on
//! Win11, so native maps each to the FIRST Win11-GUARANTEED face in the stack:
//! terminal → `"Consolas"`, editorial → `"Georgia"` (both ship in-box on
//! Win11). This is a deliberate 1:1-INTENT (not byte) mapping. The existing
//! `ensure_text_format_for_active_theme` font-swap machinery (render.rs)
//! rebuilds the DirectWrite format automatically when this value changes — no
//! new mechanism is needed.
//!
//! ## §8 / §10 / §11
//!
//! `font_family` is `&'static str` (string literal, no allocation, no
//! `SmolStr` dep in the leaf style crate). Sizes/weights are copied from the
//! global `TYPOGRAPHY`. `typography_tauri_for_theme` is a panic-free
//! `match`→`const` lookup returning `None` for unknown ids.

use super::{TYPOGRAPHY, TypographyTauri};

/// `terminal` — monospace. First Win11-guaranteed face in the Tauri stack.
pub const TYPOGRAPHY_TERMINAL: TypographyTauri = TypographyTauri {
    font_family: "Consolas",
    ..TYPOGRAPHY
};

/// `editorial` — serif. First Win11-guaranteed face in the Tauri stack.
pub const TYPOGRAPHY_EDITORIAL: TypographyTauri = TypographyTauri {
    font_family: "Georgia",
    ..TYPOGRAPHY
};

/// M6b — resolve a builtin theme id to its authored `TypographyTauri`. The 15
/// non-overriding themes return the global `TYPOGRAPHY` ("Segoe UI").
/// Unknown ids (custom JSON themes) return `None`; the caller falls back to the
/// global `TYPOGRAPHY`. Returns `Copy`, no allocation (§10); panic-free (§11).
pub fn typography_tauri_for_theme(theme_id: &str) -> Option<TypographyTauri> {
    let t = match theme_id {
        "terminal" => TYPOGRAPHY_TERMINAL,
        "editorial" => TYPOGRAPHY_EDITORIAL,
        "dark" | "light" | "midnight" | "forest" | "sunset" | "frosted" | "solid"
        | "ocean-blue" | "rose-gold" | "forest-green" | "order" | "flat" | "brutalism" | "neo"
        | "cyberpunk" => TYPOGRAPHY,
        _ => return None,
    };
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                typography_tauri_for_theme(id).is_some(),
                "builtin id {id} did not resolve typography",
            );
        }
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        assert_eq!(typography_tauri_for_theme("shell-purple"), None);
        assert_eq!(typography_tauri_for_theme(""), None);
    }

    #[test]
    fn terminal_is_monospace_consolas() {
        assert_eq!(
            typography_tauri_for_theme("terminal").unwrap().font_family,
            "Consolas",
        );
    }

    #[test]
    fn editorial_is_serif_georgia() {
        assert_eq!(
            typography_tauri_for_theme("editorial").unwrap().font_family,
            "Georgia",
        );
    }

    #[test]
    fn default_themes_keep_tauri_css_primary() {
        for id in ["dark", "light", "neo", "cyberpunk", "order", "brutalism"] {
            assert_eq!(
                typography_tauri_for_theme(id).unwrap().font_family,
                "Segoe UI",
                "{id} font drifted",
            );
        }
    }

    #[test]
    fn sizes_and_weights_are_global_for_all_themes() {
        // Only font_family is per-theme — sizes/weights stay the global scale.
        let term = typography_tauri_for_theme("terminal").unwrap();
        assert_eq!(term.xs.size_px, TYPOGRAPHY.xs.size_px);
        assert_eq!(term.weight_bold, TYPOGRAPHY.weight_bold);
        let edit = typography_tauri_for_theme("editorial").unwrap();
        assert_eq!(edit.lg.size_px, TYPOGRAPHY.lg.size_px);
    }
}
