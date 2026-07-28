//! StackTray + FocusedZonePreview selected-stack runtime geometry.
//!
//! The Tauri baseline renders StackCapsule, StackTray, and
//! FocusedZonePreview as separate Solid components. The selected stack keeps
//! those as one D2D overlay so renderer and shell hit-testing share the same
//! constants and cannot drift.

use bentodesk_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bentodesk_style::{BorderRadius, Color, Rect, Shadow, Size};
use bentodesk_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bentodesk_zone::{Zone, ZoneId};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::zone_pill_geometry;

pub const TRAY_WIDTH_PX: f32 = 340.0;
pub const TRAY_MIN_HEIGHT_PX: f32 = 168.0;
pub const TRAY_HEADER_HEIGHT_PX: f32 = 42.0;
pub const TRAY_ROW_HEIGHT_PX: f32 = 38.0;
pub const TRAY_ROW_STRIDE_PX: f32 = 42.0;
pub const TRAY_INSET_PX: f32 = 14.0;
pub const TRAY_GAP_PX: f32 = 12.0;
pub const TRAY_VIEWPORT_MARGIN_PX: f32 = 14.0;
pub const TRAY_ACTION_BUTTON_HEIGHT_PX: f32 = 24.0;
pub const TRAY_DETACH_BUTTON_WIDTH_PX: f32 = 70.0;
pub const TRAY_CLOSE_BUTTON_WIDTH_PX: f32 = 54.0;
pub const TRAY_DISSOLVE_BUTTON_WIDTH_PX: f32 = 76.0;
pub const TRAY_VISIBLE_ROW_LIMIT: usize = 6;
pub const TRAY_HEADER_TITLE_WIDTH_PX: f32 = 92.0;
pub const TRAY_HEADER_TITLE_HEIGHT_PX: f32 = 18.0;
pub const TRAY_HEADER_COUNT_HEIGHT_PX: f32 = 20.0;
pub const TRAY_HEADER_COUNT_BADGE_MIN_WIDTH_PX: f32 = 24.0;
pub const TRAY_HEADER_COUNT_BADGE_PAD_X_PX: f32 = 9.0;
pub const TRAY_HEADER_COUNT_BADGE_DIGIT_WIDTH_PX: f32 = 7.0;
pub const TRAY_MEMBER_TEXT_X_PX: f32 = 44.0;
pub const TRAY_MEMBER_TEXT_RESERVED_RIGHT_PX: f32 = 128.0;
pub const TRAY_MEMBER_META_Y_PX: f32 = 23.0;
pub const TRAY_MEMBER_META_HEIGHT_PX: f32 = 14.0;
pub const TRAY_MEMBER_META_COUNT_WIDTH_PX: f32 = 30.0;
pub const TRAY_MEMBER_META_GAP_PX: f32 = 4.0;
pub const TRAY_STATUS_HEIGHT_PX: f32 = 14.0;
pub const TRAY_STATUS_BOTTOM_OFFSET_PX: f32 = 18.0;
pub const TRAY_STATUS_PREFIX_WIDTH_PX: f32 = 10.0;
pub const TRAY_STATUS_COUNT_WIDTH_PX: f32 = 30.0;
pub const TRAY_STATUS_GAP_PX: f32 = 3.0;
pub const PREVIEW_META_NUMBER_WIDTH_PX: f32 = 34.0;
pub const PREVIEW_META_MARK_WIDTH_PX: f32 = 12.0;
pub const PREVIEW_META_GAP_PX: f32 = 4.0;
pub const TRAY_TITLE_FONT_PX: f32 = 13.0;
pub const TRAY_TITLE_FONT_WEIGHT: u16 = 600;
pub const TRAY_COUNT_FONT_PX: f32 = 11.0;
pub const TRAY_COUNT_FONT_WEIGHT: u16 = 400;
pub const TRAY_TOOLBAR_FONT_PX: f32 = 11.0;
pub const TRAY_TOOLBAR_FONT_WEIGHT: u16 = 400;
pub const TRAY_MEMBER_NAME_FONT_PX: f32 = 13.0;
pub const TRAY_MEMBER_NAME_FONT_WEIGHT: u16 = 600;
pub const TRAY_MEMBER_META_FONT_PX: f32 = 11.0;
pub const TRAY_MEMBER_META_FONT_WEIGHT: u16 = 400;
pub const TRAY_ACTION_FONT_PX: f32 = 11.0;
pub const TRAY_ACTION_FONT_WEIGHT: u16 = 400;
pub const TRAY_STATUS_FONT_PX: f32 = 11.0;
pub const TRAY_STATUS_FONT_WEIGHT: u16 = 400;
pub const TRAY_TEXT_LINE_HEIGHT: f32 = 1.25;

