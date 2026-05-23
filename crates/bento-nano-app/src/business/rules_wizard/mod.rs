//! `RulesWizard` — 5-step modal that creates / edits a `bento_nano_backend::
//! rules::Rule` (Outlook-style file-organisation engine).
//!
//! 1.x source: `bentodesk/src/components/RulesWizard/RulesWizard.tsx` ships a
//! list-+-form layout. The nano redesign keeps the same data model (`Rule`,
//! `ConditionGroup`, `Action`, `RunMode`) but presents the create-flow as a
//! 5-step wizard for clearer affordances. The list of existing rules is the
//! Settings panel's responsibility (Wave-E `settings::rules_card`); this
//! widget owns the create / edit experience only.
//!
//! Visual fidelity reference: `rules_wizard.snap.md`.
//!
//! # Module layout
//!
//! - `mod.rs` — chrome + step navigation + state machine + action enum +
//!   `build()` entry point.
//! - `drafts.rs` — `PredicateKind` / `ConditionDraft` / `CombineMode` /
//!   `ActionKind` / `ActionDraft` / `RunModeChoice` plus the conversion
//!   helpers that promote drafts into backend types and decompose backend
//!   types back into drafts (Edit-mode load).
//!
//! # Hosting
//!
//! Rendered inside a dedicated layered HWND (560 × 600 default, 16 px corner
//! radius via DComp visual clip). The HWND is created on demand by the shell
//! when the user picks "+ New rule" or "Edit rule" from the rules card and
//! torn down when [`RulesWizardAction::Save`] / [`RulesWizardAction::Cancel`]
//! surfaces.
//!
//! # Save flow
//!
//! The widget is a *shell* — it doesn't talk to `bento_nano_backend::rules`
//! directly. The `*Action::Save` payload carries a fully populated [`Rule`];
//! the shell calls `rules::upsert(state_dir, rule)` and tears the HWND down
//! on success. New rules carry an empty `id`; the shell stamps a UUID before
//! upsert.
//!
//! # Q2 anchor-free predicates
//!
//! The condition catalogue here mirrors the post-Q2 `Condition` enum
//! variants exactly: `NameStartsWith` / `NameContains` / `NameEndsWith`
//! (no `NameMatchesRegex`). The wizard renders only these three name-based
//! options so users can never type a regex pattern that would fail at upsert.
//!
//! ## Spec compliance
//!
//! - §10 hot-path: short identifiers (rule id, predicate / action discriminator,
//!   tag / extension tokens, zone id) use [`SmolStr`]; free-form strings (rule
//!   name, notify message, folder path) use `String`.
//! - §11 ΔB: every public DTO derives `serde::{Serialize, Deserialize}`.
//! - §11.1: zero `unsafe` in this UI layer.
//! - §15: each .rs file ships under the 800-LOC budget — drafts split out
//!   to `drafts.rs` to honour the limit.
//! - §17: zero `todo!()` / `unimplemented!()` / `panic!()` / `unwrap()` /
//!   `expect()` in production code.

use core::fmt;

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bento_nano_theme as theme;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use bento_nano_backend::rules::{ConditionGroup, ConditionNode, Rule, RunMode};

pub mod drafts;

pub use drafts::{
    ActionDraft, ActionKind, CombineMode, ConditionDraft, PredicateKind, RunModeChoice,
};

// -----------------------------------------------------------------------------
// Snap.md derived geometry constants — pinned per visual spec.
// -----------------------------------------------------------------------------

/// Modal width in DIPs.
pub const PANEL_WIDTH: f32 = 560.0;

/// Modal height in DIPs.
pub const PANEL_HEIGHT: f32 = 600.0;

/// Modal corner radius — 16 px (`var(--radius-expanded)`).
pub const PANEL_CORNER_RADIUS: f32 = 16.0;

/// Header row height.
pub const HEADER_HEIGHT: f32 = 56.0;

/// Step indicator row height.
pub const STEP_INDICATOR_HEIGHT: f32 = 48.0;

