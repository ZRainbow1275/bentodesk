//! Business surface — `SmartGroupSuggestor` (T-069a).
//!
//! Floating panel that surfaces AI-driven groupings the user can apply
//! with one click. Visual spec: `smart_group_suggestor.snap.md`. Backend
//! feed: [`bento_nano_backend::grouping::suggest_groups`] →
//! [`SuggestedGroup`] list, ranked by confidence.
//!
//! The state machine mirrors the Wave-E pattern (see
//! `business::search_bar::SearchBarState`): user intents collapse into
//! a closed [`SuggestorAction`] enum, drained one-shot via
//! [`SuggestorState::take_action`]. The shell translates the action into
//! a [`crate::dispatcher::Command`] every frame.
//!
//! Hover-preview bridge is **pure local state** — `on_row_hover(id)`
//! records the suggestion id; the render layer reads it and asks
//! `business::highlight_overlay::HighlightOverlayState::set_targets` to
//! draw translucent fills over the matching items. No dispatcher
//! round-trip per team-lead R-2026-05-03 ruling.
//!
//! # Naming note
//!
//! 1.x referenced `GroupSuggestion` / `IconKind` from the brief. Reality
//! (T-087) is `SuggestedGroup` with a free-form `icon: String` slug.
//! Per Option-A ruling, the 2.0 module uses the real backend names and
//! resolves `icon` via [`crate::business::icons::IconRef::parse`].

use bento_nano_backend::grouping::SuggestedGroup;
use bento_nano_layout::Direction;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bento_nano_theme as theme;
use bento_nano_widget::{ContainerNode, WidgetNode};
use core::fmt;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::dispatcher::Command;

// -----------------------------------------------------------------------------
// Snap.md geometry constants — pinned per the visual spec.
// -----------------------------------------------------------------------------

/// Panel width in DIPs — `width: min(480px, 92vw)` per snap.md.
pub const PANEL_WIDTH_PX: f32 = 480.0;

/// Maximum panel width as fraction of viewport — `min(_, 92vw)` clamp.
pub const PANEL_MAX_WIDTH_FRACTION: f32 = 0.92;

/// Maximum panel height as fraction of viewport — `max-height: 80vh`.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.94;

/// Outer panel padding — 24 px uniform per snap.md.
pub const PANEL_PADDING_PX: f32 = 24.0;

/// Outer panel corner radius — 16 px (shared with timeline / settings).
pub const PANEL_CORNER_RADIUS_PX: f32 = 16.0;

/// Vertical gap between rows in the suggestion list.
pub const ROW_GAP_PX: f32 = 8.0;

/// Per-row inner vertical padding.
pub const ROW_PADDING_Y_PX: f32 = 12.0;

/// Per-row inner horizontal padding.
pub const ROW_PADDING_X_PX: f32 = 16.0;

/// Per-row corner radius.
pub const ROW_CORNER_RADIUS_PX: f32 = 8.0;

/// Icon slot size (square) on each row.
pub const ROW_ICON_SIZE_PX: f32 = 28.0;

/// Inline cap on visible suggestions — backend caps at 5 already
/// (`suggestions::suggest_groups` truncates), this matches.
pub const MAX_VISIBLE_SUGGESTIONS: usize = 5;

/// Runtime renderer margin inside the selected-stack aux HWND.
pub const RUNTIME_PANEL_MARGIN_PX: f32 = 16.0;

/// Runtime status/header body top in the D2D aux panel.
pub const RUNTIME_STATUS_TOP_PX: f32 = 74.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 108.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 58.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 66.0;

/// Runtime Apply button width.
pub const RUNTIME_APPLY_BUTTON_WIDTH_PX: f32 = 68.0;

/// Runtime Dismiss button width.
pub const RUNTIME_DISMISS_BUTTON_WIDTH_PX: f32 = 30.0;

/// Runtime close button size.
pub const RUNTIME_CLOSE_BUTTON_SIZE_PX: f32 = 58.0;

pub const RUNTIME_PREVIEW_TOP_PX: f32 =
    RUNTIME_ROW_TOP_PX + (MAX_VISIBLE_SUGGESTIONS as f32 * RUNTIME_ROW_STRIDE_PX) + 10.0;

pub const RUNTIME_PREVIEW_ROW_HEIGHT_PX: f32 = 18.0;

pub const RUNTIME_PREVIEW_ROW_STRIDE_PX: f32 = 20.0;

pub const MAX_VISIBLE_PREVIEW_FILES: usize = 2;

pub const RUNTIME_PREVIEW_BUTTON_WIDTH_PX: f32 = 54.0;

/// SmartGroupSuggestor colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartGroupSuggestorChrome {
    /// Drop shadow descriptor drawn behind the Suggestor panel.
    pub panel_shadow: Shadow,
    /// Suggestor panel radius.
    pub panel_radius: BorderRadius,
    /// Suggestion row radius.
    pub row_radius: BorderRadius,
    /// Confidence badge radius.
    pub badge_radius: BorderRadius,
    /// Apply/Dismiss row action radius.
    pub action_radius: BorderRadius,
    /// Close button radius.
    pub close_radius: BorderRadius,
    /// Manual-selection preview panel radius.
    pub preview_radius: BorderRadius,
    /// Manual-selection All/None button radius.
    pub preview_button_radius: BorderRadius,
    /// Panel fill colour.
    pub panel_background: Color,
    /// Default suggestion row fill colour.
    pub row_background: Color,
    /// Selected suggestion row fill colour.
    pub selected_background: Color,
    /// Apply/select button fill colour.
    pub action_background: Color,
    /// Dismiss/close button fill colour.
    pub danger_background: Color,
    /// Manual-selection preview panel fill colour.
    pub preview_background: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
}

impl SmartGroupSuggestorChrome {
    /// Build SmartGroupSuggestor chrome from the currently active app palette.
    pub fn from_palette(palette: theme::PaletteTokens) -> Self {
        Self::from_tokens(palette, theme::radius::DEFAULT, theme::shadow::DEFAULT)
    }

