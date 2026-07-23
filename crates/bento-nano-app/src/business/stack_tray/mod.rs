//! StackTray + FocusedZonePreview selected-stack runtime geometry.
//!
//! The Tauri baseline renders StackCapsule, StackTray, and
//! FocusedZonePreview as separate Solid components. The selected stack keeps
//! those as one D2D overlay so renderer and shell hit-testing share the same
//! constants and cannot drift.

use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_zone::{Zone, ZoneId};
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
/// Tauri `StackWrapper.css` per-petal entry duration:
/// `stack-bloom-petal-enter 420ms cubic-bezier(0.34, 1.56, 0.64, 1)`.
pub const BLOOM_PETAL_ENTER_DURATION_MS: u32 = 420;
/// Tauri capped entry stagger: `(360ms / count) * petal_index`.
pub const BLOOM_ENTRY_STAGGER_BUDGET_MS: u32 = 360;
/// Tauri `StackWrapper.css` per-petal exit duration:
/// `stack-bloom-petal-exit 140ms cubic-bezier(0.4, 0, 0.7, 0.2)`.
pub const BLOOM_PETAL_EXIT_DURATION_MS: u32 = 140;
/// Tauri reverse-staggered exit budget:
/// `(120ms / count) * (count - 1 - petal_index)`.
pub const BLOOM_EXIT_STAGGER_BUDGET_MS: u32 = 120;
/// Tauri keeps the bloom petal DOM mounted for 160ms after leave.
pub const BLOOM_EXIT_VISIBLE_DURATION_MS: u32 = 160;
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
    /// (`bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri}`).
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

pub fn stack_tray_visible_rows(member_count: usize) -> usize {
    member_count.min(TRAY_VISIBLE_ROW_LIMIT)
}

pub fn stack_tray_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let visible_rows = stack_tray_visible_rows(member_count);
    let height = (TRAY_HEADER_HEIGHT_PX + visible_rows as f32 * TRAY_ROW_STRIDE_PX + TRAY_INSET_PX)
        .max(TRAY_MIN_HEIGHT_PX);
    let anchor_right = anchor.x as f32 + anchor.w as f32;
    let right_candidate = anchor_right + TRAY_GAP_PX;
    let left_candidate = anchor.x as f32 - TRAY_GAP_PX - TRAY_WIDTH_PX;
    let x = if right_candidate + TRAY_WIDTH_PX + TRAY_VIEWPORT_MARGIN_PX <= viewport.width {
        right_candidate
    } else {
        left_candidate
    };
    let max_x =
        (viewport.width - TRAY_WIDTH_PX - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - height - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: (anchor.y as f32).clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: TRAY_WIDTH_PX,
        height,
    }
}

pub fn stack_tray_row_rect(viewport: Size, anchor: &Zone, member_count: usize, row: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.y + TRAY_HEADER_HEIGHT_PX + row as f32 * TRAY_ROW_STRIDE_PX,
        width: tray.width - TRAY_INSET_PX * 2.0,
        height: TRAY_ROW_HEIGHT_PX,
    }
}

pub fn stack_tray_detach_rect(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    row: usize,
) -> Rect {
    let row_rect = stack_tray_row_rect(viewport, anchor, member_count, row);
    Rect {
        x: row_rect.right() - TRAY_DETACH_BUTTON_WIDTH_PX - 6.0,
        y: row_rect.y + 7.0,
        width: TRAY_DETACH_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn stack_tray_dissolve_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.right()
            - TRAY_INSET_PX
            - TRAY_CLOSE_BUTTON_WIDTH_PX
            - TRAY_GAP_PX
            - TRAY_DISSOLVE_BUTTON_WIDTH_PX,
        y: tray.y + 9.0,
        width: TRAY_DISSOLVE_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

/// Whether the side FocusedZonePreview pane should be visible for this tray state.
pub fn focused_preview_visible(anchor_zone_id: ZoneId, selected_member_id: ZoneId) -> bool {
    selected_member_id != anchor_zone_id
}

pub fn stack_tray_header_title_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.y + 10.0,
        width: TRAY_HEADER_TITLE_WIDTH_PX,
        height: TRAY_HEADER_TITLE_HEIGHT_PX,
    }
}

pub fn stack_tray_header_count_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let title = stack_tray_header_title_rect(viewport, anchor, member_count);
    let dissolve = stack_tray_dissolve_rect(viewport, anchor, member_count);
    let x = title.right() + TRAY_GAP_PX;
    let max_width = (dissolve.x - TRAY_GAP_PX - x).max(0.0);
    Rect {
        x,
        y: title.y,
        width: stack_tray_header_count_badge_width(member_count).min(max_width),
        height: TRAY_HEADER_COUNT_HEIGHT_PX,
    }
}

pub fn stack_tray_header_count_badge_width(member_count: usize) -> f32 {
    let text_width = stack_tray_header_count_label_len(member_count) as f32
        * TRAY_HEADER_COUNT_BADGE_DIGIT_WIDTH_PX;
    (text_width + TRAY_HEADER_COUNT_BADGE_PAD_X_PX * 2.0).max(TRAY_HEADER_COUNT_BADGE_MIN_WIDTH_PX)
}

pub fn stack_tray_header_count_label_len(member_count: usize) -> usize {
    if member_count >= 1000 {
        4
    } else if member_count >= 100 {
        3
    } else if member_count >= 10 {
        2
    } else {
        1
    }
}

pub fn stack_tray_member_meta_count_rect(row_rect: Rect) -> Rect {
    Rect {
        x: row_rect.x + TRAY_MEMBER_TEXT_X_PX,
        y: row_rect.y + TRAY_MEMBER_META_Y_PX,
        width: TRAY_MEMBER_META_COUNT_WIDTH_PX,
        height: TRAY_MEMBER_META_HEIGHT_PX,
    }
}

pub fn stack_tray_member_meta_suffix_rect(row_rect: Rect) -> Rect {
    let count = stack_tray_member_meta_count_rect(row_rect);
    let text_width = (row_rect.width - TRAY_MEMBER_TEXT_RESERVED_RIGHT_PX).max(0.0);
    Rect {
        x: count.right() + TRAY_MEMBER_META_GAP_PX,
        y: count.y,
        width: (text_width - TRAY_MEMBER_META_COUNT_WIDTH_PX - TRAY_MEMBER_META_GAP_PX).max(0.0),
        height: count.height,
    }
}

pub fn stack_tray_close_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.right() - TRAY_INSET_PX - TRAY_CLOSE_BUTTON_WIDTH_PX,
        y: tray.y + 9.0,
        width: TRAY_CLOSE_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn stack_tray_status_rect(tray: Rect) -> Rect {
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.bottom() - TRAY_STATUS_BOTTOM_OFFSET_PX,
        width: tray.width - TRAY_INSET_PX * 2.0,
        height: TRAY_STATUS_HEIGHT_PX,
    }
}

pub fn stack_tray_status_prefix_rect(status_rect: Rect) -> Rect {
    Rect {
        x: status_rect.x,
        y: status_rect.y,
        width: TRAY_STATUS_PREFIX_WIDTH_PX,
        height: status_rect.height,
    }
}

pub fn stack_tray_status_count_rect(status_rect: Rect) -> Rect {
    Rect {
        x: status_rect.x + TRAY_STATUS_PREFIX_WIDTH_PX,
        y: status_rect.y,
        width: TRAY_STATUS_COUNT_WIDTH_PX,
        height: status_rect.height,
    }
}

