//! Business surface — `ItemGrid`, the per-zone tile layout.
//!
//! Visual spec: see `item_grid.snap.md`. Picks between `Direct` (small zone),
//! `Virtual` (≥ 50 items) and `Empty` modes via `pick_layout`. Geometry
//! constants are locked here so re-layout never goes through string keys.
//!
//! Status: scaffolding per Wave E Option-A. The Container body returned by
//! `build()` is the outer grid host; child cards land when widget-library
//! ships the `GridLayout` + `VirtualGrid` primitives. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::Length;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};

/// Item count above which the grid switches to virtualized rendering.
/// Mirrors 1.x `VIRTUAL_THRESHOLD = 50` from `ItemGrid.tsx`.
pub const ITEM_GRID_VIRTUAL_THRESHOLD: usize = 50;

/// Logical-pixel row height inside a virtualized grid. Mirrors 1.x
/// `ROW_HEIGHT = 80` from `VirtualItemGrid.tsx`.
pub const ITEM_GRID_ROW_HEIGHT_PX: f32 = 80.0;

/// Number of rows kept rendered above and below the viewport while
/// virtualized — mirrors 1.x `OVERSCAN_ROWS = 3`.
pub const ITEM_GRID_OVERSCAN_ROWS: usize = 3;

/// Inter-column gap (logical px) — locked.
pub const ITEM_GRID_COLUMN_GAP_PX: f32 = 8.0;

/// Inter-row gap (logical px) — locked.
pub const ITEM_GRID_ROW_GAP_PX: f32 = 8.0;

/// Default column count when the zone configuration supplies none.
/// Mirrors 1.x `props.gridColumns ?? 4`.
pub const ITEM_GRID_DEFAULT_COLUMNS: u32 = 4;

/// Layout mode chosen once per zone load — switched only when item count
/// crosses `ITEM_GRID_VIRTUAL_THRESHOLD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    /// Zone is empty; render only the parent BentoPanel chrome.
    Empty,
    /// Direct render of every ItemCard (item_count < threshold).
    Direct,
    /// Virtualized render via VirtualGrid (item_count >= threshold).
    Virtual,
}

/// Pick the layout mode for a zone holding `item_count` items.
/// `_grid_columns` is reserved for a future "always virtualize when
/// columns × overscan exceeds budget" rule; today it is unused.
pub fn pick_layout(item_count: usize, _grid_columns: u32) -> LayoutMode {
    if item_count == 0 {
        LayoutMode::Empty
    } else if item_count >= ITEM_GRID_VIRTUAL_THRESHOLD {
        LayoutMode::Virtual
    } else {
        LayoutMode::Direct
    }
}

/// Column span for a single item, accounting for the wide-card variant.
/// Mirrors 1.x `style={{ "grid-column": is_wide ? "span 2" : undefined }}`.
pub const fn column_span_for(is_wide: bool) -> u32 {
    if is_wide { 2 } else { 1 }
}

/// Build the grid outer container. Children — the ItemCards or virtualizer
/// surface — land when widget-library ships `GridLayout`/`VirtualGrid`.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn locked_constants_match_snap_md() {
        assert_eq!(ITEM_GRID_VIRTUAL_THRESHOLD, 50);
        assert!((ITEM_GRID_ROW_HEIGHT_PX - 80.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_OVERSCAN_ROWS, 3);
        assert!((ITEM_GRID_COLUMN_GAP_PX - 8.0).abs() < 0.01);
        assert!((ITEM_GRID_ROW_GAP_PX - 8.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_DEFAULT_COLUMNS, 4);
    }

    #[test]
    fn pick_layout_empty_when_zero() {
        assert_eq!(pick_layout(0, 4), LayoutMode::Empty);
    }

    #[test]
    fn pick_layout_direct_below_threshold() {
        assert_eq!(pick_layout(1, 4), LayoutMode::Direct);
        assert_eq!(pick_layout(49, 4), LayoutMode::Direct);
    }

    #[test]
    fn pick_layout_virtual_at_and_above_threshold() {
        assert_eq!(pick_layout(50, 4), LayoutMode::Virtual);
        assert_eq!(pick_layout(500, 4), LayoutMode::Virtual);
    }

    #[test]
    fn column_span_wide_takes_two() {
        assert_eq!(column_span_for(false), 1);
        assert_eq!(column_span_for(true), 2);
    }

    #[test]
    fn build_produces_a_container() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Column);
    }

    /// Wire-format lock: layout snapshots persisted in 1.x JSON should
    /// continue to round-trip after the rewrite.
    #[test]
    fn layout_mode_serde_round_trip() {
        for v in [LayoutMode::Empty, LayoutMode::Direct, LayoutMode::Virtual] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: LayoutMode = serde_json::from_str(&s).unwrap_or(LayoutMode::Empty);
            assert_eq!(v, back);
        }
    }
}