    /// Build SmartGroupSuggestor chrome from explicit active theme token groups.
    pub fn from_tokens(
        palette: theme::PaletteTokens,
        radius: theme::RadiusTokens,
        shadow: theme::ShadowTokens,
    ) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            row_radius: radius.lg,
            badge_radius: radius.md,
            action_radius: radius.lg,
            close_radius: radius.lg,
            preview_radius: radius.lg,
            preview_button_radius: radius.md,
            panel_background: palette.surface,
            row_background: palette.surface_alt,
            selected_background: palette.selection,
            action_background: palette.accent,
            danger_background: palette.danger,
            preview_background: palette.surface_alt,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
        }
    }

    /// Build SmartGroupSuggestor chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `search-bar-and-suggestor.md` SmartGroup metrics +
    /// Wave B `token-mapping.md`):
    /// - panel + preview bg ← `surface_expanded`
    /// - row bg ← `surface_subtle`; selected ← `surface_active`
    /// - apply action button ← `accent_blue`; dismiss ← `accent_red`
    /// - panel radius ← `expanded` (16); rows/actions/preview ← `card` (10); badge ← `card`
    /// - panel shadow ← `expanded` outer layer
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            panel_shadow: shadow.expanded,
            panel_radius: BorderRadius::all(radius.expanded),
            row_radius: BorderRadius::all(radius.card),
            badge_radius: BorderRadius::all(radius.card),
            action_radius: BorderRadius::all(radius.card),
            close_radius: BorderRadius::all(radius.card),
            preview_radius: BorderRadius::all(radius.card),
            preview_button_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            row_background: palette.surface_subtle,
            selected_background: palette.surface_active,
            action_background: palette.accent_blue,
            danger_background: palette.accent_red,
            preview_background: palette.surface_expanded,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
        }
    }
}

/// Pointer hit target in the runtime D2D SmartGroupSuggestor panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestorPointerHit {
    Apply(usize),
    Dismiss(usize),
    Row(usize),
    SelectAllFiles,
    SelectNoFiles,
    TogglePreviewFile(usize),
    Close,
}

/// Confidence score boundary — High vs Medium tone.
pub const CONFIDENCE_HIGH_THRESHOLD: f64 = 0.80;

/// Confidence score boundary — Medium vs Low tone.
pub const CONFIDENCE_MEDIUM_THRESHOLD: f64 = 0.50;

/// Translucency applied when deriving the badge background from a base
/// palette tone (≈ 20 %).
pub const BADGE_BG_ALPHA: f32 = 0.20;

// -----------------------------------------------------------------------------
// Confidence tone — the three visual buckets the snap.md badge uses.
// -----------------------------------------------------------------------------

/// Three confidence buckets. The renderer keys badge background + text
/// off this enum so theme switches re-paint via `palette.success` /
/// `palette.warning` / `palette.text_muted` without rebuilding the
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceTone {
    Low,
    Medium,
    High,
}

impl ConfidenceTone {
    /// Static label slot — picks up the localised string in the
    /// renderer; this constant is the i18n key suffix mirror of the 1.x
    /// `t("smartGroupConfidenceHigh")` family.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Bucket a confidence score into a tone per snap.md thresholds.
pub fn confidence_tone(score: f64) -> ConfidenceTone {
    if score >= CONFIDENCE_HIGH_THRESHOLD {
        ConfidenceTone::High
    } else if score >= CONFIDENCE_MEDIUM_THRESHOLD {
        ConfidenceTone::Medium
    } else {
        ConfidenceTone::Low
    }
}

/// Resolve the (background, text) palette colours for a tone. Background
/// inherits the base tone with a snap.md-mandated ~20 % alpha so the
/// badge sits politely on top of the row's `surface_alt` tint.
pub fn tone_colors(tone: ConfidenceTone) -> (Color, Color) {
    let palette = theme::current().palette;
    tone_colors_from_palette(tone, palette)
}

/// Resolve confidence badge colours from an explicit active palette.
pub fn tone_colors_from_palette(
    tone: ConfidenceTone,
    palette: theme::PaletteTokens,
) -> (Color, Color) {
    let base = match tone {
        ConfidenceTone::High => palette.success,
        ConfidenceTone::Medium => palette.warning,
        ConfidenceTone::Low => palette.text_muted,
    };
    let bg = Color {
        a: BADGE_BG_ALPHA,
        ..base
    };
    (bg, base)
}

/// Resolve confidence badge colours from Wave B Tauri tokens.
///
/// Mapping (Wave A search-bar-and-suggestor.md: `--accent-green` confidence-high,
/// `--accent-orange` mid, `--text-muted` low):
/// - High → `accent_green`
/// - Medium → `accent_orange`
/// - Low → `text_muted`
pub fn tone_colors_from_tauri_palette(
    tone: ConfidenceTone,
    palette: PaletteTauri,
) -> (Color, Color) {
    let base = match tone {
        ConfidenceTone::High => palette.accent_green,
        ConfidenceTone::Medium => palette.accent_orange,
        ConfidenceTone::Low => palette.text_muted,
    };
    let bg = Color {
        a: BADGE_BG_ALPHA,
        ..base
    };
    (bg, base)
}

/// Stable derived id for a backend suggestion.
pub fn suggestion_id(suggestion: &SuggestedGroup) -> SmolStr {
    SmolStr::new(format!(
        "{}:{}",
        suggestion.name,
        suggestion.matching_files.len()
    ))
}

/// Compact rule summary shown in the selected-stack D2D row.
pub fn rule_summary(suggestion: &SuggestedGroup) -> SmolStr {
    match suggestion.rule.rule_type {
        bento_nano_backend::layout::GroupRuleType::Extension => {
            if let Some(extensions) = suggestion.rule.extensions.as_ref() {
                if !extensions.is_empty() {
                    return SmolStr::new(extensions.join(", "));
                }
            }
            SmolStr::new_static("Extension")
        }
        bento_nano_backend::layout::GroupRuleType::NamePattern => suggestion
            .rule
            .pattern
            .as_deref()
            .filter(|pattern| !pattern.trim().is_empty())
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new_static("Name pattern")),
        bento_nano_backend::layout::GroupRuleType::ModifiedDate => {
            SmolStr::new_static("Modified date")
        }
    }
}

