//! TimelinePanel — R4-C1 time-machine slider.
//!
//! Visual spec: `timeline_panel.snap.md` (820 px modal, scrubber + delta
//! card + thumbnail). Composition lands when widget-library ships List
//! (T-026), Slider (T-016), Modal (T-023). Today's chrome is a typed
//! Container plus the supporting state-machine + helper surface that the
//! body composition will pull on directly so dispatcher wiring is stable
//! ahead of the visual layer.
//!
//! Backend dep: `bento_nano_backend::timeline::{CheckpointMeta, DeltaSummary,
//! TimelineEvent}` — already shipped (T-089). The panel reads from a
//! `crossbeam_channel::Receiver<TimelineEvent>` published by the hook so
//! the slider stays in sync without polling the disk-backed store.

use bento_nano_backend::timeline::{CheckpointMeta, DeltaSummary};
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_widget::WidgetNode;
use smol_str::SmolStr;

use super::default_modal_chrome;

// -----------------------------------------------------------------------------
// Snap.md derived geometry constants — pinned per visual spec for downstream
// hit-testing + animation timing. Mirrors `TimelinePanel.css` 1:1.
// -----------------------------------------------------------------------------

/// Modal preferred width in DIPs — `width: min(820px, 92vw)`. Resolves to
/// 820 px on any viewport ≥ 891 px wide (820 / 0.92).
pub const PANEL_PREFERRED_WIDTH: f32 = 820.0;

/// Maximum modal width as fraction of viewport — `min(_, 92vw)` clamp.
pub const PANEL_MAX_WIDTH_FRACTION: f32 = 0.92;

/// Maximum modal height as fraction of viewport — `max-height: 80vh`.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.80;

/// Outer panel padding — `.timeline-panel { padding: 24px }`.
pub const PANEL_PADDING: f32 = 24.0;

/// Outer panel corner radius — `border-radius: 16px`.
pub const PANEL_CORNER_RADIUS: f32 = 16.0;

/// Open animation duration — 200 ms `cubic-bezier(0.16, 1, 0.3, 1)` shared
/// keyframe with all other modals.
pub const PANEL_OPEN_DURATION_MS: u32 = 200;

/// Vertical gap between header / body / details inside the panel column.
pub const PANEL_INNER_GAP: f32 = 16.0;

/// Slider wrap padding — `.timeline-slider-wrap { padding: 28px 8px 16px }`.
/// Top padding is intentionally larger to give the marker dots room above
/// the native range input.
pub const SLIDER_WRAP_PADDING_TOP: f32 = 28.0;
pub const SLIDER_WRAP_PADDING_X: f32 = 8.0;
pub const SLIDER_WRAP_PADDING_BOTTOM: f32 = 16.0;

/// Auto-marker dot diameter (idle state).
pub const MARKER_DOT_DIAMETER: f32 = 6.0;

/// Auto-marker dot diameter (active / hovered state).
pub const MARKER_DOT_DIAMETER_ACTIVE: f32 = 9.0;

/// Details thumbnail aspect ratio (`aspect-ratio: 16 / 9`).
pub const THUMBNAIL_ASPECT_RATIO: f32 = 16.0 / 9.0;

/// Details thumbnail max width — `max-width: 480px`.
pub const THUMBNAIL_MAX_WIDTH: f32 = 480.0;

/// Selected-stack aux renderer panel margin.
pub const RUNTIME_PANEL_MARGIN_PX: f32 = 16.0;

/// Left/right inset used by the D2D runtime renderer.
pub const RUNTIME_PANEL_INSET_PX: f32 = 18.0;

/// Runtime action button height in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_HEIGHT_PX: f32 = 28.0;

/// Runtime action button top in the D2D aux panel.
pub const RUNTIME_ACTION_BUTTON_TOP_PX: f32 = 108.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 148.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 38.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 46.0;

/// The current runtime renderer shows at most eight visible rows.
pub const RUNTIME_VISIBLE_ROW_LIMIT: usize = 8;

/// TimelinePanel colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelinePanelChrome {
    /// Drop shadow descriptor drawn behind the panel.
    pub panel_shadow: Shadow,
    /// Panel radius.
    pub panel_radius: BorderRadius,
    /// Action button radius.
    pub button_radius: BorderRadius,
    /// Timeline row radius.
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

