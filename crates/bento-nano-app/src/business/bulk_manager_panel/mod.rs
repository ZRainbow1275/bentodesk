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

use bento_nano_layout::Direction;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bento_nano_theme as theme;
use bento_nano_theme::PaletteTokens;
use bento_nano_widget::{ContainerNode, WidgetNode};
use bento_nano_zone::ZoneId;
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
pub const RUNTIME_ACTION_BUTTON_HEIGHT_PX: f32 = 22.0;

/// Runtime search field maximum length in Unicode scalar values.
pub const RUNTIME_SEARCH_LIMIT: usize = 80;

/// Runtime sort-header row top in the D2D aux panel.
pub const RUNTIME_SORT_HEADER_TOP_PX: f32 = 160.0;

/// Runtime sort-header row height in the D2D aux panel.
pub const RUNTIME_SORT_HEADER_HEIGHT_PX: f32 = 14.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 176.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 34.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 42.0;

/// The current runtime renderer shows at most eight visible rows.
pub const RUNTIME_VISIBLE_ROW_LIMIT: usize = 8;

/// BulkManager colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulkManagerChrome {
    /// Drop shadow descriptor drawn behind the panel.
    pub panel_shadow: bento_nano_style::Shadow,
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
        y_offset: 106.0,
        width: 44.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Invert,
        label: "Invert",
        x_offset: 50.0,
        y_offset: 106.0,
        width: 58.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Hide,
        label: "Hide",
        x_offset: 114.0,
        y_offset: 106.0,
        width: 50.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Show,
        label: "Show",
        x_offset: 170.0,
        y_offset: 106.0,
        width: 50.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutGrid,
        label: "Grid",
        x_offset: 226.0,
        y_offset: 106.0,
        width: 50.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutRow,
        label: "Row",
        x_offset: 282.0,
        y_offset: 106.0,
        width: 50.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutColumn,
        label: "Col",
        x_offset: 0.0,
        y_offset: 136.0,
        width: 44.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutSpiral,
        label: "Spiral",
        x_offset: 50.0,
        y_offset: 136.0,
        width: 56.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::LayoutOrganic,
        label: "Organic",
        x_offset: 112.0,
        y_offset: 136.0,
        width: 68.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Update,
        label: "Update",
        x_offset: 186.0,
        y_offset: 136.0,
        width: 62.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::TextEdit,
        label: "Text",
        x_offset: 438.0,
        y_offset: 136.0,
        width: 50.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::IconPicker,
        label: "Icon",
        x_offset: 494.0,
        y_offset: 136.0,
        width: 48.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::AccentPicker,
        label: "Color",
        x_offset: 548.0,
        y_offset: 136.0,
        width: 56.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Delete,
        label: "Delete",
        x_offset: 254.0,
        y_offset: 136.0,
        width: 58.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Move,
        label: "Move",
        x_offset: 318.0,
        y_offset: 136.0,
        width: 52.0,
    },
    BulkManagerButtonSpec {
        hit: BulkManagerPointerHit::Close,
        label: "Close",
        x_offset: 376.0,
        y_offset: 136.0,
        width: 54.0,
    },
];

pub fn bulk_manager_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: RUNTIME_PANEL_MARGIN_PX,
        y: RUNTIME_PANEL_MARGIN_PX,
        width: (viewport.width - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(480.0),
        height: (viewport.height - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(360.0),
    }
}

pub fn bulk_manager_panel_shadow_rect(panel: Rect, shadow: bento_nano_style::Shadow) -> Rect {
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
        x: panel.x + panel.width - RUNTIME_PANEL_INSET_PX - SEARCH_INPUT_WIDTH_PX,
        y: panel.y + 18.0,
        width: SEARCH_INPUT_WIDTH_PX,
        height: SEARCH_INPUT_HEIGHT_PX,
    }
}

