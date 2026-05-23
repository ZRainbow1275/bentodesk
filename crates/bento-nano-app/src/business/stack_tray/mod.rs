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

pub const PREVIEW_WIDTH_PX: f32 = 300.0;
pub const PREVIEW_HEIGHT_PX: f32 = 196.0;
pub const PREVIEW_GAP_PX: f32 = 10.0;

pub const BLOOM_PETAL_WIDTH_PX: f32 = 164.0;
pub const BLOOM_PETAL_HEIGHT_PX: f32 = 28.0;
pub const BLOOM_PETAL_STRIDE_PX: f32 = 34.0;
pub const BLOOM_PETAL_GAP_PX: f32 = 12.0;
pub const BLOOM_PETAL_FAN_STEP_PX: f32 = 10.0;
pub const BLOOM_VISIBLE_PETAL_LIMIT: usize = 5;
pub const BLOOM_MOTION_STAGGER_STEP: f32 = 0.075;
pub const BLOOM_MOTION_MIN_PROGRESS: f32 = 0.64;
pub const BLOOM_MOTION_MIN_SCALE: f32 = 0.88;
pub const BLOOM_MOTION_MIN_ALPHA: f32 = 0.58;
pub const BLOOM_CONNECTOR_THICKNESS_PX: f32 = 3.0;
pub const BLOOM_WRAPPER_BASE_PAD_PX: f32 = 5.0;
pub const BLOOM_WRAPPER_MEMBER_PAD_PX: f32 = 1.35;
pub const BLOOM_REVEAL_DURATION_MS: u32 = 180;

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
            panel_shadow: shadow.expanded,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTrayState {
    pub anchor_zone_id: ZoneId,
    pub selected_member_id: ZoneId,
    pub status: Option<SmolStr>,
}