/// Runtime panel rectangle shared by renderer and shell hit-testing.
pub fn suggestor_panel_rect(viewport: Size) -> Rect {
    let max_width = (viewport.width * PANEL_MAX_WIDTH_FRACTION).max(320.0);
    let width = PANEL_WIDTH_PX.min(max_width).max(320.0);
    let max_height = (viewport.height * PANEL_MAX_HEIGHT_FRACTION).max(240.0);
    let preferred_height = PANEL_PADDING_PX.mul_add(2.0, 472.0);
    let height = preferred_height.min(max_height).max(240.0);
    Rect {
        x: ((viewport.width - width) * 0.5).max(RUNTIME_PANEL_MARGIN_PX),
        y: RUNTIME_PANEL_MARGIN_PX,
        width,
        height,
    }
}

pub fn suggestor_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Runtime close button rectangle.
pub fn suggestor_close_rect(viewport: Size) -> Rect {
    let panel = suggestor_panel_rect(viewport);
    Rect {
        x: panel.right() - RUNTIME_CLOSE_BUTTON_SIZE_PX - 14.0,
        y: panel.y + 14.0,
        width: RUNTIME_CLOSE_BUTTON_SIZE_PX,
        height: 26.0,
    }
}

/// Runtime suggestion row rectangle.
pub fn suggestor_row_rect(viewport: Size, row_index: usize) -> Rect {
    let panel = suggestor_panel_rect(viewport);
    Rect {
        x: panel.x + PANEL_PADDING_PX,
        y: panel.y + RUNTIME_ROW_TOP_PX + (row_index as f32 * RUNTIME_ROW_STRIDE_PX),
        width: panel.width - (PANEL_PADDING_PX * 2.0),
        height: RUNTIME_ROW_HEIGHT_PX,
    }
}

/// Runtime row Apply button rectangle.
pub fn suggestor_apply_rect(viewport: Size, row_index: usize) -> Rect {
    let row = suggestor_row_rect(viewport, row_index);
    Rect {
        x: row.right() - RUNTIME_APPLY_BUTTON_WIDTH_PX - RUNTIME_DISMISS_BUTTON_WIDTH_PX - 14.0,
        y: row.y + 17.0,
        width: RUNTIME_APPLY_BUTTON_WIDTH_PX,
        height: 24.0,
    }
}

/// Runtime row dismiss button rectangle.
pub fn suggestor_dismiss_rect(viewport: Size, row_index: usize) -> Rect {
    let row = suggestor_row_rect(viewport, row_index);
    Rect {
        x: row.right() - RUNTIME_DISMISS_BUTTON_WIDTH_PX - 8.0,
        y: row.y + 17.0,
        width: RUNTIME_DISMISS_BUTTON_WIDTH_PX,
        height: 24.0,
    }
}

pub fn suggestor_preview_rect(viewport: Size) -> Rect {
    let panel = suggestor_panel_rect(viewport);
    Rect {
        x: panel.x + PANEL_PADDING_PX,
        y: panel.y + RUNTIME_PREVIEW_TOP_PX,
        width: panel.width - (PANEL_PADDING_PX * 2.0),
        height: panel.bottom() - (panel.y + RUNTIME_PREVIEW_TOP_PX) - 12.0,
    }
}

pub fn suggestor_select_all_rect(viewport: Size) -> Rect {
    let preview = suggestor_preview_rect(viewport);
    Rect {
        x: preview.right() - (RUNTIME_PREVIEW_BUTTON_WIDTH_PX * 2.0) - 8.0,
        y: preview.y + 7.0,
        width: RUNTIME_PREVIEW_BUTTON_WIDTH_PX,
        height: 20.0,
    }
}

pub fn suggestor_select_none_rect(viewport: Size) -> Rect {
    let preview = suggestor_preview_rect(viewport);
    Rect {
        x: preview.right() - RUNTIME_PREVIEW_BUTTON_WIDTH_PX,
        y: preview.y + 7.0,
        width: RUNTIME_PREVIEW_BUTTON_WIDTH_PX,
        height: 20.0,
    }
}

pub fn suggestor_preview_file_rect(viewport: Size, preview_offset: usize) -> Rect {
    let preview = suggestor_preview_rect(viewport);
    Rect {
        x: preview.x + 8.0,
        y: preview.y + 32.0 + (preview_offset as f32 * RUNTIME_PREVIEW_ROW_STRIDE_PX),
        width: preview.width - 16.0,
        height: RUNTIME_PREVIEW_ROW_HEIGHT_PX,
    }
}

/// Hit-test the runtime D2D SmartGroupSuggestor panel.
pub fn suggestor_hit_test(
    viewport: Size,
    visible_row_count: usize,
    visible_preview_file_count: usize,
    x: f32,
    y: f32,
) -> Option<SuggestorPointerHit> {
    if rect_contains(suggestor_close_rect(viewport), x, y) {
        return Some(SuggestorPointerHit::Close);
    }
    if rect_contains(suggestor_select_all_rect(viewport), x, y) {
        return Some(SuggestorPointerHit::SelectAllFiles);
    }
    if rect_contains(suggestor_select_none_rect(viewport), x, y) {
        return Some(SuggestorPointerHit::SelectNoFiles);
    }
    for preview_offset in 0..visible_preview_file_count.min(MAX_VISIBLE_PREVIEW_FILES) {
        if rect_contains(suggestor_preview_file_rect(viewport, preview_offset), x, y) {
            return Some(SuggestorPointerHit::TogglePreviewFile(preview_offset));
        }
    }
    for row_index in 0..visible_row_count.min(MAX_VISIBLE_SUGGESTIONS) {
        if rect_contains(suggestor_apply_rect(viewport, row_index), x, y) {
            return Some(SuggestorPointerHit::Apply(row_index));
        }
        if rect_contains(suggestor_dismiss_rect(viewport, row_index), x, y) {
            return Some(SuggestorPointerHit::Dismiss(row_index));
        }
        if rect_contains(suggestor_row_rect(viewport, row_index), x, y) {
            return Some(SuggestorPointerHit::Row(row_index));
        }
    }
    None
}

