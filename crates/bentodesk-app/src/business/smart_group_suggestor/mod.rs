//! Business surface — `SmartGroupSuggestor` (T-069a).
//!
//! Floating panel that surfaces AI-driven groupings the user can apply
//! with one click. Visual spec: `smart_group_suggestor.snap.md`. Backend
//! feed: [`bentodesk_backend::grouping::suggest_groups`] →
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

use bentodesk_backend::grouping::SuggestedGroup;
use bentodesk_layout::Direction;
use bentodesk_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bentodesk_theme as theme;
use bentodesk_widget::{ContainerNode, WidgetNode};
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
pub const RUNTIME_STATUS_TOP_PX: f32 = 76.0;

/// Runtime row top in the D2D aux panel.
pub const RUNTIME_ROW_TOP_PX: f32 = 96.0;

/// Runtime row height in the D2D aux panel.
pub const RUNTIME_ROW_HEIGHT_PX: f32 = 54.0;

/// Runtime row stride in the D2D aux panel.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 62.0;

/// Runtime Apply button width.
pub const RUNTIME_APPLY_BUTTON_WIDTH_PX: f32 = 60.0;

/// Runtime Dismiss button width.
pub const RUNTIME_DISMISS_BUTTON_WIDTH_PX: f32 = 28.0;

/// Runtime close button size.
pub const RUNTIME_CLOSE_BUTTON_SIZE_PX: f32 = 32.0;

pub const RUNTIME_PREVIEW_TOP_PX: f32 =
    RUNTIME_ROW_TOP_PX + (MAX_VISIBLE_SUGGESTIONS as f32 * RUNTIME_ROW_STRIDE_PX) + 10.0;

pub const RUNTIME_PREVIEW_ROW_HEIGHT_PX: f32 = 18.0;

pub const RUNTIME_PREVIEW_ROW_STRIDE_PX: f32 = 20.0;

pub const MAX_VISIBLE_PREVIEW_FILES: usize = 2;

pub const RUNTIME_PREVIEW_BUTTON_WIDTH_PX: f32 = 50.0;

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
    /// Quiet close-button fill colour.
    pub close_background: Color,
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
            close_background: palette.hover_overlay,
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
    /// - panel bg ← `surface_expanded`
    /// - row + preview bg ← `surface_subtle`; selected ← `surface_active`
    /// - apply action button ← `accent_blue`; dismiss ← `accent_red`
    /// - panel radius ← `expanded` (16); rows/actions/preview ← `card` (10); badge ← `card`
    /// - panel shadow ← `expanded` outer layer
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        let controls = palette.control_palette();
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
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
            close_background: controls.fill,
            preview_background: palette.surface_subtle,
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
        bentodesk_backend::layout::GroupRuleType::Extension => {
            if let Some(extensions) = suggestion.rule.extensions.as_ref()
                && !extensions.is_empty()
            {
                return SmolStr::new(extensions.join(", "));
            }
            SmolStr::new_static("Extension")
        }
        bentodesk_backend::layout::GroupRuleType::NamePattern => suggestion
            .rule
            .pattern
            .as_deref()
            .filter(|pattern| !pattern.trim().is_empty())
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new_static("Name pattern")),
        bentodesk_backend::layout::GroupRuleType::ModifiedDate => {
            SmolStr::new_static("Modified date")
        }
    }
}

/// Runtime panel rectangle shared by renderer and shell hit-testing.
pub fn suggestor_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
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
        y: row.y + 15.0,
        width: RUNTIME_APPLY_BUTTON_WIDTH_PX,
        height: 24.0,
    }
}

/// Runtime row dismiss button rectangle.
pub fn suggestor_dismiss_rect(viewport: Size, row_index: usize) -> Rect {
    let row = suggestor_row_rect(viewport, row_index);
    Rect {
        x: row.right() - RUNTIME_DISMISS_BUTTON_WIDTH_PX - 8.0,
        y: row.y + 15.0,
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

mod state;

pub use state::*;

#[cfg(test)]
mod tests;