pub fn stack_tray_status_suffix_rect(status_rect: Rect) -> Rect {
    let x = status_rect.x
        + TRAY_STATUS_PREFIX_WIDTH_PX
        + TRAY_STATUS_COUNT_WIDTH_PX
        + TRAY_STATUS_GAP_PX;
    Rect {
        x,
        y: status_rect.y,
        width: (status_rect.right() - x).max(0.0),
        height: status_rect.height,
    }
}

pub fn focused_preview_meta_number_rect(preview: Rect, index: usize) -> Rect {
    let first_x = preview.x + 16.0;
    let step = PREVIEW_META_NUMBER_WIDTH_PX + PREVIEW_META_MARK_WIDTH_PX + PREVIEW_META_GAP_PX;
    Rect {
        x: first_x + index as f32 * step,
        y: preview.y + 58.0,
        width: PREVIEW_META_NUMBER_WIDTH_PX,
        height: 16.0,
    }
}

pub fn focused_preview_meta_mark_rect(preview: Rect, index: usize) -> Rect {
    let number = focused_preview_meta_number_rect(preview, index);
    Rect {
        x: number.right(),
        y: number.y,
        width: PREVIEW_META_MARK_WIDTH_PX,
        height: number.height,
    }
}

pub fn focused_preview_meta_suffix_rect(preview: Rect) -> Rect {
    let item_number = focused_preview_meta_number_rect(preview, 2);
    let x = item_number.right() + PREVIEW_META_GAP_PX;
    Rect {
        x,
        y: item_number.y,
        width: (preview.right() - 16.0 - x).max(0.0),
        height: item_number.height,
    }
}

pub fn focused_preview_rect(viewport: Size, tray: Rect) -> Rect {
    let right_candidate = tray.right() + PREVIEW_GAP_PX;
    let left_candidate = tray.x - PREVIEW_GAP_PX - PREVIEW_WIDTH_PX;
    let left_available = (tray.x - PREVIEW_GAP_PX - TRAY_VIEWPORT_MARGIN_PX).max(0.0);
    let right_available =
        (viewport.width - TRAY_VIEWPORT_MARGIN_PX - PREVIEW_GAP_PX - tray.right()).max(0.0);
    let right_fits = right_available >= PREVIEW_WIDTH_PX;
    let left_fits = left_available >= PREVIEW_WIDTH_PX;
    let x = if right_fits && (!left_fits || right_available >= left_available) {
        right_candidate
    } else if left_fits {
        left_candidate
    } else if right_available >= left_available {
        right_candidate
    } else {
        left_candidate
    };
    let max_x =
        (viewport.width - PREVIEW_WIDTH_PX - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - PREVIEW_HEIGHT_PX - TRAY_VIEWPORT_MARGIN_PX)
        .max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: tray.y.clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: PREVIEW_WIDTH_PX,
        height: PREVIEW_HEIGHT_PX,
    }
}

/// Place the focused Bloom preview beside the complete visible petal family.
///
/// The original Tauri implementation used only the selected petal as its
/// horizontal anchor. In a multi-member row that lets the preview cover every
/// sibling to the selected petal's right. Keep the selected petal as the
/// vertical/attention anchor, but use the union of all visible petals for side
/// placement. If neither side fits (a wrapped row on a narrow display), move
/// the preview below or above the family before falling back to a clamped side.
/// Paint and hit-test callers pass the same petal slice, so the surface cannot
/// drift from its interactive geometry.
pub fn focused_bloom_preview_rect(
    viewport: Size,
    selected_petal: Rect,
    petals: &[Rect],
    zone: &Zone,
) -> Rect {
    let width = if zone.w > 0 {
        (zone.w as f32).min(FLOATING_PREVIEW_MAX_WIDTH_PX)
    } else {
        FLOATING_PREVIEW_MAX_WIDTH_PX
    };
    let height = if zone.h > 0 {
        (zone.h as f32).min(FLOATING_PREVIEW_MAX_HEIGHT_PX)
    } else {
        FLOATING_PREVIEW_MAX_HEIGHT_PX
    };
    let max_x = (viewport.width - width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX)
        .max(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX)
        .max(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);

    let family = bloom_petal_family_bounds(selected_petal, petals);
    let right = family.right() + FLOATING_PREVIEW_GAP_PX;
    let left = family.x - FLOATING_PREVIEW_GAP_PX - width;
    let right_fits = right + width <= viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
    let left_fits = left >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;

    let (x, y) = if right_fits || left_fits {
        let x = if right_fits { right } else { left };
        let y =
            if selected_petal.y + height <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX {
                selected_petal.y
            } else {
                selected_petal.bottom() - height
            };
        (x, y.clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_y))
    } else {
        let below = family.bottom() + FLOATING_PREVIEW_GAP_PX;
        let above = family.y - FLOATING_PREVIEW_GAP_PX - height;
        let below_fits = below + height <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
        let above_fits = above >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
        if below_fits || above_fits {
            let x = (selected_petal.x + (selected_petal.width - width) * 0.5)
                .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_x);
            (x, if below_fits { below } else { above })
        } else {
            let right_space = viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX - family.right();
            let left_space = family.x - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
            let x = if right_space >= left_space {
                right
            } else {
                left
            }
            .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_x);
            let y = selected_petal
                .y
                .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_y);
            (x, y)
        }
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn focused_bloom_preview_contains(
    viewport: Size,
    selected_petal: Rect,
    petals: &[Rect],
    zone: &Zone,
    x: f32,
    y: f32,
) -> bool {
    rect_contains(
        focused_bloom_preview_rect(viewport, selected_petal, petals, zone),
        x,
        y,
    )
}

fn bloom_petal_family_bounds(selected_petal: Rect, petals: &[Rect]) -> Rect {
    let mut left = selected_petal.x;
    let mut top = selected_petal.y;
    let mut right = selected_petal.right();
    let mut bottom = selected_petal.bottom();
    for petal in petals {
        left = left.min(petal.x);
        top = top.min(petal.y);
        right = right.max(petal.right());
        bottom = bottom.max(petal.bottom());
    }
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

pub fn focused_bloom_preview_search_rect(preview: Rect) -> Rect {
    Rect {
        x: preview.right() - 70.0,
        y: preview.y + 10.0,
        width: 28.0,
        height: 28.0,
    }
}

pub fn focused_bloom_preview_close_rect(preview: Rect) -> Rect {
    Rect {
        x: preview.right() - 36.0,
        y: preview.y + 10.0,
        width: 28.0,
        height: 28.0,
    }
}

pub fn stack_wrapper_halo_rect(anchor: &Zone, member_count: usize) -> Rect {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    let pad = BLOOM_WRAPPER_BASE_PAD_PX + visible_count as f32 * BLOOM_WRAPPER_MEMBER_PAD_PX;
    Rect {
        x: anchor.x as f32 - pad,
        y: anchor.y as f32 - pad,
        width: anchor.w as f32 + pad * 2.0,
        height: anchor.h as f32 + pad * 2.0,
    }
}

pub fn stack_bloom_petal_size(member_count: usize) -> StackBloomPetalSize {
    if member_count <= 4 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_PX,
            height: BLOOM_PETAL_HEIGHT_PX,
            icon_size: BLOOM_PETAL_ICON_PX,
        }
    } else if member_count <= 8 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_MEDIUM_PX,
            height: BLOOM_PETAL_HEIGHT_MEDIUM_PX,
            icon_size: BLOOM_PETAL_ICON_MEDIUM_PX,
        }
    } else if member_count <= 16 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_DENSE_PX,
            height: BLOOM_PETAL_HEIGHT_DENSE_PX,
            icon_size: BLOOM_PETAL_ICON_DENSE_PX,
        }
    } else {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_COMPACT_PX,
            height: BLOOM_PETAL_HEIGHT_COMPACT_PX,
            icon_size: BLOOM_PETAL_ICON_COMPACT_PX,
        }
    }
}