pub fn path_basename(path: &str) -> &str {
    let slash = path.rfind('/');
    let backslash = path.rfind('\\');
    let index = match (slash, backslash) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    index.map_or(path, |idx| &path[idx + 1..])
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

// -----------------------------------------------------------------------------
// SuggestorAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the panel state machine. Drained once per
/// frame via [`SuggestorState::take_action`]. Translates 1:1 into a
/// [`Command`] (or no-op for `Close`) in the shell consumer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestorAction {
    /// User clicked **Apply** on a suggestion row. Carries the full
    /// suggestion payload so the shell can forward it to the backend
    /// without re-resolving from the suggestion list.
    Apply {
        suggestion_id: SmolStr,
        suggestion: Box<SuggestedGroup>,
    },
    /// User dismissed a single row. Carries the suggestion's stable id
    /// so the shell can prune the matching entry.
    Dismiss { suggestion_id: SmolStr },
    /// User closed the entire panel (close button / Escape / scrim
    /// click). Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for SuggestorAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Apply { suggestion, .. } => write!(f, "Apply({:?})", suggestion.name),
            Self::Dismiss { suggestion_id } => write!(f, "Dismiss({suggestion_id:?})"),
            Self::Close => f.write_str("Close"),
        }
    }
}

impl SuggestorAction {
    /// Translate the action into a dispatcher [`Command`], or `None` for
    /// the `Close` variant which is shell-local.
    pub fn into_command(self) -> Option<Command> {
        match self {
            Self::Apply { suggestion, .. } => Some(Command::GroupingApply { suggestion }),
            Self::Dismiss { suggestion_id } => Some(Command::SuggestorDismiss { suggestion_id }),
            Self::Close => None,
        }
    }
}

// -----------------------------------------------------------------------------
// SuggestorState — runtime state for the panel.
// -----------------------------------------------------------------------------

/// One suggestion view-row in the panel. Stable `id` is what the hover
/// bridge + dismiss action key off — the backend's `SuggestedGroup`
/// has no native id field, so we derive a `SmolStr` from
/// `name:matching_files.len()` at insertion time.
#[derive(Debug, Clone)]
pub struct SuggestionEntry {
    pub id: SmolStr,
    pub suggestion: SuggestedGroup,
    selected_paths: SmallVec<[SmolStr; 8]>,
    focused_path_index: usize,
}

impl SuggestionEntry {
    /// Build an entry from a backend suggestion, deriving a stable id.
    pub fn from_suggestion(s: SuggestedGroup) -> Self {
        let id = suggestion_id(&s);
        let selected_paths = s
            .matching_files
            .iter()
            .map(SmolStr::new)
            .collect::<SmallVec<[SmolStr; 8]>>();
        Self {
            id,
            suggestion: s,
            selected_paths,
            focused_path_index: 0,
        }
    }

    pub fn total_path_count(&self) -> usize {
        self.suggestion.matching_files.len()
    }

    pub fn selected_path_count(&self) -> usize {
        self.selected_paths.len()
    }

    pub fn focused_path_index(&self) -> usize {
        if self.suggestion.matching_files.is_empty() {
            0
        } else {
            self.focused_path_index
                .min(self.suggestion.matching_files.len() - 1)
        }
    }

    pub fn is_path_selected(&self, path_index: usize) -> bool {
        let Some(path) = self.suggestion.matching_files.get(path_index) else {
            return false;
        };
        self.selected_paths
            .iter()
            .any(|selected| selected.as_str() == path)
    }

    pub fn preview_start_index(&self) -> usize {
        let total = self.total_path_count();
        if total <= MAX_VISIBLE_PREVIEW_FILES {
            return 0;
        }
        let half = MAX_VISIBLE_PREVIEW_FILES / 2;
        self.focused_path_index()
            .saturating_sub(half)
            .min(total.saturating_sub(MAX_VISIBLE_PREVIEW_FILES))
    }

    pub fn preview_file_count(&self) -> usize {
        self.total_path_count()
            .saturating_sub(self.preview_start_index())
            .min(MAX_VISIBLE_PREVIEW_FILES)
    }

    pub fn preview_path_index(&self, preview_offset: usize) -> Option<usize> {
        let path_index = self.preview_start_index().checked_add(preview_offset)?;
        (path_index < self.total_path_count()).then_some(path_index)
    }

    pub fn selected_matching_files(&self) -> Vec<String> {
        self.suggestion
            .matching_files
            .iter()
            .filter(|path| {
                self.selected_paths
                    .iter()
                    .any(|selected| selected.as_str() == path.as_str())
            })
            .cloned()
            .collect()
    }

    fn select_all_paths(&mut self) {
        self.selected_paths = self
            .suggestion
            .matching_files
            .iter()
            .map(SmolStr::new)
            .collect();
    }

    fn select_no_paths(&mut self) {
        self.selected_paths.clear();
    }

    fn focus_prev_path(&mut self) -> bool {
        if self.suggestion.matching_files.is_empty() {
            self.focused_path_index = 0;
            return false;
        }
        self.focused_path_index = self.focused_path_index().saturating_sub(1);
        true
    }

    fn focus_next_path(&mut self) -> bool {
        if self.suggestion.matching_files.is_empty() {
            self.focused_path_index = 0;
            return false;
        }
        self.focused_path_index =
            (self.focused_path_index() + 1).min(self.suggestion.matching_files.len() - 1);
        true
    }