/// Footer row height.
pub const FOOTER_HEIGHT: f32 = 64.0;

/// Per-side body padding.
pub const BODY_PADDING: f32 = 24.0;

/// Open animation duration — 200 ms ease-out.
pub const PANEL_OPEN_DURATION_MS: u32 = 200;

/// Maximum allowed length for the rule name input.
pub const NAME_MAX_LEN: usize = 64;

/// Minimum interval (minutes) for `RunMode::Interval` — at least once per
/// minute is the 1.x cap.
pub const INTERVAL_MIN_MINUTES: u32 = 1;

/// Maximum interval (minutes) for `RunMode::Interval` — once per day is the
/// 1.x cap (`<input type="number" max="1440">`).
pub const INTERVAL_MAX_MINUTES: u32 = 1440;

/// Default interval when the user first picks `RunMode::Interval`.
pub const INTERVAL_DEFAULT_MINUTES: u32 = 60;

/// Selected-stack aux renderer panel margin.
pub const RUNTIME_PANEL_MARGIN_PX: f32 = 16.0;

/// Left/right inset used by the D2D runtime renderer.
pub const RUNTIME_PANEL_INSET_PX: f32 = 18.0;

/// Runtime action button height in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_HEIGHT_PX: f32 = 24.0;

/// Runtime top offset for the first action-button row.
pub const RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX: f32 = 132.0;

/// Runtime top offset for the second action-button row.
pub const RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX: f32 = 162.0;

/// Runtime form/list heading top in the D2D aux panel.
pub const RUNTIME_FORM_TOP_PX: f32 = 196.0;

/// Runtime persisted-rule row top in the D2D aux panel.
pub const RUNTIME_RULE_ROW_TOP_PX: f32 = RUNTIME_FORM_TOP_PX + 32.0;

/// Runtime persisted-rule row height in the D2D aux panel.
pub const RUNTIME_RULE_ROW_HEIGHT_PX: f32 = 30.0;

/// Runtime persisted-rule row stride in the D2D aux panel.
pub const RUNTIME_RULE_ROW_STRIDE_PX: f32 = 38.0;

/// The current runtime renderer shows at most six visible persisted rules.
pub const RUNTIME_VISIBLE_RULE_LIMIT: usize = 6;

/// Runtime condition-row list top in the D2D aux panel.
pub const RUNTIME_CONDITION_ROW_TOP_PX: f32 = RUNTIME_FORM_TOP_PX + 32.0;

/// Runtime condition-row height in the D2D aux panel.
pub const RUNTIME_CONDITION_ROW_HEIGHT_PX: f32 = 24.0;

/// Runtime condition-row stride in the D2D aux panel.
pub const RUNTIME_CONDITION_ROW_STRIDE_PX: f32 = 28.0;

/// The runtime renderer shows a sliding window of four condition rows.
pub const RUNTIME_VISIBLE_CONDITION_LIMIT: usize = 4;

/// RulesWizard colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulesWizardChrome {
    /// Drop shadow descriptor drawn behind the panel.
    pub panel_shadow: Shadow,
    /// Panel radius.
    pub panel_radius: BorderRadius,
    /// Action button radius.
    pub button_radius: BorderRadius,
    /// Condition and persisted-rule row radius.
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
}

impl RulesWizardChrome {
    /// Build RulesWizard chrome from the currently active app palette.
    pub fn from_palette(palette: theme::PaletteTokens) -> Self {
        Self::from_tokens(palette, theme::radius::DEFAULT, theme::shadow::DEFAULT)
    }

    /// Build RulesWizard chrome from explicit active theme token groups.
    pub fn from_tokens(
        palette: theme::PaletteTokens,
        radius: theme::RadiusTokens,
        shadow: theme::ShadowTokens,
    ) -> Self {
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
        }
    }
}

/// Pointer hit target in the runtime D2D RulesWizard panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesWizardPointerHit {
    NextSave,
    Predicate,
    Action,
    RunMode,
    Combine,
    AddCondition,
    RemoveCondition,
    NextCondition,
    Edit,
    Run,
    Delete,
    Close,
    ConditionRow(usize),
    Row(usize),
}

