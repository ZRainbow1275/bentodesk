//! Shared geometry for the selected-stack item-file rename auxiliary window.
//!
//! The D2D renderer and shell tooltip producer use these same rectangles so
//! hover anchors stay aligned with the visible rename form.

use bentodesk_style::{BorderRadius, Color, Rect, Shadow, Size};
use bentodesk_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};

/// Item-file rename colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemFileRenameChrome {
    /// Drop shadow descriptor drawn behind the rename panel.
    pub panel_shadow: Shadow,
    /// Main panel radius.
    pub panel_radius: BorderRadius,
    /// Input focus ring radius.
    pub input_radius: BorderRadius,
    /// Input fill radius.
    pub input_inner_radius: BorderRadius,
    /// Panel fill colour for the rename surface.
    pub panel_background: Color,
    /// Input field fill colour.
    pub input_background: Color,
    /// Active focus/accent colour around the input.
    pub accent_color: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
    /// Error/status text colour for validation failures.
    pub error_color: Color,
}

impl ItemFileRenameChrome {
    /// Build rename chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build rename chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            input_radius: radius.lg,
            input_inner_radius: radius.md,
            panel_background: palette.surface,
            input_background: palette.surface_alt,
            accent_color: palette.accent,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
            error_color: palette.danger,
        }
    }
}

/// Hover target inside the item-file rename aux HWND.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemFileRenameHit {
    /// Current effective/visible file path row.
    CurrentPath,
    /// Editable filename input field.
    Input,
    /// Status/help line below the input.
    Status,
}

/// Panel rectangle shared by renderer and hit-test producers.
pub fn item_file_rename_panel_rect(viewport: Size) -> Rect {
    let panel_width = (viewport.width - 32.0).clamp(220.0, 472.0);
    let panel_height = (viewport.height - 24.0).clamp(180.0, 206.0);
    Rect {
        x: ((viewport.width - panel_width) * 0.5).max(8.0),
        y: ((viewport.height - panel_height) * 0.5).max(8.0),
        width: panel_width,
        height: panel_height,
    }
}

pub fn item_file_rename_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Current file path rectangle.
pub fn item_file_rename_path_rect(viewport: Size) -> Rect {
    let panel = item_file_rename_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 48.0,
        width: panel.width - 36.0,
        height: 28.0,
    }
}

/// Filename input rectangle.
pub fn item_file_rename_input_rect(viewport: Size) -> Rect {
    let panel = item_file_rename_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 106.0,
        width: panel.width - 36.0,
        height: 38.0,
    }
}

/// Status/help text rectangle.
pub fn item_file_rename_status_rect(viewport: Size) -> Rect {
    let panel = item_file_rename_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 158.0,
        width: panel.width - 36.0,
        height: 24.0,
    }
}

/// Hit-test the currently rendered item-file rename geometry.
pub fn item_file_rename_hit_test(viewport: Size, x: f32, y: f32) -> Option<ItemFileRenameHit> {
    if contains_point(item_file_rename_path_rect(viewport), x, y) {
        return Some(ItemFileRenameHit::CurrentPath);
    }
    if contains_point(item_file_rename_input_rect(viewport), x, y) {
        return Some(ItemFileRenameHit::Input);
    }
    if contains_point(item_file_rename_status_rect(viewport), x, y) {
        return Some(ItemFileRenameHit::Status);
    }
    None
}

fn contains_point(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.right() && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_theme as theme;

    #[test]
    fn chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.accent = Color::from_u8(0x44, 0xAA, 0xEE, 0xFF);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

        let chrome = ItemFileRenameChrome::from_palette(palette);

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
        assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
    }

    #[test]
    fn chrome_accepts_explicit_radius_shadow_tokens() {
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

        let chrome = ItemFileRenameChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.input_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.input_inner_radius, BorderRadius::all(7.0));
    }

    #[test]
    fn panel_shadow_rect_uses_token_shadow_geometry() {
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

        let rect = item_file_rename_panel_shadow_rect(panel, shadow);

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

    #[test]
    fn hit_test_uses_shared_rectangles() {
        let viewport = Size {
            width: 480.0,
            height: 220.0,
        };

        let path = item_file_rename_path_rect(viewport);
        let input = item_file_rename_input_rect(viewport);
        let status = item_file_rename_status_rect(viewport);

        assert_eq!(
            item_file_rename_hit_test(viewport, path.x + 1.0, path.y + 1.0),
            Some(ItemFileRenameHit::CurrentPath)
        );
        assert_eq!(
            item_file_rename_hit_test(viewport, input.x + 1.0, input.y + 1.0),
            Some(ItemFileRenameHit::Input)
        );
        assert_eq!(
            item_file_rename_hit_test(viewport, status.x + 1.0, status.y + 1.0),
            Some(ItemFileRenameHit::Status)
        );
    }
}