impl StackTrayState {
    pub fn new(anchor_zone_id: ZoneId, selected_member_id: ZoneId) -> Self {
        Self {
            anchor_zone_id,
            selected_member_id,
            status: None,
        }
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

pub fn stack_tray_close_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.right() - TRAY_INSET_PX - TRAY_CLOSE_BUTTON_WIDTH_PX,
        y: tray.y + 9.0,
        width: TRAY_CLOSE_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn focused_preview_rect(viewport: Size, tray: Rect) -> Rect {
    let right_candidate = tray.right() + PREVIEW_GAP_PX;
    let left_candidate = tray.x - PREVIEW_GAP_PX - PREVIEW_WIDTH_PX;
    let x = if right_candidate + PREVIEW_WIDTH_PX + TRAY_VIEWPORT_MARGIN_PX <= viewport.width {
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
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    let mut frames = SmallVec::<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]>::new();
    if visible_count == 0 {
        return frames;
    }

    let total_height =
        BLOOM_PETAL_HEIGHT_PX + (visible_count.saturating_sub(1) as f32 * BLOOM_PETAL_STRIDE_PX);
    let anchor_center_y = anchor.y as f32 + anchor.h as f32 / 2.0;
    let raw_top = anchor_center_y - total_height / 2.0;
    let max_y =
        (viewport.height - total_height - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let top = raw_top.clamp(TRAY_VIEWPORT_MARGIN_PX, max_y);

    let right_x = anchor.x as f32 + anchor.w as f32 + BLOOM_PETAL_GAP_PX;
    let left_x = anchor.x as f32 - BLOOM_PETAL_GAP_PX - BLOOM_PETAL_WIDTH_PX;
    let opens_right = right_x + BLOOM_PETAL_WIDTH_PX + TRAY_VIEWPORT_MARGIN_PX <= viewport.width;
    let center_index = (visible_count.saturating_sub(1)) as f32 / 2.0;

    for index in 0..visible_count {
        let fan_offset = ((index as f32) - center_index).abs() * BLOOM_PETAL_FAN_STEP_PX;
        let final_x = if opens_right {
            right_x + fan_offset
        } else {
            left_x - fan_offset
        };
        let max_x = (viewport.width - BLOOM_PETAL_WIDTH_PX - TRAY_VIEWPORT_MARGIN_PX)
            .max(TRAY_VIEWPORT_MARGIN_PX);
        let final_rect = Rect {
            x: final_x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
            y: top + index as f32 * BLOOM_PETAL_STRIDE_PX,
            width: BLOOM_PETAL_WIDTH_PX,
            height: BLOOM_PETAL_HEIGHT_PX,
        };
        frames.push(stack_bloom_motion_frame(
            viewport,
            anchor,
            final_rect,
            opens_right,
            index,
            visible_count,
            reveal_progress,
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
        .position(|rect| rect_contains(*rect, x, y))
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

fn stack_bloom_motion_frame(
    viewport: Size,
    anchor: &Zone,
    final_rect: Rect,
    opens_right: bool,
    index: usize,
    visible_count: usize,
    reveal_progress: f32,
) -> StackBloomFrame {
    let settled_stagger =
        (index as f32 * BLOOM_MOTION_STAGGER_STEP).min(1.0 - BLOOM_MOTION_MIN_PROGRESS);
    let settled_progress = (1.0 - settled_stagger).max(BLOOM_MOTION_MIN_PROGRESS);
    let reveal_delay = settled_stagger;
    let local_reveal = if reveal_progress <= reveal_delay {
        0.0
    } else {
        ((reveal_progress - reveal_delay) / (1.0 - reveal_delay)).clamp(0.0, 1.0)
    };
    let progress = settled_progress * ease_out_cubic(local_reveal);
    let eased = ease_out_cubic(progress);
    let scale = BLOOM_MOTION_MIN_SCALE + (1.0 - BLOOM_MOTION_MIN_SCALE) * eased;
    let alpha = BLOOM_MOTION_MIN_ALPHA + (1.0 - BLOOM_MOTION_MIN_ALPHA) * eased;
    let anchor_center_y = anchor.y as f32 + anchor.h as f32 / 2.0;
    let start_center_x = if opens_right {
        anchor.x as f32 + anchor.w as f32 + BLOOM_PETAL_GAP_PX + BLOOM_PETAL_WIDTH_PX / 2.0
    } else {
        anchor.x as f32 - BLOOM_PETAL_GAP_PX - BLOOM_PETAL_WIDTH_PX / 2.0
    };
    let final_center_x = final_rect.x + final_rect.width / 2.0;
    let final_center_y = final_rect.y + final_rect.height / 2.0;
    let center_x = lerp(start_center_x, final_center_x, eased);
    let center_y = lerp(anchor_center_y, final_center_y, eased);
    let rect = clamp_rect_to_viewport(
        rect_from_center(
            center_x,
            center_y,
            final_rect.width * scale,
            final_rect.height * scale,
        ),
        viewport,
    );
    let connector = stack_bloom_connector_rect(anchor, rect, opens_right, visible_count);

    StackBloomFrame {
        rect,
        connector,
        progress,
        scale,
        alpha,
    }
}

fn stack_bloom_connector_rect(
    anchor: &Zone,
    rect: Rect,
    opens_right: bool,
    visible_count: usize,
) -> Rect {
    let y = rect.y + rect.height / 2.0 - BLOOM_CONNECTOR_THICKNESS_PX / 2.0;
    let member_boost = visible_count.saturating_sub(1) as f32 * 0.18;
    let height = BLOOM_CONNECTOR_THICKNESS_PX + member_boost.min(0.9);
    if opens_right {
        let start = anchor.x as f32 + anchor.w as f32;
        Rect {
            x: start,
            y,
            width: (rect.x - start).max(0.0),
            height,
        }
    } else {
        let end = anchor.x as f32;
        Rect {
            x: rect.right(),
            y,
            width: (end - rect.right()).max(0.0),
            height,
        }
    }
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

fn ease_out_cubic(progress: f32) -> f32 {
    let inverse = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - inverse * inverse * inverse
}

fn lerp(start: f32, end: f32, progress: f32) -> f32 {
    start + (end - start) * progress.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::tokens as style_tokens;
    use std::borrow::Cow;

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
        assert_eq!(chrome.panel_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.preview_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.row_background, style_tokens::PALETTE_DARK.surface_hover);
        assert_eq!(chrome.selected_background, style_tokens::PALETTE_DARK.surface_active);
        assert_eq!(chrome.danger_background, style_tokens::PALETTE_DARK.accent_red);
        assert_eq!(chrome.text_primary, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.text_muted, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.text_accent, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(chrome.panel_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        assert_eq!(chrome.row_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.button_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded);
    }

    fn anchor() -> Zone {
        Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 700, 120, 180, 130)
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
                color: Color::from_u8(0x01, 0x01, 0x01, 0x20),
            },
            md: Shadow {
                offset_x: 4.0,
                offset_y: 5.0,
                blur: 6.0,
                color: Color::from_u8(0x02, 0x02, 0x02, 0x40),
            },
            lg: Shadow {
                offset_x: 7.0,
                offset_y: 8.0,
                blur: 9.0,
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
    fn stack_bloom_petals_are_limited_and_stable() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        let rects = stack_bloom_petal_rects(viewport, &zone, 8);

        assert_eq!(rects.len(), BLOOM_VISIBLE_PETAL_LIMIT);
        assert!(rects.windows(2).all(|pair| pair[0].y < pair[1].y));
        assert!(
            rects
                .iter()
                .all(|rect| rect.x > zone.x as f32 + zone.w as f32)
        );
    }

    #[test]
    fn stack_bloom_flips_left_near_viewport_edge() {
        let viewport = Size {
            width: 500.0,
            height: 360.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 340, 120, 140, 100);

        let rects = stack_bloom_petal_rects(viewport, &zone, 3);

        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|rect| rect.right() <= zone.x as f32));
        assert!(
            rects
                .iter()
                .all(|rect| rect.x >= TRAY_VIEWPORT_MARGIN_PX && rect.right() <= viewport.width)
        );
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
    fn stack_bloom_frames_apply_staggered_motion_without_losing_hit_targets() {
        let viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

        let frames = stack_bloom_frames(viewport, &zone, 5);

        assert_eq!(frames.len(), BLOOM_VISIBLE_PETAL_LIMIT);
        assert!(
            frames
                .windows(2)
                .all(|pair| pair[0].progress > pair[1].progress)
        );
        assert!(frames.iter().all(|frame| {
            frame.scale >= BLOOM_MOTION_MIN_SCALE
                && frame.scale <= 1.0
                && frame.alpha >= BLOOM_MOTION_MIN_ALPHA
                && frame.alpha <= 1.0
                && frame.connector.width >= 0.0
                && frame.rect.x >= TRAY_VIEWPORT_MARGIN_PX
                && frame.rect.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX
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
        assert!(
            start
                .iter()
                .all(|frame| frame.progress.abs() < f32::EPSILON)
        );
        assert!(midway[0].progress > midway[1].progress);
        assert!(midway[0].progress > start[0].progress);
        assert!(settled[0].progress > midway[0].progress);
        assert!(settled[3].progress > midway[3].progress);
        assert!(start[0].rect.x < settled[0].rect.x);
        assert!(start[0].alpha <= midway[0].alpha && midway[0].alpha <= settled[0].alpha);
        assert!(start[0].scale <= midway[0].scale && midway[0].scale <= settled[0].scale);
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
        let expected_width = zone.w as f32
            + (BLOOM_WRAPPER_BASE_PAD_PX
                + BLOOM_VISIBLE_PETAL_LIMIT as f32 * BLOOM_WRAPPER_MEMBER_PAD_PX)
                * 2.0;
        assert!((many.width - expected_width).abs() < f32::EPSILON);
    }
}