/// Static action-button descriptor shared by renderer and shell hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulesWizardButtonSpec {
    pub hit: RulesWizardPointerHit,
    pub label: &'static str,
    pub x_offset: f32,
    pub y_offset: f32,
    pub width: f32,
}

pub const RULES_WIZARD_ACTION_BUTTONS: &[RulesWizardButtonSpec] = &[
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::NextSave,
        label: "Next/Save",
        x_offset: 0.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX,
        width: 84.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Predicate,
        label: "Pred",
        x_offset: 90.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX,
        width: 54.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Action,
        label: "Action",
        x_offset: 150.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX,
        width: 66.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::RunMode,
        label: "Mode",
        x_offset: 222.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX,
        width: 58.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Combine,
        label: "All/Any",
        x_offset: 246.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 74.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::AddCondition,
        label: "Cond+",
        x_offset: 326.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 64.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::RemoveCondition,
        label: "Cond-",
        x_offset: 396.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 64.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::NextCondition,
        label: "Cond>",
        x_offset: 466.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 64.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Edit,
        label: "Edit",
        x_offset: 0.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 52.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Run,
        label: "Run",
        x_offset: 58.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 48.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Delete,
        label: "Delete",
        x_offset: 112.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 64.0,
    },
    RulesWizardButtonSpec {
        hit: RulesWizardPointerHit::Close,
        label: "Close",
        x_offset: 182.0,
        y_offset: RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX,
        width: 58.0,
    },
];

/// Runtime D2D panel rectangle shared by rendering and pointer hit-testing.
pub fn rules_wizard_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: RUNTIME_PANEL_MARGIN_PX,
        y: RUNTIME_PANEL_MARGIN_PX,
        width: (viewport.width - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(PANEL_WIDTH),
        height: (viewport.height - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(420.0),
    }
}

pub fn rules_wizard_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Runtime action button rectangle shared by rendering and pointer hit-testing.
pub fn rules_wizard_button_rect(viewport: Size, spec: RulesWizardButtonSpec) -> Rect {
    let panel = rules_wizard_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX + spec.x_offset,
        y: panel.y + spec.y_offset,
        width: spec.width,
        height: RUNTIME_ACTION_BUTTON_HEIGHT_PX,
    }
}

/// Runtime persisted-rule row rectangle shared by rendering and pointer hit-testing.
pub fn rules_wizard_rule_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = rules_wizard_panel_rect(viewport);
    let list_x = panel.x + panel.width * 0.54;
    let list_w = panel.width - (list_x - panel.x) - RUNTIME_PANEL_INSET_PX;
    Rect {
        x: list_x,
        y: panel.y + RUNTIME_RULE_ROW_TOP_PX + (row_index as f32 * RUNTIME_RULE_ROW_STRIDE_PX),
        width: list_w,
        height: RUNTIME_RULE_ROW_HEIGHT_PX,
    }
}

/// Runtime condition row rectangle shared by rendering and pointer hit-testing.
pub fn rules_wizard_condition_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = rules_wizard_panel_rect(viewport);
    let form_w = (panel.width * 0.50).max(260.0);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX,
        y: panel.y
            + RUNTIME_CONDITION_ROW_TOP_PX
            + (row_index as f32 * RUNTIME_CONDITION_ROW_STRIDE_PX),
        width: form_w,
        height: RUNTIME_CONDITION_ROW_HEIGHT_PX,
    }
}

pub fn rules_wizard_visible_rule_window_start(
    cursor_index: usize,
    visible_rule_count: usize,
) -> usize {
    if visible_rule_count <= RUNTIME_VISIBLE_RULE_LIMIT {
        0
    } else {
        cursor_index
            .min(visible_rule_count - 1)
            .saturating_add(1)
            .saturating_sub(RUNTIME_VISIBLE_RULE_LIMIT)
    }
}

