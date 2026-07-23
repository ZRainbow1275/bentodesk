//! Shared geometry for the selected-stack ZoneEditor auxiliary window.
//!
//! The D2D renderer and shell pointer producer use these same rectangles so
//! direct pointer controls stay aligned with the visible editor rows.

use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};

use crate::business::zone_editor::{
    ACCENT_PALETTE, CapsuleShapeChoice, CapsuleSizeChoice, GRID_COLUMNS_MAX, GRID_COLUMNS_MIN,
};

/// Height of the self-painted draggable editor header.
pub const ZONE_EDITOR_HEADER_HEIGHT: f32 = 52.0;
/// Square pointer target for the self-painted close glyph.
pub const ZONE_EDITOR_CLOSE_SIZE: f32 = 32.0;

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

    /// Build ZoneEditor chrome from the live Tauri-parity theme tokens.
    ///
    /// The editor is a native top-level dialog, so it uses the denser dialog
    /// surface while fields and secondary actions use the shared semantic
    /// control fill. This keeps light, frosted and dark palettes readable
    /// without hard-coded white overlays.
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        let controls = palette.control_palette();
        Self {
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            input_radius: BorderRadius::all(radius.card),
            input_inner_radius: BorderRadius::all(radius.card),
            row_radius: BorderRadius::all(radius.card),
            swatch_radius: BorderRadius::all(radius.badge),
            swatch_inner_radius: BorderRadius::all(radius.badge),
            button_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_dialog,
            input_background: controls.fill,
            accent_color: palette.accent_blue,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
        }
    }
}

/// Hit target inside the ZoneEditor aux HWND.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneEditorHit {
    /// Self-painted title-bar close button.
    Close,
    /// Canonical zone-name input.
    Name,
    /// Icon row opens the full IconPicker for the edited zone.
    Icon,
    /// Clear the draft Zone accent and fall back to the active theme.
    AccentClear,
    /// Pick one concrete built-in accent swatch without cycling.
    AccentSwatch(usize),
    /// Open the native free-form colour chooser for a custom accent.
    AccentCustom,
    /// Pick one concrete grid-column count without cycling.
    GridColumns(u32),
    /// One explicit capsule-size segment.
    CapsuleSize(CapsuleSizeChoice),
    /// One explicit capsule-shape segment.
    CapsuleShape(CapsuleShapeChoice),
    /// Save button commits the current draft.
    Save,
    /// Cancel button discards the current draft.
    Cancel,
}

/// Panel rectangle shared by renderer and hit-test producers.
pub fn zone_editor_panel(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
    }
}

/// Self-painted header band. The shell maps this band to `HTCAPTION`, except
/// for [`zone_editor_close_rect`], so the borderless native HWND remains
/// draggable without restoring an OS title bar.
pub fn zone_editor_header_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.x,
        y: panel.y,
        width: panel.width,
        height: ZONE_EDITOR_HEADER_HEIGHT,
    }
}

/// Close-button pointer target inside the self-painted header.
pub fn zone_editor_close_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.right() - 16.0 - ZONE_EDITOR_CLOSE_SIZE,
        y: panel.y + (ZONE_EDITOR_HEADER_HEIGHT - ZONE_EDITOR_CLOSE_SIZE) * 0.5,
        width: ZONE_EDITOR_CLOSE_SIZE,
        height: ZONE_EDITOR_CLOSE_SIZE,
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
        y: panel.y + 72.0,
        width: panel.width - 36.0,
        height: 36.0,
    }
}

/// Icon row rectangle.
pub fn zone_editor_icon_rect(viewport: Size) -> Rect {
    row_rect(viewport, 128.0)
}

/// Accent row rectangle.
pub fn zone_editor_accent_rect(viewport: Size) -> Rect {
    row_rect(viewport, 166.0)
}

/// Hit rectangle for a direct accent choice. Index `0` is Clear, indices
/// `1..=ACCENT_PALETTE.len()` are built-in swatches, and the final index is
/// the native custom-colour chooser.
pub fn zone_editor_accent_option_rect(viewport: Size, index: usize) -> Option<Rect> {
    segmented_option_rect(
        zone_editor_accent_rect(viewport),
        index,
        ACCENT_PALETTE.len() + 2,
    )
}

/// Compact circular visual inside the wider accent hit target.
pub fn zone_editor_accent_option_visual_rect(viewport: Size, index: usize) -> Option<Rect> {
    let hit = zone_editor_accent_option_rect(viewport, index)?;
    let size = 22.0_f32.min(hit.width).min(hit.height);
    Some(Rect {
        x: hit.x + (hit.width - size) * 0.5,
        y: hit.y + (hit.height - size) * 0.5,
        width: size,
        height: size,
    })
}

