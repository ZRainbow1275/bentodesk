//! `Toolbar` — horizontal flex container for icon buttons.
//!
//! Spec §10: `children` is `SmallVec<[NodeId; 8]>`, covering the BentoDesk
//! toolbar's typical ≤6 icons with margin to spare; deeper toolbars spill to
//! heap rather than panic.
//!
//! Layout contract: `spacing` is applied **between** children (n-1 gaps for
//! n children), not on either end. The current `bento-nano-layout` engine
//! doesn't have a native `gap` knob (planned for layout PHASE_2), so we
//! surface the spacing here and let the renderer/layout caller honour it
//! once support lands. The widget data is correct today; the visual gap
//! materialises when layout grows the feature.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Edges, Length};
use bento_nano_tree::NodeId;
use smallvec::SmallVec;

/// Horizontal icon strip. Children are intentionally indexed in declaration
/// order — focus traversal follows that order.
#[derive(Debug, Clone)]
pub struct Toolbar {
    pub spacing: f32,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
    pub children: SmallVec<[NodeId; 8]>,
}

impl Toolbar {
    /// Empty toolbar with the BentoDesk default of 8px spacing + 4px padding.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a child id and return self for chaining. Caller is still
    /// responsible for the matching `tree.append_child` — this widget only
    /// caches the order.
    pub fn with_child(mut self, child: NodeId) -> Self {
        self.children.push(child);
        self
    }

    /// Total inter-child gap given `self.children.len()`. Returns 0 when
    /// fewer than 2 children — single-button toolbars don't have gaps.
    pub fn total_gap(&self) -> f32 {
        let n = self.children.len();
        if n < 2 {
            return 0.0;
        }
        self.spacing * (n - 1) as f32
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            spacing: 8.0,
            padding: Edges::all(4.0),
            width: Length::Auto,
            height: Length::Px(40.0),
            children: SmallVec::new(),
        }
    }
}

impl LayoutSource for Toolbar {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
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
    use bento_nano_tree::NodeId;

    #[test]
    fn toolbar_lays_out_three_children_with_spacing() {
        let mut tb = Toolbar::new();
        // Use the invalid sentinel as a unique-id stand-in — total_gap
        // doesn't dereference the ids, just counts them.
        tb.children.push(NodeId::ROOT_INVALID);
        tb.children.push(NodeId::ROOT_INVALID);
        tb.children.push(NodeId::ROOT_INVALID);
        assert_eq!(tb.children.len(), 3);
        // 3 children → 2 gaps × 8.0 spacing = 16.0
        assert!((tb.total_gap() - 16.0).abs() < 1e-6);
    }

    #[test]
    fn toolbar_no_gap_when_fewer_than_two_children() {
        let mut tb = Toolbar::new();
        assert_eq!(tb.total_gap(), 0.0);
        tb.children.push(NodeId::ROOT_INVALID);
        assert_eq!(tb.total_gap(), 0.0);
    }
}