pub fn rules_wizard_visible_condition_window_start(
    cursor_index: usize,
    condition_count: usize,
) -> usize {
    if condition_count <= RUNTIME_VISIBLE_CONDITION_LIMIT {
        0
    } else {
        cursor_index
            .min(condition_count - 1)
            .saturating_add(1)
            .saturating_sub(RUNTIME_VISIBLE_CONDITION_LIMIT)
    }
}

pub fn rules_wizard_visible_rule_summary(
    visible_rule_window_start: usize,
    visible_rule_count: usize,
) -> Option<SmolStr> {
    if visible_rule_count <= RUNTIME_VISIBLE_RULE_LIMIT {
        return None;
    }
    let visible_start = visible_rule_window_start
        .min(visible_rule_count.saturating_sub(RUNTIME_VISIBLE_RULE_LIMIT));
    let visible_end = visible_rule_count.min(visible_start + RUNTIME_VISIBLE_RULE_LIMIT);
    Some(SmolStr::new(format!(
        "Rules {}-{} of {}",
        visible_start + 1,
        visible_end,
        visible_rule_count
    )))
}

pub fn rules_wizard_visible_condition_summary(
    visible_condition_window_start: usize,
    condition_count: usize,
) -> Option<SmolStr> {
    if condition_count <= RUNTIME_VISIBLE_CONDITION_LIMIT {
        return None;
    }
    let visible_start = visible_condition_window_start
        .min(condition_count.saturating_sub(RUNTIME_VISIBLE_CONDITION_LIMIT));
    let visible_end = condition_count.min(visible_start + RUNTIME_VISIBLE_CONDITION_LIMIT);
    Some(SmolStr::new(format!(
        "Conditions {}-{} of {}",
        visible_start + 1,
        visible_end,
        condition_count
    )))
}

/// Hit-test the runtime RulesWizard action buttons, visible condition rows, and visible persisted-rule rows.
pub fn rules_wizard_hit_test(
    viewport: Size,
    visible_rule_count: usize,
    visible_rule_window_start: usize,
    condition_count: usize,
    visible_condition_window_start: usize,
    x: f32,
    y: f32,
) -> Option<RulesWizardPointerHit> {
    for spec in RULES_WIZARD_ACTION_BUTTONS {
        if rect_contains(rules_wizard_button_rect(viewport, *spec), x, y) {
            return Some(spec.hit);
        }
    }
    let visible_condition_start = visible_condition_window_start
        .min(condition_count.saturating_sub(RUNTIME_VISIBLE_CONDITION_LIMIT));
    let visible_condition_end =
        condition_count.min(visible_condition_start + RUNTIME_VISIBLE_CONDITION_LIMIT);
    for (display_index, condition_index) in
        (visible_condition_start..visible_condition_end).enumerate()
    {
        if rect_contains(
            rules_wizard_condition_row_rect(viewport, display_index),
            x,
            y,
        ) {
            return Some(RulesWizardPointerHit::ConditionRow(condition_index));
        }
    }
    let visible_start = visible_rule_window_start
        .min(visible_rule_count.saturating_sub(RUNTIME_VISIBLE_RULE_LIMIT));
    let visible_end = visible_rule_count.min(visible_start + RUNTIME_VISIBLE_RULE_LIMIT);
    for (display_index, row_index) in (visible_start..visible_end).enumerate() {
        if rect_contains(rules_wizard_rule_row_rect(viewport, display_index), x, y) {
            return Some(RulesWizardPointerHit::Row(row_index));
        }
    }
    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

// -----------------------------------------------------------------------------
// WizardStep — five distinct steps. The wizard advances through them in a
// fixed order; the user can skip back via "Back" but can't jump arbitrarily.
// -----------------------------------------------------------------------------

/// Which step the wizard is currently on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WizardStep {
    /// Step 1 — pick the condition tree.
    #[default]
    Conditions,
    /// Step 2 — pick the action to run.
    Action,
    /// Step 3 — preview matches.
    Preview,
    /// Step 4 — name + enable + run mode.
    Name,
    /// Step 5 — review + save.
    Review,
}

