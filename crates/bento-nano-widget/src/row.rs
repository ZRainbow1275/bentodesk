//! `Row` — horizontal layout primitive.
//!
//! Thin ergonomics wrapper around `Container { direction: Row }` that surfaces
//! the new layout-engine knobs (`gap` / `align` / `justify` / `margin`) as
//! first-class fields. Keeps the renderer match arm short — same code path as
//! Container, but the call sites read like `Row { gap: 8.0, .. }` instead of
//! the longer struct-literal noise.

use bento_nano_layout::{Align, Direction, Justify, LayoutDesc, LayoutSource};
use bento_nano_style::{Edges, Length};

/// Horizontal flex row. Defaults: `gap=0`, `align=Stretch`, `justify=Start`,
/// no padding/margin — matches the layout-engine defaults so a bare `Row {}`
/// behaves like a `Container { direction: Row }`.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub width: Length,
    pub height: Length,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
}

impl Row {
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

impl Default for Row {
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

impl LayoutSource for Row {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
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
    fn row_defaults_to_row_direction() {
        let r = Row::new();
        assert!(matches!(r.layout().direction, Direction::Row));
        assert_eq!(r.gap, 0.0);
        assert!(matches!(r.align, Align::Stretch));
    }

    #[test]
    fn row_with_gap_sets_layout_desc_gap() {
        let r = Row::new().with_gap(12.0);
        assert!((r.layout().gap - 12.0).abs() < 1e-6);
    }

    #[test]
    fn row_with_align_and_justify_propagates() {
        let r = Row::new()
            .with_align(Align::Center)
            .with_justify(Justify::SpaceBetween);
        let d = r.layout();
        assert!(matches!(d.align, Align::Center));
        assert!(matches!(d.justify, Justify::SpaceBetween));
    }
}