/// Grid-column row rectangle.
pub fn zone_editor_grid_rect(viewport: Size) -> Rect {
    row_rect(viewport, 204.0)
}

pub fn zone_editor_grid_option_rect(viewport: Size, columns: u32) -> Option<Rect> {
    if !(GRID_COLUMNS_MIN..=GRID_COLUMNS_MAX).contains(&columns) {
        return None;
    }
    segmented_option_rect(
        zone_editor_grid_rect(viewport),
        (columns - GRID_COLUMNS_MIN) as usize,
        (GRID_COLUMNS_MAX - GRID_COLUMNS_MIN + 1) as usize,
    )
}

/// Capsule-size row rectangle.
pub fn zone_editor_capsule_size_rect(viewport: Size) -> Rect {
    row_rect(viewport, 246.0)
}

/// Capsule-shape row rectangle.
pub fn zone_editor_capsule_shape_rect(viewport: Size) -> Rect {
    row_rect(viewport, 282.0)
}

pub fn zone_editor_capsule_size_option_rect(viewport: Size, index: usize) -> Option<Rect> {
    segmented_option_rect(
        zone_editor_capsule_size_rect(viewport),
        index,
        CapsuleSizeChoice::ALL.len(),
    )
}

pub fn zone_editor_capsule_shape_option_rect(viewport: Size, index: usize) -> Option<Rect> {
    segmented_option_rect(
        zone_editor_capsule_shape_rect(viewport),
        index,
        CapsuleShapeChoice::ALL.len(),
    )
}

/// Header preview for the selected capsule pair. Non-circle widths mirror the
/// live native size taxonomy (120/160/200) at a compact editor scale; Circle
/// remains square, so the user sees the real shape rule before saving.
pub fn zone_editor_capsule_preview_rect(
    viewport: Size,
    size: CapsuleSizeChoice,
    shape: CapsuleShapeChoice,
) -> Rect {
    let panel = zone_editor_panel(viewport);
    let height = match size {
        CapsuleSizeChoice::Small => 22.0,
        CapsuleSizeChoice::Medium => 25.0,
        CapsuleSizeChoice::Large => 28.0,
    };
    let width = if shape == CapsuleShapeChoice::Circle {
        height
    } else {
        match size {
            CapsuleSizeChoice::Small => 72.0,
            CapsuleSizeChoice::Medium => 102.0,
            CapsuleSizeChoice::Large => 132.0,
        }
    };
    let close = zone_editor_close_rect(viewport);
    Rect {
        x: close.x - 12.0 - width,
        y: panel.y + 16.0 + (28.0 - height) * 0.5,
        width,
        height,
    }
}

/// Save button rectangle.
pub fn zone_editor_save_rect(viewport: Size) -> Rect {
    let panel = zone_editor_panel(viewport);
    Rect {
        x: panel.right() - 18.0 - 96.0,
        y: panel.bottom() - 50.0,
        width: 96.0,
        height: 34.0,
    }
}

/// Cancel button rectangle.
pub fn zone_editor_cancel_rect(viewport: Size) -> Rect {
    let save = zone_editor_save_rect(viewport);
    Rect {
        x: save.x - 8.0 - 88.0,
        y: save.y,
        width: 88.0,
        height: save.height,
    }
}