impl WizardStep {
    /// One-based step index (1..=5) — surfaced in the indicator dot label.
    pub const fn index(self) -> u32 {
        match self {
            Self::Conditions => 1,
            Self::Action => 2,
            Self::Preview => 3,
            Self::Name => 4,
            Self::Review => 5,
        }
    }

    /// Total number of steps in the wizard — pinned by snap.md.
    pub const TOTAL: u32 = 5;

    /// Move to the next step. Saturates at [`Self::Review`].
    pub const fn next(self) -> Self {
        match self {
            Self::Conditions => Self::Action,
            Self::Action => Self::Preview,
            Self::Preview => Self::Name,
            Self::Name | Self::Review => Self::Review,
        }
    }

    /// Move to the previous step. Saturates at [`Self::Conditions`].
    pub const fn prev(self) -> Self {
        match self {
            Self::Conditions | Self::Action => Self::Conditions,
            Self::Preview => Self::Action,
            Self::Name => Self::Preview,
            Self::Review => Self::Name,
        }
    }

    /// Iteration order for the indicator dot row.
    pub const ALL: &'static [Self] = &[
        Self::Conditions,
        Self::Action,
        Self::Preview,
        Self::Name,
        Self::Review,
    ];
}

// -----------------------------------------------------------------------------
// RulesWizard descriptor — the visual chrome.
// -----------------------------------------------------------------------------

/// Modal-panel chrome for the RulesWizard. The host HWND is sized to the
/// panel; this descriptor describes what paints inside.
#[derive(Debug, Clone)]
pub struct RulesWizard {
    pub background: Color,
    pub border: Color,
    pub title_color: Color,
    pub border_radius: BorderRadius,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
}

impl RulesWizard {
    pub fn new() -> Self {
        let palette = theme::current().palette;
        Self {
            background: palette.surface,
            border: palette.border,
            title_color: palette.text,
            border_radius: BorderRadius::all(PANEL_CORNER_RADIUS),
            padding: Edges::ZERO,
            width: Length::Px(PANEL_WIDTH),
            height: Length::Px(PANEL_HEIGHT),
        }
    }
}

impl Default for RulesWizard {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutSource for RulesWizard {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column: header → step indicator → body → footer.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

// -----------------------------------------------------------------------------
// RulesWizardAction — drained by the shell once per frame.
// -----------------------------------------------------------------------------

/// Action emitted by the wizard. Drained via [`take_action`].
///
/// [`take_action`]: RulesWizardState::take_action
//
// `Rule` is ~150-200 bytes (id SmolStr + name String + ConditionGroup with
// inline Vec + actions Vec + RunMode + last_run + run_count). Boxed inside
// `Save` and `PreviewRequest` to keep `clippy::large_enum_variant` quiet —
// the action is dispatched once per click, so a single heap-alloc on the
// rare Save / PreviewRequest paths is acceptable.
#[derive(Debug, Clone, PartialEq)]
pub enum RulesWizardAction {
    /// User clicked Save on step 5. Carries a fully populated [`Rule`]
    /// (id stays empty for create — the shell stamps a UUID before
    /// calling `rules::upsert`).
    Save(Box<Rule>),
    /// User clicked Cancel / pressed Escape / clicked the scrim.
    Cancel,
    /// Step 3 — user clicked "Refresh" / wizard advanced to Preview. The
    /// shell calls `rules::executor::preview` (which scans the desktop FS,
    /// must run off-thread) and pushes the hits back via
    /// [`set_preview_hits`]. Carries a snapshot of the `Rule` built from
    /// the wizard's current state so the shell can hand a stable struct
    /// to the preview executor.
    ///
    /// [`set_preview_hits`]: RulesWizardState::set_preview_hits
    PreviewRequest(Box<Rule>),
}

impl fmt::Display for RulesWizardAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Save(rule) => write!(f, "Save({})", rule.name),
            Self::Cancel => f.write_str("Cancel"),
            Self::PreviewRequest(rule) => write!(f, "PreviewRequest({})", rule.name),
        }
    }
}

