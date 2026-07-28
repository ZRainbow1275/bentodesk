//! Business surface — `BulkManagerPanel` (T-067a).
//!
//! Modal that lists every zone in a sortable, searchable table and lets
//! the user multi-select rows (per-row checkbox) to run a single bulk
//! action: Hide / Show / Delete / Move. The shell drains the resulting
//! [`BulkManagerAction`] once per frame and sequences the appropriate
//! per-zone Commands.
//!
//! Visual spec: `bulk_manager_panel.snap.md`. Pairs with
//! `business::auto_layout_menu` and `business::palette_picker` (the
//! popovers anchored from this panel).
//!
//! # State machine
//!
//! Mirrors the Wave-E shape (see `business::capsule_picker` and
//! `business::smart_group_suggestor`): user intents collapse into a closed
//! [`BulkManagerAction`] enum, drained one-shot via
//! [`BulkManagerState::take_action`].
//!
//! # Selection invariants
//!
//! - Selection is keyed by [`ZoneId`] (not row index) so it survives
//!   sort + search re-flow.
//! - `select_all` / `deselect_all` / `invert_selection` operate on the
//!   visible-after-search row set; offscreen rows are unaffected.
//! - `set_zones` resets the selection (old ids may be stale).
//!
//! # Spec compliance
//!
//! - §10 hot-path: `selected: SmallVec<[ZoneId; 8]>` keeps the steady-state
//!   selection alloc-free (typical user batches ≤ 8 zones); search input
//!   is `String` because user-typed search terms regularly exceed the
//!   22-byte SmolStr inline budget for non-ASCII text.
//! - §11 ΔB: every public DTO derives `serde::{Serialize, Deserialize}`.
//! - §11.1: zero `unsafe` in this UI layer.
//! - §15: this single .rs file ships under the 800-LOC budget.
//! - §17: zero `todo!()` / `unimplemented!()` / `panic!()` / `unwrap()` /
//!   `expect()` in production code.

use core::fmt;

use bentodesk_layout::Direction;
use bentodesk_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bentodesk_theme as theme;
use bentodesk_theme::PaletteTokens;
use bentodesk_widget::{ContainerNode, WidgetNode};
use bentodesk_zone::ZoneId;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::SmolStr;

mod text_edit;

pub use text_edit::{BulkTextEditField, BulkTextEditState, TEXT_EDIT_DRAFT_LIMIT};

use crate::dispatcher::Point;

// -----------------------------------------------------------------------------
// Snap.md geometry constants — pinned per the visual spec.
// -----------------------------------------------------------------------------

/// Panel width in DIPs — `min(960px, 92vw)` per snap.md.
pub const PANEL_WIDTH_PX: f32 = 960.0;

/// Maximum panel width as fraction of viewport — `min(_, 92vw)` clamp.
pub const PANEL_MAX_WIDTH_FRACTION: f32 = 0.92;

/// Panel height in DIPs — `min(640px, 80vh)` per snap.md.
pub const PANEL_HEIGHT_PX: f32 = 640.0;

/// Maximum panel height as fraction of viewport — `min(_, 80vh)` clamp.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.80;

/// Outer panel padding — 20 px uniform per snap.md.
pub const PANEL_PADDING_PX: f32 = 20.0;

/// Outer panel corner radius — 16 px (matches every 2.0 modal panel).
pub const PANEL_CORNER_RADIUS_PX: f32 = 16.0;

/// Header row height — title + search input + close button.
pub const HEADER_HEIGHT_PX: f32 = 52.0;

/// Toolbar row height — select-all + invert + bulk action buttons.
pub const TOOLBAR_HEIGHT_PX: f32 = 44.0;

/// Footer action bar height (collapses to zero when nothing selected).
pub const FOOTER_HEIGHT_PX: f32 = 56.0;

/// Per-row height in the zone table.
pub const TABLE_ROW_HEIGHT_PX: f32 = 44.0;

/// Cell vertical padding inside the table.
pub const TABLE_CELL_PADDING_Y_PX: f32 = 8.0;

/// Search input width inside the header row.
pub const SEARCH_INPUT_WIDTH_PX: f32 = 240.0;

/// Search input height (matches header row inset).
pub const SEARCH_INPUT_HEIGHT_PX: f32 = 32.0;

/// Selected-row left-edge accent stripe width.
pub const SELECTED_ROW_STRIPE_PX: f32 = 2.0;

/// Selected-stack aux renderer panel margin.
pub const RUNTIME_PANEL_MARGIN_PX: f32 = 16.0;

