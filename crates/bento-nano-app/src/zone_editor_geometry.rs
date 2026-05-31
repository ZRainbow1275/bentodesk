//! Shared geometry for the selected-stack ZoneEditor auxiliary window.
//!
//! The D2D renderer and shell pointer producer use these same rectangles so
//! direct pointer controls stay aligned with the visible editor rows.

use bento_nano_style::{BorderRadius, Color, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};

/// ZoneEditor colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneEditorChrome {
    /// Drop shadow descriptor drawn behind the editor panel.
    pub panel_shadow: Shadow,
    /// Main panel radius.
    pub panel_radius: BorderRadius,
    /// Focus/input outer radius.
    pub input_radius: BorderRadius,
    /// Focus/input inner radius.
    pub input_inner_radius: BorderRadius,
    /// Editable row radius.
    pub row_radius: BorderRadius,
    /// Accent swatch radius.
    pub swatch_radius: BorderRadius,
    /// Accent swatch inner radius.
    pub swatch_inner_radius: BorderRadius,
    /// Save/Cancel button radius.
    pub button_radius: BorderRadius,
    /// Panel fill colour for the editor surface.
    pub panel_background: Color,
    /// Editable row/input fill colour.
    pub input_background: Color,
    /// Active focus/accent colour for primary controls.
    pub accent_color: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
}

impl ZoneEditorChrome {
    /// Build ZoneEditor chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build ZoneEditor chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            input_radius: radius.lg,
            input_inner_radius: radius.md,
            row_radius: radius.md,
            swatch_radius: radius.full,
            swatch_inner_radius: radius.full,
            button_radius: radius.md,
            panel_background: palette.surface,
            input_background: palette.surface_alt,
            accent_color: palette.accent,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
        }
    }
}

/// Hit target inside the ZoneEditor aux HWND.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneEditorHit {
    /// Icon row opens the full IconPicker for the edited zone.
    Icon,
    /// Accent row cycles the draft accent swatch.
    Accent,
    /// Grid row cycles draft grid columns.
    GridColumns,
    /// Capsule-size row cycles draft capsule size.
    CapsuleSize,
    /// Capsule-shape row cycles draft capsule shape.
    CapsuleShape,
    /// Save button commits the current draft.
    Save,
    /// Cancel button discards the current draft.
    Cancel,
}

/// Panel rectangle shared by renderer and hit-test producers.
pub fn zone_editor_panel(viewport: Size) -> Rect {
    Rect {
        x: 16.0,
        y: 16.0,
        width: (viewport.width - 32.0).max(240.0),
        height: (viewport.height - 32.0).max(180.0),
    }
}

pub fn zone_editor_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Zone-name input rectangle.
pub fn zone_editor_name_input_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 86.0,
        width: panel.width - 36.0,
        height: 42.0,
    }
}

/// Icon row rectangle.
pub fn zone_editor_icon_rect(viewport: Size) -> Rect {
    row_rect(viewport, 142.0)
}

/// Accent row rectangle.
pub fn zone_editor_accent_rect(viewport: Size) -> Rect {
    row_rect(viewport, 178.0)
}

/// Accent swatch preview rectangle inside the accent row.
pub fn zone_editor_accent_swatch_rect(viewport: Size) -> Rect {
    let row = zone_editor_accent_rect(viewport);
    Rect {
        x: row.x + 4.0,
        y: row.y,
        width: 26.0,
        height: 26.0,
    }
}

/// Grid-column row rectangle.
pub fn zone_editor_grid_rect(viewport: Size) -> Rect {
    row_rect(viewport, 214.0)
}

/// Capsule-size row rectangle.
pub fn zone_editor_capsule_size_rect(viewport: Size) -> Rect {
    row_rect(viewport, 240.0)
}

/// Capsule-shape row rectangle.
pub fn zone_editor_capsule_shape_rect(viewport: Size) -> Rect {
    row_rect(viewport, 266.0)
}

/// Help/hint text rectangle.
pub fn zone_editor_hint_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 296.0,
        width: panel.width - 36.0,
        height: 18.0,
    }
}

/// Save button rectangle.
pub fn zone_editor_save_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    let button_width = (panel.width - 42.0) * 0.5;
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 318.0,
        width: button_width,
        height: 24.0,
    }
}

/// Cancel button rectangle.
pub fn zone_editor_cancel_rect(viewport: Size) -> Rect {
    let save = zone_editor_save_rect(viewport);
    Rect {
        x: save.x + save.width + 6.0,
        y: save.y,
        width: save.width,
        height: save.height,
    }
}

/// Hit-test the currently rendered ZoneEditor geometry.
pub fn zone_editor_hit_test(viewport: Size, x: f32, y: f32) -> Option<ZoneEditorHit> {
    if contains_point(zone_editor_icon_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Icon);
    }
    if contains_point(zone_editor_accent_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Accent);
    }
    if contains_point(zone_editor_grid_rect(viewport), x, y) {
        return Some(ZoneEditorHit::GridColumns);
    }
    if contains_point(zone_editor_capsule_size_rect(viewport), x, y) {
        return Some(ZoneEditorHit::CapsuleSize);
    }
    if contains_point(zone_editor_capsule_shape_rect(viewport), x, y) {
        return Some(ZoneEditorHit::CapsuleShape);
    }
    if contains_point(zone_editor_save_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Save);
    }
    if contains_point(zone_editor_cancel_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Cancel);
    }
    None
}

fn row_rect(viewport: Size, y: f32) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.x + 96.0,
        y: panel.y + y,
        width: panel.width - 114.0,
        height: 26.0,
    }
}

fn contains_point(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.right() && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_theme as theme;

    #[test]
    fn zone_editor_chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.accent = Color::from_u8(0x44, 0xAA, 0xEE, 0xFF);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);

        let chrome = ZoneEditorChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.input_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(chrome.accent_color, Color::from_u8(0x44, 0xAA, 0xEE, 0xFF));
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    }

    #[test]
    fn zone_editor_chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = theme::current().palette;
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

        let chrome = ZoneEditorChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.input_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.input_inner_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.row_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.swatch_radius, BorderRadius::all(999.0));
        assert_eq!(chrome.swatch_inner_radius, BorderRadius::all(999.0));
        assert_eq!(chrome.button_radius, BorderRadius::all(7.0));
    }

    #[test]
    fn zone_editor_panel_shadow_rect_uses_token_shadow_geometry() {
        let panel = Rect {
            x: 24.0,
            y: 30.0,
            width: 320.0,
            height: 180.0,
        };
        let shadow = Shadow {
            offset_x: 3.0,
            offset_y: 5.0,
            blur: 11.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
        };

        let rect = zone_editor_panel_shadow_rect(panel, shadow);

        assert_eq!(
            rect,
            Rect {
                x: 16.0,
                y: 24.0,
                width: 342.0,
                height: 202.0,
            }
        );
    }
}
