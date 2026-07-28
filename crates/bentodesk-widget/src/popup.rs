//! `Popup` — primitive positioned floating surface with anchor + flip placement.
//!
//! Spec §3.2: this widget is the placement-math layer; the actual HWND lives
//! in `bentodesk-platform::window` (`WindowKind::Tooltip` / `Popup`-style).
//! The widget computes the desired logical rect given an anchor rect + a
//! preferred placement; the runtime opens / hibernates the HWND via T-099 and
//! sets its position via `SetWindowPos`.
//!
//! Spec §10: `Copy` everywhere — anchor rect, placement enum, content size
//! are all small POD. No allocation in the placement-resolution hot path.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::tokens as style_tokens;
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bentodesk_theme as theme;

/// Anchor rectangle in logical (DIP) screen coordinates. The popup positions
/// itself relative to this rect; typically supplied by the caller from the
/// triggering widget's layout result.
pub type PopupAnchor = Rect;

/// Side of the anchor the popup opens against. The renderer flips the
/// preference when there isn't enough room (e.g. `Bottom` becomes `Top` near
/// the screen's bottom edge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    Bottom,
    Top,
    Right,
    Left,
}

impl PopupPlacement {
    /// Opposite-side companion for the flip-on-overflow case.
    pub fn flipped(self) -> Self {
        match self {
            PopupPlacement::Bottom => PopupPlacement::Top,
            PopupPlacement::Top => PopupPlacement::Bottom,
            PopupPlacement::Right => PopupPlacement::Left,
            PopupPlacement::Left => PopupPlacement::Right,
        }
    }
}

/// 4 px gap between anchor and popup body — matches the React design system's
/// `--popover-gap` token.
pub const POPUP_GAP_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct Popup {
    pub anchor: PopupAnchor,
    pub placement: PopupPlacement,
    /// Desired content size. The runtime opens an HWND of this logical size.
    pub content_size: Size,
    /// Visible flag — false = hidden / hibernated swap chain (T-099).
    pub visible: bool,
    pub padding: Edges,
    pub background: Color,
    pub border_color: Color,
    pub border_radius: BorderRadius,
    pub shadow: Shadow,
}

impl Popup {
    pub fn new(content_size: Size) -> Self {
        let p = theme::current().palette;
        Self {
            anchor: Rect::ZERO,
            placement: PopupPlacement::Bottom,
            content_size,
            visible: false,
            padding: Edges::all(8.0),
            background: p.surface,
            border_color: p.border,
            border_radius: BorderRadius::all(6.0),
            // Wave B migration: `bentodesk_style::tokens::SHADOW.ink_popup`
            // (offset (0,4), blur 16, color 0x00000066). Byte-identical to the
            // pre-migration literal — `tokens::tests::shadow_ink_popup_*` guards parity.
            shadow: style_tokens::SHADOW.ink_popup,
        }
    }

    /// Compute the popup's screen rect against `screen` (the available screen
    /// space in logical pixels) using `placement`, flipping when the
    /// preferred side has insufficient room. Returns `(resolved_rect,
    /// effective_placement)` so the renderer can draw the arrow on the
    /// correct edge.
    pub fn resolve_rect(&self, screen: Size) -> (Rect, PopupPlacement) {
        let p = self.placement;
        if let Some(r) = self.try_place(p, screen) {
            return (r, p);
        }
        let flipped = p.flipped();
        if let Some(r) = self.try_place(flipped, screen) {
            return (r, flipped);
        }
        // Both sides overflow — clamp to screen bounds at preferred side.
        (self.clamped_rect(p, screen), p)
    }

    fn try_place(&self, p: PopupPlacement, screen: Size) -> Option<Rect> {
        let r = self.placed_rect(p);
        if r.x >= 0.0 && r.y >= 0.0 && r.right() <= screen.width && r.bottom() <= screen.height {
            Some(r)
        } else {
            None
        }
    }