/// Left/right inset used by the D2D runtime renderer.
pub const RUNTIME_PANEL_INSET_PX: f32 = 18.0;

/// Runtime action button height in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_HEIGHT_PX: f32 = 24.0;

/// Runtime helper copy top in the D2D aux panel.
pub const RUNTIME_HELPER_TOP_PX: f32 = 52.0;

/// Runtime live-status top in the D2D aux panel.
pub const RUNTIME_STATUS_TOP_PX: f32 = 78.0;

/// Square close-button target in the self-painted header.
pub const RUNTIME_CLOSE_BUTTON_SIZE_PX: f32 = 32.0;

/// Runtime search field maximum length in Unicode scalar values.
pub const RUNTIME_SEARCH_LIMIT: usize = 80;

/// Runtime sort-header row top in the D2D aux panel.
pub const RUNTIME_SORT_HEADER_TOP_PX: f32 = 166.0;

/// Runtime sort-header row height in the D2D aux panel.
pub const RUNTIME_SORT_HEADER_HEIGHT_PX: f32 = 22.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 194.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 38.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 40.0;

/// The current runtime renderer shows at most eight visible rows.
pub const RUNTIME_VISIBLE_ROW_LIMIT: usize = 8;

/// BulkManager colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulkManagerChrome {
    /// Drop shadow descriptor drawn behind the panel.
    pub panel_shadow: bentodesk_style::Shadow,
    /// Panel radius.
    pub panel_radius: BorderRadius,
    /// Search field radius.
    pub search_radius: BorderRadius,
    /// Action/button radius.
    pub button_radius: BorderRadius,
    /// Sort header radius.
    pub sort_radius: BorderRadius,
    /// Row radius.
    pub row_radius: BorderRadius,
    /// Text edit radius.
    pub edit_radius: BorderRadius,
    /// Panel fill colour.
    pub panel_background: Color,
    /// Default row, button, and search-field fill colour.
    pub row_background: Color,
    /// Cursor/search-focus fill colour.
    pub cursor_background: Color,
    /// Selected row fill colour.
    pub selected_background: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
}

impl BulkManagerChrome {
    /// Build BulkManager chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, theme::radius::DEFAULT, theme::shadow::DEFAULT)
    }

    /// Build BulkManager chrome from explicit active theme token groups.
    pub fn from_tokens(
        palette: PaletteTokens,
        radius: theme::RadiusTokens,
        shadow: theme::ShadowTokens,
    ) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            search_radius: radius.lg,
            button_radius: radius.md,
            sort_radius: radius.md,
            row_radius: radius.lg,
            edit_radius: radius.md,
            panel_background: palette.surface,
            row_background: palette.surface_alt,
            cursor_background: palette.hover_overlay,
            selected_background: palette.selection,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
        }
    }

    /// Build BulkManager chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `bulk-manager.md` table + button metrics):
    /// - panel bg ← `surface_expanded`
    /// - row bg ← `surface_hover` (rows + action pills on the BulkManager
    ///   table; bumped from `surface_subtle` in Wave F to give pill chips
    ///   visible contrast against the panel — `surface_subtle` was so close
    ///   to `surface_expanded` that the chips read as text-on-transparent)
    /// - cursor (search-focus / sort active) ← `surface_active`
    /// - selected row ← `surface_active` (with `--accent-blue` left stripe in 1.x)
    /// - panel radius ← `expanded` (16); search/row ← `card`; button/sort/edit ← `card`
    /// - panel shadow ← `expanded` outer layer
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is now a `ShadowStack`; the single-`Shadow`
            // `panel_shadow` consumes the dominant outer layer (== pre-M6b
            // `SHADOW.expanded` for dark, the §5.3 byte-parity contract).
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            search_radius: BorderRadius::all(radius.card),
            button_radius: BorderRadius::all(radius.card),
            sort_radius: BorderRadius::all(radius.card),
            row_radius: BorderRadius::all(radius.card),
            edit_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            row_background: palette.surface_hover,
            cursor_background: palette.surface_active,
            selected_background: palette.surface_active,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
        }
    }
}

/// Pointer hit target in the runtime D2D BulkManager panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkManagerPointerHit {
    SearchInput,
    Sort(SortKey),
    SelectAll,
    Invert,
    Hide,
    Show,
    LayoutGrid,
    LayoutRow,
    LayoutColumn,
    LayoutSpiral,
    LayoutOrganic,
    Update,
    TextEdit,
    IconPicker,
    AccentPicker,
    Delete,
    Move,
    Close,
    Row(usize),
}

