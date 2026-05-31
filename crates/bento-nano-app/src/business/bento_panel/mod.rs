//! Business surface — `BentoPanel`, the expanded host of a focused BentoZone.
//!
//! Visual spec: see `bento_panel.snap.md`. The panel is a vertical
//! Container hosting `PanelHeader` + optional `SearchBar` + `ItemGrid`.
//!
//! Status: scaffolding per Wave E Option-A. The `build()` returns a typed
//! Container with the locked geometry; the children land when widget-library
//! ships Input (search box) + the item-grid layer wires up to the
//! `item_grid` sibling module. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length};
use bento_nano_widget::{ContainerNode, WidgetNode};

/// Default font size (logical px) for ItemCard names inside this panel.
/// Matches 1.x `ITEM_CARD_DEFAULT_FONT_PX = 11` (Theme v8 FontGroup default).
pub const PANEL_ITEM_CARD_FONT_PX: f32 = 11.0;

/// Default ItemGrid column count when the zone supplies none.
/// Matches 1.x `ItemGrid` `props.gridColumns ?? 4`.
pub const PANEL_DEFAULT_GRID_COLUMNS: u32 = 4;

/// Locked panel-header height in logical px. The header row is non-resizable;
/// see `bento_panel.snap.md`.
///
/// M2③ (05-31, ruling = A / 1:1): realigned to Tauri `.panel-header
/// { height: 48px }` (PanelHeader.css:6). This is the same value the live
/// renderer now uses via `item_grid::ITEM_GRID_TOP_OFFSET_PX` /
/// `expanded_zone_grid::HEADER_BAND_HEIGHT`; kept in sync so this scaffold
/// constant cannot document a stale header height.
pub const PANEL_HEADER_HEIGHT_PX: f32 = 48.0;

/// Build the expanded-panel subtree. Returns a typed Container today; the
/// real composition (PanelHeader → SearchBar → ItemGrid) lands when the
/// upstream Input + grid primitives are ready. Geometry is final per
/// `bento_panel.snap.md`.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges {
            top: 12.0,
            right: 16.0,
            bottom: 16.0,
            left: 16.0,
        },
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn locked_constants_match_snap_md() {
        assert!((PANEL_ITEM_CARD_FONT_PX - 11.0).abs() < 0.01);
        assert_eq!(PANEL_DEFAULT_GRID_COLUMNS, 4);
        assert!((PANEL_HEADER_HEIGHT_PX - 48.0).abs() < 0.01);
    }

    #[test]
    fn build_produces_a_column_container() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Column);
    }
}
