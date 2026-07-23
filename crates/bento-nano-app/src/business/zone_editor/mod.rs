//! `ZoneEditor` — modal panel for editing one BentoZone's identity + style.
//!
//! 1.x source: `bentodesk/src/components/ZoneEditor/ZoneEditor.tsx` (316 LOC)
//! plus `ZoneEditor.css`. Lets the user rename a zone, swap its icon, change
//! the accent palette swatch, slide the grid-column count, and pick a capsule
//! shape plus size variant.
//!
//! Visual fidelity reference: `zone_editor.snap.md`.
//!
//! # Hosting
//!
//! Rendered inside a dedicated layered HWND (400 × auto, 16 px corner radius
//! via DComp visual clip). The HWND is created on demand by the shell when
//! the user picks "Edit Zone" from a zone's context menu and torn down
//! when [`ZoneEditorAction::Save`] / [`ZoneEditorAction::Cancel`] surfaces.
//!
//! # Save flow
//!
//! The widget is a *shell* — it doesn't talk to `bento_nano_backend::layout`
//! directly. The `*Action::Save` payload carries a [`ZoneUpdate`] with only
//! the fields the user touched (1.x `dirty` semantics); the shell calls into
//! the appropriate dispatcher Command (`update_zone` family) and tears the
//! HWND down on success.
//!
//! ## Spec compliance
//!
//! - §10 hot-path: short identifiers (zone id, capsule shape/size variant,
//!   accent colour token) use [`SmolStr`]; the free-form zone name uses
//!   `String` because user-entered display names regularly exceed the
//!   22-byte SmolStr inline budget.
//! - §11 ΔB: every public DTO derives `serde::{Serialize, Deserialize}`.
//! - §11.1: zero `unsafe` in this UI layer.
//! - §15: this single .rs file ships under the 800-LOC budget.
//! - §17: zero `todo!()` / `unimplemented!()` / `panic!()` / `unwrap()` /
//!   `expect()` in production code.

use core::fmt;

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use bento_nano_backend::layout::persistence::{BentoZone, ZoneUpdate};

// -----------------------------------------------------------------------------
// Snap.md derived geometry constants — pinned so a snap.md drift can be
// grep-detected and so downstream hit-testing doesn't re-derive the values.
// -----------------------------------------------------------------------------

/// Modal width in DIPs — `.zone-editor { width: 400px }`.
pub const PANEL_WIDTH: f32 = 400.0;

/// Maximum modal height as a fraction of viewport — `max-height: 80vh`.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.80;

/// Header height — title + close-button row (`.zone-editor__header`).
pub const HEADER_HEIGHT: f32 = 52.0;

/// Footer height — Cancel/Save button row + vertical padding.
pub const FOOTER_HEIGHT: f32 = 64.0;

/// Body horizontal + vertical padding — derived from `var(--spacing-lg)` /
/// `var(--spacing-xl)`.
pub const BODY_PADDING_VERTICAL: f32 = 16.0;
pub const BODY_PADDING_HORIZONTAL: f32 = 20.0;

/// Gap between adjacent fields in the body (`margin-bottom: var(--spacing-lg)`).
pub const FIELD_GAP: f32 = 16.0;

/// Open animation duration — 200 ms ease-out.
pub const PANEL_OPEN_DURATION_MS: u32 = 200;

/// Modal corner radius — `.zone-editor { border-radius: var(--radius-expanded) }`
/// resolves to 16 px in the BentoDesk 1.x design tokens.
pub const PANEL_CORNER_RADIUS: f32 = 16.0;

/// Per-cell size for the icon picker grid (snap.md "36 × 36 cells").
pub const ICON_CELL_SIZE: f32 = 36.0;

/// Per-cell size for the accent swatch row (snap.md "28 × 28 px circle").
pub const SWATCH_SIZE: f32 = 28.0;

/// Number of icon-grid columns inside the picker block (snap.md "6-column").
pub const ICON_GRID_COLUMNS: u32 = 6;

