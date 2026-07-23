//! Business surface — IconPicker.
//!
//! Visual spec: `icon_picker.snap.md`. Composition lands when widget-library
//! ships VirtualGrid (T-029) for the 1600+ Lucide icon scrollable + Input
//! (T-017) for the search field + Modal (T-023) for the host window
//! (alternate path: render in dedicated `WindowKind::IconPicker` HWND
//! already added in window factory T-011).
//!
//! Status: selected-stack reachable for the built-in gallery. The dedicated
//! `WindowKind::IconPicker` HWND renders the 30 source ZoneIcons through D2D;
//! Lucide search/upload expansion remains separate future parity work.

use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Shadow};
use bento_nano_theme::{self as theme, PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_widget::WidgetNode;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Build the IconPicker widget subtree. The current runtime renderer owns the
/// native HWND draw path for built-in icons; this retained subtree preserves
/// the window metrics/chrome contract for future Lucide search/upload widgets.
pub fn build() -> WidgetNode {
    use bento_nano_layout::Direction;
    use bento_nano_style::{Edges, Length};
    use bento_nano_widget::ContainerNode;
    let chrome = IconPickerChrome::from_palette(theme::current().palette);
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(WINDOW_WIDTH),
        height: Length::Px(WINDOW_HEIGHT),
        padding: Edges::ZERO,
        background: chrome.panel_background,
        radius: chrome.panel_radius,
        ..ContainerNode::default()
    })
}

/// Default IconPicker window geometry (DIPs) per snap.md.
pub const WINDOW_WIDTH: f32 = 480.0;
pub const WINDOW_HEIGHT: f32 = 640.0;

/// Visible-cell cap per render — matches the 1.x `VISIBLE_CAP` so the
/// IntersectionObserver (→ VirtualGrid) port stays behaviour-equivalent.
/// The "Refine your search" hint surfaces when results would exceed this.
pub const VISIBLE_CAP: usize = 200;

/// IconPicker colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconPickerChrome {
    pub panel_shadow: Shadow,
    pub panel_radius: BorderRadius,
    pub chip_radius: BorderRadius,
    pub chip_inner_radius: BorderRadius,
    pub slot_radius: BorderRadius,
    pub slot_inner_radius: BorderRadius,
    pub panel_background: Color,
    pub chip_background: Color,
    pub accent_color: Color,
    pub title_color: Color,
    pub body_color: Color,
    pub muted_color: Color,
    pub warning_color: Color,
}

impl IconPickerChrome {
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            chip_radius: radius.lg,
            chip_inner_radius: radius.md,
            slot_radius: radius.lg,
            slot_inner_radius: radius.md,
            panel_background: palette.surface,
            chip_background: palette.surface_alt,
            accent_color: palette.accent,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
            warning_color: palette.warning,
        }
    }

    /// Build IconPicker chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `icon-picker.md` + Wave B `token-mapping.md`):
    /// - panel bg ← `surface_expanded` (0.82α dark glass)
    /// - chip / slot bg ← `surface_subtle` (Wave A: `--surface-subtle` for
    ///   search input + cell idle)
    /// - accent (selected ring, active tab) ← `accent_blue` (#3B82F6)
    /// - title + body text ← `text_primary` (#F0F0F5)
    /// - muted (empty state, hint, secondary tab labels) ← `text_muted`
    /// - warning (no-target hint) ← `accent_orange`
    /// - radii: panel = `expanded` (16), chip/slot = `card` (10)
    /// - shadow ← `expanded` (outer)
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            chip_radius: BorderRadius::all(radius.card),
            chip_inner_radius: BorderRadius::all(radius.card),
            slot_radius: BorderRadius::all(radius.card),
            slot_inner_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            chip_background: palette.surface_subtle,
            accent_color: palette.accent_blue,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
            warning_color: palette.accent_orange,
        }
    }
}

/// Icon source taxonomy — matches the 1.x `GridItem` discriminator.
///
/// `Lucide` and `Builtin` carry a name slug; `Custom` carries the `uuid`
/// allocated by `bento-nano-backend::custom_icons::upload_custom_icon`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IconKind {
    /// Named entry from the bundled Lucide icon set
    /// (`lucide:<name>` wire format).
    Lucide(SmolStr),
    /// Built-in `ZONE_ICON_NAMES` slug (no prefix in 1.x wire format).
    Builtin(SmolStr),
    /// User-uploaded custom icon (`custom:<uuid>` wire format).
    Custom(SmolStr),
}

impl IconKind {
    /// Wire-format string that downstream consumers (`onSelect` callback,
    /// dispatcher Command payload) carry. Identical to the 1.x
    /// `iconKey()` helper output so existing settings.json values match.
    pub fn to_wire_key(&self) -> SmolStr {
        match self {
            Self::Lucide(name) => SmolStr::new(format!("lucide:{name}")),
            Self::Builtin(name) => name.clone(),
            Self::Custom(uuid) => SmolStr::new(format!("custom:{uuid}")),
        }
    }
}

