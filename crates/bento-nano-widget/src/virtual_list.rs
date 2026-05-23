//! `VirtualList` — windowed vertical list. Renders only the items whose row
//! intersects the viewport; recycles row NodeIds across scrolls (TanStack-
//! style). Caller supplies a `total_count` (the model's full size) and a
//! `row_height` (uniform); the widget computes `visible_range()` from the
//! current scroll offset.
//!
//! Spec §10: row height is uniform here so we keep the math O(1). Variable-
//! height lists belong in a future variant — measure-first row-cache adds
//! ~150 LOC and is not in T-028's budget.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Edges, Length};
use bento_nano_tree::Signal;

#[derive(Debug)]
pub struct VirtualList {
    pub total_count: u32,
    pub row_height: f32,
    pub viewport_height: f32,
    pub viewport_width: f32,
    /// Reactive scroll offset (DIPs from top). Caller writes via wheel events;
    /// observers (the scrollbar widget, if any) subscribe to dirty.
    pub scroll_offset: Signal<f32>,
    pub padding: Edges,
    /// Number of off-screen rows to materialise above + below the visible
    /// window. Smooths fast scrolls at minor LOC cost.
    pub overscan: u32,
}

impl VirtualList {
    pub fn new(total_count: u32, row_height: f32, viewport_height: f32) -> Self {
        Self {
            total_count,
            row_height: row_height.max(1.0),
            viewport_height: viewport_height.max(0.0),
            viewport_width: 0.0,
            scroll_offset: Signal::new(0.0),
            padding: Edges::ZERO,
            overscan: 4,
        }
    }

    pub fn content_height(&self) -> f32 {
        self.row_height * self.total_count as f32
    }

    pub fn max_offset(&self) -> f32 {
        (self.content_height() - self.viewport_height).max(0.0)
    }

    /// Apply a wheel delta (positive = scroll down). Clamps to
    /// `[0, max_offset()]`. Returns the new offset.
    pub fn scroll_by(&mut self, delta: f32) -> f32 {
        let next = (*self.scroll_offset.get() + delta).clamp(0.0, self.max_offset());
        let _ = self.scroll_offset.set(next);
        next
    }

    /// Direct-set offset (clamped). Returns true when the value changed.
    pub fn set_offset(&mut self, offset: f32) -> bool {
        let clamped = offset.clamp(0.0, self.max_offset());
        self.scroll_offset.set(clamped)
    }

    /// `[start_index, end_index_exclusive)` over the model. Caller materialises
    /// only these row payloads.
    pub fn visible_range(&self) -> (u32, u32) {
        if self.total_count == 0 || self.row_height <= 0.0 {
            return (0, 0);
        }
        let offset = (*self.scroll_offset.get()).max(0.0);
        let offset_rows = (offset / self.row_height).floor() as i64;
        let visible_rows = (self.viewport_height / self.row_height).ceil() as i64;
        let overscan = self.overscan as i64;
        // Window size stays `visible_rows + 2*overscan` — overscan that would
        // sit above the model top spills below the visible window.
        let start = (offset_rows - overscan).max(0) as u32;
        let last_excl = start as i64 + visible_rows + 2 * overscan;
        let end = (last_excl.max(0) as u32).min(self.total_count);
        (start, end)
    }

    /// Y-offset (DIPs from the list's content origin) of the first visible row.
    /// Renderer translates the row subtree by this much so the recycled rows
    /// appear at the right spot inside the viewport.
    pub fn first_row_y(&self) -> f32 {
        let (start, _) = self.visible_range();
        start as f32 * self.row_height
    }
}

impl LayoutSource for VirtualList {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: if self.viewport_width > 0.0 {
                Length::Px(self.viewport_width)
            } else {
                Length::Auto
            },
            height: Length::Px(self.viewport_height),
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_list_visible_range_at_top_has_overscan_below() {
        let v = VirtualList::new(1000, 24.0, 240.0);
        // viewport rows = ceil(240/24) = 10, overscan 4.
        let (s, e) = v.visible_range();
        assert_eq!(s, 0);
        assert_eq!(e, 18); // 10 + 2*4 = 18 (clamped to total).
    }

    #[test]
    fn virtual_list_visible_range_after_scroll_advances() {
        let mut v = VirtualList::new(1000, 24.0, 240.0);
        v.set_offset(120.0); // = 5 rows
        let (s, e) = v.visible_range();
        // first = floor(5) - overscan = 1.
        assert_eq!(s, 1);
        // visible_rows = 10, last = 1 + 10 + 8 = 19.
        assert_eq!(e, 19);
    }

    #[test]
    fn virtual_list_max_offset_zero_when_content_fits() {
        let v = VirtualList::new(5, 24.0, 240.0);
        assert!((v.max_offset() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn virtual_list_first_row_y_aligned_to_row_grid() {
        let mut v = VirtualList::new(1000, 24.0, 240.0);
        v.set_offset(125.0);
        // visible_range start = 1 → first_row_y = 24.
        assert!((v.first_row_y() - 24.0).abs() < 1e-3);
    }

    #[test]
    fn virtual_list_total_count_zero_yields_empty_range() {
        let v = VirtualList::new(0, 24.0, 240.0);
        assert_eq!(v.visible_range(), (0, 0));
    }
}