/// Hit-test the currently rendered ZoneEditor geometry.
pub fn zone_editor_hit_test(viewport: Size, x: f32, y: f32) -> Option<ZoneEditorHit> {
    if contains_point(zone_editor_close_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Close);
    }
    if contains_point(zone_editor_name_input_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Name);
    }
    if contains_point(zone_editor_icon_rect(viewport), x, y) {
        return Some(ZoneEditorHit::Icon);
    }
    for index in 0..(ACCENT_PALETTE.len() + 2) {
        if zone_editor_accent_option_rect(viewport, index)
            .is_some_and(|rect| contains_point(rect, x, y))
        {
            return Some(if index == 0 {
                ZoneEditorHit::AccentClear
            } else if index <= ACCENT_PALETTE.len() {
                ZoneEditorHit::AccentSwatch(index - 1)
            } else {
                ZoneEditorHit::AccentCustom
            });
        }
    }
    for columns in GRID_COLUMNS_MIN..=GRID_COLUMNS_MAX {
        if zone_editor_grid_option_rect(viewport, columns)
            .is_some_and(|rect| contains_point(rect, x, y))
        {
            return Some(ZoneEditorHit::GridColumns(columns));
        }
    }
    for (index, size) in CapsuleSizeChoice::ALL.iter().copied().enumerate() {
        if zone_editor_capsule_size_option_rect(viewport, index)
            .is_some_and(|rect| contains_point(rect, x, y))
        {
            return Some(ZoneEditorHit::CapsuleSize(size));
        }
    }
    for (index, shape) in CapsuleShapeChoice::ALL.iter().copied().enumerate() {
        if zone_editor_capsule_shape_option_rect(viewport, index)
            .is_some_and(|rect| contains_point(rect, x, y))
        {
            return Some(ZoneEditorHit::CapsuleShape(shape));
        }
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

fn segmented_option_rect(row: Rect, index: usize, count: usize) -> Option<Rect> {
    if count == 0 || index >= count {
        return None;
    }
    let gap = 4.0;
    let width = (row.width - gap * (count.saturating_sub(1) as f32)) / count as f32;
    Some(Rect {
        x: row.x + index as f32 * (width + gap),
        y: row.y,
        width,
        height: row.height,
    })
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

    #[test]
    fn zone_editor_panel_owns_the_full_native_client_without_a_host_mask_band() {
        let viewport = Size {
            width: 480.0,
            height: 460.0,
        };
        let panel = zone_editor_panel(viewport);

        assert_eq!(
            panel,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 480.0,
                height: 460.0,
            }
        );
        for control in [
            zone_editor_close_rect(viewport),
            zone_editor_name_input_rect(viewport),
            zone_editor_icon_rect(viewport),
            zone_editor_accent_rect(viewport),
            zone_editor_grid_rect(viewport),
            zone_editor_capsule_size_rect(viewport),
            zone_editor_capsule_shape_rect(viewport),
            zone_editor_cancel_rect(viewport),
            zone_editor_save_rect(viewport),
        ] {
            assert!(control.x >= panel.x);
            assert!(control.y >= panel.y);
            assert!(control.right() <= panel.right());
            assert!(control.bottom() <= panel.bottom());
        }
    }

    #[test]
    fn zone_editor_chrome_from_tauri_tokens_uses_semantic_dialog_controls() {
        use bento_nano_style::tokens as style_tokens;

        let palette = style_tokens::PALETTE_LIGHT;
        let chrome = ZoneEditorChrome::from_tauri_tokens(
            palette,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );

        assert_eq!(chrome.panel_background, palette.surface_dialog);
        assert_eq!(chrome.input_background, palette.control_palette().fill);
        assert_eq!(chrome.accent_color, palette.accent_blue);
        assert_eq!(chrome.title_color, palette.text_primary);
        assert_eq!(
            chrome.panel_radius,
            BorderRadius::all(style_tokens::RADIUS.expanded)
        );
    }

    #[test]
    fn capsule_segments_hit_independent_size_and_shape_choices() {
        let viewport = Size {
            width: 480.0,
            height: 460.0,
        };
        let large = zone_editor_capsule_size_option_rect(viewport, 2).expect("large");
        assert_eq!(
            zone_editor_hit_test(viewport, large.x + 2.0, large.y + 2.0),
            Some(ZoneEditorHit::CapsuleSize(CapsuleSizeChoice::Large))
        );

        let minimal = zone_editor_capsule_shape_option_rect(viewport, 3).expect("minimal");
        assert_eq!(
            zone_editor_hit_test(viewport, minimal.x + 2.0, minimal.y + 2.0),
            Some(ZoneEditorHit::CapsuleShape(CapsuleShapeChoice::Minimal))
        );

        let square = zone_editor_capsule_shape_option_rect(viewport, 4).expect("square");
        assert_eq!(
            zone_editor_hit_test(viewport, square.x + 2.0, square.y + 2.0),
            Some(ZoneEditorHit::CapsuleShape(CapsuleShapeChoice::Square))
        );

        let close = zone_editor_close_rect(viewport);
        assert_eq!(
            zone_editor_hit_test(
                viewport,
                close.x + close.width * 0.5,
                close.y + close.height * 0.5
            ),
            Some(ZoneEditorHit::Close)
        );
        let header = zone_editor_header_rect(viewport);
        assert!(header.bottom() <= zone_editor_name_input_rect(viewport).y);

        let panel = zone_editor_panel(viewport);
        assert!(zone_editor_save_rect(viewport).bottom() < panel.bottom());
        assert!(zone_editor_cancel_rect(viewport).bottom() < panel.bottom());
        assert!(zone_editor_cancel_rect(viewport).right() < zone_editor_save_rect(viewport).x);
    }
}
