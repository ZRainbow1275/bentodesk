//! Palette-derived chrome for native form controls and auxiliary actions.
//!
//! Theme surfaces can be dark, light, opaque, translucent, or strongly
//! coloured.  Form controls therefore cannot safely reuse a hard-coded white
//! alpha overlay.  This module keeps that polarity decision in one small,
//! allocation-free boundary shared by every native HWND renderer.

use super::PaletteTauri;
use crate::Color;

/// Semantic chrome shared by native fields, chips, toggles and buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPaletteTauri {
    pub fill: Color,
    pub hover_fill: Color,
    pub border: Color,
    pub track_off: Color,
    pub divider: Color,
    pub knob: Color,
    pub disabled_fill: Color,
    pub disabled_border: Color,
    pub disabled_text: Color,
    pub on_accent: Color,
}

impl PaletteTauri {
    /// Neutral overlay whose ink follows the active surface polarity.
    #[inline]
    pub fn neutral_overlay(self, alpha: f32) -> Color {
        with_alpha(
            if self.is_dark {
                Color::WHITE
            } else {
                Color::BLACK
            },
            alpha,
        )
    }

    /// Black or white text, whichever has the stronger WCAG contrast.
    #[inline]
    pub fn readable_text_on(self, background: Color) -> Color {
        let black = contrast_ratio(Color::BLACK, background);
        let white = contrast_ratio(Color::WHITE, background);
        if black >= white {
            Color::BLACK
        } else {
            Color::WHITE
        }
    }

    /// Derive native control chrome from this palette without allocation.
    #[inline]
    pub fn control_palette(self) -> ControlPaletteTauri {
        ControlPaletteTauri {
            fill: self.neutral_overlay(0.06),
            hover_fill: self.neutral_overlay(0.10),
            border: self.neutral_overlay(0.12),
            track_off: self.neutral_overlay(0.16),
            divider: self.neutral_overlay(0.10),
            knob: Color::WHITE,
            disabled_fill: self.neutral_overlay(0.03),
            disabled_border: self.neutral_overlay(0.06),
            disabled_text: with_alpha(self.text_muted, 0.64),
            on_accent: self.readable_text_on(self.accent_blue),
        }
    }
}

#[inline]
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

#[inline]
fn contrast_ratio(foreground: Color, background: Color) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

#[inline]
fn relative_luminance(color: Color) -> f32 {
    0.2126 * linear_channel(color.r)
        + 0.7152 * linear_channel(color.g)
        + 0.0722 * linear_channel(color.b)
}

#[inline]
fn linear_channel(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::palette_tauri_for_theme;

    const BUILTIN_THEME_IDS: [&str; 17] = [
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
    ];

    #[test]
    fn all_builtin_dialog_text_pairs_remain_readable() {
        for id in BUILTIN_THEME_IDS {
            let palette = palette_tauri_for_theme(id).expect("builtin palette");
            let backdrop = if palette.is_dark {
                Color::BLACK
            } else {
                Color::WHITE
            };
            let dialog = composite_over(palette.surface_dialog, backdrop);
            let primary = contrast_ratio(palette.text_primary, dialog);
            let secondary = contrast_ratio(palette.text_secondary, dialog);
            assert!(primary >= 4.5, "{id} primary contrast was {primary:.2}");
            assert!(
                secondary >= 3.0,
                "{id} secondary contrast was {secondary:.2}"
            );
        }
    }

    #[test]
    fn all_builtin_primary_actions_choose_readable_text() {
        for id in BUILTIN_THEME_IDS {
            let palette = palette_tauri_for_theme(id).expect("builtin palette");
            let controls = palette.control_palette();
            let ratio = contrast_ratio(controls.on_accent, palette.accent_blue);
            assert!(ratio >= 4.5, "{id} action contrast was {ratio:.2}");
        }
    }

    #[test]
    fn neutral_control_ink_follows_surface_polarity() {
        let dark = palette_tauri_for_theme("dark")
            .expect("dark")
            .control_palette();
        let light = palette_tauri_for_theme("light")
            .expect("light")
            .control_palette();
        assert_eq!((dark.fill.r, dark.fill.g, dark.fill.b), (1.0, 1.0, 1.0));
        assert_eq!((light.fill.r, light.fill.g, light.fill.b), (0.0, 0.0, 0.0));
        assert_eq!(dark.track_off.a, 0.16);
        assert_eq!(light.track_off.a, 0.16);
    }

    fn composite_over(foreground: Color, background: Color) -> Color {
        let inv = 1.0 - foreground.a;
        Color::rgba(
            foreground.r * foreground.a + background.r * inv,
            foreground.g * foreground.a + background.g * inv,
            foreground.b * foreground.a + background.b * inv,
            1.0,
        )
    }
}