/// Tab category names — order matches the 1.x `CATEGORIES` array so the
/// horizontal tab strip ports verbatim. "all" is the default selection.
pub const CATEGORIES: &[&str] = &[
    "all", "builtin", "custom", "work", "creative", "dev", "media", "finance", "health", "files",
    "arrows", "system", "charts", "security", "general",
];

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::tokens as style_tokens;

    #[test]
    fn icon_picker_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        let chrome = IconPickerChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(
            chrome.panel_background,
            style_tokens::PALETTE_DARK.surface_expanded
        );
        assert_eq!(
            chrome.chip_background,
            style_tokens::PALETTE_DARK.surface_subtle
        );
        assert_eq!(chrome.accent_color, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.body_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(
            chrome.warning_color,
            style_tokens::PALETTE_DARK.accent_orange
        );
        assert_eq!(
            chrome.panel_radius,
            BorderRadius::all(style_tokens::RADIUS.expanded)
        );
        assert_eq!(
            chrome.chip_radius,
            BorderRadius::all(style_tokens::RADIUS.card)
        );
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }

    #[test]
    fn lucide_wire_key_uses_lucide_prefix() {
        let kind = IconKind::Lucide(SmolStr::new_static("settings"));
        assert_eq!(kind.to_wire_key().as_str(), "lucide:settings");
    }

    #[test]
    fn builtin_wire_key_uses_bare_name() {
        let kind = IconKind::Builtin(SmolStr::new_static("folder"));
        assert_eq!(kind.to_wire_key().as_str(), "folder");
    }

    #[test]
    fn custom_wire_key_uses_custom_prefix() {
        let kind = IconKind::Custom(SmolStr::new_static("abc-123"));
        assert_eq!(kind.to_wire_key().as_str(), "custom:abc-123");
    }

    #[test]
    fn categories_starts_with_all_for_default_selection() {
        // Snap.md mandates "all" as the open-state tab; pinning here so a
        // future re-order of the array doesn't silently change the default.
        assert_eq!(CATEGORIES.first().copied(), Some("all"));
        // Total of 15 categories — matches the 1.x list length.
        assert_eq!(CATEGORIES.len(), 15);
    }

    #[test]
    fn visible_cap_matches_one_x_baseline() {
        // 1.x VISIBLE_CAP = 200 — the trigger for the "Refine your search"
        // hint. Pin so a refactor doesn't silently widen the cap.
        assert_eq!(VISIBLE_CAP, 200);
    }

    #[test]
    fn chrome_accepts_explicit_active_palette() {
        let palette = PaletteTokens {
            bg: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
            surface: Color::from_u8(0x22, 0x33, 0x44, 0xDD),
            surface_alt: Color::from_u8(0x11, 0x22, 0x33, 0xEE),
            border: Color::from_u8(0x01, 0x01, 0x01, 0xFF),
            text: Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF),
            text_muted: Color::from_u8(0x88, 0x99, 0xAA, 0xFF),
            accent: Color::from_u8(0x44, 0xAA, 0xEE, 0xFF),
            accent_hover: Color::from_u8(0x55, 0xBB, 0xFF, 0xFF),
            danger: Color::from_u8(0xCC, 0x44, 0x44, 0xFF),
            success: Color::from_u8(0x44, 0xCC, 0x66, 0xFF),
            warning: Color::from_u8(0xCC, 0x99, 0x44, 0xFF),
            info: Color::from_u8(0x44, 0x88, 0xCC, 0xFF),
            scrim: Color::from_u8(0x00, 0x00, 0x00, 0x80),
            hover_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x14),
            active_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x29),
            selection: Color::from_u8(0x44, 0x55, 0x66, 0xCC),
        };

        let chrome = IconPickerChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.chip_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(chrome.accent_color, Color::from_u8(0x44, 0xAA, 0xEE, 0xFF));
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
        assert_eq!(chrome.warning_color, Color::from_u8(0xCC, 0x99, 0x44, 0xFF));
    }

    #[test]
    fn chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = PaletteTokens {
            bg: Color::from_u8(0x00, 0x00, 0x00, 0xFF),
            surface: Color::from_u8(0x22, 0x33, 0x44, 0xDD),
            surface_alt: Color::from_u8(0x11, 0x22, 0x33, 0xEE),
            border: Color::from_u8(0x01, 0x01, 0x01, 0xFF),
            text: Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF),
            text_muted: Color::from_u8(0x88, 0x99, 0xAA, 0xFF),
            accent: Color::from_u8(0x44, 0xAA, 0xEE, 0xFF),
            accent_hover: Color::from_u8(0x55, 0xBB, 0xFF, 0xFF),
            danger: Color::from_u8(0xCC, 0x44, 0x44, 0xFF),
            success: Color::from_u8(0x44, 0xCC, 0x66, 0xFF),
            warning: Color::from_u8(0xCC, 0x99, 0x44, 0xFF),
            info: Color::from_u8(0x44, 0x88, 0xCC, 0xFF),
            scrim: Color::from_u8(0x00, 0x00, 0x00, 0x80),
            hover_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x14),
            active_overlay: Color::from_u8(0xFF, 0xFF, 0xFF, 0x29),
            selection: Color::from_u8(0x44, 0x55, 0x66, 0xCC),
        };
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let mut shadow = shadow::DEFAULT;
        shadow.md = Shadow {
            offset_x: 2.0,
            offset_y: 5.0,
            blur: 13.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
        };

        let chrome = IconPickerChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.chip_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.chip_inner_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.slot_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.slot_inner_radius, BorderRadius::all(7.0));
    }

    #[test]
    fn build_returns_picker_window_sized_container() {
        use bento_nano_layout::LayoutSource;
        use bento_nano_style::Length;
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - WINDOW_WIDTH).abs() < 0.01));
        assert!(matches!(layout.height, Length::Px(h) if (h - WINDOW_HEIGHT).abs() < 0.01));
    }
}
