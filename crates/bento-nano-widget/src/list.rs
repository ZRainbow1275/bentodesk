//! `List` — vertical, non-virtualised stack of NodeIds. Wraps a
//! `ScrollContainer` semantically; layout-wise it's a column with a
//! configurable gap.
//!
//! Use `List` for short, fixed-set lists (snapshot picker, plugin entries).
//! Switch to [`crate::virtual_list::VirtualList`] when entry count exceeds
//! ~100 to keep the layout pass O(viewport) instead of O(n).

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Color, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_tree::NodeId;
use smallvec::SmallVec;

#[derive(Debug, Clone)]
pub struct List {
    pub items: SmallVec<[NodeId; 16]>,
    pub gap: f32,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
    pub background: Color,
    /// Per-item hover index — `Some(i)` lets the renderer paint a hover
    /// background under entry `i`.
    pub hovered: Option<u32>,
    /// Currently-selected entry, if any. Caller updates on click.
    pub selected: Option<u32>,
    pub hover_color: Color,
    pub selection_color: Color,
}

impl List {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an item id and return self for chaining. Caller is responsible
    /// for the matching `tree.append_child(list_node, item)`.
    pub fn with_item(mut self, item: NodeId) -> Self {
        self.items.push(item);
        self
    }

    pub fn select(&mut self, index: Option<u32>) {
        self.selected = match index {
            None => None,
            Some(i) if (i as usize) < self.items.len() => Some(i),
            _ => None,
        };
    }

    pub fn set_hover(&mut self, index: Option<u32>) {
        self.hovered = match index {
            None => None,
            Some(i) if (i as usize) < self.items.len() => Some(i),
            _ => None,
        };
    }

    pub fn item_at(&self, index: u32) -> Option<NodeId> {
        self.items.get(index as usize).copied()
    }
}

impl Default for List {
    fn default() -> Self {
        let p = theme::current().palette;
        Self {
            items: SmallVec::new(),
            gap: 0.0,
            padding: Edges::ZERO,
            width: Length::Auto,
            height: Length::Auto,
            background: Color::TRANSPARENT,
            hovered: None,
            selected: None,
            hover_color: p.hover_overlay,
            selection_color: p.selection,
        }
    }
}

impl LayoutSource for List {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
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
    fn list_select_clamps_to_in_range() {
        let mut l = List::new()
            .with_item(NodeId::ROOT_INVALID)
            .with_item(NodeId::ROOT_INVALID);
        l.select(Some(99));
        assert_eq!(l.selected, None);
        l.select(Some(1));
        assert_eq!(l.selected, Some(1));
    }

    #[test]
    fn list_set_hover_drops_oob() {
        let mut l = List::new().with_item(NodeId::ROOT_INVALID);
        l.set_hover(Some(7));
        assert_eq!(l.hovered, None);
        l.set_hover(Some(0));
        assert_eq!(l.hovered, Some(0));
    }
}