/// Runtime sortable column-header rectangle shared by renderer and hit-testing.
pub fn bulk_manager_sort_header_rect(viewport: Size, key: SortKey) -> Rect {
    let panel = bulk_manager_panel_rect(viewport);
    let table_x = panel.x + RUNTIME_PANEL_INSET_PX;
    let table_width = panel.width - (RUNTIME_PANEL_INSET_PX * 2.0);
    let (x_fraction, width_fraction) = match key {
        SortKey::Name => (0.0, 0.46),
        SortKey::Items => (0.46, 0.14),
        SortKey::Accent => (0.60, 0.20),
        SortKey::Size => (0.80, 0.20),
    };
    Rect {
        x: table_x + (table_width * x_fraction),
        y: panel.y + RUNTIME_SORT_HEADER_TOP_PX,
        width: table_width * width_fraction,
        height: RUNTIME_SORT_HEADER_HEIGHT_PX,
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

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

// -----------------------------------------------------------------------------
// Table column metadata — drives sort cycling + column header rendering.
// -----------------------------------------------------------------------------

/// One sortable column in the zone table. The sort key cycles through
/// `Name → Items → Accent → Size`; same-key clicks toggle direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    #[default]
    Name,
    Items,
    Accent,
    Size,
}

impl SortKey {
    /// Iteration order matches snap.md (left → right across the table).
    pub const ALL: &'static [Self] = &[Self::Name, Self::Items, Self::Accent, Self::Size];

    /// Wire-format token for serde / scripting forward-compat.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Items => "items",
            Self::Accent => "accent",
            Self::Size => "size",
        }
    }

    /// Static label for the column header cell.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Items => "Items",
            Self::Accent => "Accent",
            Self::Size => "Size",
        }
    }
}

/// Sort direction — ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    /// Flip ascending ⇄ descending.
    pub const fn flipped(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

// -----------------------------------------------------------------------------
// ZoneRow — one entry the panel renders in the table. Mirrors the 1.x
// `BentoZone` slice that the bulk panel actually reads.
// -----------------------------------------------------------------------------

/// One row in the zone table. Pruned shape — only the columns the panel
/// renders + the [`ZoneId`] needed to key selection / dispatch.
///
/// Built by the shell from the live `BentoZone` list before pumping into
/// [`BulkManagerState::set_zones`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneRow {
    /// Stable zone id — selection + bulk-action dispatch key.
    pub id: ZoneId,
    /// Display name (alias if the user set one, else the canonical name).
    pub display_name: SmolStr,
    /// Item count rendered in the Items column.
    pub item_count: u32,
    /// Accent colour hex string (`#rrggbb`); empty when unset.
    pub accent_hex: SmolStr,
    /// Whether the zone is currently rendered on the desktop canvas.
    pub visible: bool,
    /// Whether user layout helpers should leave the zone in place.
    pub locked: bool,
    /// Icon slug currently assigned to the zone.
    pub icon_slug: SmolStr,
    /// Capsule size token currently assigned to the zone.
    pub capsule_size: SmolStr,
    /// Display mode override, or `inherit` when unset.
    pub display_mode: SmolStr,
    /// Width % of the canvas (0..=100).
    pub width_percent: u32,
    /// Height % of the canvas (0..=100).
    pub height_percent: u32,
    /// Position x % of the canvas (0..=100).
    pub position_x_percent: u32,
    /// Position y % of the canvas (0..=100).
    pub position_y_percent: u32,
}

impl ZoneRow {
    /// Area metric used by `SortKey::Size` (`w% × h%`).
    pub fn area_percent(&self) -> u64 {
        u64::from(self.width_percent) * u64::from(self.height_percent)
    }
}

// -----------------------------------------------------------------------------
// BulkManagerAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the panel state machine. Drained once per
/// frame via [`BulkManagerState::take_action`]. The shell sequences
/// the appropriate per-zone dispatcher Commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulkManagerAction {
    /// Hide the listed zones via `Command::BulkSetZonesVisible`.
    Hide { ids: Vec<ZoneId> },
    /// Show the listed zones via `Command::BulkSetZonesVisible`.
    Show { ids: Vec<ZoneId> },
    /// Delete the listed zones via `Command::BulkDeleteZones`.
    Delete { ids: Vec<ZoneId> },
    /// Move the listed zones by `delta` via `Command::BulkMoveZones`.
    Move { ids: Vec<ZoneId>, delta: Point },
    /// User dismissed the panel (close button, Escape, or scrim click).
    /// Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for BulkManagerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hide { ids } => write!(f, "Hide({})", ids.len()),
            Self::Show { ids } => write!(f, "Show({})", ids.len()),
            Self::Delete { ids } => write!(f, "Delete({})", ids.len()),
            Self::Move { ids, delta } => {
                write!(f, "Move({}, dx={}, dy={})", ids.len(), delta.x, delta.y)
            }
            Self::Close => f.write_str("Close"),
        }
    }
}

