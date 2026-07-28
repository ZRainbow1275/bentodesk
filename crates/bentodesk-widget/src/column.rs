//! `Column` — vertical layout primitive.
//!
//! Mirror of [`crate::row::Row`] with `Direction::Column`. Same field shape so
//! call sites can swap `Row` ↔ `Column` without restructuring.

use bentodesk_layout::{Align, Direction, Justify, LayoutDesc, LayoutSource};
use bentodesk_style::{Edges, Length};

/// Vertical flex column. Defaults match layout-engine defaults so a bare
/// `Column {}` behaves like `Container { direction: Column }`.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub width: Length,
    pub height: Length,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
}

impl Column {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn with_align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn with_justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }
}

impl Default for Column {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
        }
    }
}

impl LayoutSource for Column {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            gap: self.gap,
            align: self.align,
            justify: self.justify,
            margin: self.margin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_defaults_to_column_direction() {
        let c = Column::new();
        assert!(matches!(c.layout().direction, Direction::Column));
    }

    #[test]
    fn column_with_gap_sets_layout_desc_gap() {
        let c = Column::new().with_gap(16.0);
        assert!((c.layout().gap - 16.0).abs() < 1e-6);
    }
}
