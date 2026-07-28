//! `GridLayout` — pure-layout grid wrapper (no per-item background or
//! selection state). Use this when the children handle their own visuals
//! (cards, item tiles) and the grid's only job is geometry. For interactive
//! lists with hover/selection use [`crate::grid::Grid`].

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Edges, Length};

#[derive(Debug, Clone, Copy)]
pub struct GridLayout {
    pub columns: u32,
    pub gap: f32,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
}

impl GridLayout {
    pub fn new(columns: u32) -> Self {
        Self {
            columns: columns.max(1),
            gap: 0.0,
            padding: Edges::ZERO,
            width: Length::Auto,
            height: Length::Auto,
        }
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl LayoutSource for GridLayout {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Grid {
                columns: self.columns,
            },
            width: self.width,
            height: self.height,
            padding: self.padding,
            gap: self.gap,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_layout_zero_columns_clamps_to_one() {
        let g = GridLayout::new(0);
        assert_eq!(g.columns, 1);
    }

    #[test]
    fn grid_layout_with_gap_propagates() {
        let g = GridLayout::new(4).with_gap(12.0);
        assert!((g.layout().gap - 12.0).abs() < 1e-6);
    }
}