    fn toggle_path(&mut self, path_index: usize) -> bool {
        let Some(path) = self.suggestion.matching_files.get(path_index) else {
            return false;
        };
        if let Some(index) = self
            .selected_paths
            .iter()
            .position(|selected| selected.as_str() == path)
        {
            self.selected_paths.remove(index);
        } else {
            self.selected_paths.push(SmolStr::new(path.as_str()));
        }
        self.focused_path_index = path_index;
        true
    }
}

/// Panel runtime state.
///
/// - `entries` — suggestions currently visible (capped at
///   [`MAX_VISIBLE_SUGGESTIONS`]).
/// - `hovered_id` — id of the row the cursor is over, drives the
///   `HighlightOverlay` preview. `None` when no row is hovered.
/// - `applying_id` — id of the row whose Apply button is currently in
///   flight; the shell sets this before forwarding the command and
///   clears it once the backend ack returns. Disables every Apply
///   button while non-`None`.
/// - `pending_action` — the latest one-shot [`SuggestorAction`] waiting
///   for the shell to drain.
#[derive(Debug, Default)]
pub struct SuggestorState {
    entries: SmallVec<[SuggestionEntry; MAX_VISIBLE_SUGGESTIONS]>,
    selected_index: usize,
    hovered_id: Option<SmolStr>,
    applying_id: Option<SmolStr>,
    pending_action: Option<SuggestorAction>,
}

impl SuggestorState {
    /// New empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the suggestion list — backend has answered the most
    /// recent analyse pass. Truncates to [`MAX_VISIBLE_SUGGESTIONS`]
    /// so the inline `SmallVec` stays inline. Resets transient state
    /// (hover, applying) since old ids are no longer valid.
    pub fn set_suggestions(&mut self, suggestions: Vec<SuggestedGroup>) {
        let mut entries: SmallVec<[SuggestionEntry; MAX_VISIBLE_SUGGESTIONS]> = SmallVec::new();
        for s in suggestions.into_iter().take(MAX_VISIBLE_SUGGESTIONS) {
            entries.push(SuggestionEntry::from_suggestion(s));
        }
        self.entries = entries;
        self.selected_index = 0;
        self.hovered_id = None;
        self.applying_id = None;
    }

    /// Borrow the current suggestion entry list.
    pub fn entries(&self) -> &[SuggestionEntry] {
        &self.entries
    }

    /// Number of rows currently visible in the runtime panel.
    pub fn visible_count(&self) -> usize {
        self.entries.len()
    }

    /// Current keyboard cursor index.
    pub const fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Current selected row.
    pub fn selected_entry(&self) -> Option<&SuggestionEntry> {
        self.entries.get(self.selected_index)
    }

    /// Number of visible manual checkbox rows for the selected suggestion.
    pub fn selected_preview_file_count(&self) -> usize {
        self.selected_entry()
            .map(SuggestionEntry::preview_file_count)
            .unwrap_or(0)
    }

    /// Select a visible row by index.
    pub fn select_index(&mut self, row_index: usize) -> bool {
        if row_index >= self.entries.len() {
            return false;
        }
        self.selected_index = row_index;
        true
    }

    /// Move the selected cursor up.
    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    /// Move the selected cursor down.
    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = (self.selected_index + 1).min(self.entries.len() - 1);
        }
    }

    /// Move the focused manual file left/up within the selected row.
    pub fn focus_prev_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.focus_prev_path()
    }

    /// Move the focused manual file right/down within the selected row.
    pub fn focus_next_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.focus_next_path()
    }

    /// Toggle the focused manual checkbox in the selected row.
    pub fn toggle_focused_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.toggle_path(entry.focused_path_index())
    }

    /// Toggle a visible manual checkbox by preview-row offset.
    pub fn toggle_preview_file(&mut self, preview_offset: usize) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        let Some(path_index) = entry.preview_path_index(preview_offset) else {
            return false;
        };
        entry.toggle_path(path_index)
    }

    /// Select all matching files for the selected row.
    pub fn select_all_for_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.select_all_paths();
        true
    }

    /// Select no matching files for the selected row.
    pub fn select_none_for_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.select_no_paths();
        true
    }

    /// Currently-hovered suggestion id (drives the highlight overlay).
    pub fn hovered_id(&self) -> Option<&SmolStr> {
        self.hovered_id.as_ref()
    }

    /// The hovered suggestion entry, if any.
    pub fn hovered_entry(&self) -> Option<&SuggestionEntry> {
        let id = self.hovered_id.as_ref()?;
        self.entries.iter().find(|e| &e.id == id)
    }

    /// Suggestion id whose Apply is in flight (for button disable
    /// gating).
    pub fn applying_id(&self) -> Option<&SmolStr> {
        self.applying_id.as_ref()
    }

    /// Mark a row's Apply as in flight — called by the shell after it
    /// drains an `Apply` action and forwards the Command.
    pub fn mark_applying(&mut self, id: SmolStr) {
        self.applying_id = Some(id);
    }

    /// Backend ack returned — clear the in-flight marker.
    pub fn clear_applying(&mut self) {
        self.applying_id = None;
    }

    /// Cursor entered a row — record the hover so the `HighlightOverlay`
    /// can preview the matching items. Idempotent; setting the same id
    /// twice is a no-op.
    pub fn on_row_hover(&mut self, id: SmolStr) {
        self.hovered_id = Some(id);
    }

    /// Cursor left every row — clear the hover.
    pub fn on_row_leave(&mut self) {
        self.hovered_id = None;
    }

    /// User clicked **Apply** on the row identified by `id`. Records an
    /// `Apply` action carrying the suggestion payload. Returns `true`
    /// when the id matched a known entry; `false` (no action recorded)
    /// when the id was stale (suggestions list changed since the click).
    pub fn apply(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter().find(|e| e.id.as_str() == id) else {
            return false;
        };
        let selected_files = entry.selected_matching_files();
        if selected_files.is_empty() {
            return false;
        }
        let mut suggestion = entry.suggestion.clone();
        suggestion.matching_files = selected_files;
        self.pending_action = Some(SuggestorAction::Apply {
            suggestion_id: entry.id.clone(),
            suggestion: Box::new(suggestion),
        });
        true
    }

    /// Apply the currently-selected row.
    pub fn apply_selected(&mut self) -> bool {
        let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) else {
            return false;
        };
        self.apply(id.as_str())
    }

    /// User dismissed a single row. Records a `Dismiss` action. Like
    /// [`apply`], returns `false` for a stale id.
    pub fn dismiss(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter().find(|e| e.id.as_str() == id) else {
            return false;
        };
        self.pending_action = Some(SuggestorAction::Dismiss {
            suggestion_id: entry.id.clone(),
        });
        true
    }

    /// Dismiss the currently-selected row.
    pub fn dismiss_selected(&mut self) -> bool {
        let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) else {
            return false;
        };
        self.dismiss(id.as_str())
    }

    /// Remove a row after the dispatcher has consumed `SuggestorDismiss`.
    pub fn remove_entry(&mut self, id: &str) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.id.as_str() == id)
        else {
            return false;
        };
        self.entries.remove(index);
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.entries.len() {
            self.selected_index = self.entries.len() - 1;
        }
        if self.hovered_id.as_deref() == Some(id) {
            self.hovered_id = None;
        }
        if self.applying_id.as_deref() == Some(id) {
            self.applying_id = None;
        }
        true
    }

    /// User closed the panel (close button / Escape / scrim click).
    pub fn close(&mut self) {
        self.pending_action = Some(SuggestorAction::Close);
    }

    /// Drain the latest action. Returns `None` until the user clicks
    /// Apply / Dismiss / Close. One-shot — subsequent calls without
    /// further interaction return `None`.
    pub fn take_action(&mut self) -> Option<SuggestorAction> {
        self.pending_action.take()
    }

    /// Whether an action is pending — diagnostics + UI affordance gating.
    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }
}