pub const PREVIEW_WIDTH_PX: f32 = 300.0;
pub const PREVIEW_HEIGHT_PX: f32 = 196.0;
pub const PREVIEW_GAP_PX: f32 = 10.0;
pub const PREVIEW_EYEBROW_FONT_PX: f32 = 11.0;
pub const PREVIEW_EYEBROW_FONT_WEIGHT: u16 = 400;
pub const PREVIEW_TITLE_FONT_PX: f32 = 13.0;
pub const PREVIEW_TITLE_FONT_WEIGHT: u16 = 600;
pub const PREVIEW_META_FONT_PX: f32 = 11.0;
pub const PREVIEW_META_FONT_WEIGHT: u16 = 400;
pub const PREVIEW_ITEM_FONT_PX: f32 = 11.0;
pub const PREVIEW_ITEM_FONT_WEIGHT: u16 = 400;
pub const PREVIEW_EMPTY_FONT_PX: f32 = 11.0;
pub const PREVIEW_EMPTY_FONT_WEIGHT: u16 = 400;
pub const PREVIEW_TEXT_LINE_HEIGHT: f32 = 1.25;
pub const FLOATING_PREVIEW_MAX_WIDTH_PX: f32 = 360.0;
pub const FLOATING_PREVIEW_MAX_HEIGHT_PX: f32 = 420.0;
pub const FLOATING_PREVIEW_GAP_PX: f32 = 12.0;
pub const FLOATING_PREVIEW_VIEWPORT_MARGIN_PX: f32 = 16.0;

