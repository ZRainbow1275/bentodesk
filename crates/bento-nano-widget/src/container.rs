//! Container widget — flex parent, optional fill / border-radius / shadow.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Shadow};

/// A non-leaf box. Renders a rounded background fill (optional) and lays its
/// children out along `direction`.
#[derive(Debug, Clone, Copy)]
pub struct ContainerNode {
    pub direction: Direction,
    pub width: Length,
    pub height: Length,
    pub padding: Edges,
    pub background: Color,
    pub radius: BorderRadius,
    pub shadow: Shadow,
}

impl Default for ContainerNode {
    fn default() -> Self {
        Self {
            direction: Direction::Column,
            width: Length::Auto,
            height: Length::Auto,
            padding: Edges::ZERO,
            background: Color::TRANSPARENT,
            radius: BorderRadius::ZERO,
            shadow: Shadow::NONE,
        }
    }
}

impl LayoutSource for ContainerNode {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: self.direction,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}
