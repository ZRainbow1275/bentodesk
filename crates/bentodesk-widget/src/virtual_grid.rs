//! `VirtualGrid` — 2D windowed grid. Combines the row-windowing logic of
//! [`crate::virtual_list::VirtualList`] with column placement from
//! [`crate::grid::Grid`]. Cell size is uniform (matches the IconPicker layout
//! contract: equal-sized icon tiles).

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Edges, Length, Size};
use bentodesk_tree::Signal;

#[derive(Debug)]
pub struct VirtualGrid {
    pub total_count: u32,
    pub columns: u32,
    pub cell_size: Size,
    pub gap: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub scroll_offset: Signal<f32>,
    pub padding: Edges,
    pub overscan_rows: u32,
}

impl VirtualGrid {
    pub fn new(
        total_count: u32,
        columns: u32,
        cell_size: Size,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self {
            total_count,
            columns: columns.max(1),
            cell_size,
            gap: 8.0,
            viewport_width: viewport_width.max(0.0),
            viewport_height: viewport_height.max(0.0),
            scroll_offset: Signal::new(0.0),
            padding: Edges::ZERO,
            overscan_rows: 2,
        }
    }

    /// Total rows in the model.
    pub fn total_rows(&self) -> u32 {
        if self.columns == 0 || self.total_count == 0 {
            return 0;
        }
        self.total_count.div_ceil(self.columns)
    }

    /// Single row's full y-stride including gap.
    pub fn row_stride(&self) -> f32 {
        self.cell_size.height + self.gap
    }

    pub fn content_height(&self) -> f32 {
        let rows = self.total_rows() as f32;
        if rows <= 0.0 {
            return 0.0;
        }
        rows * self.cell_size.height + (rows - 1.0).max(0.0) * self.gap
    }

    pub fn max_offset(&self) -> f32 {
        (self.content_height() - self.viewport_height).max(0.0)
    }

    pub fn scroll_by(&mut self, delta: f32) -> f32 {
        let next = (*self.scroll_offset.get() + delta).clamp(0.0, self.max_offset());
        let _ = self.scroll_offset.set(next);
        next
    }

    pub fn set_offset(&mut self, offset: f32) -> bool {
        let clamped = offset.clamp(0.0, self.max_offset());
        self.scroll_offset.set(clamped)
    }

    /// `(start_index, end_index_exclusive)` over the flat model. The renderer
    /// converts to `(row, col)` via `idx % columns` / `idx / columns`.
    pub fn visible_range(&self) -> (u32, u32) {
        let stride = self.row_stride().max(1.0);
        let offset = (*self.scroll_offset.get()).max(0.0);
        let offset_rows = (offset / stride).floor() as i64;
        let visible_rows = (self.viewport_height / stride).ceil() as i64 + 1; // +1 partial row
        let overscan = self.overscan_rows as i64;
        let start_row = (offset_rows - overscan).max(0) as u32;
        let last_row_excl = start_row as i64 + visible_rows + 2 * overscan;
        let end_row = (last_row_excl.max(0) as u32).min(self.total_rows());
        let start = start_row * self.columns;
        let end = (end_row * self.columns).min(self.total_count);
        (start, end)
    }

    /// Y-offset (DIPs from content origin) of the first visible row's top.
    pub fn first_row_y(&self) -> f32 {
        let (start, _) = self.visible_range();
        let row = start / self.columns.max(1);
        row as f32 * self.row_stride()
    }
}

impl LayoutSource for VirtualGrid {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Grid {
                columns: self.columns,
            },
            width: if self.viewport_width > 0.0 {
                Length::Px(self.viewport_width)
            } else {
                Length::Auto
            },
            height: Length::Px(self.viewport_height),
            padding: self.padding,
            gap: self.gap,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(w: f32, h: f32) -> Size {
        Size {
            width: w,
            height: h,
        }
    }

    #[test]
    fn virtual_grid_total_rows_round_up() {
        let g = VirtualGrid::new(50, 7, cell(40.0, 40.0), 280.0, 240.0);
        // 50 / 7 = 7 rem 1 → 8 rows.
        assert_eq!(g.total_rows(), 8);
    }

    #[test]
    fn virtual_grid_visible_range_at_top_includes_overscan() {
        let g = VirtualGrid::new(100, 5, cell(40.0, 40.0), 200.0, 160.0);
        // row_stride = 48; visible rows = ceil(160/48)+1 = 5; overscan 2.
        let (s, e) = g.visible_range();
        assert_eq!(s, 0);
        // last_row = 0 + 5 + 4 = 9; clamped to total_rows=20 → end = 9*5 = 45.
        assert_eq!(e, 45);
    }

    #[test]
    fn virtual_grid_max_offset_zero_when_grid_fits_viewport() {
        let g = VirtualGrid::new(4, 2, cell(40.0, 40.0), 100.0, 240.0);
        assert!((g.max_offset() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn virtual_grid_first_row_y_aligns_to_row_stride() {
        let mut g = VirtualGrid::new(100, 5, cell(40.0, 40.0), 200.0, 160.0);
        // Scroll to row 3 (offset = 3*48 = 144). first_row_y must be a row
        // boundary at 48 px stride.
        g.set_offset(144.0);
        let y = g.first_row_y();
        assert!((y / g.row_stride()).fract() < 1e-3);
    }
}