/// Whether a runtime action is available for the current list/selection.
///
/// Layout and metadata refresh deliberately fall back to all listed rows when
/// no explicit selection exists.  Keep that contract shared by paint and
/// pointer dispatch so an action never looks disabled while still accepting a
/// click (or the reverse).
pub const fn bulk_manager_action_enabled(
    hit: BulkManagerPointerHit,
    has_rows: bool,
    has_selection: bool,
) -> bool {
    match hit {
        BulkManagerPointerHit::Close => true,
        BulkManagerPointerHit::SelectAll
        | BulkManagerPointerHit::Invert
        | BulkManagerPointerHit::LayoutGrid
        | BulkManagerPointerHit::LayoutRow
        | BulkManagerPointerHit::LayoutColumn
        | BulkManagerPointerHit::LayoutSpiral
        | BulkManagerPointerHit::LayoutOrganic
        | BulkManagerPointerHit::Update => has_rows,
        BulkManagerPointerHit::Hide
        | BulkManagerPointerHit::Show
        | BulkManagerPointerHit::TextEdit
        | BulkManagerPointerHit::IconPicker
        | BulkManagerPointerHit::AccentPicker
        | BulkManagerPointerHit::Delete
        | BulkManagerPointerHit::Move => has_selection,
        BulkManagerPointerHit::SearchInput
        | BulkManagerPointerHit::Sort(_)
        | BulkManagerPointerHit::Row(_) => has_rows,
    }
}

/// Static action-button descriptor shared by renderer and shell hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulkManagerButtonSpec {
    pub hit: BulkManagerPointerHit,
    pub label: &'static str,
    pub x_offset: f32,
    pub y_offset: f32,
    pub width: f32,
}

pub const BULK_MANAGER_ACTION_BUTTONS: &[BulkManagerButtonSpec] = &[
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::SelectAll,
        label: "All",
        x_offset: 0.0,
        y_offset: 104.0,
        width: 48.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Invert,
        label: "Invert",
        x_offset: 54.0,
        y_offset: 104.0,
        width: 58.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Hide,
        label: "Hide",
        x_offset: 118.0,
        y_offset: 104.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Show,
        label: "Show",
        x_offset: 176.0,
        y_offset: 104.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutGrid,
        label: "Grid",
        x_offset: 234.0,
        y_offset: 104.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutRow,
        label: "Row",
        x_offset: 292.0,
        y_offset: 104.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutColumn,
        label: "Col",
        x_offset: 350.0,
        y_offset: 104.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutSpiral,
        label: "Spiral",
        x_offset: 408.0,
        y_offset: 104.0,
        width: 62.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutOrganic,
        label: "Organic",
        x_offset: 476.0,
        y_offset: 104.0,
        width: 70.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Update,
        label: "Update",
        x_offset: 0.0,
        y_offset: 134.0,
        width: 64.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::TextEdit,
        label: "Text",
        x_offset: 70.0,
        y_offset: 134.0,
        width: 58.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::IconPicker,
        label: "Icon",
        x_offset: 134.0,
        y_offset: 134.0,
        width: 54.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::AccentPicker,
        label: "Color",
        x_offset: 194.0,
        y_offset: 134.0,
        width: 60.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Delete,
        label: "Delete",
        x_offset: 610.0,
        y_offset: 134.0,
        width: 64.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Move,
        label: "Move",
        x_offset: 546.0,
        y_offset: 134.0,
        width: 58.0,
    },
];

pub fn bulk_manager_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
    }
}

pub fn bulk_manager_panel_shadow_rect(panel: Rect, shadow: bentodesk_style::Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

pub fn bulk_manager_button_rect(viewport: Size, spec: BulkManagerButtonSpec) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX + spec.x_offset,
        y: panel.y + spec.y_offset,
        width: spec.width,
        height: RUNTIME_ACTION_BUTTON_HEIGHT_PX,
    }
}

/// Runtime search input rectangle shared by renderer, hit-testing and tests.
pub fn bulk_manager_search_rect(viewport: Size) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    Rect {
        x: panel.right()
            - RUNTIME_PANEL_INSET_PX
            - RUNTIME_CLOSE_BUTTON_SIZE_PX
            - 8.0
            - SEARCH_INPUT_WIDTH_PX,
        y: panel.y + 18.0,
        width: SEARCH_INPUT_WIDTH_PX,
        height: SEARCH_INPUT_HEIGHT_PX,
    }
}