// -----------------------------------------------------------------------------
// Builder — returns the chrome Container.
// -----------------------------------------------------------------------------

/// Build the SmartGroupSuggestor panel subtree. Returns the chrome
/// Container today; the row composition (icon + meta + badge + Apply +
/// Dismiss) attaches in the next pass when widget-library ships the
/// final List + Modal primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let palette = theme::current().palette;
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(PANEL_WIDTH_PX),
        height: Length::Auto,
        padding: Edges::all(PANEL_PADDING_PX),
        background: palette.surface,
        radius: BorderRadius::all(PANEL_CORNER_RADIUS_PX),
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_backend::grouping::SuggestedGroup;
    use bento_nano_backend::layout::{AutoGroupRule, GroupRuleType};
    use bento_nano_layout::LayoutSource;

    fn make_suggestion(name: &str, files: usize, confidence: f64) -> SuggestedGroup {
        SuggestedGroup {
            name: name.to_string(),
            icon: "folder".to_string(),
            rule: AutoGroupRule {
                rule_type: GroupRuleType::Extension,
                pattern: None,
                extensions: Some(vec!["pdf".to_string()]),
            },
            matching_files: (0..files)
                .map(|i| format!("C:/Desktop/file{i}.pdf"))
                .collect(),
            confidence,
        }
    }

    #[test]
    fn confidence_tone_high_threshold() {
        assert_eq!(confidence_tone(0.85), ConfidenceTone::High);
        assert_eq!(confidence_tone(0.80), ConfidenceTone::High);
    }

    #[test]
    fn confidence_tone_medium_threshold() {
        assert_eq!(confidence_tone(0.79), ConfidenceTone::Medium);
        assert_eq!(confidence_tone(0.50), ConfidenceTone::Medium);
    }

    #[test]
    fn confidence_tone_low_threshold() {
        assert_eq!(confidence_tone(0.49), ConfidenceTone::Low);
        assert_eq!(confidence_tone(0.0), ConfidenceTone::Low);
    }

    #[test]
    fn tone_colors_uses_palette_tokens() {
        let palette = theme::current().palette;
        let (high_bg, high_fg) = tone_colors(ConfidenceTone::High);
        assert_eq!(high_fg, palette.success);
        // Background is the same RGB but with the snap.md alpha applied.
        assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);
        assert_eq!(high_bg.r, palette.success.r);

        let (med_bg, med_fg) = tone_colors(ConfidenceTone::Medium);
        assert_eq!(med_fg, palette.warning);
        assert!((med_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

        let (low_bg, low_fg) = tone_colors(ConfidenceTone::Low);
        assert_eq!(low_fg, palette.text_muted);
        assert!((low_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);
    }

    #[test]
    fn suggestor_chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
        palette.accent = Color::from_u8(0x12, 0x34, 0x56, 0x78);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);

        let chrome = SmartGroupSuggestorChrome::from_palette(palette);

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
            Color::from_u8(0x12, 0x34, 0x56, 0x78)
        );
        assert_eq!(
            chrome.danger_background,
            Color::from_u8(0xCC, 0x44, 0x44, 0xFF)
        );
        assert_eq!(
            chrome.preview_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    }

    #[test]
    fn suggestor_chrome_accepts_explicit_radius_shadow_tokens() {
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
            color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
        };

        let chrome = SmartGroupSuggestorChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.badge_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.action_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.close_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.preview_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.preview_button_radius, BorderRadius::all(7.0));
    }

    #[test]
    fn suggestor_panel_shadow_rect_uses_token_shadow_geometry() {
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
            color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
        };

        let rect = suggestor_panel_shadow_rect(panel, shadow);

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
    fn tone_colors_accept_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.success = Color::from_u8(0x11, 0xAA, 0x22, 0xFF);
        palette.warning = Color::from_u8(0xCC, 0x88, 0x11, 0xFF);
        palette.text_muted = Color::from_u8(0x77, 0x88, 0x99, 0xFF);

        let (high_bg, high_fg) = tone_colors_from_palette(ConfidenceTone::High, palette);
        assert_eq!(high_fg, Color::from_u8(0x11, 0xAA, 0x22, 0xFF));
        assert_eq!(high_bg.r, high_fg.r);
        assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

        let (_, med_fg) = tone_colors_from_palette(ConfidenceTone::Medium, palette);
        assert_eq!(med_fg, Color::from_u8(0xCC, 0x88, 0x11, 0xFF));

        let (_, low_fg) = tone_colors_from_palette(ConfidenceTone::Low, palette);
        assert_eq!(low_fg, Color::from_u8(0x77, 0x88, 0x99, 0xFF));
    }

    #[test]
    fn snap_geometry_constants_pinned() {
        assert_eq!(PANEL_WIDTH_PX, 480.0);
        assert_eq!(PANEL_PADDING_PX, 24.0);
        assert_eq!(PANEL_CORNER_RADIUS_PX, 16.0);
        assert_eq!(ROW_GAP_PX, 8.0);
        assert_eq!(ROW_PADDING_X_PX, 16.0);
        assert_eq!(ROW_PADDING_Y_PX, 12.0);
        assert_eq!(ROW_ICON_SIZE_PX, 28.0);
        assert_eq!(MAX_VISIBLE_SUGGESTIONS, 5);
        assert_eq!(MAX_VISIBLE_PREVIEW_FILES, 2);
        assert!((CONFIDENCE_HIGH_THRESHOLD - 0.80).abs() < f64::EPSILON);
        assert!((CONFIDENCE_MEDIUM_THRESHOLD - 0.50).abs() < f64::EPSILON);
        assert!((BADGE_BG_ALPHA - 0.20).abs() < f32::EPSILON);
    }

    #[test]
    fn runtime_hit_test_distinguishes_row_apply_dismiss_and_close() {
        let viewport = bento_nano_style::Size {
            width: 480.0,
            height: 360.0,
        };
        let row = suggestor_row_rect(viewport, 0);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, row.x + 3.0, row.y + 3.0),
            Some(SuggestorPointerHit::Row(0))
        );
        let apply = suggestor_apply_rect(viewport, 1);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, apply.x + 2.0, apply.y + 2.0),
            Some(SuggestorPointerHit::Apply(1))
        );
        let dismiss = suggestor_dismiss_rect(viewport, 1);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, dismiss.x + 2.0, dismiss.y + 2.0),
            Some(SuggestorPointerHit::Dismiss(1))
        );
        let close = suggestor_close_rect(viewport);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, close.x + 2.0, close.y + 2.0),
            Some(SuggestorPointerHit::Close)
        );
    }

    #[test]
    fn runtime_hit_test_distinguishes_manual_preview_targets() {
        let viewport = bento_nano_style::Size {
            width: 640.0,
            height: 560.0,
        };
        let all = suggestor_select_all_rect(viewport);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, all.x + 2.0, all.y + 2.0),
            Some(SuggestorPointerHit::SelectAllFiles)
        );
        let none = suggestor_select_none_rect(viewport);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, none.x + 2.0, none.y + 2.0),
            Some(SuggestorPointerHit::SelectNoFiles)
        );
        let file = suggestor_preview_file_rect(viewport, 1);
        assert_eq!(
            suggestor_hit_test(viewport, 2, 2, file.x + 2.0, file.y + 2.0),
            Some(SuggestorPointerHit::TogglePreviewFile(1))
        );
    }

    #[test]
    fn build_returns_panel_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH_PX).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
        assert!((layout.padding.top - PANEL_PADDING_PX).abs() < 0.01);
        assert!((layout.padding.left - PANEL_PADDING_PX).abs() < 0.01);
    }

    #[test]
    fn set_suggestions_truncates_to_visible_cap() {
        let mut state = SuggestorState::new();
        let many = (0..(MAX_VISIBLE_SUGGESTIONS + 3))
            .map(|i| make_suggestion(&format!("g{i}"), 4, 0.6))
            .collect::<Vec<_>>();
        state.set_suggestions(many);
        assert_eq!(state.entries().len(), MAX_VISIBLE_SUGGESTIONS);
    }

    #[test]
    fn set_suggestions_resets_transient_state() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
        let id = state.entries()[0].id.clone();
        state.on_row_hover(id.clone());
        state.mark_applying(id);
        // Replacing the list invalidates old ids → both get cleared.
        state.set_suggestions(vec![make_suggestion("images", 4, 0.7)]);
        assert!(state.hovered_id().is_none());
        assert!(state.applying_id().is_none());
    }

    #[test]
    fn keyboard_selection_and_remove_entry_clamp_cursor() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![
            make_suggestion("docs", 4, 0.6),
            make_suggestion("images", 4, 0.7),
        ]);
        state.select_next();
        assert_eq!(state.selected_index(), 1);
        let removed = state.entries()[1].id.clone();
        assert!(state.remove_entry(removed.as_str()));
        assert_eq!(state.selected_index(), 0);
        assert_eq!(state.visible_count(), 1);
    }

    #[test]
    fn entry_id_is_stable_for_same_payload() {
        let s = make_suggestion("docs", 4, 0.6);
        let a = SuggestionEntry::from_suggestion(s.clone());
        let b = SuggestionEntry::from_suggestion(s);
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn on_row_hover_records_id_and_lookup_finds_entry() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 3, 0.7)]);
        let id = state.entries()[0].id.clone();
        state.on_row_hover(id.clone());
        assert_eq!(state.hovered_id(), Some(&id));
        assert!(state.hovered_entry().is_some());
        state.on_row_leave();
        assert!(state.hovered_id().is_none());
        assert!(state.hovered_entry().is_none());
    }

    #[test]
    fn apply_records_action_with_suggestion_payload() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
        let id = state.entries()[0].id.clone();
        assert!(state.apply(id.as_str()));
        assert!(state.has_pending_action());
        let action = state.take_action().expect("action recorded");
        match action {
            SuggestorAction::Apply {
                suggestion_id,
                suggestion,
            } => {
                assert_eq!(suggestion_id.as_str(), "docs:4");
                assert_eq!(suggestion.name, "docs");
                assert_eq!(suggestion.matching_files.len(), 4);
            }
            other => panic!("expected Apply, got {other:?}"),
        }
        // One-shot.
        assert!(state.take_action().is_none());
    }

    #[test]
    fn apply_with_stale_id_records_nothing() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
        assert!(!state.apply("does:not:exist"));
        assert!(!state.has_pending_action());
    }

    #[test]
    fn manual_selection_filters_apply_payload() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);

        assert!(state.toggle_preview_file(0));
        assert!(state.apply_selected());

        match state.take_action() {
            Some(SuggestorAction::Apply { suggestion, .. }) => {
                assert_eq!(suggestion.matching_files.len(), 3);
                assert!(
                    !suggestion
                        .matching_files
                        .iter()
                        .any(|path| path.ends_with("file0.pdf"))
                );
            }
            other => panic!("expected filtered apply action, got {other:?}"),
        }
    }

    #[test]
    fn manual_selection_blocks_empty_apply() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);

        assert!(state.select_none_for_selected());
        assert!(!state.apply_selected());
        assert!(!state.has_pending_action());
        assert!(state.select_all_for_selected());
        assert!(state.apply_selected());
    }

    #[test]
    fn dismiss_records_action_with_suggestion_id() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
        let id = state.entries()[0].id.clone();
        assert!(state.dismiss(id.as_str()));
        assert_eq!(
            state.take_action(),
            Some(SuggestorAction::Dismiss { suggestion_id: id }),
        );
    }

    #[test]
    fn selected_apply_and_dismiss_use_cursor_row() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![
            make_suggestion("docs", 4, 0.6),
            make_suggestion("images", 4, 0.7),
        ]);
        state.select_next();
        assert!(state.apply_selected());
        match state.take_action() {
            Some(SuggestorAction::Apply { suggestion, .. }) => {
                assert_eq!(suggestion.name, "images");
            }
            other => panic!("expected selected apply action, got {other:?}"),
        }
        assert!(state.dismiss_selected());
        assert_eq!(
            state.take_action(),
            Some(SuggestorAction::Dismiss {
                suggestion_id: SmolStr::new("images:4")
            })
        );
    }

    #[test]
    fn close_records_close_action() {
        let mut state = SuggestorState::new();
        state.close();
        assert_eq!(state.take_action(), Some(SuggestorAction::Close));
    }

    #[test]
    fn take_action_clears_pending_flag() {
        let mut state = SuggestorState::new();
        state.close();
        let _ = state.take_action();
        assert!(!state.has_pending_action());
    }

    #[test]
    fn into_command_apply_maps_to_grouping_apply() {
        let s = make_suggestion("docs", 4, 0.6);
        let action = SuggestorAction::Apply {
            suggestion_id: SmolStr::new("docs:4"),
            suggestion: Box::new(s.clone()),
        };
        match action.into_command() {
            Some(Command::GroupingApply { suggestion }) => {
                assert_eq!(suggestion.name, s.name);
            }
            other => panic!("expected GroupingApply, got {other:?}"),
        }
    }

    #[test]
    fn into_command_dismiss_maps_to_suggestor_dismiss() {
        let action = SuggestorAction::Dismiss {
            suggestion_id: SmolStr::new("docs:4"),
        };
        match action.into_command() {
            Some(Command::SuggestorDismiss { suggestion_id }) => {
                assert_eq!(suggestion_id.as_str(), "docs:4");
            }
            other => panic!("expected SuggestorDismiss, got {other:?}"),
        }
    }

    #[test]
    fn into_command_close_yields_none() {
        assert!(SuggestorAction::Close.into_command().is_none());
    }

    #[test]
    fn applying_marker_round_trip() {
        let mut state = SuggestorState::new();
        state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
        let id = state.entries()[0].id.clone();
        state.mark_applying(id.clone());
        assert_eq!(state.applying_id(), Some(&id));
        state.clear_applying();
        assert!(state.applying_id().is_none());
    }

    /// ΔB lock: the action enum round-trips through serde, mirroring
    /// every other dispatcher payload.
    #[test]
    fn suggestor_action_serde_round_trip() {
        let action = SuggestorAction::Dismiss {
            suggestion_id: SmolStr::new("docs:4"),
        };
        let s = serde_json::to_string(&action).unwrap_or_default();
        let back: SuggestorAction = serde_json::from_str(&s).unwrap_or(SuggestorAction::Close);
        assert_eq!(back, action);
    }

    #[test]
    fn suggestor_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let chrome = SmartGroupSuggestorChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(chrome.panel_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.row_background, style_tokens::PALETTE_DARK.surface_subtle);
        assert_eq!(chrome.selected_background, style_tokens::PALETTE_DARK.surface_active);
        assert_eq!(chrome.action_background, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(chrome.danger_background, style_tokens::PALETTE_DARK.accent_red);
        assert_eq!(chrome.preview_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.panel_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        assert_eq!(chrome.row_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.action_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded);
    }

    #[test]
    fn tone_colors_from_tauri_palette_maps_to_accent_green_orange_muted() {
        use bento_nano_style::tokens as style_tokens;
        let (high_bg, high_fg) =
            tone_colors_from_tauri_palette(ConfidenceTone::High, style_tokens::PALETTE_DARK);
        assert_eq!(high_fg, style_tokens::PALETTE_DARK.accent_green);
        assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

        let (_, med_fg) =
            tone_colors_from_tauri_palette(ConfidenceTone::Medium, style_tokens::PALETTE_DARK);
        assert_eq!(med_fg, style_tokens::PALETTE_DARK.accent_orange);

        let (_, low_fg) =
            tone_colors_from_tauri_palette(ConfidenceTone::Low, style_tokens::PALETTE_DARK);
        assert_eq!(low_fg, style_tokens::PALETTE_DARK.text_muted);
    }
}