// -----------------------------------------------------------------------------
// RulesWizardState — wizard navigation + per-step drafts.
// -----------------------------------------------------------------------------

/// Wizard state — owns the in-progress rule, the current step, and the
/// per-step draft buffers. Mutated as the user clicks; drained per-frame
/// for the latest action.
#[derive(Debug)]
pub struct RulesWizardState {
    step: WizardStep,
    rule_id: SmolStr,
    name: String,
    enabled: bool,
    combine: CombineMode,
    conditions: Vec<ConditionDraft>,
    condition_cursor: usize,
    action: ActionDraft,
    run_mode: RunModeChoice,
    interval_minutes: u32,
    preview_hits: Vec<String>,
    preview_busy: bool,
    last_error: Option<SmolStr>,
    pending_action: Option<RulesWizardAction>,
}

impl Default for RulesWizardState {
    fn default() -> Self {
        Self::new()
    }
}

impl RulesWizardState {
    /// New state — defaults to step 1 with one empty condition row, the
    /// MoveToZone action, and OnDemand run mode. Matches 1.x `emptyRule()`.
    pub fn new() -> Self {
        Self {
            step: WizardStep::default(),
            rule_id: SmolStr::default(),
            name: String::new(),
            enabled: true,
            combine: CombineMode::default(),
            conditions: vec![ConditionDraft::new()],
            condition_cursor: 0,
            action: ActionDraft::new(),
            run_mode: RunModeChoice::default(),
            interval_minutes: INTERVAL_DEFAULT_MINUTES,
            preview_hits: Vec::new(),
            preview_busy: false,
            last_error: None,
            pending_action: None,
        }
    }

    /// Seed the wizard with an existing rule (Edit mode). Resets navigation
    /// to step 1 so the user can step through the existing values to
    /// confirm the edit.
    pub fn load_rule(&mut self, rule: Rule) {
        self.rule_id = rule.id;
        self.name = rule.name;
        self.enabled = rule.enabled;
        let (combine, draft_rows) = drafts::decompose_conditions(&rule.conditions);
        self.combine = combine;
        self.conditions = if draft_rows.is_empty() {
            vec![ConditionDraft::new()]
        } else {
            draft_rows
        };
        self.condition_cursor = 0;
        self.action = rule
            .actions
            .into_iter()
            .next()
            .map(drafts::action_to_draft)
            .unwrap_or_default();
        self.run_mode = match &rule.run_mode {
            RunMode::OnDemand => RunModeChoice::OnDemand,
            RunMode::OnFileChange => RunModeChoice::OnFileChange,
            RunMode::Interval { .. } => RunModeChoice::Interval,
        };
        self.interval_minutes = match rule.run_mode {
            RunMode::Interval { minutes } => {
                minutes.clamp(INTERVAL_MIN_MINUTES, INTERVAL_MAX_MINUTES)
            }
            _ => INTERVAL_DEFAULT_MINUTES,
        };
        self.step = WizardStep::default();
        self.preview_hits.clear();
        self.preview_busy = false;
        self.last_error = None;
        self.pending_action = None;
    }

    /// Borrow the current step.
    pub fn step(&self) -> WizardStep {
        self.step
    }

    /// Borrow the current name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Update the rule name. Truncates to [`NAME_MAX_LEN`] codepoints.
    pub fn set_name(&mut self, value: impl Into<String>) {
        let v = value.into();
        let truncated: String = v.chars().take(NAME_MAX_LEN).collect();
        self.name = truncated;
    }

    /// Borrow the current enabled flag.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Toggle the enabled flag.
    pub fn set_enabled(&mut self, value: bool) {
        self.enabled = value;
    }

    /// Borrow the current combine mode.
    pub fn combine(&self) -> CombineMode {
        self.combine
    }

    /// Switch the combine mode.
    pub fn set_combine(&mut self, value: CombineMode) {
        self.combine = value;
    }

    /// Borrow the condition drafts.
    pub fn conditions(&self) -> &[ConditionDraft] {
        &self.conditions
    }