// -----------------------------------------------------------------------------
// BulkManagerState — runtime state for the panel.
// -----------------------------------------------------------------------------

/// Panel runtime state.
///
/// - `zones` — full row list as last seeded by the shell.
/// - `search` — current search filter (case-insensitive substring match
///   on `display_name`). Empty string disables the filter.
/// - `sort_key` / `sort_direction` — table sort. Cycle direction on
///   same-key click.
/// - `selected` — set of currently-selected zone ids (inline buffer for
///   the steady-state ≤ 8 batch).
/// - `cursor_index` — keyboard-focused visible row; selection remains keyed
///   by `ZoneId`, not row index.
/// - `pending_action` — latest one-shot [`BulkManagerAction`] the shell
///   has yet to drain.
#[derive(Debug, Default)]
pub struct BulkManagerState {
    zones: Vec<ZoneRow>,
    search: String,
    sort_key: SortKey,
    sort_direction: SortDirection,
    selected: SmallVec<[ZoneId; 8]>,
    cursor_index: usize,
    pending_action: Option<BulkManagerAction>,
    text_edit: Option<BulkTextEditState>,
    search_focused: bool,
    delete_confirm_ids: SmallVec<[ZoneId; 8]>,
}

impl BulkManagerState {
    /// New empty state. The shell calls [`set_zones`] before the first
    /// paint with the live zone list.
    ///
    /// [`set_zones`]: BulkManagerState::set_zones
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the zone list — typically called after a refresh. Resets
    /// the selection (old ids may be stale) and drops any pending
    /// action (it referenced the old set).
    pub fn set_zones(&mut self, zones: Vec<ZoneRow>) {
        self.zones = zones;
        self.selected.clear();
        self.cursor_index = 0;
        self.pending_action = None;
        self.text_edit = None;
        self.delete_confirm_ids.clear();
    }

    /// Borrow the current zone row list (pre-search, pre-sort).
    pub fn zones(&self) -> &[ZoneRow] {
        &self.zones
    }

    /// Borrow the current search input.
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Update the search input. Selection survives (the panel does not
    /// touch `selected` when the search filter changes).
    pub fn set_search(&mut self, value: impl Into<String>) {
        self.search = value.into().chars().take(RUNTIME_SEARCH_LIMIT).collect();
        self.clamp_cursor();
    }

    /// Whether WM_CHAR input currently targets the search filter.
    pub fn search_focused(&self) -> bool {
        self.search_focused
    }

    /// Focus the search filter and cancel any active typed metadata edit.
    pub fn focus_search(&mut self) {
        self.cancel_text_edit();
        self.search_focused = true;
    }

    /// Blur the search filter without changing the current filter text.
    pub fn blur_search(&mut self) {
        self.search_focused = false;
    }

    /// Append one user-typed character to the search filter.
    pub fn push_search_char(&mut self, ch: char) -> bool {
        if ch.is_control() || self.search.chars().count() >= RUNTIME_SEARCH_LIMIT {
            return false;
        }
        self.search.push(ch);
        self.clamp_cursor();
        true
    }

    /// Remove the last character from the search filter.
    pub fn backspace_search(&mut self) -> bool {
        let changed = self.search.pop().is_some();
        if changed {
            self.clamp_cursor();
        }
        changed
    }

    /// Clear the search filter.
    pub fn clear_search(&mut self) -> bool {
        if self.search.is_empty() {
            return false;
        }
        self.search.clear();
        self.clamp_cursor();
        true
    }

    /// Borrow the current sort key.
    pub fn sort_key(&self) -> SortKey {
        self.sort_key
    }

    /// Borrow the current sort direction.
    pub fn sort_direction(&self) -> SortDirection {
        self.sort_direction
    }

    /// Click on a column header. Same-key clicks toggle direction;
    /// different-key clicks snap direction back to ascending.
    pub fn set_sort_key(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_direction = self.sort_direction.flipped();
        } else {
            self.sort_key = key;
            self.sort_direction = SortDirection::Ascending;
        }
    }