pub fn stack_bloom_petal_content_layout(
    petal_rect: Rect,
    icon_size: f32,
    scale: f32,
) -> StackBloomPetalContentLayout {
    let scale = scale.max(0.01);
    let pad_x = BLOOM_PETAL_PADDING_X_PX * scale;
    let pad_y = BLOOM_PETAL_PADDING_Y_PX * scale;
    let gap = BLOOM_PETAL_CONTENT_GAP_PX * scale;
    let content_width = (petal_rect.width - pad_x * 2.0).max(0.0);
    let content_height = (petal_rect.height - pad_y * 2.0).max(0.0);
    let icon_side = icon_size.min(content_width).min(content_height).max(0.0);
    let name_line_height = BLOOM_PETAL_NAME_FONT_PX * BLOOM_PETAL_NAME_LINE_HEIGHT * scale;
    let max_title_height = name_line_height * BLOOM_PETAL_NAME_MAX_LINES as f32;
    let available_title_height = (content_height - icon_side - gap).max(0.0);
    // DWrite needs the full two-line box (28.75 DIP for the Tauri 11.5/1.25
    // role). The CSS flex box can borrow the sub-pixel remainder from its
    // vertical padding; give native layout the same one-DIP tolerance instead
    // of trimming a long title on its first line.
    let title_height = (available_title_height + scale).min(max_title_height + scale);
    let stack_height = icon_side
        + if title_height > 0.0 {
            gap + title_height
        } else {
            0.0
        };
    let stack_top = petal_rect.y + pad_y + ((content_height - stack_height) * 0.5).max(0.0);
    let icon_rect = Rect {
        x: petal_rect.x + (petal_rect.width - icon_side) * 0.5,
        y: stack_top,
        width: icon_side,
        height: icon_side,
    };
    let title_rect = Rect {
        x: petal_rect.x + pad_x,
        y: icon_rect.bottom() + gap,
        width: content_width,
        height: title_height,
    };
    StackBloomPetalContentLayout {
        icon_rect,
        title_rect,
    }
}

pub fn stack_bloom_frames(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at(viewport, anchor, member_count, 1.0)
}

pub fn stack_bloom_frames_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at_with_motion(viewport, anchor, member_count, reveal_progress, false)
}

pub fn stack_bloom_exit_frames_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at_with_motion(viewport, anchor, member_count, exit_progress, true)
}

fn stack_bloom_frames_at_with_motion(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    motion_progress: f32,
    exiting: bool,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    let mut frames = SmallVec::<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]>::new();
    if visible_count == 0 {
        return frames;
    }

    let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, member_count).rect;
    let petal = stack_bloom_petal_size(member_count);
    let single_row_width = visible_count as f32 * petal.width
        + visible_count.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
    let available_width = (viewport.width - BLOOM_VIEWPORT_INSET_PX * 2.0).max(0.0);

    if single_row_width > available_width {
        let petals_per_row = ((available_width + BLOOM_PETAL_GAP_PX)
            / (petal.width + BLOOM_PETAL_GAP_PX))
            .floor()
            .max(1.0) as usize;
        let total_rows = visible_count.div_ceil(petals_per_row);
        let total_height = total_rows as f32 * petal.height
            + total_rows.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
        let grid_top = stack_bloom_row_top(viewport, capsule, total_height);
        for index in 0..visible_count {
            let row = index / petals_per_row;
            let col = index % petals_per_row;
            let row_start = row * petals_per_row;
            let row_end = (row_start + petals_per_row).min(visible_count);
            let petals_in_row = row_end - row_start;
            let row_width = petals_in_row as f32 * petal.width
                + petals_in_row.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
            let row_left = stack_bloom_row_left(viewport, capsule, row_width);
            let final_rect = Rect {
                x: row_left + col as f32 * (petal.width + BLOOM_PETAL_GAP_PX),
                y: grid_top + row as f32 * (petal.height + BLOOM_PETAL_GAP_PX),
                width: petal.width,
                height: petal.height,
            };
            frames.push(stack_bloom_motion_frame(
                viewport,
                capsule,
                final_rect,
                index,
                visible_count,
                motion_progress,
                exiting,
            ));
        }
        return frames;
    }

    let row_left = stack_bloom_row_left(viewport, capsule, single_row_width);
    let row_top = stack_bloom_row_top(viewport, capsule, petal.height);
    for index in 0..visible_count {
        let final_rect = Rect {
            x: row_left + index as f32 * (petal.width + BLOOM_PETAL_GAP_PX),
            y: row_top,
            width: petal.width,
            height: petal.height,
        };
        frames.push(stack_bloom_motion_frame(
            viewport,
            capsule,
            final_rect,
            index,
            visible_count,
            motion_progress,
            exiting,
        ));
    }
    frames
}

pub fn stack_bloom_petal_rects(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_petal_rects_at(viewport, anchor, member_count, 1.0)
}

pub fn stack_bloom_petal_rects_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at(viewport, anchor, member_count, reveal_progress)
        .iter()
        .map(|frame| frame.rect)
        .collect()
}

pub fn stack_bloom_exit_petal_rects_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_exit_frames_at(viewport, anchor, member_count, exit_progress)
        .iter()
        .map(|frame| frame.rect)
        .collect()
}

pub fn stack_bloom_hit_test(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_hit_test_at(viewport, anchor, member_count, 1.0, x, y)
}

pub fn stack_bloom_hit_test_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_petal_rects_at(viewport, anchor, member_count, reveal_progress)
        .iter()
        .position(|rect| rect_contains(inflate_rect(*rect, BLOOM_PETAL_HIT_INFLATE_PX), x, y))
}

pub fn stack_bloom_exit_hit_test_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_exit_petal_rects_at(viewport, anchor, member_count, exit_progress)
        .iter()
        .position(|rect| rect_contains(inflate_rect(*rect, BLOOM_PETAL_HIT_INFLATE_PX), x, y))
}

/// Map a visible Bloom slot to a real member. The final slot is reserved for
/// Tauri's `+N more` indicator when the stack exceeds the 24-slot cap.
pub fn stack_bloom_member_index_for_petal(
    member_count: usize,
    petal_index: usize,
) -> Option<usize> {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    if petal_index >= visible_count
        || (member_count > BLOOM_VISIBLE_PETAL_LIMIT && petal_index + 1 == visible_count)
    {
        return None;
    }
    Some(petal_index)
}

pub fn stack_bloom_overflow_count(member_count: usize) -> usize {
    if member_count > BLOOM_VISIBLE_PETAL_LIMIT {
        member_count - (BLOOM_VISIBLE_PETAL_LIMIT - 1)
    } else {
        0
    }
}

