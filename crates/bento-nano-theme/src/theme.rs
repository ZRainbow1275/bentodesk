//! Top-level theme aggregation + the two baked themes.
//!
//! Per the §11 ruling, themes are referenced by `&'static ThemeTokens`. Both
//! `DARK_DEFAULT` and `LIGHT_DEFAULT` are `static` (not `const`) so we can
//! return references to them; identity is by pointer equality of the `&'static
//! ThemeTokens` handle the observer hands out.

use smol_str::SmolStr;

use crate::{
    ThemeTokens, palette, radius, shadow, spacing,
    typo::{FontSizes, FontWeights, LineHeights, TypoTokens},
};

/// Stable identifier for a theme. `SmolStr` keeps the literal "dark" / "light"
/// inline (≤22 bytes, no allocation) and the type stays equality-cheap.
pub type ThemeId = SmolStr;

pub static DARK_DEFAULT: ThemeTokens = ThemeTokens {
    palette: palette::DARK,
    spacing: spacing::DEFAULT,
    radius: radius::DEFAULT,
    shadow: shadow::DEFAULT,
    typo: TypoTokens {
        font_family: SmolStr::new_static("Segoe UI"),
        sizes: FontSizes {
            xs: 11.0,
            sm: 13.0,
            md: 14.0,
            lg: 16.0,
            xl: 24.0,
            xxl: 32.0,
        },
        weights: FontWeights {
            normal: 400,
            medium: 500,
            bold: 700,
        },
        line_heights: LineHeights {
            tight: 1.1,
            normal: 1.4,
            loose: 1.7,
        },
    },
};

pub static LIGHT_DEFAULT: ThemeTokens = ThemeTokens {
    palette: palette::LIGHT,
    spacing: spacing::DEFAULT,
    radius: radius::DEFAULT,
    shadow: shadow::DEFAULT,
    typo: TypoTokens {
        font_family: SmolStr::new_static("Segoe UI"),
        sizes: FontSizes {
            xs: 11.0,
            sm: 13.0,
            md: 14.0,
            lg: 16.0,
            xl: 24.0,
            xxl: 32.0,
        },
        weights: FontWeights {
            normal: 400,
            medium: 500,
            bold: 700,
        },
        line_heights: LineHeights {
            tight: 1.1,
            normal: 1.4,
            loose: 1.7,
        },
    },
};

/// Registry of every baked theme. The observer initialises against the first
/// entry; the renderer / settings UI iterates this slice to populate any
/// theme-picker.
pub static THEMES: &[(&str, &ThemeTokens)] = &[("dark", &DARK_DEFAULT), ("light", &LIGHT_DEFAULT)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_default_is_distinct_from_light() {
        assert_ne!(DARK_DEFAULT.palette.bg, LIGHT_DEFAULT.palette.bg);
    }

    #[test]
    fn themes_registry_lists_both() {
        assert_eq!(THEMES.len(), 2);
        assert_eq!(THEMES[0].0, "dark");
        assert_eq!(THEMES[1].0, "light");
    }

    #[test]
    fn dark_default_palette_surface_preserves_bento_card_alpha() {
        // Visual-parity guard rail: the original BentoCard ships
        // background = 0x18181CCC. The DARK theme MUST keep the same
        // alpha byte (0xCC) so T-004 can swap literals without ΔE.
        assert_eq!(DARK_DEFAULT.palette.surface.a, 0xCC as f32 / 255.0);
    }

    #[test]
    fn default_typography_scale_matches_tauri_css_and_style_tokens() {
        let expected = bento_nano_style::tokens::TYPOGRAPHY;
        for (_, theme) in THEMES {
            assert_eq!(theme.typo.font_family.as_str(), expected.font_family);
            assert_eq!(theme.typo.sizes.xs, expected.xs.size_px);
            assert_eq!(theme.typo.sizes.sm, expected.sm.size_px);
            assert_eq!(theme.typo.sizes.md, expected.md.size_px);
            assert_eq!(theme.typo.sizes.lg, expected.lg.size_px);
        }
    }
}
