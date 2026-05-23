//! Business surface — `AutoLayoutMenu` (T-067b).
//!
//! Popover triggered from `BulkManagerPanel`'s "Auto Layout" button. Lets
//! the user pick one of five Tauri-compatible layout strategies (`grid` /
//! `row` / `column` / `spiral` / `organic`); the shell then applies a single
//! `BulkApplyLayout` Command.
//!
//! Visual spec: `auto_layout_menu.snap.md`. Pairs with
//! `business::bulk_manager_panel`.
//!
//! # State machine
//!
//! Mirrors the Wave-E shape (see `business::capsule_picker` and
//! `business::smart_group_suggestor`): user intents collapse into a closed
//! [`AutoLayoutAction`] enum, drained one-shot via
//! [`AutoLayoutMenuState::take_action`].
//!
//! # Spec compliance
//!
//! - §10 hot-path: every label/description is a `&'static str` table look-up
//!   (no allocation on render); the popover never heap-allocates on open.
//! - §11 ΔB: every public DTO derives `serde::{Serialize, Deserialize}`.
//! - §11.1: zero `unsafe` in this UI layer.
//! - §15: this single .rs file ships under the 800-LOC budget.
//! - §17: zero `todo!()` / `unimplemented!()` / `panic!()` / `unwrap()` /
//!   `expect()` in production code.

use core::fmt;

use bento_nano_layout::Direction;
use bento_nano_style::{BorderRadius, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};

use crate::dispatcher::{BulkLayoutAlgorithm, Command};

// -----------------------------------------------------------------------------
// Snap.md geometry constants — pinned per the visual spec.
// -----------------------------------------------------------------------------

/// Popover width in DIPs — `min(280px, 92vw)` per snap.md.
pub const POPOVER_WIDTH_PX: f32 = 280.0;

/// Maximum popover width as fraction of viewport — `min(_, 92vw)` clamp.
pub const POPOVER_MAX_WIDTH_FRACTION: f32 = 0.92;

/// Outer popover padding — 8 px uniform per snap.md.
pub const POPOVER_PADDING_PX: f32 = 8.0;

/// Outer popover corner radius — 12 px.
pub const POPOVER_CORNER_RADIUS_PX: f32 = 12.0;

/// Header row height (title + close button row).
pub const HEADER_HEIGHT_PX: f32 = 32.0;

/// Gap between the header and the option list.
pub const HEADER_BOTTOM_MARGIN_PX: f32 = 8.0;

/// Per-row outer vertical padding.
pub const ROW_PADDING_Y_PX: f32 = 10.0;

/// Per-row outer horizontal padding.
pub const ROW_PADDING_X_PX: f32 = 12.0;

/// Per-row corner radius.
pub const ROW_CORNER_RADIUS_PX: f32 = 8.0;

/// Per-row icon slot size (square).
pub const ROW_ICON_SIZE_PX: f32 = 32.0;

/// Inner gap between row slots (icon → info column).
pub const ROW_INNER_GAP_PX: f32 = 12.0;

// -----------------------------------------------------------------------------
// LayoutStrategy — closed enum of the Tauri-compatible algorithms.
// -----------------------------------------------------------------------------

/// Public strategy alias used by this business surface. The actual command
/// payload lives in `dispatcher` so producers and shell handlers share one
/// enum.
pub type LayoutStrategy = BulkLayoutAlgorithm;

// -----------------------------------------------------------------------------
// AutoLayoutAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the popover state machine. Drained once per
/// frame via [`AutoLayoutMenuState::take_action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutAction {
    /// User clicked a strategy row. Carries the picked strategy so the
    /// shell can sequence the resulting per-zone Commands without
    /// re-resolving the slug.
    Pick { strategy: LayoutStrategy },
    /// User dismissed the popover (close button, Escape, or scrim
    /// click). Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for AutoLayoutAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pick { strategy } => write!(f, "Pick({})", strategy.wire()),
            Self::Close => f.write_str("Close"),
        }
    }
}

impl AutoLayoutAction {
    /// Translate the action into a dispatcher [`Command`].
    ///
    /// The popover itself does not know the selected row ids, so `Pick` still
    /// returns `None`. The focused BulkManager shell handler resolves selected
    /// ids or listed rows, then emits `Command::BulkApplyLayout`.
    pub fn into_command(self) -> Option<Command> {
        match self {
            Self::Pick { .. } => None,
            Self::Close => None,
        }
    }
}

// -----------------------------------------------------------------------------
// AutoLayoutMenuState — runtime state for the popover.
// -----------------------------------------------------------------------------

/// Popover runtime state.
///
/// - `pending_action` — the latest one-shot [`AutoLayoutAction`] the
///   shell has yet to drain.
///
/// The popover carries no preview / hover state — strategy hover is a
/// renderer-only overlay (matches snap.md).
#[derive(Debug, Default)]
pub struct AutoLayoutMenuState {
    pending_action: Option<AutoLayoutAction>,
}

impl AutoLayoutMenuState {
    /// New empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// User clicked a strategy row. Records a `Pick` action carrying
    /// the strategy.
    pub fn pick(&mut self, strategy: LayoutStrategy) {
        self.pending_action = Some(AutoLayoutAction::Pick { strategy });
    }

    /// User clicked the close button / pressed Escape / clicked the
    /// scrim.
    pub fn close(&mut self) {
        self.pending_action = Some(AutoLayoutAction::Close);
    }

