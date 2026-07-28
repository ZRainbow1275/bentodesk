//! `Grid` — fixed-column grid widget. Pairs with `Direction::Grid` in the
//! layout engine (T-040) — this widget is the call-site ergonomics wrapper.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Color, Edges, Length};
use bentodesk_tree::NodeId;
use smallvec::SmallVec;

#[derive(Debug, Clone)]
pub struct Grid {
    pub items: SmallVec<[NodeId; 32]>,
    pub columns: u32,
    pub gap: f32,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
    pub background: Color,
}

impl Grid {
    pub fn new(columns: u32) -> Self {
        Self {
            items: SmallVec::new(),
            columns: columns.max(1),
            gap: 8.0,
            padding: Edges::ZERO,
            width: Length::Auto,
            height: Length::Auto,
            background: Color::TRANSPARENT,
        }
    }

    pub fn with_item(mut self, item: NodeId) -> Self {
        self.items.push(item);
        self
    }

    pub fn rows(&self) -> u32 {
        let n = self.items.len() as u32;
        if self.columns == 0 {
            return 0;
        }
        n.div_ceil(self.columns)
    }
}

impl LayoutSource for Grid {
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
    fn grid_rows_round_up() {
        let g = Grid::new(3)
            .with_item(NodeId::ROOT_INVALID)
            .with_item(NodeId::ROOT_INVALID)
            .with_item(NodeId::ROOT_INVALID)
            .with_item(NodeId::ROOT_INVALID);
        assert_eq!(g.rows(), 2);
    }

    #[test]
    fn grid_zero_columns_clamped_to_one() {
        let g = Grid::new(0);
        assert_eq!(g.columns, 1);
    }

    #[test]
    fn grid_layout_routes_through_grid_direction() {
        let g = Grid::new(2);
        assert!(matches!(
            g.layout().direction,
            Direction::Grid { columns } if columns == 2
        ));
    }
}