pub fn stack_tray_hit_test(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    x: f32,
    y: f32,
) -> Option<StackTrayPointerHit> {
    if rect_contains(stack_tray_close_rect(viewport, anchor, member_count), x, y) {
        return Some(StackTrayPointerHit::Close);
    }
    if rect_contains(
        stack_tray_dissolve_rect(viewport, anchor, member_count),
        x,
        y,
    ) {
        return Some(StackTrayPointerHit::Dissolve);
    }
    for row in 0..stack_tray_visible_rows(member_count) {
        if rect_contains(
            stack_tray_detach_rect(viewport, anchor, member_count, row),
            x,
            y,
        ) {
            return Some(StackTrayPointerHit::Detach(row));
        }
        if rect_contains(
            stack_tray_row_rect(viewport, anchor, member_count, row),
            x,
            y,
        ) {
            return Some(StackTrayPointerHit::Row(row));
        }
    }
    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

fn inflate_rect(rect: Rect, amount: f32) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.0,
        height: rect.height + amount * 2.0,
    }
}

fn stack_bloom_motion_frame(
    viewport: Size,
    capsule: Rect,
    final_rect: Rect,
    index: usize,
    visible_count: usize,
    motion_progress: f32,
    exiting: bool,
) -> StackBloomFrame {
    let start_center_x = capsule.x + capsule.width / 2.0;
    let start_center_y = capsule.y + capsule.height / 2.0;
    let final_center_x = final_rect.x + final_rect.width / 2.0;
    let final_center_y = final_rect.y + final_rect.height / 2.0;
    let (center_x, center_y, scale, alpha, progress) = if exiting {
        let elapsed_ms =
            motion_progress.clamp(0.0, 1.0) * stack_bloom_exit_duration_ms(visible_count) as f32;
        let exit_delay_ms = stack_bloom_exit_delay_ms(index, visible_count);
        let local_exit = if elapsed_ms <= exit_delay_ms {
            0.0
        } else {
            ((elapsed_ms - exit_delay_ms) / BLOOM_PETAL_EXIT_DURATION_MS as f32).clamp(0.0, 1.0)
        };
        let eased = zone_pill_geometry::ease_stack_bloom_exit_progress(local_exit);
        let remaining = (1.0 - eased).clamp(0.0, 1.0);
        (
            lerp(final_center_x, start_center_x, eased),
            lerp(final_center_y, start_center_y, eased),
            lerp(1.0, BLOOM_EXIT_SCALE, eased),
            remaining,
            remaining,
        )
    } else {
        let elapsed_ms =
            motion_progress.clamp(0.0, 1.0) * stack_bloom_reveal_duration_ms(visible_count) as f32;
        let reveal_delay_ms = stack_bloom_entry_delay_ms(index, visible_count);
        let local_reveal = if elapsed_ms <= reveal_delay_ms {
            0.0
        } else {
            ((elapsed_ms - reveal_delay_ms) / BLOOM_PETAL_ENTER_DURATION_MS as f32).clamp(0.0, 1.0)
        };
        let eased = zone_pill_geometry::ease_out_back_progress(local_reveal).max(0.0);
        let progress = eased.clamp(0.0, 1.0);
        (
            lerp(start_center_x, final_center_x, eased),
            lerp(start_center_y, final_center_y, eased),
            BLOOM_MOTION_MIN_SCALE + (1.0 - BLOOM_MOTION_MIN_SCALE) * eased,
            (BLOOM_MOTION_MIN_ALPHA + (1.0 - BLOOM_MOTION_MIN_ALPHA) * progress).clamp(0.0, 1.0),
            progress,
        )
    };
    let rect = clamp_rect_to_viewport(
        rect_from_center(
            center_x,
            center_y,
            final_rect.width * scale,
            final_rect.height * scale,
        ),
        viewport,
    );
    let connector = Rect {
        x: capsule.x + capsule.width * 0.5,
        y: capsule.y + capsule.height * 0.5,
        width: 0.0,
        height: 0.0,
    };

    StackBloomFrame {
        rect,
        connector,
        progress,
        scale,
        alpha,
    }
}

fn stack_bloom_row_left(viewport: Size, capsule: Rect, row_width: f32) -> f32 {
    let raw_left = capsule.x + capsule.width / 2.0 - row_width / 2.0;
    let max_left =
        (viewport.width - BLOOM_VIEWPORT_INSET_PX - row_width).max(BLOOM_VIEWPORT_INSET_PX);
    raw_left.clamp(BLOOM_VIEWPORT_INSET_PX, max_left)
}

fn stack_bloom_row_top(viewport: Size, capsule: Rect, row_height: f32) -> f32 {
    let below = capsule.bottom() + BLOOM_PETAL_GAP_BELOW_CAPSULE_PX;
    if below + row_height <= viewport.height - BLOOM_VIEWPORT_INSET_PX {
        return below;
    }
    let above = capsule.y - BLOOM_PETAL_GAP_BELOW_CAPSULE_PX - row_height;
    let max_top =
        (viewport.height - BLOOM_VIEWPORT_INSET_PX - row_height).max(BLOOM_VIEWPORT_INSET_PX);
    above.clamp(BLOOM_VIEWPORT_INSET_PX, max_top)
}

fn rect_from_center(center_x: f32, center_y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x: center_x - width / 2.0,
        y: center_y - height / 2.0,
        width,
        height,
    }
}