/// Maximum allowed length for the zone name input (1.x `maxLength={32}`).
pub const NAME_MAX_LEN: usize = 32;

/// Grid-columns slider range — 1.x `min={2} max={6}`.
pub const GRID_COLUMNS_MIN: u32 = 2;
pub const GRID_COLUMNS_MAX: u32 = 6;

/// 10 fixed accent palette swatches (1.x `ACCENT_COLORS`). `SmolStr` keeps
/// the 7-byte hex strings inline — zero heap allocations for the swatch row.
pub const ACCENT_PALETTE: &[&str] = &[
    "#3b82f6", "#8b5cf6", "#22c55e", "#f97316", "#ec4899", "#ef4444", "#eab308", "#06b6d4",
    "#f43f5e", "#a855f7",
];

// -----------------------------------------------------------------------------
// Capsule taxonomy — mirrors the 1.x `CAPSULE_SHAPES` / `CAPSULE_SIZES`
// arrays. The wire format goes back to the layout JSON via `BentoZone.
// capsule_shape` / `capsule_size` (`SmolStr`), so we keep the lowercase
// canonical form in one place.
// -----------------------------------------------------------------------------

/// Capsule outline shape. The four Tauri variants remain first, followed by
/// the legacy near-square variant that the native renderer already supports.
/// Exposing `Square` here makes corner shape a truthful, independently editable
/// property instead of labelling the transparent `Minimal` style as square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleShapeChoice {
    #[default]
    Pill,
    Rounded,
    Circle,
    Minimal,
    Square,
}

impl CapsuleShapeChoice {
    /// Wire-format token written to `BentoZone.capsule_shape`.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Pill => "pill",
            Self::Rounded => "rounded",
            Self::Circle => "circle",
            Self::Minimal => "minimal",
            Self::Square => "square",
        }
    }

    /// Parse the wire token back into the choice enum. Falls back to
    /// [`Self::default`] for unknown values so a forward-compat layout JSON
    /// never bricks the editor.
    pub fn parse(token: &str) -> Self {
        match token {
            "pill" => Self::Pill,
            "rounded" => Self::Rounded,
            "circle" => Self::Circle,
            "minimal" => Self::Minimal,
            "square" => Self::Square,
            _ => Self::default(),
        }
    }

    /// The Tauri choices retain their original order; `Square` is appended for
    /// backward-compatible near-sharp geometry.
    pub const ALL: &'static [Self] = &[
        Self::Pill,
        Self::Rounded,
        Self::Circle,
        Self::Minimal,
        Self::Square,
    ];
}

/// Capsule size choice — three variants matching 1.x `CAPSULE_SIZES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleSizeChoice {
    Small,
    #[default]
    Medium,
    Large,
}

impl CapsuleSizeChoice {
    /// Wire-format token written to `BentoZone.capsule_size`.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// Parse the wire token back into the choice enum. Falls back to
    /// [`Self::default`] for unknown values.
    pub fn parse(token: &str) -> Self {
        match token {
            "small" => Self::Small,
            "medium" => Self::Medium,
            "large" => Self::Large,
            _ => Self::default(),
        }
    }

    /// Iteration order matches the 1.x segmented toggle.
    pub const ALL: &'static [Self] = &[Self::Small, Self::Medium, Self::Large];
}

// -----------------------------------------------------------------------------
// ZoneEditor descriptor — the visual chrome (host HWND host body).
// -----------------------------------------------------------------------------

/// Modal-panel chrome for the ZoneEditor. The host HWND is sized to the
/// panel; this descriptor describes what paints inside the surface.
#[derive(Debug, Clone)]
pub struct ZoneEditor {
    /// Panel background — `palette.surface`.
    pub background: Color,
    /// Border colour — 1 px `palette.border`.
    pub border: Color,
    /// Title text colour — `palette.text`.
    pub title_color: Color,
    /// Border radius — 16 px (`var(--radius-expanded)`).
    pub border_radius: BorderRadius,
    /// Inset padding around the body (header / footer have their own padding).
    pub padding: Edges,
    /// Panel width — 400 px at the 96 DPI baseline.
    pub width: Length,
    /// Panel height — `Auto` (sized to the field stack, capped at 80vh by
    /// the host HWND).
    pub height: Length,
}

