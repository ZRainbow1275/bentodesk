//! SnapshotPicker — modal dialog for layout snapshots.
//!
//! Visual spec: `snapshot_picker.snap.md` (440 px modal, list of named
//! snapshots with Load + Delete actions). Composition lands when widget-
//! library ships List (T-026) and Modal (T-023). Today's chrome is a
//! typed Container plus the row state machine that the body composition
//! will dispatch off directly.
//!
//! Backend dep: the snapshot model lives in
//! `bento_nano_backend::layout::DesktopSnapshot` (T-097, shipped); list /
//! load / delete operations are layered on top of `CheckpointStore` plus
//! the layout module's `SnapshotStore` API.

use bento_nano_backend::layout::DesktopSnapshot;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_widget::WidgetNode;
use smol_str::SmolStr;

use super::default_modal_chrome;

// -----------------------------------------------------------------------------
// Snap.md derived geometry constants — pinned per visual spec.
// -----------------------------------------------------------------------------

/// Modal width in DIPs — `.snapshot-picker { width: 440px }`.
pub const PANEL_WIDTH: f32 = 440.0;

/// Maximum modal height as fraction of viewport — `max-height: 70vh`.
/// Note: this is 70vh, NOT the SettingsPanel/TimelinePanel 80vh — the
/// snapshot list deliberately sits a bit shorter so it nests visually
/// under those panels when stacked.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.70;

/// Header height — `.snapshot-picker__header { height: 52px }`.
pub const HEADER_HEIGHT: f32 = 52.0;

/// Header horizontal padding — `padding: 0 var(--spacing-xl)` resolves to
/// 20 px in the BentoDesk 1.x design tokens.
pub const HEADER_PADDING_X: f32 = 20.0;

/// Body padding — `padding: var(--spacing-lg) var(--spacing-xl)`.
pub const BODY_PADDING_X: f32 = 20.0;
pub const BODY_PADDING_Y: f32 = 16.0;

/// Open animation duration — `dialogScaleIn` 200 ms ease-out, matches
/// every other modal in the suite.
pub const PANEL_OPEN_DURATION_MS: u32 = 200;

/// Left/right inset used by the D2D runtime renderer.
pub const RUNTIME_PANEL_INSET_PX: f32 = 18.0;

/// Square close target in the self-painted native title bar.
pub const RUNTIME_CLOSE_BUTTON_SIZE_PX: f32 = 32.0;

/// Runtime action button height in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_HEIGHT_PX: f32 = 28.0;

/// Runtime action button top in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_TOP_PX: f32 = 108.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 148.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 44.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 52.0;

/// The current runtime renderer shows at most eight visible rows.
pub const RUNTIME_VISIBLE_ROW_LIMIT: usize = 8;

/// Snapshot thumbnail colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotThumbnailChrome {
    /// Thumbnail outer radius.
    pub border_radius: BorderRadius,
    /// Thumbnail content radius.
    pub content_radius: BorderRadius,
    /// Per-zone preview radius.
    pub zone_radius: BorderRadius,
    /// Thumbnail outer border colour.
    pub border_color: Color,
    /// Thumbnail content background colour.
    pub background_color: Color,
    /// Fallback zone fill colour when a zone has no explicit accent.
    pub fallback_zone_color: Color,
    /// Empty-state text colour.
    pub empty_text_color: Color,
}

impl SnapshotThumbnailChrome {
    /// Build snapshot thumbnail chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT)
    }

    /// Build snapshot thumbnail chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens) -> Self {
        Self {
            border_radius: radius.md,
            content_radius: radius.md,
            zone_radius: radius.sm,
            border_color: palette.border,
            background_color: palette.hover_overlay,
            fallback_zone_color: palette.accent,
            empty_text_color: palette.text_muted,
        }
    }

    /// Build snapshot thumbnail chrome from Wave B Tauri SSoT tokens.
    pub fn from_tauri_tokens(palette: PaletteTauri, radius: RadiusTauri) -> Self {
        Self {
            border_radius: BorderRadius::all(radius.card),
            content_radius: BorderRadius::all(radius.card),
            zone_radius: BorderRadius::all(radius.card),
            border_color: palette.border_expanded,
            background_color: palette.surface_subtle,
            fallback_zone_color: palette.accent_blue,
            empty_text_color: palette.text_muted,
        }
    }
}

/// SnapshotPicker colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotPickerChrome {
    /// Drop shadow descriptor drawn behind the panel.
    pub panel_shadow: Shadow,
    /// Panel radius.
    pub panel_radius: BorderRadius,
    /// Action button radius.
    pub button_radius: BorderRadius,
    /// Snapshot row radius.
    pub row_radius: BorderRadius,
    /// Panel fill colour.
    pub panel_background: Color,
    /// Default row fill colour.
    pub row_background: Color,
    /// Selected row fill colour.
    pub selected_background: Color,
    /// Action button fill colour.
    pub action_background: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
    /// Error/status text colour.
    pub error_color: Color,
    /// Shared thumbnail chrome for embedded snapshot previews.
    pub thumbnail_chrome: SnapshotThumbnailChrome,
}