    /// Borrow the current selection.
    pub fn selected(&self) -> &[ZoneId] {
        &self.selected
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor_index
    }

    pub fn cursor_zone_id(&self) -> Option<ZoneId> {
        let visible = self.visible_rows();
        visible.get(self.cursor_index).map(|row| row.id)
    }

    pub fn set_cursor_index(&mut self, index: usize) {
        let visible_count = self.visible_count();
        self.cursor_index = if visible_count == 0 {
            0
        } else {
            index.min(visible_count - 1)
        };
    }

    pub fn select_next(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else {
            self.cursor_index = (self.cursor_index + 1) % visible_count;
        }
    }

    pub fn select_prev(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index == 0 {
            self.cursor_index = visible_count - 1;
        } else {
            self.cursor_index -= 1;
        }
    }

    /// Whether `id` is currently selected.
    pub fn is_selected(&self, id: ZoneId) -> bool {
        self.selected.contains(&id)
    }

    /// Toggle the membership of `id` in the selection set.
    pub fn toggle_selection(&mut self, id: ZoneId) {
        if let Some(idx) = self.selected.iter().position(|s| *s == id) {
            self.selected.remove(idx);
        } else {
            self.selected.push(id);
        }
        self.clear_delete_confirmation();
    }

    pub fn toggle_cursor_selection(&mut self) {
        if let Some(id) = self.cursor_zone_id() {
            self.toggle_selection(id);
        }
    }

    pub fn toggle_visible_row_selection(&mut self, index: usize) {
        let visible = self.visible_rows();
        if let Some(row) = visible.get(index) {
            self.cursor_index = index;
            self.toggle_selection(row.id);
        }
    }

    /// Add every visible row's id to the selection (visible = post-
    /// search filter). Idempotent: duplicates are skipped.
    pub fn select_all(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        for id in visible_ids {
            if !self.selected.contains(&id) {
                self.selected.push(id);
            }
        }
        self.clear_delete_confirmation();
    }

    /// Remove every visible row's id from the selection. Off-screen
    /// (search-filtered) selections survive.
    pub fn deselect_all(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        self.selected.retain(|id| !visible_ids.contains(id));
        self.clear_delete_confirmation();
    }

    /// Flip selection membership for every visible row's id.
    pub fn invert_selection(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        for id in visible_ids {
            if let Some(idx) = self.selected.iter().position(|s| *s == id) {
                self.selected.remove(idx);
            } else {
                self.selected.push(id);
            }
        }
        self.clear_delete_confirmation();
    }

    /// Whether every visible row is currently selected (the header
    /// checkbox renders "deselect all" in that state).
    pub fn all_visible_selected(&self) -> bool {
        let visible = self.visible_rows();
        if visible.is_empty() {
            return false;
        }
        visible.iter().all(|r| self.is_selected(r.id))
    }