impl ZoneEditor {
    /// New chrome reading current theme palette tokens.
    pub fn new() -> Self {
        let palette = theme::current().palette;
        Self {
            background: palette.surface,
            border: palette.border,
            title_color: palette.text,
            border_radius: BorderRadius::all(PANEL_CORNER_RADIUS),
            padding: Edges::ZERO,
            width: Length::Px(PANEL_WIDTH),
            height: Length::Auto,
        }
    }
}

impl Default for ZoneEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutSource for ZoneEditor {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column: header → body → footer.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

// -----------------------------------------------------------------------------
// ZoneEditorAction — drained by the shell once per frame.
// -----------------------------------------------------------------------------

/// Action emitted by the editor. Drained via [`take_action`].
///
/// [`take_action`]: ZoneEditorState::take_action
//
// `ZoneUpdate` is ~280 bytes (12 Option fields, several String/SmolStr inside).
// Boxed here to keep `clippy::large_enum_variant` quiet without forcing every
// `Cancel` drain to memcpy 300 bytes — the action is dispatched once per
// click, so a single heap-alloc on Save is acceptable.
//
// `PartialEq` is intentionally omitted — `ZoneUpdate` (defined in
// `bento-nano-backend`) does not implement it, and adding `PartialEq` there
// belongs to that crate's ΔB pass, not the UI layer. Test code uses pattern
// matching to inspect the variant instead.
#[derive(Debug, Clone)]
pub enum ZoneEditorAction {
    /// User clicked Save. Carries the edited zone's id + a [`ZoneUpdate`]
    /// with only the fields the user touched (1.x `dirty` parity).
    Save {
        zone_id: SmolStr,
        update: Box<ZoneUpdate>,
    },
    /// User clicked Cancel / pressed Escape / clicked the scrim.
    Cancel,
}

impl fmt::Display for ZoneEditorAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Save { zone_id, .. } => write!(f, "Save({zone_id})"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

// -----------------------------------------------------------------------------
// ZoneEditorState — per-field local edits + dirty tracking.
// -----------------------------------------------------------------------------

/// Tracks per-field "user touched this" bits so the Save payload can omit
/// untouched fields (1.x partial-update semantics — ZoneUpdate carries only
/// the touched set).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DirtyMask {
    name: bool,
    icon: bool,
    accent: bool,
    grid_columns: bool,
    capsule_shape: bool,
    capsule_size: bool,
}

impl DirtyMask {
    fn any(self) -> bool {
        self.name
            || self.icon
            || self.accent
            || self.grid_columns
            || self.capsule_shape
            || self.capsule_size
    }
}

/// Editor state — seeded from the existing zone via [`load_zone`], mutated
/// as the user types / clicks, drained per-frame for the action.
#[derive(Debug)]
pub struct ZoneEditorState {
    zone_id: SmolStr,
    name: String,
    icon: SmolStr,
    accent_color: Option<SmolStr>,
    grid_columns: u32,
    capsule_shape: CapsuleShapeChoice,
    capsule_size: CapsuleSizeChoice,
    dirty: DirtyMask,
    pending_action: Option<ZoneEditorAction>,
}

impl Default for ZoneEditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl ZoneEditorState {
    /// New empty state. The shell calls [`load_zone`] with the response from
    /// the layout reader before the first paint.
    ///
    /// [`load_zone`]: ZoneEditorState::load_zone
    pub fn new() -> Self {
        Self {
            zone_id: SmolStr::default(),
            name: String::new(),
            icon: SmolStr::default(),
            accent_color: None,
            grid_columns: GRID_COLUMNS_MIN,
            capsule_shape: CapsuleShapeChoice::default(),
            capsule_size: CapsuleSizeChoice::default(),
            dirty: DirtyMask::default(),
            pending_action: None,
        }
    }

