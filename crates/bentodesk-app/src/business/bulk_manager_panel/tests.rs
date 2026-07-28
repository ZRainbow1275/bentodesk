//! BulkManagerPanel — unit + smoke tests, kept in a sibling file so the
//! main `mod.rs` stays under the §15 800-LOC budget.
//!
//! Per the Wave G ruling: every backend type the test body references must
//! be explicitly imported here (the `mod tests` declaration in `mod.rs`
//! does NOT inherit the parent's `use` lines once the tests live in their
//! own file).

use super::{
    BULK_MANAGER_ACTION_BUTTONS, BulkManagerAction, BulkManagerChrome, BulkManagerPointerHit,
    BulkManagerState, BulkTextEditField, FOOTER_HEIGHT_PX, HEADER_HEIGHT_PX,
    PANEL_CORNER_RADIUS_PX, PANEL_HEIGHT_PX, PANEL_MAX_HEIGHT_FRACTION, PANEL_MAX_WIDTH_FRACTION,
    PANEL_PADDING_PX, PANEL_WIDTH_PX, RUNTIME_SEARCH_LIMIT, SEARCH_INPUT_HEIGHT_PX,
    SEARCH_INPUT_WIDTH_PX, SELECTED_ROW_STRIPE_PX, SortDirection, SortKey, TABLE_CELL_PADDING_Y_PX,
    TABLE_ROW_HEIGHT_PX, TOOLBAR_HEIGHT_PX, ZoneRow, build, bulk_manager_action_enabled,
    bulk_manager_button_rect, bulk_manager_close_rect, bulk_manager_hit_test,
    bulk_manager_panel_rect, bulk_manager_panel_shadow_rect, bulk_manager_row_cell_rect,
    bulk_manager_row_rect, bulk_manager_search_rect, bulk_manager_sort_header_rect,
    bulk_manager_visible_window_start, bulk_manager_visible_window_summary,
};
use bentodesk_layout::{Direction, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Length, Rect, Shadow, Size};
use bentodesk_theme as theme;
use bentodesk_zone::ZoneId;
use smol_str::SmolStr;

use crate::dispatcher::Point;

fn sample_row(id: u64, name: &str, items: u32, accent: &str, w: u32, h: u32) -> ZoneRow {
    ZoneRow {
        id: ZoneId(id),
        display_name: SmolStr::new(name),
        item_count: items,
        accent_hex: SmolStr::new(accent),
        visible: true,
        locked: false,
        icon_slug: SmolStr::new_static("folder"),
        capsule_size: SmolStr::new_static("medium"),
        display_mode: SmolStr::new_static("inherit"),
        width_percent: w,
        height_percent: h,
        position_x_percent: 0,
        position_y_percent: 0,
    }
}

fn sample_zones() -> Vec<ZoneRow> {
    vec![
        sample_row(1, "Inbox", 3, "#3b82f6", 30, 40),
        sample_row(2, "Projects", 12, "#22c55e", 60, 50),
        sample_row(3, "Archive", 5, "#64748b", 20, 25),
        sample_row(4, "Notes", 8, "#22c55e", 25, 30),
    ]
}

include!("tests/01_snap_geometry_constants_pinned.rs");
include!("tests/02_bulk_manager_chrome_accepts_explicit_active_palette.rs");
