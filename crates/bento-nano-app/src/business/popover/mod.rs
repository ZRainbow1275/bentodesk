//! `Popover` — anchored floating panel with viewport-aware placement.
//!
//! 1.x parallels: the inline `<Portal>` mounted floating chrome used by
//! `ContextMenu`, `Dropdown`, `AutoLayoutMenu`, `PalettePicker`. Unlike
//! `Tooltip`, popovers are interactive (the user clicks inside), so the
//! hosting HWND uses `WindowKind::ContextMenu` (NoActivate + Topmost +
//! NoRedirectionBitmap) rather than the click-through Tooltip class.
//!
//! Visual fidelity reference: `popover.snap.md`.
//!
//! # Placement contract
//!
//! 1.x positioning logic (excerpted from `Tooltip.tsx::position` — every
//! popover-style component re-implements the same flip-on-overflow rule):
//!   * Default placement: above the anchor, horizontally centred.
//!   * If the placement rect's `top` falls below 4 px from the screen
//!     edge, flip to below.
//!   * Clamp horizontal position to `[4, viewport.width - tip.width - 4]`.
//!
//! 2.0 mirror: [`Popover::resolve_placement`] produces an `(x, y, placement)`
//! triple from an anchor rect + content size + viewport size, faithfully
//! reproducing the 1.x flip + clamp behaviour. The result is fed to the
//! shell when it positions the `WindowKind::ContextMenu` HWND via
//! `SetWindowPos`.
//!
//! No interactive state lives here — popover content is a tree of widgets
//! the caller composes inside the host HWND. The popover descriptor is the
//! chrome (background + border + radius + padding).

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, SpacingTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bento_nano_theme as theme;

/// Where the popover sits relative to its anchor. `Above` is the 1.x
/// default; `Below` is the auto-flip fallback when the anchor is too
/// close to the top of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverPlacement {
    /// Popover floats above the anchor (default).
    Above,
    /// Popover floats below the anchor (1.x flip fallback).
    Below,
}

impl PopoverPlacement {
    /// Inverse — used by [`Popover::resolve_placement`] when the requested
    /// placement doesn't fit and the auto-flip kicks in.
    pub const fn flipped(self) -> Self {
        match self {
            Self::Above => Self::Below,
            Self::Below => Self::Above,
        }
    }
}

/// Floating-panel descriptor. The visual chrome of a context menu /
/// dropdown / popover. Children are appended via the tree (matching
/// `BentoCard` / `Toolbar`); this struct is the panel chrome only.
#[derive(Debug, Clone, Copy)]
pub struct Popover {
    /// Background fill — `palette.surface_alt` for the elevated panel feel.
    pub background: Color,
    /// Hairline border colour — `palette.border`.
    pub border_color: Color,
    /// Border thickness in px (1.x `1px` per `ContextMenu.css`).
    pub border_thickness: f32,
    /// Border radius — `8 px` matches 1.x context-menu chrome.
    pub border_radius: BorderRadius,
    /// Inset padding — `4 px` so menu items get a tight feel.
    pub padding: Edges,
    /// Default content width. `Length::Auto` lets the layout engine size
    /// to the longest menu item; callers needing a fixed-width dropdown
    /// pass `Length::Px(N)`.
    pub width: Length,
    /// Default content height — `Length::Auto`.
    pub height: Length,
    /// Margin between the popover and its anchor (px). 1.x uses 8 px so
    /// the panel doesn't touch the trigger button.
    pub anchor_margin: f32,
    /// Default placement preference. Auto-flip applies when the requested
    /// side doesn't fit.
    pub preferred_placement: PopoverPlacement,
}

impl Popover {
    /// Default chrome — `palette.surface_alt` background, hairline border,
    /// 8 px radius, 4 px padding, 8 px anchor margin, prefer-above.
    pub fn new() -> Self {
        let palette = theme::current().palette;
        Self {
            background: palette.surface_alt,
            border_color: palette.border,
            border_thickness: 1.0,
            border_radius: BorderRadius::all(8.0),
            padding: Edges::all(4.0),
            width: Length::Auto,
            height: Length::Auto,
            anchor_margin: 8.0,
            preferred_placement: PopoverPlacement::Above,
        }
    }