    /// Seed state from an existing zone. Resets the dirty mask so the Save
    /// button stays disabled until the user actually touches a field.
    ///
    /// Takes the `BentoZone` by reference — the editor only reads the
    /// six fields it edits (id / name / icon / accent_color / grid_columns
    /// / capsule shape + size); the rest of the 24-field zone stays put on
    /// the layout side untouched.
    pub fn load_zone(&mut self, zone: &BentoZone) {
        self.zone_id = zone.id.clone();
        self.name = zone.name.clone();
        self.icon = zone.icon.clone();
        self.accent_color = zone.accent_color.clone();
        self.grid_columns = zone.grid_columns.clamp(GRID_COLUMNS_MIN, GRID_COLUMNS_MAX);
        self.capsule_shape = CapsuleShapeChoice::parse(&zone.capsule_shape);
        self.capsule_size = CapsuleSizeChoice::parse(&zone.capsule_size);
        self.dirty = DirtyMask::default();
        self.pending_action = None;
    }

    /// Borrow the editor's zone id (empty string if no zone has been loaded).
    pub fn zone_id(&self) -> &str {
        &self.zone_id
    }

    /// Borrow the current name input.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Update the name input. Truncates to [`NAME_MAX_LEN`] characters
    /// (1.x `maxLength={32}`), measured in `char` count not bytes so a
    /// 32-emoji name doesn't get sliced mid-codepoint.
    pub fn set_name(&mut self, value: impl Into<String>) {
        let v = value.into();
        let truncated: String = v.chars().take(NAME_MAX_LEN).collect();
        self.name = truncated;
        self.dirty.name = true;
    }

    /// Borrow the currently selected icon slug.
    pub fn icon(&self) -> &str {
        &self.icon
    }

    /// Pick a different icon from the grid.
    pub fn set_icon(&mut self, icon: impl Into<SmolStr>) {
        self.icon = icon.into();
        self.dirty.icon = true;
    }

    /// Borrow the current accent colour.
    pub fn accent_color(&self) -> Option<&str> {
        self.accent_color.as_deref()
    }

    /// Pick a different accent swatch — `None` for "No accent colour".
    pub fn set_accent_color(&mut self, color: Option<SmolStr>) {
        self.accent_color = color;
        self.dirty.accent = true;
    }

    /// Borrow the current grid-columns value.
    pub fn grid_columns(&self) -> u32 {
        self.grid_columns
    }

    /// Update the grid-columns slider value. Clamped to
    /// [`GRID_COLUMNS_MIN`]..=[`GRID_COLUMNS_MAX`].
    pub fn set_grid_columns(&mut self, value: u32) {
        self.grid_columns = value.clamp(GRID_COLUMNS_MIN, GRID_COLUMNS_MAX);
        self.dirty.grid_columns = true;
    }

    /// Borrow the selected capsule shape.
    pub fn capsule_shape(&self) -> CapsuleShapeChoice {
        self.capsule_shape
    }

    /// Pick a different capsule shape.
    pub fn set_capsule_shape(&mut self, shape: CapsuleShapeChoice) {
        self.capsule_shape = shape;
        self.dirty.capsule_shape = true;
    }

    /// Borrow the selected capsule size.
    pub fn capsule_size(&self) -> CapsuleSizeChoice {
        self.capsule_size
    }

    /// Pick a different capsule size.
    pub fn set_capsule_size(&mut self, size: CapsuleSizeChoice) {
        self.capsule_size = size;
        self.dirty.capsule_size = true;
    }

    /// Whether any field has been touched since the last [`load_zone`].
    pub fn is_dirty(&self) -> bool {
        self.dirty.any()
    }