impl SnapshotPickerChrome {
    /// Build SnapshotPicker chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build SnapshotPicker chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            button_radius: radius.md,
            row_radius: radius.lg,
            panel_background: palette.surface,
            row_background: palette.surface_alt,
            selected_background: palette.selection,
            action_background: palette.active_overlay,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
            error_color: palette.danger,
            thumbnail_chrome: SnapshotThumbnailChrome::from_tokens(palette, radius),
        }
    }

    /// Build SnapshotPicker chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `timeline-snapshot.md`):
    /// - panel bg ← `surface_expanded`
    /// - row bg ← `surface_subtle`; selected ← `surface_active`
    /// - action button bg ← `accent_blue` (primary save action)
    /// - panel radius ← `expanded` (16); button/row ← `card` (10)
    /// - panel shadow ← `expanded` outer layer; error ← `accent_red`
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            button_radius: BorderRadius::all(radius.card),
            row_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            row_background: palette.surface_subtle,
            selected_background: palette.surface_active,
            action_background: palette.accent_blue,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
            error_color: palette.accent_red,
            thumbnail_chrome: SnapshotThumbnailChrome::from_tauri_tokens(palette, radius),
        }
    }
}

/// Pointer hit target in the runtime D2D SnapshotPicker panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotPickerPointerHit {
    Save,
    Load,
    Delete,
    Timeline,
    Close,
    Row(usize),
}

/// Static action-button descriptor shared by renderer and shell hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotPickerButtonSpec {
    pub hit: SnapshotPickerPointerHit,
    pub label: &'static str,
    pub x_offset: f32,
    pub width: f32,
}

pub const SNAPSHOT_PICKER_ACTION_BUTTONS: &[SnapshotPickerButtonSpec] = &[
    SnapshotPickerButtonSpec {
        hit: SnapshotPickerPointerHit::Save,
        label: "Save",
        x_offset: 0.0,
        width: 58.0,
    },
    SnapshotPickerButtonSpec {
        hit: SnapshotPickerPointerHit::Load,
        label: "Load",
        x_offset: 66.0,
        width: 58.0,
    },
    SnapshotPickerButtonSpec {
        hit: SnapshotPickerPointerHit::Delete,
        label: "Delete",
        x_offset: 132.0,
        width: 64.0,
    },
    SnapshotPickerButtonSpec {
        hit: SnapshotPickerPointerHit::Timeline,
        label: "Timeline",
        x_offset: 204.0,
        width: 82.0,
    },
];

pub fn snapshot_picker_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
    }
}

/// Header close button.  Close is deliberately not another toolbar pill: a
/// title-bar affordance gives the modal one obvious dismissal point and keeps
/// the data actions visually separate from window chrome.
pub fn snapshot_picker_close_rect(viewport: Size) -> Rect {
    let panel = snapshot_picker_panel_rect(viewport);
    Rect {
        x: panel.right() - RUNTIME_PANEL_INSET_PX - RUNTIME_CLOSE_BUTTON_SIZE_PX,
        y: panel.y + 10.0,
        width: RUNTIME_CLOSE_BUTTON_SIZE_PX,
        height: RUNTIME_CLOSE_BUTTON_SIZE_PX,
    }
}

pub fn snapshot_picker_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

pub fn snapshot_picker_button_rect(viewport: Size, spec: SnapshotPickerButtonSpec) -> Rect {
    let panel = snapshot_picker_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX + spec.x_offset,
        y: panel.y + RUNTIME_ACTION_BUTTON_TOP_PX,
        width: spec.width,
        height: RUNTIME_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn snapshot_picker_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = snapshot_picker_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX,
        y: panel.y + RUNTIME_ROW_TOP_PX + (row_index as f32 * RUNTIME_ROW_STRIDE_PX),
        width: panel.width - (RUNTIME_PANEL_INSET_PX * 2.0),
        height: RUNTIME_ROW_HEIGHT_PX,
    }
}

pub fn snapshot_picker_hit_test(
    viewport: Size,
    visible_row_count: usize,
    x: f32,
    y: f32,
) -> Option<SnapshotPickerPointerHit> {
    if rect_contains(snapshot_picker_close_rect(viewport), x, y) {
        return Some(SnapshotPickerPointerHit::Close);
    }
    for spec in SNAPSHOT_PICKER_ACTION_BUTTONS {
        if rect_contains(snapshot_picker_button_rect(viewport, *spec), x, y) {
            return Some(spec.hit);
        }
    }
    for row_index in 0..visible_row_count.min(RUNTIME_VISIBLE_ROW_LIMIT) {
        if rect_contains(snapshot_picker_row_rect(viewport, row_index), x, y) {
            return Some(SnapshotPickerPointerHit::Row(row_index));
        }
    }
    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

// -----------------------------------------------------------------------------
// Per-row state — confirm-delete two-step gesture.
// -----------------------------------------------------------------------------

/// Row interaction state — mirrors the 1.x `confirmDeleteId` signal as a
/// closed enum so the body composition's match arms are exhaustive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum RowAction {
    /// Default — Load + Delete buttons visible.
    #[default]
    Default,
    /// Delete was clicked once on the named row id; now showing
    /// "confirm-text + Yes + No" until either is tapped or the user
    /// clicks Delete on a different row (which resets to `Default`).
    AwaitingDeleteConfirmation { row_id: SmolStr },
}