pub const BLOOM_PETAL_WIDTH_PX: f32 = 108.0;
pub const BLOOM_PETAL_HEIGHT_PX: f32 = 96.0;
pub const BLOOM_PETAL_ICON_PX: f32 = 36.0;
pub const BLOOM_PETAL_WIDTH_MEDIUM_PX: f32 = 92.0;
pub const BLOOM_PETAL_HEIGHT_MEDIUM_PX: f32 = 84.0;
pub const BLOOM_PETAL_ICON_MEDIUM_PX: f32 = 32.0;
pub const BLOOM_PETAL_WIDTH_DENSE_PX: f32 = 80.0;
pub const BLOOM_PETAL_HEIGHT_DENSE_PX: f32 = 72.0;
pub const BLOOM_PETAL_ICON_DENSE_PX: f32 = 28.0;
pub const BLOOM_PETAL_WIDTH_COMPACT_PX: f32 = 72.0;
pub const BLOOM_PETAL_HEIGHT_COMPACT_PX: f32 = 64.0;
pub const BLOOM_PETAL_ICON_COMPACT_PX: f32 = 24.0;
pub const BLOOM_PETAL_PADDING_X_PX: f32 = 10.0;
pub const BLOOM_PETAL_PADDING_Y_PX: f32 = 12.0;
pub const BLOOM_PETAL_CONTENT_GAP_PX: f32 = 8.0;
pub const BLOOM_PETAL_NAME_FONT_PX: f32 = 11.5;
pub const BLOOM_PETAL_NAME_FONT_WEIGHT: u16 = 600;
pub const BLOOM_PETAL_NAME_LINE_HEIGHT: f32 = 1.25;
pub const BLOOM_PETAL_NAME_MAX_LINES: usize = 2;
pub const BLOOM_PETAL_GAP_PX: f32 = 12.0;
pub const BLOOM_PETAL_GAP_BELOW_CAPSULE_PX: f32 = 16.0;
pub const BLOOM_VIEWPORT_INSET_PX: f32 = 16.0;
/// Tauri `MAX_VISIBLE_MEMBERS`: 24 total slots; for an overflowing stack the
/// final slot is a non-member `+N` indicator.
pub const BLOOM_VISIBLE_PETAL_LIMIT: usize = 24;
/// Tauri v9 registers an independent 12px hit halo around every petal instead
/// of a full-viewport pointer-capturing overlay.
pub const BLOOM_PETAL_HIT_INFLATE_PX: f32 = 12.0;
/// Tauri `services/hoverIntent.ts::HOVER_INTENT_MS`: a petal must remain the
/// active target for this long before its focused preview opens automatically.
pub const BLOOM_PREVIEW_HOVER_INTENT_MS: u32 = 150;
/// Tauri `services/hoverIntent.ts::LEAVE_GRACE_MS`: blank pixels between the
/// capsule, petals, and preview do not collapse the Bloom immediately.
pub const BLOOM_LEAVE_GRACE_MS: u32 = 80;
pub const BLOOM_MOTION_STAGGER_STEP: f32 = 0.075;
pub const BLOOM_MOTION_MIN_PROGRESS: f32 = 0.64;
/// Tauri `stack-bloom-petal-enter` starts at `scale(0.4)`.
pub const BLOOM_MOTION_MIN_SCALE: f32 = 0.4;
/// Tauri `stack-bloom-petal-enter` starts fully transparent.
pub const BLOOM_MOTION_MIN_ALPHA: f32 = 0.0;
pub const BLOOM_CONNECTOR_THICKNESS_PX: f32 = 3.0;
pub const BLOOM_WRAPPER_BASE_PAD_PX: f32 = 5.0;
pub const BLOOM_WRAPPER_MEMBER_PAD_PX: f32 = 1.35;
/// Fast-release per-petal entry duration using the Tauri Bloom easing.
pub const BLOOM_PETAL_ENTER_DURATION_MS: u32 = 300;
/// Capped entry stagger keeps the 24-slot visible envelope below 480 ms.
pub const BLOOM_ENTRY_STAGGER_BUDGET_MS: u32 = 180;
/// Fast-release per-petal exit duration using the Tauri exit easing.
pub const BLOOM_PETAL_EXIT_DURATION_MS: u32 = 100;
/// Reverse-staggered exit budget; the 24-slot envelope remains below 140 ms.
pub const BLOOM_EXIT_STAGGER_BUDGET_MS: u32 = 30;
/// Hard upper bound for mounted exiting petals.
pub const BLOOM_EXIT_VISIBLE_DURATION_MS: u32 = 140;
pub const BLOOM_EXIT_SCALE: f32 = 0.5;
/// Maximum visible-cluster reveal duration for Tauri's 24-slot cap.
pub const BLOOM_REVEAL_DURATION_MS: u32 = BLOOM_PETAL_ENTER_DURATION_MS
    + (BLOOM_ENTRY_STAGGER_BUDGET_MS * (BLOOM_VISIBLE_PETAL_LIMIT as u32 - 1)
        / BLOOM_VISIBLE_PETAL_LIMIT as u32);

/// StackTray and FocusedZonePreview chrome contract derived from active theme tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackTrayChrome {
    /// Drop shadow descriptor drawn behind tray and focused preview panels.
    pub panel_shadow: Shadow,
    /// Shared panel radius for tray and focused preview panels.
    pub panel_radius: BorderRadius,
    /// Member-row radius.
    pub row_radius: BorderRadius,
    /// Action/icon button radius.
    pub button_radius: BorderRadius,
    /// Focused preview item-row radius.
    pub preview_item_radius: BorderRadius,
    /// Stack tray panel fill colour.
    pub panel_background: Color,
    /// Focused preview panel fill colour.
    pub preview_background: Color,
    /// Default member/item row fill colour.
    pub row_background: Color,
    /// Selected member row fill colour.
    pub selected_background: Color,
    /// Dragged member row fill colour.
    pub dragged_background: Color,
    /// Neutral action/icon button fill colour.
    pub button_background: Color,
    /// Destructive action button fill colour.
    pub danger_background: Color,
    /// Primary text colour.
    pub text_primary: Color,
    /// Secondary/muted text colour.
    pub text_muted: Color,
    /// Accent/help text colour.
    pub text_accent: Color,
}