    /// Drain the latest action — one-shot. Returns `None` until the
    /// user clicks something next.
    pub fn take_action(&mut self) -> Option<AutoLayoutAction> {
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

/// Build the AutoLayoutMenu popover subtree. Returns the chrome
/// Container today; the strategy row composition (icon + label +
/// description per row) attaches when widget-library ships the final
/// List + Popup primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let palette = theme::current().palette;
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(POPOVER_WIDTH_PX),
        height: Length::Auto,
        padding: Edges::all(POPOVER_PADDING_PX),
        background: palette.surface_alt,
        radius: BorderRadius::all(POPOVER_CORNER_RADIUS_PX),
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn snap_geometry_constants_pinned() {
        assert_eq!(POPOVER_WIDTH_PX, 280.0);
        assert!((POPOVER_MAX_WIDTH_FRACTION - 0.92).abs() < f32::EPSILON);
        assert_eq!(POPOVER_PADDING_PX, 8.0);
        assert_eq!(POPOVER_CORNER_RADIUS_PX, 12.0);
        assert_eq!(HEADER_HEIGHT_PX, 32.0);
        assert_eq!(HEADER_BOTTOM_MARGIN_PX, 8.0);
        assert_eq!(ROW_PADDING_Y_PX, 10.0);
        assert_eq!(ROW_PADDING_X_PX, 12.0);
        assert_eq!(ROW_CORNER_RADIUS_PX, 8.0);
        assert_eq!(ROW_ICON_SIZE_PX, 32.0);
        assert_eq!(ROW_INNER_GAP_PX, 12.0);
    }

    #[test]
    fn strategy_all_lists_five_in_snap_order() {
        assert_eq!(
            LayoutStrategy::ALL,
            &[
                LayoutStrategy::Grid,
                LayoutStrategy::Row,
                LayoutStrategy::Column,
                LayoutStrategy::Spiral,
                LayoutStrategy::Organic,
            ]
        );
    }

    #[test]
    fn strategy_wire_round_trip_and_unknown_falls_back() {
        for v in LayoutStrategy::ALL {
            assert_eq!(LayoutStrategy::parse(v.wire()), *v);
        }
        assert_eq!(LayoutStrategy::parse("hexagon"), LayoutStrategy::default());
    }

    #[test]
    fn strategy_label_description_icon_are_non_empty() {
        for v in LayoutStrategy::ALL {
            assert!(!v.label().is_empty(), "label empty for {:?}", v);
            assert!(!v.description().is_empty(), "description empty for {:?}", v);
            assert!(!v.icon_slug().is_empty(), "icon_slug empty for {:?}", v);
        }
    }

    #[test]
    fn strategy_default_is_grid() {
        assert_eq!(LayoutStrategy::default(), LayoutStrategy::Grid);
    }

    #[test]
    fn strategy_display_uses_label() {
        assert_eq!(LayoutStrategy::Organic.to_string(), "Organic");
    }

    #[test]
    fn fresh_state_has_no_pending_action() {
        let s = AutoLayoutMenuState::new();
        assert!(!s.has_pending_action());
    }

    #[test]
    fn pick_records_pick_action_with_strategy() {
        let mut s = AutoLayoutMenuState::new();
        s.pick(LayoutStrategy::Row);
        assert!(s.has_pending_action());
        assert_eq!(
            s.take_action(),
            Some(AutoLayoutAction::Pick {
                strategy: LayoutStrategy::Row,
            })
        );
    }

    #[test]
    fn close_records_close_action() {
        let mut s = AutoLayoutMenuState::new();
        s.close();
        assert_eq!(s.take_action(), Some(AutoLayoutAction::Close));
    }

    #[test]
    fn take_action_is_one_shot() {
        let mut s = AutoLayoutMenuState::new();
        s.close();
        assert!(s.take_action().is_some());
        assert!(s.take_action().is_none());
    }

    #[test]
    fn into_command_returns_none_for_both_variants() {
        // Phase 1: shell sequences per-zone Commands itself.
        assert!(
            AutoLayoutAction::Pick {
                strategy: LayoutStrategy::Organic
            }
            .into_command()
            .is_none()
        );
        assert!(AutoLayoutAction::Close.into_command().is_none());
    }

    #[test]
    fn build_returns_popover_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - POPOVER_WIDTH_PX).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
        assert!((layout.padding.top - POPOVER_PADDING_PX).abs() < 0.01);
        assert!((layout.padding.left - POPOVER_PADDING_PX).abs() < 0.01);
    }

    /// ΔB lock: `LayoutStrategy` round-trips through serde (`lowercase`
    /// rename matches the wire format).
    #[test]
    fn layout_strategy_serde_round_trip() {
        for v in LayoutStrategy::ALL {
            let s = serde_json::to_string(v).unwrap_or_default();
            let back: LayoutStrategy = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(*v, back);
        }
        assert_eq!(
            serde_json::to_string(&LayoutStrategy::Organic).unwrap_or_default(),
            "\"organic\""
        );
    }

    /// ΔB lock: `AutoLayoutAction` round-trips through serde so any future
    /// scripting surface (Phase 5+) can hand actions back to the popover.
    #[test]
    fn auto_layout_action_serde_round_trip() {
        let action = AutoLayoutAction::Pick {
            strategy: LayoutStrategy::Column,
        };
        let s = serde_json::to_string(&action).unwrap_or_default();
        let back: AutoLayoutAction = serde_json::from_str(&s).unwrap_or(AutoLayoutAction::Close);
        assert_eq!(back, action);
    }
}