    /// Current condition row targeted by typed input and predicate cycling.
    pub fn condition_cursor(&self) -> usize {
        self.condition_cursor
    }

    /// Select the condition row targeted by typed input and predicate cycling.
    pub fn set_condition_cursor(&mut self, idx: usize) {
        self.condition_cursor = if self.conditions.is_empty() {
            0
        } else {
            idx.min(self.conditions.len() - 1)
        };
    }

    /// Move the condition cursor forward, wrapping within existing rows.
    pub fn select_next_condition(&mut self) {
        if self.conditions.is_empty() {
            self.condition_cursor = 0;
        } else {
            self.condition_cursor = (self.condition_cursor + 1) % self.conditions.len();
        }
    }

    /// Update one condition row's kind. No-op for out-of-range indices.
    /// Clears the value if the new kind doesn't need one.
    pub fn set_condition_kind(&mut self, idx: usize, kind: PredicateKind) {
        if let Some(row) = self.conditions.get_mut(idx) {
            row.kind = kind;
            if !kind.needs_value() {
                row.value.clear();
            }
        }
    }

    /// Update one condition row's value. No-op for out-of-range indices.
    pub fn set_condition_value(&mut self, idx: usize, value: impl Into<String>) {
        if let Some(row) = self.conditions.get_mut(idx) {
            row.value = value.into();
        }
    }

    /// Append an empty condition row.
    pub fn add_condition(&mut self) {
        self.conditions.push(ConditionDraft::new());
        self.condition_cursor = self.conditions.len().saturating_sub(1);
    }

    /// Remove a condition row. The wizard always keeps at least one row to
    /// preserve the "empty all-group matches nothing" guard at the
    /// executor level — removing the last row is a no-op.
    pub fn remove_condition(&mut self, idx: usize) {
        if self.conditions.len() <= 1 {
            return;
        }
        if idx < self.conditions.len() {
            self.conditions.remove(idx);
            self.condition_cursor = self
                .condition_cursor
                .min(self.conditions.len().saturating_sub(1));
        }
    }

    /// Borrow the action draft.
    pub fn action(&self) -> &ActionDraft {
        &self.action
    }

    /// Switch the action kind. Clears the inline value when the new kind
    /// doesn't need one (DeleteToRecycleBin).
    pub fn set_action_kind(&mut self, kind: ActionKind) {
        self.action.kind = kind;
        if matches!(kind, ActionKind::DeleteToRecycleBin) {
            self.action.value.clear();
        }
    }

    /// Update the action's inline value.
    pub fn set_action_value(&mut self, value: impl Into<String>) {
        self.action.value = value.into();
    }

    /// Borrow the run mode choice.
    pub fn run_mode(&self) -> RunModeChoice {
        self.run_mode
    }

    /// Switch the run mode.
    pub fn set_run_mode(&mut self, value: RunModeChoice) {
        self.run_mode = value;
    }

    /// Borrow the interval minutes.
    pub fn interval_minutes(&self) -> u32 {
        self.interval_minutes
    }

    /// Update the interval minutes — clamped to
    /// [`INTERVAL_MIN_MINUTES`]..=[`INTERVAL_MAX_MINUTES`].
    pub fn set_interval_minutes(&mut self, value: u32) {
        self.interval_minutes = value.clamp(INTERVAL_MIN_MINUTES, INTERVAL_MAX_MINUTES);
    }

    /// Borrow the preview hits.
    pub fn preview_hits(&self) -> &[String] {
        &self.preview_hits
    }

    /// Replace the preview hits — called by the shell after the off-thread
    /// `rules::executor::preview` returns.
    pub fn set_preview_hits(&mut self, hits: Vec<String>) {
        self.preview_hits = hits;
        self.preview_busy = false;
    }

    /// Whether the preview list is being recomputed off-thread.
    pub fn preview_busy(&self) -> bool {
        self.preview_busy
    }

    /// Mark the preview as busy. Called by the wizard internally before it
    /// emits [`RulesWizardAction::PreviewRequest`].
    fn mark_preview_busy(&mut self) {
        self.preview_busy = true;
    }