impl RowAction {
    /// `true` when the given row id is in the awaiting-confirmation state.
    /// Other rows always render in the `Default` state regardless of which
    /// row owns the confirmation gesture (1.x: only one row can be in
    /// confirm at any time).
    pub fn is_awaiting_for(&self, row_id: &str) -> bool {
        matches!(self, Self::AwaitingDeleteConfirmation { row_id: id } if id.as_str() == row_id)
    }

    /// Begin awaiting confirmation for `row_id`. Replaces any prior
    /// confirm gesture (matches the 1.x `setConfirmDeleteId(id)`).
    pub fn begin_confirm(row_id: impl Into<SmolStr>) -> Self {
        Self::AwaitingDeleteConfirmation {
            row_id: row_id.into(),
        }
    }
}

// -----------------------------------------------------------------------------
// Display helpers — pulled in today so the body composition pass doesn't
// have to re-derive them.
// -----------------------------------------------------------------------------

/// Format the snapshot meta line per the 1.x `<n> Zones • <w>x<h> • <date>`
/// composition. Uses `\u{2022}` (U+2022 BULLET) verbatim so the rendered
/// glyph matches screenshots.
pub fn meta_line(snapshot: &DesktopSnapshot, date_label: &str, zones_word: &str) -> SmolStr {
    let zone_count = snapshot.zones.len();
    let res = &snapshot.resolution;
    SmolStr::from(format!(
        "{zone_count} {zones_word} \u{2022} {}x{} \u{2022} {date_label}",
        res.width, res.height
    ))
}

// -----------------------------------------------------------------------------
// Runtime panel state — selected-stack shell owns IO; renderer consumes this.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SnapshotPickerState {
    entries: Vec<DesktopSnapshot>,
    cursor: usize,
    status: Option<SmolStr>,
    error: Option<SmolStr>,
    row_action: RowAction,
}

impl SnapshotPickerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[DesktopSnapshot] {
        &self.entries
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor
    }

    pub fn status(&self) -> Option<&SmolStr> {
        self.status.as_ref()
    }

    pub fn error(&self) -> Option<&SmolStr> {
        self.error.as_ref()
    }

    pub fn row_action(&self) -> &RowAction {
        &self.row_action
    }

    pub fn selected_id(&self) -> Option<SmolStr> {
        self.entries.get(self.cursor).map(|entry| entry.id.clone())
    }

    pub fn set_entries(&mut self, entries: Vec<DesktopSnapshot>) {
        self.entries = entries;
        if self.entries.is_empty() {
            self.cursor = 0;
            self.row_action = RowAction::Default;
        } else if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len() - 1;
        }
    }

    pub fn set_status(&mut self, status: impl Into<SmolStr>) {
        self.status = Some(status.into());
        self.error = None;
    }

    pub fn set_error(&mut self, error: impl Into<SmolStr>) {
        self.error = Some(error.into());
    }

    pub fn clear_status(&mut self) {
        self.status = None;
        self.error = None;
    }

    pub fn begin_delete_confirm(&mut self, row_id: impl Into<SmolStr>) {
        self.row_action = RowAction::begin_confirm(row_id);
    }

    pub fn clear_delete_confirm(&mut self) {
        self.row_action = RowAction::Default;
    }

    pub fn select_prev(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = self.cursor.saturating_sub(1);
            self.clear_delete_confirm();
        }
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = (self.cursor + 1).min(self.entries.len() - 1);
            self.clear_delete_confirm();
        }
    }

    pub fn select_index(&mut self, index: usize) -> bool {
        if index < self.entries.len() {
            self.cursor = index;
            self.clear_delete_confirm();
            true
        } else {
            false
        }
    }
}

// -----------------------------------------------------------------------------
// Builder — returns the chrome Container today.
// -----------------------------------------------------------------------------

/// Build the SnapshotPicker widget subtree. Returns the chrome Container
/// today; the header + body + per-row actions compose when widget-library
/// ships List + Modal.
pub fn build() -> WidgetNode {
    // 0 padding on the outer chrome — the inner header + body each carry
    // their own padding per snap.md so the outer chrome stays at zero to
    // avoid a "double padding" off-by-20-px landmine when the body
    // composes inside.
    WidgetNode::Container(default_modal_chrome(Edges::ZERO))
}

#[cfg(test)]
mod tests;