    fn placed_rect(&self, p: PopupPlacement) -> Rect {
        let cs = self.content_size;
        let ax = self.anchor.x;
        let ay = self.anchor.y;
        let aw = self.anchor.width;
        let ah = self.anchor.height;
        match p {
            PopupPlacement::Bottom => Rect {
                x: ax + (aw - cs.width) * 0.5,
                y: ay + ah + POPUP_GAP_PX,
                width: cs.width,
                height: cs.height,
            },
            PopupPlacement::Top => Rect {
                x: ax + (aw - cs.width) * 0.5,
                y: ay - cs.height - POPUP_GAP_PX,
                width: cs.width,
                height: cs.height,
            },
            PopupPlacement::Right => Rect {
                x: ax + aw + POPUP_GAP_PX,
                y: ay + (ah - cs.height) * 0.5,
                width: cs.width,
                height: cs.height,
            },
            PopupPlacement::Left => Rect {
                x: ax - cs.width - POPUP_GAP_PX,
                y: ay + (ah - cs.height) * 0.5,
                width: cs.width,
                height: cs.height,
            },
        }
    }

    fn clamped_rect(&self, p: PopupPlacement, screen: Size) -> Rect {
        let mut r = self.placed_rect(p);
        if r.x < 0.0 {
            r.x = 0.0;
        }
        if r.y < 0.0 {
            r.y = 0.0;
        }
        if r.right() > screen.width {
            r.x = (screen.width - r.width).max(0.0);
        }
        if r.bottom() > screen.height {
            r.y = (screen.height - r.height).max(0.0);
        }
        r
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }
}

impl LayoutSource for Popup {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: Length::Px(self.content_size.width),
            height: Length::Px(self.content_size.height),
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn popup_placement_flipped_pairs() {
        assert_eq!(PopupPlacement::Top.flipped(), PopupPlacement::Bottom);
        assert_eq!(PopupPlacement::Bottom.flipped(), PopupPlacement::Top);
        assert_eq!(PopupPlacement::Left.flipped(), PopupPlacement::Right);
        assert_eq!(PopupPlacement::Right.flipped(), PopupPlacement::Left);
    }

    #[test]
    fn popup_resolves_below_anchor_with_gap() {
        let mut p = Popup::new(Size {
            width: 100.0,
            height: 50.0,
        });
        p.anchor = anchor(100.0, 100.0, 80.0, 24.0);
        p.placement = PopupPlacement::Bottom;
        let (r, eff) = p.resolve_rect(Size {
            width: 800.0,
            height: 600.0,
        });
        assert_eq!(eff, PopupPlacement::Bottom);
        // y = 100 + 24 + 4 = 128
        assert!((r.y - 128.0).abs() < 1e-3);
        // x centred: 100 + (80 - 100)/2 = 90
        assert!((r.x - 90.0).abs() < 1e-3);
    }

    #[test]
    fn popup_flips_when_below_overflow() {
        let mut p = Popup::new(Size {
            width: 100.0,
            height: 200.0,
        });
        p.anchor = anchor(100.0, 580.0, 80.0, 24.0);
        p.placement = PopupPlacement::Bottom;
        let (_, eff) = p.resolve_rect(Size {
            width: 800.0,
            height: 600.0,
        });
        assert_eq!(eff, PopupPlacement::Top);
    }

    #[test]
    fn popup_clamps_when_both_sides_overflow() {
        // Anchor near top with content too tall for either side.
        let mut p = Popup::new(Size {
            width: 100.0,
            height: 700.0,
        });
        p.anchor = anchor(100.0, 50.0, 80.0, 24.0);
        p.placement = PopupPlacement::Bottom;
        let (r, _) = p.resolve_rect(Size {
            width: 800.0,
            height: 600.0,
        });
        // Clamped — top must be 0 and we don't tolerate negative coords.
        assert!(r.y >= 0.0);
        assert!(r.x >= 0.0);
    }

    #[test]
    fn popup_default_invisible_until_shown() {
        let p = Popup::new(Size {
            width: 10.0,
            height: 10.0,
        });
        assert!(!p.visible);
        let mut p2 = p;
        p2.show();
        assert!(p2.visible);
        p2.hide();
        assert!(!p2.visible);
    }
}