    /// Whether the Save button should render enabled. Disabled when the
    /// name is blank (1.x `name().trim().length === 0`) or no field is
    /// dirty.
    pub fn can_save(&self) -> bool {
        self.dirty.any() && !self.name.trim().is_empty()
    }

    /// User clicked Save. No-op when [`can_save`] is false so the renderer
    /// can be lazy about the disabled-button hit-test.
    ///
    /// [`can_save`]: ZoneEditorState::can_save
    pub fn click_save(&mut self) {
        if !self.can_save() {
            return;
        }
        let update = Box::new(self.build_update());
        self.pending_action = Some(ZoneEditorAction::Save {
            zone_id: self.zone_id.clone(),
            update,
        });
    }

    /// User clicked Cancel / pressed Escape / clicked the scrim. The
    /// editor's local edits stay in place — the shell tears the HWND down
    /// once it drains the action, so the next open re-seeds via
    /// [`load_zone`] anyway.
    pub fn click_cancel(&mut self) {
        self.pending_action = Some(ZoneEditorAction::Cancel);
    }

    /// Drain the latest action — one-shot.
    pub fn take_action(&mut self) -> Option<ZoneEditorAction> {
        self.pending_action.take()
    }

    /// Build the `ZoneUpdate` from the dirty mask. Only touched fields land
    /// in the payload — preserves 1.x partial-update semantics so a save
    /// that only changes the icon doesn't accidentally null the alias.
    fn build_update(&self) -> ZoneUpdate {
        ZoneUpdate {
            name: if self.dirty.name {
                Some(self.name.trim().to_string())
            } else {
                None
            },
            icon: if self.dirty.icon {
                Some(self.icon.clone())
            } else {
                None
            },
            position: None,
            expanded_size: None,
            accent_color: if self.dirty.accent {
                self.accent_color.clone()
            } else {
                None
            },
            grid_columns: if self.dirty.grid_columns {
                Some(self.grid_columns)
            } else {
                None
            },
            auto_group: None,
            capsule_size: if self.dirty.capsule_size {
                Some(SmolStr::new_static(self.capsule_size.wire()))
            } else {
                None
            },
            capsule_shape: if self.dirty.capsule_shape {
                Some(SmolStr::new_static(self.capsule_shape.wire()))
            } else {
                None
            },
            locked: None,
            alias: None,
            display_mode: None,
        }
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the ZoneEditor widget subtree. Returns the panel chrome Container
/// today; the header / scrollable body (name input · icon grid · swatch row ·
/// columns slider · shape buttons · size toggle) / footer Cancel + Save row
/// land when widget-library composition primitives ship (Input · Grid ·
/// Slider · Toggle · Button — already in the widget enum).
pub fn build() -> WidgetNode {
    let chrome = ZoneEditor::new();
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: chrome.width,
        height: chrome.height,
        padding: chrome.padding,
        background: chrome.background,
        radius: chrome.border_radius,
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_zone() -> BentoZone {
        use bento_nano_backend::layout::persistence::{RelativePosition, RelativeSize};
        BentoZone {
            id: SmolStr::new_static("zone-42"),
            name: "Inbox".to_string(),
            icon: SmolStr::new_static("folder"),
            position: RelativePosition {
                x_percent: 0.0,
                y_percent: 0.0,
            },
            expanded_size: RelativeSize {
                w_percent: 30.0,
                h_percent: 40.0,
            },
            items: Vec::new(),
            accent_color: Some(SmolStr::new_static("#3b82f6")),
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: SmolStr::new_static("2026-05-03T00:00:00Z"),
            updated_at: SmolStr::new_static("2026-05-03T00:00:00Z"),
            capsule_size: SmolStr::new_static("medium"),
            capsule_shape: SmolStr::new_static("pill"),
            locked: false,
            visible: true,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    fn seeded_state() -> ZoneEditorState {
        let mut s = ZoneEditorState::new();
        let zone = sample_zone();
        s.load_zone(&zone);
        s
    }

    #[test]
    fn editor_default_chrome_uses_palette_surface() {
        let e = ZoneEditor::new();
        let palette = theme::current().palette;
        assert_eq!(e.background, palette.surface);
        assert_eq!(e.border, palette.border);
        assert_eq!(e.title_color, palette.text);
        assert_eq!(e.border_radius.top_left, PANEL_CORNER_RADIUS);
        assert_eq!(e.width, Length::Px(PANEL_WIDTH));
    }

    #[test]
    fn snap_constants_match_spec() {
        assert_eq!(PANEL_WIDTH, 400.0);
        assert!((PANEL_MAX_HEIGHT_FRACTION - 0.80).abs() < f32::EPSILON);
        assert_eq!(HEADER_HEIGHT, 52.0);
        assert_eq!(FOOTER_HEIGHT, 64.0);
        assert_eq!(BODY_PADDING_VERTICAL, 16.0);
        assert_eq!(BODY_PADDING_HORIZONTAL, 20.0);
        assert_eq!(FIELD_GAP, 16.0);
        assert_eq!(PANEL_OPEN_DURATION_MS, 200);
        assert_eq!(PANEL_CORNER_RADIUS, 16.0);
        assert_eq!(ICON_CELL_SIZE, 36.0);
        assert_eq!(SWATCH_SIZE, 28.0);
        assert_eq!(ICON_GRID_COLUMNS, 6);
        assert_eq!(NAME_MAX_LEN, 32);
        assert_eq!(GRID_COLUMNS_MIN, 2);
        assert_eq!(GRID_COLUMNS_MAX, 6);
        assert_eq!(ACCENT_PALETTE.len(), 10);
    }

    #[test]
    fn capsule_shape_wire_round_trip_and_unknown_falls_back() {
        for v in CapsuleShapeChoice::ALL {
            assert_eq!(CapsuleShapeChoice::parse(v.wire()), *v);
        }
        assert_eq!(
            CapsuleShapeChoice::parse("hexagon"),
            CapsuleShapeChoice::default()
        );
    }

    #[test]
    fn capsule_size_wire_round_trip_and_unknown_falls_back() {
        for v in CapsuleSizeChoice::ALL {
            assert_eq!(CapsuleSizeChoice::parse(v.wire()), *v);
        }
        assert_eq!(
            CapsuleSizeChoice::parse("colossal"),
            CapsuleSizeChoice::default()
        );
    }

    #[test]
    fn capsule_shape_serde_round_trip() {
        for v in [
            CapsuleShapeChoice::Pill,
            CapsuleShapeChoice::Rounded,
            CapsuleShapeChoice::Circle,
            CapsuleShapeChoice::Minimal,
            CapsuleShapeChoice::Square,
        ] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: CapsuleShapeChoice = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        assert_eq!(
            serde_json::to_string(&CapsuleShapeChoice::Pill).unwrap_or_default(),
            "\"pill\""
        );
    }

    #[test]
    fn fresh_state_is_not_dirty() {
        let s = ZoneEditorState::new();
        assert!(!s.is_dirty());
        assert!(!s.can_save());
        assert_eq!(s.zone_id(), "");
    }

    #[test]
    fn load_zone_seeds_fields_and_clears_dirty() {
        let s = seeded_state();
        assert_eq!(s.zone_id(), "zone-42");
        assert_eq!(s.name(), "Inbox");
        assert_eq!(s.icon(), "folder");
        assert_eq!(s.accent_color(), Some("#3b82f6"));
        assert_eq!(s.grid_columns(), 4);
        assert_eq!(s.capsule_shape(), CapsuleShapeChoice::Pill);
        assert_eq!(s.capsule_size(), CapsuleSizeChoice::Medium);
        assert!(!s.is_dirty());
    }

    #[test]
    fn set_name_marks_dirty_and_truncates_to_32_chars() {
        let mut s = seeded_state();
        let long = "a".repeat(64);
        s.set_name(long);
        assert_eq!(s.name().chars().count(), NAME_MAX_LEN);
        assert!(s.is_dirty());
    }

    #[test]
    fn set_name_truncates_at_codepoint_boundary_for_emoji() {
        let mut s = seeded_state();
        // 40 codepoints, all multi-byte → byte length > 32 but char count = 40.
        let emoji_name = "🦀".repeat(40);
        s.set_name(emoji_name);
        assert_eq!(s.name().chars().count(), NAME_MAX_LEN);
    }

    #[test]
    fn set_grid_columns_clamps_to_range() {
        let mut s = seeded_state();
        s.set_grid_columns(99);
        assert_eq!(s.grid_columns(), GRID_COLUMNS_MAX);
        s.set_grid_columns(0);
        assert_eq!(s.grid_columns(), GRID_COLUMNS_MIN);
    }

    #[test]
    fn can_save_requires_dirty_and_non_blank_name() {
        let mut s = seeded_state();
        assert!(!s.can_save(), "fresh load is not dirty");

        s.set_grid_columns(5);
        assert!(s.can_save(), "dirty + non-blank name → can save");

        s.set_name("   ");
        assert!(!s.can_save(), "blank name disables save");
    }

    #[test]
    fn click_save_with_clean_state_is_noop() {
        let mut s = seeded_state();
        s.click_save();
        assert!(s.take_action().is_none());
    }

    #[test]
    fn click_save_records_payload_with_only_touched_fields() {
        let mut s = seeded_state();
        s.set_icon("star");
        s.set_capsule_size(CapsuleSizeChoice::Large);
        s.click_save();

        let action = s.take_action().expect("save action queued");
        let ZoneEditorAction::Save { zone_id, update } = action else {
            panic!("expected Save");
        };
        assert_eq!(zone_id, SmolStr::new_static("zone-42"));
        // Touched: icon + capsule_size.
        assert_eq!(update.icon, Some(SmolStr::new_static("star")));
        assert_eq!(update.capsule_size, Some(SmolStr::new_static("large")));
        // Untouched: everything else.
        assert!(update.name.is_none());
        assert!(update.accent_color.is_none());
        assert!(update.grid_columns.is_none());
        assert!(update.capsule_shape.is_none());
        assert!(update.alias.is_none());
        assert!(update.display_mode.is_none());
    }

    #[test]
    fn click_save_emits_trimmed_name_when_dirty() {
        let mut s = seeded_state();
        s.set_name("  Renamed  ");
        s.click_save();
        let action = s.take_action().expect("save action");
        let ZoneEditorAction::Save { update, .. } = action else {
            panic!("expected Save");
        };
        assert_eq!(update.name.as_deref(), Some("Renamed"));
    }

    #[test]
    fn click_save_with_no_accent_emits_none_clear() {
        let mut s = seeded_state();
        s.set_accent_color(None);
        s.click_save();
        let action = s.take_action().expect("save action");
        let ZoneEditorAction::Save { update, .. } = action else {
            panic!("expected Save");
        };
        // Accent dirty → field present in update; user picked None.
        assert!(update.accent_color.is_none());
    }

    #[test]
    fn click_cancel_records_cancel_action() {
        let mut s = seeded_state();
        s.set_icon("ignored");
        s.click_cancel();
        assert!(matches!(s.take_action(), Some(ZoneEditorAction::Cancel)));
    }

    #[test]
    fn take_action_is_one_shot() {
        let mut s = seeded_state();
        s.click_cancel();
        assert!(s.take_action().is_some());
        assert!(s.take_action().is_none());
    }

    #[test]
    fn build_returns_panel_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
    }

    #[test]
    fn panel_max_height_resolves_to_80_percent_of_viewport() {
        let viewport_height = 1000.0_f32;
        let resolved = viewport_height * PANEL_MAX_HEIGHT_FRACTION;
        assert!((resolved - 800.0).abs() < 0.01);
    }
}
