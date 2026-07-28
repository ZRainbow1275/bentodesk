//! Shared geometry for selected-stack picker auxiliary windows.
//!
//! Renderer painting and shell pointer hit-testing both call these helpers so
//! visible picker controls and clickable regions cannot drift.

use bentodesk_style::{Rect, Shadow, Size};

pub const ICON_SLOT_WIDTH: f32 = 56.0;
pub const ICON_SLOT_HEIGHT: f32 = 56.0;
pub const ICON_SLOT_GAP: f32 = 8.0;
pub const ICON_SLOT_MAX_COLUMNS: usize = 6;
pub const ICON_SLOT_MIN_COLUMNS: usize = 3;
pub const PICKER_CLOSE_SIZE: f32 = 32.0;

pub const PALETTE_SWATCH_SIZE: f32 = 26.0;
pub const PALETTE_SWATCH_GAP: f32 = 8.0;
pub const PALETTE_SWATCH_COLUMNS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconPickerHit {
    Icon(usize),
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalettePickerHit {
    Swatch(usize),
    Clear,
}

pub fn picker_panel(viewport: Size) -> Rect {
    Rect {
        x: 16.0,
        y: 16.0,
        width: (viewport.width - 32.0).max(260.0),
        height: (viewport.height - 32.0).max(180.0),
    }
}

pub fn picker_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

pub fn icon_picker_columns(viewport: Size) -> usize {
    let panel = picker_panel(viewport);
    let content_width = (panel.width - 36.0).max(ICON_SLOT_WIDTH);
    let raw = ((content_width + ICON_SLOT_GAP) / (ICON_SLOT_WIDTH + ICON_SLOT_GAP)).floor();
    (raw as usize).clamp(ICON_SLOT_MIN_COLUMNS, ICON_SLOT_MAX_COLUMNS)
}

pub fn icon_picker_close_rect(viewport: Size) -> Rect {
    let panel = picker_panel(viewport);
    Rect {
        x: panel.right() - 14.0 - PICKER_CLOSE_SIZE,
        y: panel.y + 10.0,
        width: PICKER_CLOSE_SIZE,
        height: PICKER_CLOSE_SIZE,
    }
}

pub fn icon_picker_selected_rect(viewport: Size) -> Rect {
    let panel = picker_panel(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 92.0,
        width: panel.width - 36.0,
        height: 44.0,
    }
}

pub fn icon_picker_slot_rect(viewport: Size, index: usize) -> Rect {
    let panel = picker_panel(viewport);
    let columns = icon_picker_columns(viewport);
    let col = index % columns;
    let row = index / columns;
    Rect {
        x: panel.x + 18.0 + col as f32 * (ICON_SLOT_WIDTH + ICON_SLOT_GAP),
        y: panel.y + 154.0 + row as f32 * (ICON_SLOT_HEIGHT + ICON_SLOT_GAP),
        width: ICON_SLOT_WIDTH,
        height: ICON_SLOT_HEIGHT,
    }
}

pub fn icon_picker_hint_rect(viewport: Size, icon_count: usize) -> Rect {
    let panel = picker_panel(viewport);
    let columns = icon_picker_columns(viewport);
    let rows = icon_count.div_ceil(columns);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 154.0 + rows as f32 * (ICON_SLOT_HEIGHT + ICON_SLOT_GAP) + 10.0,
        width: panel.width - 36.0,
        height: 24.0,
    }
}

pub fn icon_picker_hit_test(
    viewport: Size,
    x: f32,
    y: f32,
    icon_count: usize,
) -> Option<IconPickerHit> {
    if contains_point(icon_picker_close_rect(viewport), x, y) {
        return Some(IconPickerHit::Close);
    }
    for index in 0..icon_count {
        if contains_point(icon_picker_slot_rect(viewport, index), x, y) {
            return Some(IconPickerHit::Icon(index));
        }
    }
    None
}

pub fn palette_picker_swatch_rect(viewport: Size, index: usize) -> Rect {
    let panel = picker_panel(viewport);
    let col = index % PALETTE_SWATCH_COLUMNS;
    let row = index / PALETTE_SWATCH_COLUMNS;
    Rect {
        x: panel.x + 18.0 + col as f32 * (PALETTE_SWATCH_SIZE + PALETTE_SWATCH_GAP),
        y: panel.y + 92.0 + row as f32 * (PALETTE_SWATCH_SIZE + PALETTE_SWATCH_GAP),
        width: PALETTE_SWATCH_SIZE,
        height: PALETTE_SWATCH_SIZE,
    }
}

pub fn palette_picker_clear_rect(viewport: Size) -> Rect {
    let panel = picker_panel(viewport);
    Rect {
        x: panel.x + 164.0,
        y: panel.y + 92.0,
        width: (panel.width - 182.0).max(78.0),
        height: 30.0,
    }
}

pub fn palette_picker_value_rect(viewport: Size) -> Rect {
    let clear = palette_picker_clear_rect(viewport);
    Rect {
        x: clear.x,
        y: clear.y + 38.0,
        width: clear.width,
        height: 22.0,
    }
}

pub fn palette_picker_hint_rect(viewport: Size) -> Rect {
    let panel = picker_panel(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 194.0,
        width: panel.width - 36.0,
        height: 24.0,
    }
}

pub fn palette_picker_hit_test(
    viewport: Size,
    x: f32,
    y: f32,
    swatch_count: usize,
) -> Option<PalettePickerHit> {
    for index in 0..swatch_count {
        if contains_point(palette_picker_swatch_rect(viewport, index), x, y) {
            return Some(PalettePickerHit::Swatch(index));
        }
    }
    if contains_point(palette_picker_clear_rect(viewport), x, y) {
        return Some(PalettePickerHit::Clear);
    }
    None
}

fn contains_point(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.right() && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_style::{Color, Shadow};

    #[test]
    fn picker_panel_shadow_rect_uses_token_shadow_geometry() {
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

        let rect = picker_panel_shadow_rect(panel, shadow);

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
    fn compact_icon_picker_uses_six_columns_and_a_real_close_target() {
        let viewport = Size {
            width: 480.0,
            height: 640.0,
        };
        assert_eq!(icon_picker_columns(viewport), 6);
        let first = icon_picker_slot_rect(viewport, 0);
        let seventh = icon_picker_slot_rect(viewport, 6);
        assert_eq!(first.x, seventh.x);
        assert!(seventh.y > first.y);

        let close = icon_picker_close_rect(viewport);
        assert_eq!(
            icon_picker_hit_test(
                viewport,
                close.x + close.width * 0.5,
                close.y + close.height * 0.5,
                30,
            ),
            Some(IconPickerHit::Close)
        );
    }
}
