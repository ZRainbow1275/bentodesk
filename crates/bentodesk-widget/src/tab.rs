//! `Tab` — header strip with animated underline + content swap. Active index
//! is reactive (`Signal<u32>`) so observers can mark themselves dirty when
//! the tab changes.
//!
//! Underline travel uses an `AnimatedValue<f32>` for the x-offset; the
//! renderer reads `current_underline_x()` per frame and lerps.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing, Signal};
use smallvec::SmallVec;
use smol_str::SmolStr;

pub const UNDERLINE_DURATION_SECS: f32 = 0.200;
pub const HEADER_HEIGHT_PX: f32 = 36.0;
pub const UNDERLINE_THICKNESS_PX: f32 = 2.0;

#[derive(Debug, Clone)]
pub struct TabItem {
    pub label: SmolStr,
    /// Width of this header item — caller measures via DWrite + padding;
    /// supplied here so the underline-x math can pre-sum offsets.
    pub width: f32,
}

impl TabItem {
    pub fn new(label: impl Into<SmolStr>, width: f32) -> Self {
        Self {
            label: label.into(),
            width,
        }
    }
}

#[derive(Debug)]
pub struct Tab {
    pub items: SmallVec<[TabItem; 8]>,
    /// Reactive active index — observers (the panel that swaps content) can
    /// subscribe via `Signal::is_dirty()`.
    pub active: Signal<u32>,
    /// Animated x-offset for the underline (DIPs from header origin).
    pub underline_anim: AnimatedValue<f32>,
    pub on_change_event: u32,
    pub padding: Edges,
    pub gap: f32,
    pub header_color: Color,
    pub header_color_active: Color,
    pub underline_color: Color,
    pub underline_radius: BorderRadius,
}

impl Tab {
    pub fn new(items: impl IntoIterator<Item = TabItem>, on_change_event: u32) -> Self {
        let p = theme::current().palette;
        let items: SmallVec<[TabItem; 8]> = items.into_iter().collect();
        Self {
            items,
            active: Signal::new(0),
            underline_anim: AnimatedValue::new(0.0),
            on_change_event,
            padding: Edges::xy(12.0, 6.0),
            gap: 4.0,
            header_color: p.text_muted,
            header_color_active: p.text,
            underline_color: p.accent,
            underline_radius: BorderRadius::all(UNDERLINE_THICKNESS_PX * 0.5),
        }
    }

    pub fn active_index(&self) -> u32 {
        *self.active.get()
    }

    /// Switch to `index` if in range; starts the underline tween. Returns
    /// `true` when the active index actually changed.
    pub fn set_active(&mut self, index: u32) -> bool {
        if index >= self.items.len() as u32 || index == self.active_index() {
            return false;
        }
        let _ = self.active.set(index);
        let target_x = self.item_origin_x(index);
        self.underline_anim
            .animate_to(target_x, UNDERLINE_DURATION_SECS, Easing::EaseOut);
        true
    }

    /// X-offset (from header origin) of the leading edge of `index`.
    fn item_origin_x(&self, index: u32) -> f32 {
        let mut x = 0.0_f32;
        for (i, item) in self.items.iter().enumerate() {
            if i as u32 == index {
                break;
            }
            x += item.width + self.gap;
        }
        x
    }

    /// Width of the active header — used by the renderer to size the underline
    /// to match the active label.
    pub fn active_underline_width(&self) -> f32 {
        let i = self.active_index() as usize;
        self.items.get(i).map(|t| t.width).unwrap_or(0.0)
    }

    pub fn current_underline_x(&self) -> f32 {
        self.underline_anim.current()
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.underline_anim.tick(dt)
    }

    pub fn emit<F: FnMut(u32, u32)>(&self, mut sink: F) -> bool {
        if self.on_change_event == 0 {
            return false;
        }
        sink(self.on_change_event, self.active_index());
        true
    }
}

impl LayoutSource for Tab {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Auto,
            height: Length::Px(HEADER_HEIGHT_PX),
            padding: Edges::ZERO,
            gap: self.gap,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_tabs() -> Tab {
        Tab::new(
            [
                TabItem::new("General", 80.0),
                TabItem::new("Theme", 70.0),
                TabItem::new("Hotkeys", 90.0),
            ],
            0,
        )
    }

    #[test]
    fn tab_starts_at_zero_active() {
        let t = three_tabs();
        assert_eq!(t.active_index(), 0);
        assert!((t.current_underline_x() - 0.0).abs() < 1e-3);
        assert!((t.active_underline_width() - 80.0).abs() < 1e-3);
    }

    #[test]
    fn tab_set_active_advances_underline_after_anim() {
        let mut t = three_tabs();
        let changed = t.set_active(1);
        assert!(changed);
        let _ = t.tick(UNDERLINE_DURATION_SECS + 0.01);
        // Item 0 width 80 + gap 4 = 84.
        assert!((t.current_underline_x() - 84.0).abs() < 1e-3);
        assert!((t.active_underline_width() - 70.0).abs() < 1e-3);
    }

    #[test]
    fn tab_set_active_out_of_range_is_noop() {
        let mut t = three_tabs();
        let changed = t.set_active(99);
        assert!(!changed);
        assert_eq!(t.active_index(), 0);
    }

    #[test]
    fn tab_set_active_to_current_is_noop() {
        let mut t = three_tabs();
        t.active.clear_dirty();
        let changed = t.set_active(0);
        assert!(!changed);
        assert!(!t.active.is_dirty());
    }
}