impl TimelinePanelChrome {
    /// Build TimelinePanel chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build TimelinePanel chrome from explicit active theme token groups.
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
        }
    }

    /// Build TimelinePanel chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `timeline-snapshot.md` metrics):
    /// - panel bg ← `surface_expanded`
    /// - row bg ← `surface_subtle`; selected row ← `surface_active`
    /// - action button bg ← `accent_blue` (primary "立即保存" + active checkpoint marker)
    /// - text primary / muted ← `text_primary` / `text_muted`; error ← `accent_red`
    /// - panel radius ← `expanded` (16); button/row ← `card` (10)
    /// - panel shadow ← `expanded` outer layer
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
        }
    }
}

/// Pointer hit target in the runtime D2D Timeline panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelinePointerHit {
    Save,
    Pin,
    Restore,
    Delete,
    Close,
    Row(usize),
}

/// Static action-button descriptor shared by renderer and shell hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineButtonSpec {
    pub hit: TimelinePointerHit,
    pub label: &'static str,
    pub x_offset: f32,
    pub width: f32,
}

pub const TIMELINE_ACTION_BUTTONS: &[TimelineButtonSpec] = &[
    TimelineButtonSpec {
        hit: TimelinePointerHit::Save,
        label: "Save",
        x_offset: 0.0,
        width: 58.0,
    },
    TimelineButtonSpec {
        hit: TimelinePointerHit::Pin,
        label: "Pin",
        x_offset: 66.0,
        width: 46.0,
    },
    TimelineButtonSpec {
        hit: TimelinePointerHit::Restore,
        label: "Restore",
        x_offset: 120.0,
        width: 78.0,
    },
    TimelineButtonSpec {
        hit: TimelinePointerHit::Delete,
        label: "Delete",
        x_offset: 206.0,
        width: 64.0,
    },
    TimelineButtonSpec {
        hit: TimelinePointerHit::Close,
        label: "Close",
        x_offset: 278.0,
        width: 58.0,
    },
];

pub fn timeline_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: RUNTIME_PANEL_MARGIN_PX,
        y: RUNTIME_PANEL_MARGIN_PX,
        width: (viewport.width - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(620.0),
        height: (viewport.height - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(420.0),
    }
}

pub fn timeline_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

pub fn timeline_button_rect(viewport: Size, spec: TimelineButtonSpec) -> Rect {
    let panel = timeline_panel_rect(viewport);
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX + spec.x_offset,
        y: panel.y + RUNTIME_ACTION_BUTTON_TOP_PX,
        width: spec.width,
        height: RUNTIME_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn timeline_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = timeline_panel_rect(viewport);
    let list_w = panel.width * 0.56;
    Rect {
        x: panel.x + RUNTIME_PANEL_INSET_PX,
        y: panel.y + RUNTIME_ROW_TOP_PX + (row_index as f32 * RUNTIME_ROW_STRIDE_PX),
        width: list_w - 24.0,
        height: RUNTIME_ROW_HEIGHT_PX,
    }
}

pub fn timeline_hit_test(
    viewport: Size,
    visible_row_count: usize,
    x: f32,
    y: f32,
) -> Option<TimelinePointerHit> {
    for spec in TIMELINE_ACTION_BUTTONS {
        if rect_contains(timeline_button_rect(viewport, *spec), x, y) {
            return Some(spec.hit);
        }
    }
    for row_index in 0..visible_row_count.min(RUNTIME_VISIBLE_ROW_LIMIT) {
        if rect_contains(timeline_row_rect(viewport, row_index), x, y) {
            return Some(TimelinePointerHit::Row(row_index));
        }
    }
    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

// -----------------------------------------------------------------------------
// State machine — what the slider is currently doing.
// -----------------------------------------------------------------------------

/// Scrubber interaction state — mirrors the 1.x `(dragIndex, hoverIndex)`
/// pair as a single closed enum so the body composition's match arms are
/// exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrubState {
    /// Idle — slider parks at the newest entry (`reversed.length - 1`).
    Idle,
    /// User is hovering an auto-marker; preview prefetches but no commit.
    Hovering { idx: u32 },
    /// User is actively dragging the range input — release commits restore.
    Dragging { idx: u32 },
}