    /// Borrow the last error message.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Set the last error — surfaced in the footer banner. Pass `None` to
    /// clear it.
    pub fn set_error(&mut self, msg: Option<SmolStr>) {
        self.last_error = msg;
    }

    /// Whether the user can advance from the current step. Maps to the
    /// "Next" button's `disabled` attribute.
    pub fn can_advance(&self) -> bool {
        match self.step {
            WizardStep::Conditions => self.conditions.iter().any(ConditionDraft::is_valid),
            WizardStep::Action => self.action.is_valid(),
            WizardStep::Preview => true,
            WizardStep::Name => !self.name.trim().is_empty(),
            WizardStep::Review => self.is_complete(),
        }
    }

    /// Whether the entire rule is complete enough to save.
    pub fn is_complete(&self) -> bool {
        !self.name.trim().is_empty()
            && self.action.is_valid()
            && self.conditions.iter().any(ConditionDraft::is_valid)
    }

    /// User clicked "Next". When transitioning into [`WizardStep::Preview`]
    /// also queues a [`RulesWizardAction::PreviewRequest`] so the shell
    /// can run the off-thread preview.
    pub fn click_next(&mut self) {
        if !self.can_advance() {
            return;
        }
        let next = self.step.next();
        if next == WizardStep::Preview {
            self.mark_preview_busy();
            self.preview_hits.clear();
            if let Some(rule) = self.build_rule() {
                self.pending_action = Some(RulesWizardAction::PreviewRequest(Box::new(rule)));
            }
        }
        self.step = next;
    }

    /// User clicked "Back". No-op on step 1.
    pub fn click_back(&mut self) {
        self.step = self.step.prev();
    }

    /// User clicked "Save" on step 5. No-op when [`is_complete`] is false.
    ///
    /// [`is_complete`]: RulesWizardState::is_complete
    pub fn click_save(&mut self) {
        if !self.is_complete() {
            return;
        }
        if let Some(rule) = self.build_rule() {
            self.pending_action = Some(RulesWizardAction::Save(Box::new(rule)));
        }
    }

    /// User clicked Cancel / pressed Escape / clicked the scrim.
    pub fn click_cancel(&mut self) {
        self.pending_action = Some(RulesWizardAction::Cancel);
    }

    /// Drain the latest action — one-shot.
    pub fn take_action(&mut self) -> Option<RulesWizardAction> {
        self.pending_action.take()
    }

    /// Build a `Rule` from the current state. Returns `None` when the
    /// drafts can't be promoted (action not valid; no valid conditions).
    fn build_rule(&self) -> Option<Rule> {
        let nodes: Vec<ConditionNode> = self
            .conditions
            .iter()
            .filter_map(|d| d.to_condition().map(ConditionNode::Leaf))
            .collect();
        if nodes.is_empty() {
            return None;
        }
        let conditions = match self.combine {
            CombineMode::All => ConditionGroup::All(nodes),
            CombineMode::Any => ConditionGroup::Any(nodes),
        };
        let action = self.action.to_action()?;
        let run_mode = match self.run_mode {
            RunModeChoice::OnDemand => RunMode::OnDemand,
            RunModeChoice::OnFileChange => RunMode::OnFileChange,
            RunModeChoice::Interval => RunMode::Interval {
                minutes: self.interval_minutes,
            },
        };
        Some(Rule {
            id: self.rule_id.clone(),
            name: self.name.trim().to_string(),
            enabled: self.enabled,
            conditions,
            actions: vec![action],
            run_mode,
            last_run: None,
            run_count: 0,
        })
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the RulesWizard widget subtree. Returns the panel chrome Container
/// today; the header / step indicator / per-step body / footer button row
/// land when widget-library composition primitives ship (Input · Dropdown ·
/// Toggle · RadioGroup · List · Button — already in the widget enum).
pub fn build() -> WidgetNode {
    let chrome = RulesWizard::new();
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
mod tests;