    /// Snapshot the visible row set: filter by search, then sort by the
    /// current key + direction. Returns owned `Vec<ZoneRow>` because the
    /// sort step needs an owned copy anyway; callers that only need
    /// length should use [`visible_count`] to skip the clone.
    ///
    /// [`visible_count`]: BulkManagerState::visible_count
    pub fn visible_rows(&self) -> Vec<ZoneRow> {
        let term = self.search.trim().to_lowercase();
        let mut rows: Vec<ZoneRow> = if term.is_empty() {
            self.zones.clone()
        } else {
            self.zones
                .iter()
                .filter(|r| r.display_name.to_lowercase().contains(&term))
                .cloned()
                .collect()
        };
        rows.sort_by(|a, b| {
            let cmp = match self.sort_key {
                SortKey::Name => a.display_name.cmp(&b.display_name),
                SortKey::Items => a.item_count.cmp(&b.item_count),
                SortKey::Accent => a.accent_hex.cmp(&b.accent_hex),
                SortKey::Size => a.area_percent().cmp(&b.area_percent()),
            };
            match self.sort_direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
        rows
    }

    /// Number of rows that pass the search filter. Cheaper than
    /// [`visible_rows`] when callers only need the count.
    ///
    /// [`visible_rows`]: BulkManagerState::visible_rows
    pub fn visible_count(&self) -> usize {
        let term = self.search.trim().to_lowercase();
        if term.is_empty() {
            self.zones.len()
        } else {
            self.zones
                .iter()
                .filter(|r| r.display_name.to_lowercase().contains(&term))
                .count()
        }
    }

    fn clamp_cursor(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index >= visible_count {
            self.cursor_index = visible_count - 1;
        }
    }

    /// Whether any bulk action button should render enabled.
    pub fn can_act(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Borrow the selected ids currently awaiting a destructive delete
    /// confirmation.
    pub fn delete_confirmation(&self) -> &[ZoneId] {
        &self.delete_confirm_ids
    }

    /// Clear any pending destructive delete confirmation.
    pub fn clear_delete_confirmation(&mut self) {
        self.delete_confirm_ids.clear();
    }

    fn delete_confirmation_matches_selection(&self) -> bool {
        !self.selected.is_empty()
            && self.selected.len() == self.delete_confirm_ids.len()
            && self
                .selected
                .iter()
                .all(|id| self.delete_confirm_ids.contains(id))
    }

    /// Two-step destructive delete guard. The first call records the current
    /// selected ids and returns `None`; a second call with the same selection
    /// returns the ids to delete and clears the pending confirmation.
    pub fn confirm_delete_or_arm(&mut self) -> Option<Vec<ZoneId>> {
        if !self.can_act() {
            self.clear_delete_confirmation();
            return None;
        }
        if self.delete_confirmation_matches_selection() {
            let ids = self.selected.to_vec();
            self.clear_delete_confirmation();
            Some(ids)
        } else {
            self.delete_confirm_ids.clear();
            self.delete_confirm_ids
                .extend(self.selected.iter().copied());
            None
        }
    }

    /// User clicked the Hide button.
    pub fn click_hide(&mut self) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Hide {
            ids: self.selected.to_vec(),
        });
    }

    /// User clicked the Show button.
    pub fn click_show(&mut self) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Show {
            ids: self.selected.to_vec(),
        });
    }

    /// User clicked the Delete button.
    pub fn click_delete(&mut self) {
        if !self.can_act() {
            return;
        }
        if let Some(ids) = self.confirm_delete_or_arm() {
            self.pending_action = Some(BulkManagerAction::Delete { ids });
        }
    }

    /// User clicked the Move… button. Shell collects the delta from a
    /// secondary input (1.x: separate dialog with x/y fields); the panel
    /// only records intent + the resolved delta.
    pub fn click_move(&mut self, delta: Point) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Move {
            ids: self.selected.to_vec(),
            delta,
        });
    }

    /// User clicked the close button / pressed Escape / clicked the
    /// scrim.
    pub fn click_close(&mut self) {
        self.pending_action = Some(BulkManagerAction::Close);
    }

    /// Drain the latest action — one-shot. Returns `None` until the
    /// user clicks something next.
    pub fn take_action(&mut self) -> Option<BulkManagerAction> {
        self.pending_action.take()
    }

    /// Whether an action is currently pending (diagnostics + UI gating).
    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the BulkManagerPanel chrome subtree. Returns the chrome
/// Container today; the header / toolbar / table / footer composition
/// attaches when widget-library ships the final Modal + Grid + List
/// primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let chrome = BulkManagerChrome::from_palette(theme::current().palette);
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(PANEL_WIDTH_PX),
        height: Length::Px(PANEL_HEIGHT_PX),
        padding: Edges::all(PANEL_PADDING_PX),
        background: chrome.panel_background,
        radius: chrome.panel_radius,
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests live in `tests.rs` sibling so this `mod.rs` stays under the §15
// 800-LOC budget; see that file for the full unit + smoke surface.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests;