impl ScrubState {
    /// Resolve the active checkpoint index per the 1.x `activeIdx` memo:
    /// drag wins over hover; otherwise the newest entry. `len` is the
    /// number of checkpoints in the slider (oldest-left, newest-right).
    pub fn active_idx(self, len: u32) -> Option<u32> {
        match self {
            Self::Dragging { idx } | Self::Hovering { idx } => {
                if len > 0 && idx < len {
                    Some(idx)
                } else if len > 0 {
                    Some(len - 1)
                } else {
                    None
                }
            }
            Self::Idle => {
                if len > 0 {
                    Some(len - 1)
                } else {
                    None
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Marker stripe — driven directly off the backend's CheckpointMeta list.
// -----------------------------------------------------------------------------

/// Compute the absolute X (in DIPs) where a marker dot sits inside the
/// slider track, given the marker's index, the total marker count, and the
/// slider track's width. Mirrors the 1.x `left: <pct>%` formula.
pub fn marker_x(idx: u32, total: u32, track_width: f32) -> f32 {
    if total <= 1 {
        track_width * 0.5
    } else {
        let denom = (total - 1) as f32;
        let pct = (idx as f32) / denom;
        track_width * pct
    }
}

/// Format the per-checkpoint detail caption — used by both the marker
/// `title` tooltip AND the details card header. 1.x format:
/// `"<localised time> · <delta_summary>"`.
pub fn marker_caption(meta: &CheckpointMeta, time_label: &str) -> SmolStr {
    if meta.delta_summary.is_empty() {
        SmolStr::from(time_label)
    } else {
        SmolStr::from(format!("{time_label} \u{00B7} {}", meta.delta_summary))
    }
}

/// Format a delta summary into the 14 px Semibold "delta line" string the
/// details card renders. Falls back to "no change" when both metric
/// buckets are zero (matches the 1.x `t("timelineNoChange")` copy slot).
pub fn delta_line(delta: &DeltaSummary) -> SmolStr {
    let s = delta.human();
    SmolStr::from(s)
}

// -----------------------------------------------------------------------------
// Builder — returns the chrome Container today.
// -----------------------------------------------------------------------------

/// Build the TimelinePanel widget subtree. Returns the chrome Container
/// today; the header / scrubber / details composition lands when the
/// widget primitives ship.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_modal_chrome(Edges::all(PANEL_PADDING)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_backend::timeline::Checkpoint;
    use bento_nano_theme as theme;

    fn make_meta(id: &str, summary: &str) -> CheckpointMeta {
        // Reuse the backend's `From<&Checkpoint>` so the test exercises the
        // real conversion shape rather than re-deriving the metadata
        // fields by hand.
        let cp = Checkpoint {
            id: SmolStr::from(id),
            snapshot: bento_nano_backend::layout::DesktopSnapshot {
                id: SmolStr::new_static("s"),
                name: "s".to_string(),
                resolution: bento_nano_backend::layout::Resolution {
                    width: 1,
                    height: 1,
                },
                dpi: 1.0,
                zones: Vec::new(),
                captured_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
            },
            delta: DeltaSummary::default(),
            delta_summary: summary.to_string(),
            trigger: SmolStr::new_static(""),
            coalesce_key: None,
            pinned: false,
        };
        CheckpointMeta::from(&cp)
    }

    #[test]
    fn build_returns_a_padded_container_at_24px() {
        use bento_nano_layout::LayoutSource;
        let node = build();
        let layout = node.layout();
        assert!((layout.padding.left - PANEL_PADDING).abs() < 0.01);
        assert!((layout.padding.right - PANEL_PADDING).abs() < 0.01);
        assert!((layout.padding.top - PANEL_PADDING).abs() < 0.01);
        assert!((layout.padding.bottom - PANEL_PADDING).abs() < 0.01);
    }

    #[test]
    fn timeline_panel_chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
        palette.active_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

        let chrome = TimelinePanelChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.row_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(
            chrome.selected_background,
            Color::from_u8(0x44, 0xAA, 0xEE, 0x66)
        );
        assert_eq!(
            chrome.action_background,
            Color::from_u8(0x33, 0x44, 0x55, 0x99)
        );
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
        assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
    }

    #[test]
    fn timeline_panel_chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = theme::current().palette;
        let radius = theme::RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let mut shadow = theme::shadow::DEFAULT;
        shadow.md = Shadow {
            offset_x: 2.0,
            offset_y: 5.0,
            blur: 13.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
        };

        let chrome = TimelinePanelChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.button_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
    }

    #[test]
    fn timeline_panel_shadow_rect_uses_token_shadow_geometry() {
        let panel = Rect {
            x: 24.0,
            y: 30.0,
            width: 320.0,
            height: 180.0,
        };
        let shadow = Shadow {
            offset_x: 3.0,
            offset_y: 5.0,
            blur: 11.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
        };

        let rect = timeline_panel_shadow_rect(panel, shadow);

        assert_eq!(
            rect,
            Rect {
                x: 16.0,
                y: 24.0,
                width: 342.0,
                height: 202.0,
            }
        );
    }

    #[test]
    fn snap_constants_match_baseline_css() {
        assert_eq!(PANEL_PREFERRED_WIDTH, 820.0);
        assert!((PANEL_MAX_WIDTH_FRACTION - 0.92).abs() < f32::EPSILON);
        assert!((PANEL_MAX_HEIGHT_FRACTION - 0.80).abs() < f32::EPSILON);
        assert_eq!(PANEL_PADDING, 24.0);
        assert_eq!(PANEL_CORNER_RADIUS, 16.0);
        assert_eq!(PANEL_OPEN_DURATION_MS, 200);
        assert_eq!(PANEL_INNER_GAP, 16.0);
        assert_eq!(SLIDER_WRAP_PADDING_TOP, 28.0);
        assert_eq!(SLIDER_WRAP_PADDING_X, 8.0);
        assert_eq!(SLIDER_WRAP_PADDING_BOTTOM, 16.0);
        assert_eq!(MARKER_DOT_DIAMETER, 6.0);
        assert_eq!(MARKER_DOT_DIAMETER_ACTIVE, 9.0);
        assert!((THUMBNAIL_ASPECT_RATIO - (16.0 / 9.0)).abs() < f32::EPSILON);
        assert_eq!(THUMBNAIL_MAX_WIDTH, 480.0);
        assert_eq!(RUNTIME_PANEL_MARGIN_PX, 16.0);
        assert_eq!(RUNTIME_PANEL_INSET_PX, 18.0);
        assert_eq!(RUNTIME_ACTION_BUTTON_HEIGHT_PX, 28.0);
        assert_eq!(RUNTIME_ACTION_BUTTON_TOP_PX, 108.0);
        assert_eq!(RUNTIME_ROW_TOP_PX, 148.0);
        assert_eq!(RUNTIME_ROW_HEIGHT_PX, 38.0);
        assert_eq!(RUNTIME_ROW_STRIDE_PX, 46.0);
        assert_eq!(RUNTIME_VISIBLE_ROW_LIMIT, 8);
    }

    #[test]
    fn runtime_hit_test_maps_buttons_and_visible_rows() {
        let viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        for spec in TIMELINE_ACTION_BUTTONS {
            let rect = timeline_button_rect(viewport, *spec);
            assert_eq!(
                timeline_hit_test(viewport, 3, rect.x + 1.0, rect.y + 1.0),
                Some(spec.hit)
            );
        }

        let second_row = timeline_row_rect(viewport, 1);
        assert_eq!(
            timeline_hit_test(viewport, 3, second_row.x + 1.0, second_row.y + 1.0),
            Some(TimelinePointerHit::Row(1))
        );
        assert_eq!(
            timeline_hit_test(viewport, 1, second_row.x + 1.0, second_row.y + 1.0),
            None
        );
    }

    #[test]
    fn scrub_state_idle_parks_on_newest() {
        let state = ScrubState::Idle;
        assert_eq!(state.active_idx(5), Some(4));
        assert_eq!(state.active_idx(1), Some(0));
        assert_eq!(state.active_idx(0), None);
    }

    #[test]
    fn scrub_state_dragging_overrides_hover() {
        // Dragging is the latest user intent — tested by the variant
        // ordering rather than by mixing two states. The 1.x `activeIdx`
        // memo prefers `dragIndex` over `hoverIndex` over the newest fallback.
        assert_eq!(ScrubState::Dragging { idx: 1 }.active_idx(5), Some(1));
        assert_eq!(ScrubState::Hovering { idx: 2 }.active_idx(5), Some(2));
    }

    #[test]
    fn scrub_state_clamps_out_of_range_to_newest() {
        // Defensive — guards against a stale Hovering(idx) carried across a
        // backend refresh that shrank the list. The 1.x React hooks guard
        // via the `Show` fallback; the Rust port collapses to the same
        // visible behaviour without a panic.
        assert_eq!(ScrubState::Hovering { idx: 99 }.active_idx(5), Some(4));
        assert_eq!(ScrubState::Dragging { idx: 99 }.active_idx(5), Some(4));
    }

    #[test]
    fn marker_x_centres_single_marker() {
        // Single-marker timelines pin the dot to the slider centre per
        // the 1.x `left: 50%` fallback when `reversed.length === 1`.
        assert!((marker_x(0, 1, 800.0) - 400.0).abs() < 0.01);
        assert!((marker_x(0, 0, 800.0) - 400.0).abs() < 0.01);
    }

    #[test]
    fn marker_x_distributes_evenly_for_two_or_more() {
        // 5 markers across an 800 px track → 0, 200, 400, 600, 800.
        assert!((marker_x(0, 5, 800.0) - 0.0).abs() < 0.01);
        assert!((marker_x(2, 5, 800.0) - 400.0).abs() < 0.01);
        assert!((marker_x(4, 5, 800.0) - 800.0).abs() < 0.01);
    }

    #[test]
    fn marker_caption_omits_separator_when_summary_empty() {
        let meta = make_meta("a", "");
        assert_eq!(marker_caption(&meta, "12:34").as_str(), "12:34");
    }

    #[test]
    fn marker_caption_includes_summary_with_middle_dot_separator() {
        let meta = make_meta("a", "+3 items");
        // U+00B7 MIDDLE DOT — matches the 1.x display character so the
        // tooltip text round-trips with screenshots.
        assert_eq!(
            marker_caption(&meta, "12:34").as_str(),
            "12:34 \u{00B7} +3 items"
        );
    }

    #[test]
    fn delta_line_falls_back_to_no_change_when_zero() {
        let line = delta_line(&DeltaSummary::default());
        assert_eq!(line.as_str(), "no change");
    }

    #[test]
    fn delta_line_renders_human_summary() {
        let delta = DeltaSummary {
            items_added: 2,
            zones_removed: 1,
            ..Default::default()
        };
        let line = delta_line(&delta);
        assert!(line.contains("+2 items"));
        assert!(line.contains("-1 zones"));
    }

    #[test]
    fn timeline_panel_state_requires_matching_confirm_for_restore_and_delete() {
        let mut state = crate::business::timeline::TimelinePanelState::new();
        state.set_entries(vec![make_meta("cp-1", "one"), make_meta("cp-2", "two")]);

        assert!(!state.confirm_restore_or_arm(SmolStr::new_static("cp-1")));
        assert_eq!(
            state.restore_confirmation().map(SmolStr::as_str),
            Some("cp-1")
        );
        assert!(state.delete_confirmation().is_none());
        assert!(state.confirm_restore_or_arm(SmolStr::new_static("cp-1")));
        assert!(state.restore_confirmation().is_none());

        assert!(!state.confirm_delete_or_arm(SmolStr::new_static("cp-1")));
        assert_eq!(
            state.delete_confirmation().map(SmolStr::as_str),
            Some("cp-1")
        );
        state.select_next();
        assert!(state.delete_confirmation().is_none());

        assert!(!state.confirm_delete_or_arm(SmolStr::new_static("cp-2")));
        assert!(state.confirm_delete_or_arm(SmolStr::new_static("cp-2")));
    }

    #[test]
    fn timeline_panel_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let chrome = TimelinePanelChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(chrome.panel_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.row_background, style_tokens::PALETTE_DARK.surface_subtle);
        assert_eq!(chrome.selected_background, style_tokens::PALETTE_DARK.surface_active);
        assert_eq!(chrome.action_background, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.error_color, style_tokens::PALETTE_DARK.accent_red);
        assert_eq!(chrome.panel_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        assert_eq!(chrome.button_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.row_radius, BorderRadius::all(style_tokens::RADIUS.card));
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }
}