impl StackTrayChrome {
    /// Build StackTray chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build StackTray chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            row_radius: radius.lg,
            button_radius: radius.md,
            preview_item_radius: radius.md,
            panel_background: palette.surface,
            preview_background: palette.surface,
            row_background: palette.surface_alt,
            selected_background: palette.selection,
            dragged_background: palette.active_overlay,
            button_background: palette.hover_overlay,
            danger_background: palette.danger,
            text_primary: palette.text,
            text_muted: palette.text_muted,
            text_accent: palette.accent,
        }
    }

    /// Build StackTray chrome from Wave B Tauri SSoT tokens
    /// (`bentodesk_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri}`).
    ///
    /// Token mapping (per Wave A `stack-tray-expanded.md` + Wave B
    /// `token-mapping.md` §1/§2/§4):
    /// - panel + preview bg ← `surface_expanded` (rgba(12,12,18,0.82))
    /// - row hover bg ← `surface_hover`
    /// - selected row bg ← `surface_active` (Wave A `.is-selected`)
    /// - dragged row bg ← `surface_hover` (no Tauri dragged-row token; reuse hover tint)
    /// - button bg ← `surface_subtle`
    /// - danger bg ← `accent_red` (Wave A: `--accent-red` powers the dissolve toolbar btn)
    /// - text primary/muted/accent ← `text_primary` / `text_muted` / `accent_blue`
    /// - radii: panel = `expanded` (16), row = `card` (10), button = `card` (10)
    /// - shadow ← `expanded` (outer; inner layer composed by renderer if needed)
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            row_radius: BorderRadius::all(radius.card),
            button_radius: BorderRadius::all(radius.card),
            preview_item_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            preview_background: palette.surface_expanded,
            row_background: palette.surface_hover,
            selected_background: palette.surface_active,
            dragged_background: palette.surface_hover,
            button_background: palette.surface_subtle,
            danger_background: palette.accent_red,
            text_primary: palette.text_primary,
            text_muted: palette.text_muted,
            text_accent: palette.accent_blue,
        }
    }
}

/// Compute a filled shadow rect for StackTray / FocusedZonePreview panels.
pub fn panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackTrayPresentation {
    Management,
    BloomPreview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTrayState {
    pub anchor_zone_id: ZoneId,
    pub selected_member_id: ZoneId,
    pub status: Option<SmolStr>,
    pub presentation: StackTrayPresentation,
}

impl StackTrayState {
    pub fn new(anchor_zone_id: ZoneId, selected_member_id: ZoneId) -> Self {
        Self {
            anchor_zone_id,
            selected_member_id,
            status: None,
            presentation: StackTrayPresentation::Management,
        }
    }

    pub fn bloom_preview(anchor_zone_id: ZoneId, selected_member_id: ZoneId) -> Self {
        Self {
            anchor_zone_id,
            selected_member_id,
            status: None,
            presentation: StackTrayPresentation::BloomPreview,
        }
    }

    pub fn is_management(&self) -> bool {
        self.presentation == StackTrayPresentation::Management
    }

    pub fn is_bloom_preview(&self) -> bool {
        self.presentation == StackTrayPresentation::BloomPreview
    }

    pub fn with_status(mut self, status: impl Into<SmolStr>) -> Self {
        self.status = Some(status.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackTrayDragState {
    pub anchor_zone_id: ZoneId,
    pub member_id: ZoneId,
    pub from_index: usize,
}

impl StackTrayDragState {
    pub const fn new(anchor_zone_id: ZoneId, member_id: ZoneId, from_index: usize) -> Self {
        Self {
            anchor_zone_id,
            member_id,
            from_index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackTrayPointerHit {
    Row(usize),
    Detach(usize),
    Dissolve,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackBloomFrame {
    pub rect: Rect,
    pub connector: Rect,
    pub progress: f32,
    pub scale: f32,
    pub alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackBloomPetalSize {
    pub width: f32,
    pub height: f32,
    pub icon_size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackBloomPetalContentLayout {
    pub icon_rect: Rect,
    pub title_rect: Rect,
}

#[inline]
fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

mod bloom;
mod geometry;

pub use bloom::*;
pub use geometry::*;

#[cfg(test)]
mod tests;
