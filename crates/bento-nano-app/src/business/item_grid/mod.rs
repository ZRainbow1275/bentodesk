//! Business surface — `ItemGrid`, the per-zone tile layout.
//!
//! Visual spec: see `item_grid.snap.md`. Picks between `Direct` (small zone),
//! `Virtual` (≥ 50 items) and `Empty` modes via `pick_layout`. Geometry
//! constants are locked here so re-layout never goes through string keys.
//!
//! The native renderer, hit-test and drag geometry consume these constants and
//! helpers as their shared grid model. `build()` is the compatibility
//! widget-tree descriptor; item painting is owned by `render::item_cards`.

use bento_nano_layout::Direction;
use bento_nano_style::Length;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};

/// Item count above which the grid switches to virtualized rendering.
/// Mirrors 1.x `VIRTUAL_THRESHOLD = 50` from `ItemGrid.tsx`.
pub const ITEM_GRID_VIRTUAL_THRESHOLD: usize = 50;

/// Logical-pixel row height inside a virtualized grid. P3.7 (2026-06-02, 1:1):
/// realigned from the legacy 80 to Tauri's 78 (card stride is `78 + 8` gap =
/// 86). 1.x `VirtualItemGrid.tsx` `ROW_HEIGHT` predates the final card height.
pub const ITEM_GRID_ROW_HEIGHT_PX: f32 = 78.0;

/// Number of rows kept rendered above and below the viewport while
/// virtualized — mirrors 1.x `OVERSCAN_ROWS = 3`.
pub const ITEM_GRID_OVERSCAN_ROWS: usize = 3;

/// Inter-column gap (logical px) — locked.
pub const ITEM_GRID_COLUMN_GAP_PX: f32 = 8.0;

/// Inter-row gap (logical px) — locked.
pub const ITEM_GRID_ROW_GAP_PX: f32 = 8.0;

/// Vertical offset (logical px) of the first item row below the panel's top
/// edge — i.e. where the item grid starts, immediately under the expanded
/// `PanelHeader` band. P3.6 (2026-06-02, 1:1): Tauri's `.panel-header` is
/// `height: 48px` (PanelHeader.css:6) with `flex-shrink: 0`, and the grid host
/// (`.bento-panel__content`) follows it in the column flex with a
/// `--spacing-sm` (8px) top pad — so the first row begins at `48 + 8 = 56`.
/// The SSoTs are now SPLIT: `expanded_zone_grid::HEADER_BAND_HEIGHT` stays 48
/// (the header band / divider seam) while this grid-top offset is 56 (header +
/// the missing content pad). This is the SSoT shared by the renderer
/// (`highlight_overlay::item_card_rect_for_grid`) and the shell hit-tests, so
/// the painted grid and its hit-rects can never drift (V-13 paint-hit parity).
pub const ITEM_GRID_TOP_OFFSET_PX: f32 = 56.0;

/// Default column count when the zone configuration supplies none.
/// Mirrors 1.x `props.gridColumns ?? 4`.
pub const ITEM_GRID_DEFAULT_COLUMNS: u32 = 4;

/// Minimum readable outer width for a standard item card. Below this threshold
/// the 14px single-line label cannot keep a professional gap from neighbouring
/// cards, so narrow panels reduce the effective column count at paint/hit time.
pub const ITEM_GRID_MIN_CARD_WIDTH_PX: f32 = 64.0;

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
/// `_grid_columns` stays in the compatibility API; the current mode depends
/// only on item count.
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

pub fn effective_column_count(zone_width: f32, requested_columns: u32, inset_x: f32) -> u32 {
    let mut columns = requested_columns.max(1);
    let available = (zone_width - inset_x * 2.0).max(ITEM_GRID_MIN_CARD_WIDTH_PX);
    while columns > 1 {
        let gap_total = ITEM_GRID_COLUMN_GAP_PX * (columns - 1) as f32;
        let cell_w = ((available - gap_total).max(0.0)) / columns as f32;
        if cell_w >= ITEM_GRID_MIN_CARD_WIDTH_PX {
            break;
        }
        columns -= 1;
    }
    columns
}

/// Build the compatibility widget-tree descriptor for the grid host.
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
        // P3.7 (1:1) — card row height is Tauri's 78 (was the legacy 80).
        assert!((ITEM_GRID_ROW_HEIGHT_PX - 78.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_OVERSCAN_ROWS, 3);
        assert!((ITEM_GRID_COLUMN_GAP_PX - 8.0).abs() < 0.01);
        assert!((ITEM_GRID_ROW_GAP_PX - 8.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_DEFAULT_COLUMNS, 4);
        assert!((ITEM_GRID_MIN_CARD_WIDTH_PX - 64.0).abs() < 0.01);
        // P3.6 (1:1) — grid starts at the 48-DIP Tauri `.panel-header` plus the
        // 8-DIP `--spacing-sm` content pad = 56.
        assert!((ITEM_GRID_TOP_OFFSET_PX - 56.0).abs() < 0.01);
    }

    #[test]
    fn effective_column_count_keeps_cards_readable_in_narrow_panels() {
        assert_eq!(effective_column_count(320.0, 5, 16.0), 4);
        assert_eq!(effective_column_count(320.0, 4, 16.0), 4);
        assert_eq!(effective_column_count(240.0, 5, 16.0), 3);
        assert_eq!(effective_column_count(720.0, 5, 16.0), 5);
        assert_eq!(effective_column_count(80.0, 5, 16.0), 1);
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