/// Header close button, separate from destructive batch actions.
pub fn bulk_manager_close_rect(viewport: Size) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    Rect {
        x: panel.right() - RUNTIME_PANEL_INSET_PX - RUNTIME_CLOSE_BUTTON_SIZE_PX,
        y: panel.y + 18.0,
        width: RUNTIME_CLOSE_BUTTON_SIZE_PX,
        height: RUNTIME_CLOSE_BUTTON_SIZE_PX,
    }
}

/// Runtime sortable column-header rectangle shared by renderer and hit-testing.
pub fn bulk_manager_sort_header_rect(viewport: Size, key: SortKey) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    let table_x = panel.x + RUNTIME_PANEL_INSET_PX;
    let table_width = panel.width - (RUNTIME_PANEL_INSET_PX * 2.0);
    let (x_fraction, width_fraction) = table_column_fractions(key);
    Rect {
        x: table_x + (table_width * x_fraction),
        y: panel.y + RUNTIME_SORT_HEADER_TOP_PX,
        width: table_width * width_fraction,
        height: RUNTIME_SORT_HEADER_HEIGHT_PX,
    }
}

/// One body cell using exactly the same horizontal partition as its header.
pub fn bulk_manager_row_cell_rect(viewport: Size, row_index: usize, key: SortKey) -> Rect {
    let row = bulk_manager_row_rect(viewport, row_index);
    let (x_fraction, width_fraction) = table_column_fractions(key);
    Rect {
        x: row.x + row.width * x_fraction,
        y: row.y,
        width: row.width * width_fraction,
        height: row.height,
    }
}

pub fn bulk_manager_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX,
        y: panel.y + RUNTIME_ROW_TOP_PX + (row_index as f32 * RUNTIME_ROW_STRIDE_PX),
        width: panel.width - (RUNTIME_PANEL_INSET_PX * 2.0),
        height: RUNTIME_ROW_HEIGHT_PX,
    }
}

pub fn bulk_manager_visible_window_start(cursor_index: usize, visible_row_count: usize) -> usize {
    if visible_row_count <= RUNTIME_VISIBLE_ROW_LIMIT {
        0
    } else {
        cursor_index
            .min(visible_row_count - 1)
            .saturating_add(1)
            .saturating_sub(RUNTIME_VISIBLE_ROW_LIMIT)
    }
}

pub fn bulk_manager_visible_window_summary(
    visible_window_start: usize,
    visible_row_count: usize,
) -> Option<SmolStr> {
    if visible_row_count <= RUNTIME_VISIBLE_ROW_LIMIT {
        return None;
    }
    let visible_start =
        visible_window_start.min(visible_row_count.saturating_sub(RUNTIME_VISIBLE_ROW_LIMIT));
    let visible_end = visible_row_count.min(visible_start + RUNTIME_VISIBLE_ROW_LIMIT);
    Some(SmolStr::new(format!(
        "Rows {}-{} of {}",
        visible_start + 1,
        visible_end,
        visible_row_count
    )))
}

pub fn bulk_manager_hit_test(
    viewport: Size,
    visible_row_count: usize,
    visible_window_start: usize,
    x: f32,
    y: f32,
) -> Option<BulkManagerPointerHit> {
    if rect_contains(bulk_manager_close_rect(viewport), x, y) {
        return Some(BulkManagerPointerHit::Close);
    }
    if rect_contains(bulk_manager_search_rect(viewport), x, y) {
        return Some(BulkManagerPointerHit::SearchInput);
    }
    for key in SortKey::ALL {
        if rect_contains(bulk_manager_sort_header_rect(viewport, *key), x, y) {
            return Some(BulkManagerPointerHit::Sort(*key));
        }
    }
    for spec in BULK_MANAGER_ACTION_BUTTONS {
        if rect_contains(bulk_manager_button_rect(viewport, *spec), x, y) {
            return Some(spec.hit);
        }
    }
    let visible_start =
        visible_window_start.min(visible_row_count.saturating_sub(RUNTIME_VISIBLE_ROW_LIMIT));
    let visible_end = visible_row_count.min(visible_start + RUNTIME_VISIBLE_ROW_LIMIT);
    for (display_index, row_index) in (visible_start..visible_end).enumerate() {
        if rect_contains(bulk_manager_row_rect(viewport, display_index), x, y) {
            return Some(BulkManagerPointerHit::Row(row_index));
        }
    }
    None
}

fn table_column_fractions(key: SortKey) -> (f32, f32) {
    match key {
        SortKey::Name => (0.0, 0.54),
        SortKey::Items => (0.54, 0.12),
        SortKey::Accent => (0.66, 0.18),
        SortKey::Size => (0.84, 0.16),
    }
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

mod state;

pub use state::*;

#[cfg(test)]
mod tests;
