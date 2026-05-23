//! `DragPreview` — transparent topmost surface that follows the cursor while
//! a drag is in flight. The HWND lives in the platform layer
//! (`WindowKind::DragPreview`); this widget owns the cursor-offset math + the
//! current attached payload metadata so the renderer can paint the right
//! visual (e.g. ghosted item icon).

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bento_nano_theme as theme;
use bento_nano_tree::{AnimatedValue, Easing};
use smol_str::SmolStr;

pub const FADE_IN_SECS: f32 = 0.080;
pub const DEFAULT_SIZE_PX: f32 = 64.0;

/// Cursor → preview offset (DIPs). Positive offsets move the preview down /
/// right of the cursor. BentoDesk default places the preview slightly
/// above-right of the pointer so the cursor stays unobstructed.
pub const DEFAULT_OFFSET_X: f32 = 12.0;
pub const DEFAULT_OFFSET_Y: f32 = -8.0;

#[derive(Debug, Clone)]
pub struct DragPreview {
    /// Identifier of the dragged payload (e.g. zone id / item id). Caller
    /// interprets via the active drag context.
    pub payload_id: SmolStr,
    pub size: Size,
    pub offset_x: f32,
    pub offset_y: f32,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub visible: bool,
    /// Fade-in opacity 0 → 1 across `FADE_IN_SECS`.
    pub fade_anim: AnimatedValue<f32>,
    pub border_radius: BorderRadius,
    pub background: Color,
}

impl DragPreview {
    pub fn new(payload_id: impl Into<SmolStr>) -> Self {
        let p = theme::current().palette;
        Self {
            payload_id: payload_id.into(),
            size: Size {
                width: DEFAULT_SIZE_PX,
                height: DEFAULT_SIZE_PX,
            },
            offset_x: DEFAULT_OFFSET_X,
            offset_y: DEFAULT_OFFSET_Y,
            cursor_x: 0.0,
            cursor_y: 0.0,
            visible: false,
            fade_anim: AnimatedValue::new(0.0),
            border_radius: BorderRadius::all(8.0),
            background: p.surface,
        }
    }

    /// Begin showing the preview at the current cursor position.
    pub fn begin_drag(&mut self, cursor: (f32, f32)) {
        self.cursor_x = cursor.0;
        self.cursor_y = cursor.1;
        self.visible = true;
        self.fade_anim
            .animate_to(1.0, FADE_IN_SECS, Easing::EaseOut);
    }

    /// Update the cursor position; renderer translates the preview HWND via
    /// `SetWindowPos` to the new screen rect.
    pub fn move_to(&mut self, cursor: (f32, f32)) {
        self.cursor_x = cursor.0;
        self.cursor_y = cursor.1;
    }

    pub fn end_drag(&mut self) {
        self.visible = false;
        self.fade_anim
            .animate_to(0.0, FADE_IN_SECS, Easing::EaseOut);
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.fade_anim.tick(dt)
    }

    pub fn fade_progress(&self) -> f32 {
        self.fade_anim.current()
    }

    /// Current screen rect (DIPs) — caller hands to `SetWindowPos`.
    pub fn current_rect(&self) -> Rect {
        Rect {
            x: self.cursor_x + self.offset_x,
            y: self.cursor_y + self.offset_y,
            width: self.size.width,
            height: self.size.height,
        }
    }
}

impl LayoutSource for DragPreview {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: Length::Px(self.size.width),
            height: Length::Px(self.size.height),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_preview_begin_drag_sets_visible_and_starts_fade() {
        let mut d = DragPreview::new("zone:1");
        d.begin_drag((100.0, 200.0));
        assert!(d.visible);
        let _ = d.tick(FADE_IN_SECS + 0.01);
        assert!((d.fade_progress() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn drag_preview_current_rect_offsets_cursor() {
        let mut d = DragPreview::new("zone:1");
        d.move_to((100.0, 200.0));
        let r = d.current_rect();
        assert!((r.x - (100.0 + DEFAULT_OFFSET_X)).abs() < 1e-3);
        assert!((r.y - (200.0 + DEFAULT_OFFSET_Y)).abs() < 1e-3);
    }

    #[test]
    fn drag_preview_end_drag_hides_after_fade() {
        let mut d = DragPreview::new("zone:1");
        d.begin_drag((10.0, 10.0));
        let _ = d.tick(FADE_IN_SECS + 0.01);
        d.end_drag();
        assert!(!d.visible);
        let _ = d.tick(FADE_IN_SECS + 0.01);
        assert!((d.fade_progress() - 0.0).abs() < 1e-3);
    }
}