    /// Construct from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `chrome.md` popover metrics — aliased to
    /// `surface_expanded` family per Wave B coverage report):
    /// - background ← `surface_expanded`
    /// - border colour ← `border_expanded`
    /// - radius ← `expanded` (16 px per Wave A chrome.md, not the legacy 8 px)
    /// - padding ← 14 px vertical / 16 px horizontal (Wave A literals)
    /// - anchor margin ← 8 px (`SPACING.sm`)
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        spacing: SpacingTauri,
    ) -> Self {
        Self {
            background: palette.surface_expanded,
            border_color: palette.border_expanded,
            border_thickness: 1.0,
            border_radius: BorderRadius::all(radius.expanded),
            padding: Edges {
                top: spacing.s14,
                right: spacing.lg,
                bottom: spacing.s14,
                left: spacing.lg,
            },
            width: Length::Auto,
            height: Length::Auto,
            anchor_margin: spacing.sm,
            preferred_placement: PopoverPlacement::Above,
        }
    }

    /// Compute the popover's top-left + final placement from an anchor
    /// rect, content size, and viewport size. Pure function — no
    /// allocation, deterministic, easily unit-tested.
    ///
    /// Algorithm (faithful 1.x port):
    ///   1. Try `preferred_placement` — compute candidate top.
    ///   2. If the candidate would clip the top edge (< 4 px), flip.
    ///   3. Compute candidate left = anchor.center_x - content.width/2.
    ///   4. Clamp left to `[4, viewport.width - content.width - 4]`.
    ///   5. Return `(left, top, placement_used)`.
    pub fn resolve_placement(
        &self,
        anchor: Rect,
        content_size: Size,
        viewport: Size,
    ) -> (f32, f32, PopoverPlacement) {
        const EDGE_MARGIN: f32 = 4.0;
        let prefer_above = matches!(self.preferred_placement, PopoverPlacement::Above);
        let above_top = anchor.y - content_size.height - self.anchor_margin;
        let below_top = anchor.y + anchor.height + self.anchor_margin;

        let (top, used) = if prefer_above {
            if above_top >= EDGE_MARGIN {
                (above_top, PopoverPlacement::Above)
            } else {
                (below_top, PopoverPlacement::Below)
            }
        } else if below_top + content_size.height <= viewport.height - EDGE_MARGIN {
            (below_top, PopoverPlacement::Below)
        } else {
            (above_top, PopoverPlacement::Above)
        };

        let raw_left = anchor.x + (anchor.width - content_size.width) * 0.5;
        let max_left = (viewport.width - content_size.width - EDGE_MARGIN).max(EDGE_MARGIN);
        let left = raw_left.clamp(EDGE_MARGIN, max_left);

        (left, top, used)
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutSource for Popover {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column matches the typical menu-list orientation.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor_at(x: f32, y: f32) -> Rect {
        Rect {
            x,
            y,
            width: 80.0,
            height: 24.0,
        }
    }
    fn vp_1024_768() -> Size {
        Size {
            width: 1024.0,
            height: 768.0,
        }
    }

    #[test]
    fn popover_default_chrome_uses_palette_tokens() {
        let p = Popover::new();
        let palette = theme::current().palette;
        assert_eq!(p.background, palette.surface_alt);
        assert_eq!(p.border_color, palette.border);
        assert_eq!(p.border_radius.top_left, 8.0);
        assert_eq!(p.preferred_placement, PopoverPlacement::Above);
    }

    #[test]
    fn popover_above_with_room_picks_above() {
        let p = Popover::new();
        let anchor = anchor_at(400.0, 400.0); // mid-screen
        let content = Size {
            width: 200.0,
            height: 100.0,
        };
        let (x, y, place) = p.resolve_placement(anchor, content, vp_1024_768());
        assert_eq!(place, PopoverPlacement::Above);
        // anchor.y = 400, content.h = 100, anchor_margin = 8 → 400 - 108 = 292
        assert!((y - 292.0).abs() < 1e-6);
        // anchor.center_x = 440, content.w/2 = 100 → 340
        assert!((x - 340.0).abs() < 1e-6);
    }

    #[test]
    fn popover_above_no_room_flips_below() {
        let p = Popover::new();
        let anchor = anchor_at(400.0, 0.0); // anchor at top of screen
        let content = Size {
            width: 200.0,
            height: 100.0,
        };
        let (_x, y, place) = p.resolve_placement(anchor, content, vp_1024_768());
        // 0 - 100 - 8 = -108 → flips below: 0 + 24 + 8 = 32
        assert_eq!(place, PopoverPlacement::Below);
        assert!((y - 32.0).abs() < 1e-6);
    }

    #[test]
    fn popover_clamps_horizontal_to_viewport() {
        let p = Popover::new();
        // Anchor near right edge: x=950, w=80 → centre = 990, content width 200
        // Raw left = 990 - 100 = 890; max_left = 1024 - 200 - 4 = 820.
        let anchor = anchor_at(950.0, 400.0);
        let content = Size {
            width: 200.0,
            height: 100.0,
        };
        let (x, _y, _place) = p.resolve_placement(anchor, content, vp_1024_768());
        assert!((x - 820.0).abs() < 1e-6, "expected clamped to 820, got {x}");
    }

    #[test]
    fn popover_clamps_horizontal_to_left_margin() {
        let p = Popover::new();
        // Anchor near left edge: x=0 → raw left = -100; clamp to 4.
        let anchor = anchor_at(0.0, 400.0);
        let content = Size {
            width: 200.0,
            height: 100.0,
        };
        let (x, _y, _place) = p.resolve_placement(anchor, content, vp_1024_768());
        assert!((x - 4.0).abs() < 1e-6);
    }

    #[test]
    fn popover_placement_flipped_inverts() {
        assert_eq!(PopoverPlacement::Above.flipped(), PopoverPlacement::Below);
        assert_eq!(PopoverPlacement::Below.flipped(), PopoverPlacement::Above);
    }

    #[test]
    fn popover_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let p = Popover::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SPACING,
        );
        assert_eq!(p.background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(p.border_color, style_tokens::PALETTE_DARK.border_expanded);
        assert_eq!(p.border_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        // Wave A chrome.md: 14 px vertical, 16 px horizontal.
        assert_eq!(p.padding.top, style_tokens::SPACING.s14);
        assert_eq!(p.padding.left, style_tokens::SPACING.lg);
        assert_eq!(p.anchor_margin, style_tokens::SPACING.sm);
    }
}
