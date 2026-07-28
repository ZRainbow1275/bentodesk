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

mod state;

pub use state::*;

#[cfg(test)]
mod tests;