fn clamp_rect_to_viewport(rect: Rect, viewport: Size) -> Rect {
    let max_x =
        (viewport.width - rect.width - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y =
        (viewport.height - rect.height - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: rect.x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: rect.y.clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: rect.width,
        height: rect.height,
    }
}

pub fn stack_bloom_reveal_duration_ms(member_count: usize) -> u32 {
    let visible_count = member_count.clamp(1, BLOOM_VISIBLE_PETAL_LIMIT) as u32;
    BLOOM_PETAL_ENTER_DURATION_MS
        + (BLOOM_ENTRY_STAGGER_BUDGET_MS * visible_count.saturating_sub(1)) / visible_count
}

pub fn stack_bloom_exit_duration_ms(member_count: usize) -> u32 {
    let visible_count = member_count.clamp(1, BLOOM_VISIBLE_PETAL_LIMIT) as u32;
    let keyframe_with_tail = BLOOM_PETAL_EXIT_DURATION_MS
        + (BLOOM_EXIT_STAGGER_BUDGET_MS * visible_count.saturating_sub(1)) / visible_count;
    keyframe_with_tail.min(BLOOM_EXIT_VISIBLE_DURATION_MS)
}

fn stack_bloom_entry_delay_ms(index: usize, visible_count: usize) -> f32 {
    let count = visible_count.max(1) as f32;
    (BLOOM_ENTRY_STAGGER_BUDGET_MS as f32 / count) * index as f32
}

fn stack_bloom_exit_delay_ms(index: usize, visible_count: usize) -> f32 {
    let count = visible_count.max(1) as f32;
    let reverse_index = visible_count.saturating_sub(1).saturating_sub(index);
    (BLOOM_EXIT_STAGGER_BUDGET_MS as f32 / count) * reverse_index as f32
}

fn lerp(start: f32, end: f32, progress: f32) -> f32 {
    start + (end - start) * progress
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::tokens as style_tokens;
    use std::borrow::Cow;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn stack_bloom_timing_matches_tauri_entry_stagger_contract() {
        assert_eq!(BLOOM_PETAL_ENTER_DURATION_MS, 420);
        assert_eq!(BLOOM_ENTRY_STAGGER_BUDGET_MS, 360);
        assert_eq!(BLOOM_PETAL_EXIT_DURATION_MS, 140);
        assert_eq!(BLOOM_EXIT_STAGGER_BUDGET_MS, 120);
        assert_eq!(BLOOM_EXIT_VISIBLE_DURATION_MS, 160);
        assert_eq!(BLOOM_PREVIEW_HOVER_INTENT_MS, 150);
        assert_eq!(BLOOM_LEAVE_GRACE_MS, 80);
        assert_eq!(stack_bloom_reveal_duration_ms(0), 420);
        assert_eq!(stack_bloom_reveal_duration_ms(1), 420);
        assert_eq!(stack_bloom_reveal_duration_ms(2), 600);
        assert_eq!(stack_bloom_exit_duration_ms(0), 140);
        assert_eq!(stack_bloom_exit_duration_ms(1), 140);
        assert_eq!(stack_bloom_exit_duration_ms(2), 160);
        assert_eq!(stack_bloom_exit_duration_ms(BLOOM_VISIBLE_PETAL_LIMIT), 160);
        assert_eq!(
            stack_bloom_reveal_duration_ms(BLOOM_VISIBLE_PETAL_LIMIT),
            765
        );
        assert_eq!(BLOOM_REVEAL_DURATION_MS, 765);
    }

    #[test]
    fn stack_tray_state_distinguishes_management_from_bloom_preview() {
        let management = StackTrayState::new(ZoneId(1), ZoneId(2));
        let preview = StackTrayState::bloom_preview(ZoneId(1), ZoneId(2));

        assert!(management.is_management());
        assert!(!management.is_bloom_preview());
        assert!(preview.is_bloom_preview());
        assert!(!preview.is_management());
    }

    #[test]
    fn focused_bloom_preview_stays_next_to_petal_and_inside_viewport() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
        let left_petal = Rect {
            x: 120.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        };
        let right_petal = Rect {
            x: 1120.0,
            ..left_petal
        };

        let right_growing = focused_bloom_preview_rect(viewport, left_petal, &[left_petal], &zone);
        let left_growing = focused_bloom_preview_rect(viewport, right_petal, &[right_petal], &zone);

        assert_close(
            right_growing.x,
            left_petal.right() + FLOATING_PREVIEW_GAP_PX,
        );
        assert_close(
            left_growing.right(),
            right_petal.x - FLOATING_PREVIEW_GAP_PX,
        );
        for preview in [right_growing, left_growing] {
            assert!(preview.x >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
            assert!(preview.right() <= viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX + 0.01);
            assert!(preview.y >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
            assert!(
                preview.bottom() <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX + 0.01
            );
        }
    }

    #[test]
    fn focused_bloom_preview_avoids_the_complete_sibling_row() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
        let petals = [
            Rect {
                x: 120.0,
                y: 180.0,
                width: 108.0,
                height: 96.0,
            },
            Rect {
                x: 240.0,
                y: 180.0,
                width: 108.0,
                height: 96.0,
            },
            Rect {
                x: 360.0,
                y: 180.0,
                width: 108.0,
                height: 96.0,
            },
            Rect {
                x: 480.0,
                y: 180.0,
                width: 108.0,
                height: 96.0,
            },
        ];

        let preview = focused_bloom_preview_rect(viewport, petals[1], &petals, &zone);

        assert_close(preview.x, petals[3].right() + FLOATING_PREVIEW_GAP_PX);
        assert!(petals.iter().all(|petal| petal.right() <= preview.x));
        assert_close(preview.y, petals[1].y);
    }

    #[test]
    fn focused_bloom_preview_hit_and_header_actions_share_painted_geometry() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
        let petal = Rect {
            x: 120.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        };
        let petals = [petal];
        let preview = focused_bloom_preview_rect(viewport, petal, &petals, &zone);
        let search = focused_bloom_preview_search_rect(preview);
        let close = focused_bloom_preview_close_rect(preview);

        assert!(focused_bloom_preview_contains(
            viewport,
            petal,
            &petals,
            &zone,
            preview.x + 1.0,
            preview.y + 1.0
        ));
        assert!(!focused_bloom_preview_contains(
            viewport,
            petal,
            &petals,
            &zone,
            preview.x - 1.0,
            preview.y
        ));
        for button in [search, close] {
            assert!(button.x >= preview.x);
            assert!(button.right() <= preview.right());
            assert!(button.y >= preview.y);
            assert!(button.bottom() <= preview.bottom());
        }
        assert!(search.right() <= close.x);
    }

    #[test]
    fn stack_tray_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        // Wave D acceptance: tray panel chrome must map to
        // `bento_nano_style::tokens::PALETTE_DARK.surface_expanded` and the
        // Tauri `--shadow-expanded` outer geometry, not the legacy
        // `bento-nano-theme` palette.
        let chrome = StackTrayChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(
            chrome.panel_background,
            style_tokens::PALETTE_DARK.surface_expanded
        );
        assert_eq!(
            chrome.preview_background,
            style_tokens::PALETTE_DARK.surface_expanded
        );
        assert_eq!(
            chrome.row_background,
            style_tokens::PALETTE_DARK.surface_hover
        );
        assert_eq!(
            chrome.selected_background,
            style_tokens::PALETTE_DARK.surface_active
        );
        assert_eq!(
            chrome.danger_background,
            style_tokens::PALETTE_DARK.accent_red
        );
        assert_eq!(chrome.text_primary, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.text_muted, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.text_accent, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(
            chrome.panel_radius,
            BorderRadius::all(style_tokens::RADIUS.expanded)
        );
        assert_eq!(
            chrome.row_radius,
            BorderRadius::all(style_tokens::RADIUS.card)
        );
        assert_eq!(
            chrome.button_radius,
            BorderRadius::all(style_tokens::RADIUS.card)
        );
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }

    #[test]
    fn stack_tray_typography_matches_tauri_compact_roles() {
        assert_eq!(TRAY_TITLE_FONT_PX, 13.0);
        assert_eq!(TRAY_TITLE_FONT_WEIGHT, 600);
        assert_eq!(TRAY_COUNT_FONT_PX, 11.0);
        assert_eq!(TRAY_TOOLBAR_FONT_PX, 11.0);
        assert_eq!(TRAY_TOOLBAR_FONT_WEIGHT, 400);
        assert_eq!(TRAY_MEMBER_NAME_FONT_PX, 13.0);
        assert_eq!(TRAY_MEMBER_NAME_FONT_WEIGHT, 600);
        assert_eq!(TRAY_MEMBER_META_FONT_PX, 11.0);
        assert_eq!(TRAY_ACTION_FONT_PX, 11.0);
        assert_eq!(TRAY_STATUS_FONT_PX, 11.0);
        const { assert!(TRAY_TEXT_LINE_HEIGHT <= 1.25) };

        assert_eq!(PREVIEW_EYEBROW_FONT_PX, 11.0);
        assert_eq!(PREVIEW_TITLE_FONT_PX, 13.0);
        assert_eq!(PREVIEW_META_FONT_PX, 11.0);
        assert_eq!(PREVIEW_ITEM_FONT_PX, 11.0);
        assert_eq!(PREVIEW_EMPTY_FONT_PX, 11.0);
        const { assert!(PREVIEW_TEXT_LINE_HEIGHT <= 1.25) };
    }

    fn anchor() -> Zone {
        Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 700, 120, 180, 130)
    }

    fn rect_center_x(rect: Rect) -> f32 {
        rect.x + rect.width / 2.0
    }

    #[test]
    fn stack_tray_chrome_accepts_explicit_active_palette() {
        let mut palette = bento_nano_theme::current().palette;
        palette.scrim = Color::from_u8(0x00, 0x00, 0x00, 0x88);
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
        palette.active_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
        palette.hover_overlay = Color::from_u8(0x10, 0x20, 0x30, 0x40);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
        palette.accent = Color::from_u8(0x12, 0x34, 0x56, 0x78);

        let chrome = StackTrayChrome::from_palette(palette);

        assert_eq!(chrome.panel_shadow, shadow::DEFAULT.md);
        assert_eq!(chrome.panel_radius, radius::DEFAULT.xl);
        assert_eq!(chrome.row_radius, radius::DEFAULT.lg);
        assert_eq!(chrome.button_radius, radius::DEFAULT.md);
        assert_eq!(chrome.preview_item_radius, radius::DEFAULT.md);
        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.preview_background,
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
            chrome.dragged_background,
            Color::from_u8(0x33, 0x44, 0x55, 0x99)
        );
        assert_eq!(
            chrome.button_background,
            Color::from_u8(0x10, 0x20, 0x30, 0x40)
        );
        assert_eq!(
            chrome.danger_background,
            Color::from_u8(0xCC, 0x44, 0x44, 0xFF)
        );
        assert_eq!(chrome.text_primary, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.text_muted, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
        assert_eq!(chrome.text_accent, Color::from_u8(0x12, 0x34, 0x56, 0x78));
    }

    #[test]
    fn stack_tray_chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = bento_nano_theme::current().palette;
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let shadow = ShadowTokens {
            sm: Shadow {
                offset_x: 1.0,
                offset_y: 2.0,
                blur: 3.0,
                spread: 0.0,
                color: Color::from_u8(0x01, 0x01, 0x01, 0x20),
            },
            md: Shadow {
                offset_x: 4.0,
                offset_y: 5.0,
                blur: 6.0,
                spread: 0.0,
                color: Color::from_u8(0x02, 0x02, 0x02, 0x40),
            },
            lg: Shadow {
                offset_x: 7.0,
                offset_y: 8.0,
                blur: 9.0,
                spread: 0.0,
                color: Color::from_u8(0x03, 0x03, 0x03, 0x60),
            },
        };

        let chrome = StackTrayChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, radius.xl);
        assert_eq!(chrome.row_radius, radius.lg);
        assert_eq!(chrome.button_radius, radius.md);
        assert_eq!(chrome.preview_item_radius, radius.md);
    }

    #[test]
    fn panel_shadow_rect_uses_token_shadow_geometry() {
        let panel = Rect {
            x: 100.0,
            y: 40.0,
            width: 340.0,
            height: 168.0,
        };
        let shadow = Shadow {
            offset_x: 3.0,
            offset_y: 7.0,
            blur: 11.0,
            spread: 0.0,
            color: Color::BLACK,
        };

        assert_eq!(
            panel_shadow_rect(panel, shadow),
            Rect {
                x: 92.0,
                y: 36.0,
                width: 362.0,
                height: 190.0,
            }
        );
    }

    #[test]
    fn stack_tray_flips_left_near_viewport_edge() {
        let viewport = Size {
            width: 900.0,
            height: 600.0,
        };

        let rect = stack_tray_rect(viewport, &anchor(), 3);

        assert!(rect.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
        assert!(rect.x < 700.0);
    }

    #[test]
    fn stack_tray_header_count_clears_action_buttons() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);

        let title = stack_tray_header_title_rect(viewport, &zone, 3);
        let count = stack_tray_header_count_rect(viewport, &zone, 3);
        let many_count = stack_tray_header_count_rect(viewport, &zone, 1000);
        let dissolve = stack_tray_dissolve_rect(viewport, &zone, 3);
        let close = stack_tray_close_rect(viewport, &zone, 3);

        assert!(title.width > 0.0);
        assert!(count.width > 0.0);
        assert_eq!(stack_tray_header_count_label_len(3), 1);
        assert_eq!(stack_tray_header_count_label_len(10), 2);
        assert_eq!(stack_tray_header_count_label_len(999), 3);
        assert_eq!(stack_tray_header_count_label_len(1000), 4);
        assert!(
            count.width >= stack_tray_header_count_badge_width(3) - 0.01,
            "3-member badge keeps the full numeric label"
        );
        assert!(
            many_count.width >= stack_tray_header_count_badge_width(1000) - 0.01,
            "1000+ member badge keeps the capped 999+ label"
        );
        assert!(
            count.x >= title.right() + TRAY_GAP_PX - 0.01,
            "count starts after title"
        );
        assert!(
            count.right() <= dissolve.x - TRAY_GAP_PX + 0.01,
            "count must not overlap Dissolve"
        );
        assert!(
            many_count.right() <= dissolve.x - TRAY_GAP_PX + 0.01,
            "wide count badge must not overlap Dissolve"
        );
        assert!(dissolve.right() <= close.x - TRAY_GAP_PX + 0.01);
    }

    #[test]
    fn stack_tray_hit_test_prefers_detach_over_row() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);
        let detach = stack_tray_detach_rect(viewport, &zone, 4, 1);

        let hit = stack_tray_hit_test(viewport, &zone, 4, detach.x + 2.0, detach.y + 2.0);

        assert_eq!(hit, Some(StackTrayPointerHit::Detach(1)));
    }

    #[test]
    fn stack_tray_member_meta_rects_reserve_detach_button_space() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);
        let row = stack_tray_row_rect(viewport, &zone, 4, 0);
        let detach = stack_tray_detach_rect(viewport, &zone, 4, 0);

        let count = stack_tray_member_meta_count_rect(row);
        let suffix = stack_tray_member_meta_suffix_rect(row);

        assert_eq!(count.width, TRAY_MEMBER_META_COUNT_WIDTH_PX);
        assert!(count.x >= row.x + TRAY_MEMBER_TEXT_X_PX - 0.01);
        assert!(suffix.x >= count.right() + TRAY_MEMBER_META_GAP_PX - 0.01);
        assert!(suffix.right() <= detach.x - TRAY_MEMBER_META_GAP_PX + 0.01);
    }

    #[test]
    fn stack_tray_status_segments_stay_inside_status_row() {
        let tray = Rect {
            x: 120.0,
            y: 80.0,
            width: TRAY_WIDTH_PX,
            height: TRAY_MIN_HEIGHT_PX,
        };

        let status = stack_tray_status_rect(tray);
        let prefix = stack_tray_status_prefix_rect(status);
        let count = stack_tray_status_count_rect(status);
        let suffix = stack_tray_status_suffix_rect(status);

        assert_eq!(status.height, TRAY_STATUS_HEIGHT_PX);
        assert!(prefix.x >= status.x);
        assert!(count.x >= prefix.right() - 0.01);
        assert!(suffix.x >= count.right() + TRAY_STATUS_GAP_PX - 0.01);
        assert!(suffix.right() <= status.right() + 0.01);
    }

    #[test]
    fn focused_preview_meta_segments_leave_suffix_space() {
        let preview = Rect {
            x: 360.0,
            y: 120.0,
            width: PREVIEW_WIDTH_PX,
            height: PREVIEW_HEIGHT_PX,
        };

        let width_number = focused_preview_meta_number_rect(preview, 0);
        let width_mark = focused_preview_meta_mark_rect(preview, 0);
        let height_number = focused_preview_meta_number_rect(preview, 1);
        let height_mark = focused_preview_meta_mark_rect(preview, 1);
        let item_number = focused_preview_meta_number_rect(preview, 2);
        let suffix = focused_preview_meta_suffix_rect(preview);

        assert!(width_number.x >= preview.x + 16.0 - 0.01);
        assert!(width_mark.x >= width_number.right() - 0.01);
        assert!(height_number.x >= width_mark.right() + PREVIEW_META_GAP_PX - 0.01);
        assert!(height_mark.x >= height_number.right() - 0.01);
        assert!(item_number.x >= height_mark.right() + PREVIEW_META_GAP_PX - 0.01);
        assert!(suffix.x >= item_number.right() + PREVIEW_META_GAP_PX - 0.01);
        assert!(suffix.right() <= preview.right() - 16.0 + 0.01);
    }

    #[test]
    fn focused_preview_stays_inside_viewport() {
        let viewport = Size {
            width: 720.0,
            height: 360.0,
        };
        let tray = Rect {
            x: 380.0,
            y: 260.0,
            width: TRAY_WIDTH_PX,
            height: 160.0,
        };

        let preview = focused_preview_rect(viewport, tray);

        assert!(preview.x >= TRAY_VIEWPORT_MARGIN_PX);
        assert!(preview.y >= TRAY_VIEWPORT_MARGIN_PX);
        assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
        assert!(preview.bottom() <= viewport.height - TRAY_VIEWPORT_MARGIN_PX);
    }

    #[test]
    fn focused_preview_uses_left_side_when_left_has_more_room() {
        let viewport = Size {
            width: 1707.0,
            height: 912.0,
        };
        let tray = Rect {
            x: 756.0,
            y: 332.0,
            width: TRAY_WIDTH_PX,
            height: TRAY_MIN_HEIGHT_PX,
        };

        let preview = focused_preview_rect(viewport, tray);

        assert!(preview.right() <= tray.x - PREVIEW_GAP_PX + 0.01);
        assert!(preview.x >= TRAY_VIEWPORT_MARGIN_PX);
        assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
    }

    #[test]
    fn focused_preview_keeps_right_side_when_right_has_more_room() {
        let viewport = Size {
            width: 2560.0,
            height: 1440.0,
        };
        let tray = Rect {
            x: 756.0,
            y: 332.0,
            width: TRAY_WIDTH_PX,
            height: TRAY_MIN_HEIGHT_PX,
        };

        let preview = focused_preview_rect(viewport, tray);

        assert!(preview.x >= tray.right() + PREVIEW_GAP_PX - 0.01);
        assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
    }

    #[test]
    fn stack_bloom_petal_size_matches_tauri_buckets() {
        assert_eq!(
            stack_bloom_petal_size(4),
            StackBloomPetalSize {
                width: 108.0,
                height: 96.0,
                icon_size: 36.0,
            }
        );
        assert_eq!(
            stack_bloom_petal_size(8),
            StackBloomPetalSize {
                width: 92.0,
                height: 84.0,
                icon_size: 32.0,
            }
        );
        assert_eq!(
            stack_bloom_petal_size(16),
            StackBloomPetalSize {
                width: 80.0,
                height: 72.0,
                icon_size: 28.0,
            }
        );
        assert_eq!(
            stack_bloom_petal_size(17),
            StackBloomPetalSize {
                width: 72.0,
                height: 64.0,
                icon_size: 24.0,
            }
        );
    }

    #[test]
    fn stack_bloom_petal_content_layout_matches_tauri_column_box() {
        let petal = Rect {
            x: 24.0,
            y: 40.0,
            width: BLOOM_PETAL_WIDTH_PX,
            height: BLOOM_PETAL_HEIGHT_PX,
        };

        let layout = stack_bloom_petal_content_layout(petal, BLOOM_PETAL_ICON_PX, 1.0);

        assert_close(
            layout.icon_rect.x,
            petal.x + (petal.width - BLOOM_PETAL_ICON_PX) * 0.5,
        );
        assert_close(layout.icon_rect.y, petal.y + BLOOM_PETAL_PADDING_Y_PX);
        assert_close(layout.icon_rect.width, BLOOM_PETAL_ICON_PX);
        assert_close(layout.title_rect.x, petal.x + BLOOM_PETAL_PADDING_X_PX);
        assert_close(
            layout.title_rect.y,
            layout.icon_rect.bottom() + BLOOM_PETAL_CONTENT_GAP_PX,
        );
        assert_close(
            layout.title_rect.width,
            petal.width - BLOOM_PETAL_PADDING_X_PX * 2.0,
        );
        assert!(
            layout.title_rect.height
                >= BLOOM_PETAL_NAME_FONT_PX
                    * BLOOM_PETAL_NAME_LINE_HEIGHT
                    * BLOOM_PETAL_NAME_MAX_LINES as f32
        );
        assert!(layout.title_rect.bottom() <= petal.bottom() + 0.01);
    }

    #[test]
    fn stack_bloom_compact_content_layout_keeps_icon_and_title_separated() {
        let petal = Rect {
            x: 8.0,
            y: 8.0,
            width: BLOOM_PETAL_WIDTH_COMPACT_PX,
            height: BLOOM_PETAL_HEIGHT_COMPACT_PX,
        };

        let layout = stack_bloom_petal_content_layout(petal, BLOOM_PETAL_ICON_COMPACT_PX, 1.0);

        assert!(layout.icon_rect.height > 0.0);
        assert!(layout.title_rect.height > 0.0);
        assert!(
            layout.icon_rect.bottom() + BLOOM_PETAL_CONTENT_GAP_PX <= layout.title_rect.y + 0.01
        );
        assert!(layout.title_rect.bottom() <= petal.bottom() + 0.01);
    }

    #[test]
    fn stack_bloom_petals_follow_member_count_and_stay_stable() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
        let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(&zone, 8).rect;

        let rects = stack_bloom_petal_rects(viewport, &zone, 8);

        assert_eq!(rects.len(), 8);
        assert!(rects.windows(2).all(|pair| {
            (pair[0].y - pair[1].y).abs() < 0.01
                && pair[0].right() + BLOOM_PETAL_GAP_PX <= pair[1].x + 0.01
        }));
        assert!(
            rects
                .iter()
                .all(|rect| rect.y >= capsule.bottom() + BLOOM_PETAL_GAP_BELOW_CAPSULE_PX - 0.01)
        );
    }

    #[test]
    fn stack_bloom_row_clamps_near_viewport_edge() {
        let viewport = Size {
            width: 500.0,
            height: 360.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 340, 120, 140, 100);

        let rects = stack_bloom_petal_rects(viewport, &zone, 3);

        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|rect| rect.x >= BLOOM_VIEWPORT_INSET_PX
            && rect.right() <= viewport.width - BLOOM_VIEWPORT_INSET_PX + 0.01));
    }

    #[test]
    fn stack_bloom_hit_test_returns_petal_index() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
        let rects = stack_bloom_petal_rects(viewport, &zone, 4);
        let target = rects[2];

        let hit = stack_bloom_hit_test(viewport, &zone, 4, target.x + 4.0, target.y + 4.0);

        assert_eq!(hit, Some(2));
    }

    #[test]
    fn stack_bloom_hit_test_uses_tauri_twelve_pixel_petal_halo() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
        let first = stack_bloom_petal_rects(viewport, &zone, 4)[0];
        let y = first.y + first.height * 0.5;

        assert_eq!(
            stack_bloom_hit_test(viewport, &zone, 4, first.x - 8.0, y),
            Some(0)
        );
        assert_eq!(
            stack_bloom_hit_test(viewport, &zone, 4, first.x - 13.0, y),
            None
        );
    }

    #[test]
    fn stack_bloom_caps_slots_and_reserves_the_last_for_overflow() {
        let viewport = Size {
            width: 1920.0,
            height: 1080.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        assert_eq!(
            stack_bloom_petal_rects(viewport, &zone, 30).len(),
            BLOOM_VISIBLE_PETAL_LIMIT
        );
        assert_eq!(stack_bloom_overflow_count(24), 0);
        assert_eq!(stack_bloom_overflow_count(25), 2);
        assert_eq!(stack_bloom_overflow_count(30), 7);
        assert_eq!(stack_bloom_member_index_for_petal(25, 22), Some(22));
        assert_eq!(stack_bloom_member_index_for_petal(25, 23), None);
        assert_eq!(stack_bloom_member_index_for_petal(24, 23), Some(23));
        assert_eq!(stack_bloom_member_index_for_petal(24, 24), None);
    }

    #[test]
    fn stack_bloom_frames_apply_staggered_motion_without_losing_hit_targets() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        let frames = stack_bloom_frames(viewport, &zone, 5);
        let partial = stack_bloom_frames_at(viewport, &zone, 5, 0.45);

        assert_eq!(frames.len(), 5);
        assert_eq!(partial.len(), 5);
        assert!(
            partial
                .windows(2)
                .all(|pair| pair[0].progress >= pair[1].progress)
        );
        assert!(partial[0].progress > partial[partial.len() - 1].progress);
        assert!(frames.iter().all(|frame| {
            (frame.progress - 1.0).abs() < 0.01
                && (frame.scale - 1.0).abs() < 0.01
                && (frame.alpha - 1.0).abs() < 0.01
                && frame.connector.width >= 0.0
                && frame.rect.x >= BLOOM_VIEWPORT_INSET_PX
                && frame.rect.right() <= viewport.width - BLOOM_VIEWPORT_INSET_PX
        }));
        assert_eq!(
            stack_bloom_hit_test(
                viewport,
                &zone,
                5,
                frames[3].rect.x + 4.0,
                frames[3].rect.y + 4.0
            ),
            Some(3)
        );
    }

    #[test]
    fn stack_bloom_frames_progress_from_anchor_to_settled_geometry() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        let start = stack_bloom_frames_at(viewport, &zone, 4, 0.0);
        let midway = stack_bloom_frames_at(viewport, &zone, 4, 0.45);
        let settled = stack_bloom_frames_at(viewport, &zone, 4, 1.0);

        assert_eq!(start.len(), 4);
        assert_eq!(midway.len(), 4);
        assert_eq!(settled.len(), 4);
        assert!((start[0].scale - BLOOM_MOTION_MIN_SCALE).abs() < 0.01);
        assert!((start[0].alpha - BLOOM_MOTION_MIN_ALPHA).abs() < 0.01);
        assert!((start[0].scale - 0.4).abs() < 0.01);
        assert!(start[0].alpha.abs() < 0.01);
        assert!(
            start
                .iter()
                .all(|frame| frame.progress.abs() < f32::EPSILON)
        );
        assert!(midway[0].progress >= midway[1].progress);
        assert!(midway[0].progress > midway[3].progress);
        assert!(midway[0].progress > start[0].progress);
        assert!(settled[0].progress >= midway[0].progress);
        assert!(settled[3].progress > midway[3].progress);
        assert!(start[0].rect.x > settled[0].rect.x);
        assert!(start[3].rect.x < settled[3].rect.x);
        assert!(start[0].rect.y < settled[0].rect.y);
        assert!(start[0].alpha <= midway[0].alpha && midway[0].alpha <= settled[0].alpha);
        assert!(start[0].scale <= midway[0].scale);
        assert!((settled[0].scale - 1.0).abs() < 0.01);
        let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(&zone, 4).rect;
        assert!((rect_center_x(start[0].rect) - rect_center_x(capsule)).abs() < 0.01);
    }

    #[test]
    fn stack_bloom_two_petal_entry_is_not_settled_at_old_180ms_cutoff() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
        let duration = stack_bloom_reveal_duration_ms(2);
        let old_cutoff_progress = 180.0 / duration as f32;

        let old_cutoff = stack_bloom_frames_at(viewport, &zone, 2, old_cutoff_progress);
        let settled = stack_bloom_frames_at(viewport, &zone, 2, 1.0);

        assert_eq!(duration, 600);
        assert!(old_cutoff[0].progress > 0.0);
        assert!(old_cutoff[1].progress.abs() < f32::EPSILON);
        assert!(old_cutoff[1].alpha < settled[1].alpha);
        assert!(old_cutoff[1].scale < settled[1].scale);
        assert!((old_cutoff[1].rect.x - settled[1].rect.x).abs() > 1.0);
    }

    #[test]
    fn stack_bloom_exit_keeps_petals_visible_at_tauri_120ms_cutoff() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
        let duration = stack_bloom_exit_duration_ms(2);
        let exit_120ms_progress = 120.0 / duration as f32;

        let stable = stack_bloom_frames_at(viewport, &zone, 2, 1.0);
        let leaving = stack_bloom_exit_frames_at(viewport, &zone, 2, exit_120ms_progress);

        assert_eq!(duration, BLOOM_EXIT_VISIBLE_DURATION_MS);
        assert_eq!(leaving.len(), 2);
        assert!(leaving[0].alpha > 0.0);
        assert!(leaving[1].alpha > 0.0);
        assert!(leaving[0].alpha < stable[0].alpha);
        assert!(leaving[1].alpha < stable[1].alpha);
        assert!(leaving[0].rect.width < stable[0].rect.width);
        assert!(leaving[1].rect.width < stable[1].rect.width);
        assert!((leaving[0].rect.x - stable[0].rect.x).abs() > 1.0);
        assert!((leaving[1].rect.x - stable[1].rect.x).abs() > 1.0);
        assert!(
            leaving[0].alpha > leaving[1].alpha,
            "reverse stagger should keep the first-in petal visible longer"
        );
    }

    #[test]
    fn stack_bloom_settled_frames_never_overlap_in_row_or_grid() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        let frames = stack_bloom_frames_at(viewport, &zone, 5, 1.0);

        assert_eq!(frames.len(), 5);
        assert!(
            frames.windows(2).all(|pair| {
                pair[0].rect.right() <= pair[1].rect.x || pair[0].rect.bottom() <= pair[1].rect.y
            }),
            "settled bloom frames must stay separated"
        );
    }

    #[test]
    fn stack_wrapper_halo_scales_with_visible_member_count() {
        let zone = anchor();

        let one = stack_wrapper_halo_rect(&zone, 1);
        let many = stack_wrapper_halo_rect(&zone, 8);

        assert!(many.x < one.x);
        assert!(many.y < one.y);
        assert!(many.width > one.width);
        assert!(many.height > one.height);
        let expected_width =
            zone.w as f32 + (BLOOM_WRAPPER_BASE_PAD_PX + 8.0 * BLOOM_WRAPPER_MEMBER_PAD_PX) * 2.0;
        assert!((many.width - expected_width).abs() < f32::EPSILON);
    }
}
