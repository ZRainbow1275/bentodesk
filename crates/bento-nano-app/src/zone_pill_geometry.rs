//! Wave C (05-20 visual parity) — collapsed zone pill geometry.
//!
//! Tauri 1.2.4 renders each zone in the Main HWND as a capsule "pill"
//! (icon glyph + name + count badge with rounded-rect shadow) by default;
//! hover or click reveals the item grid via the existing expanded path in
//! `render::draw_zones`. Geometry constants live here so the renderer +
//! hit-test + unit tests share one source of truth — Wave A baseline
//! `research/baseline/zone-collapsed-pill.md` and Wave B SSoT
//! `bento_nano_style::tokens::{RADIUS, SPACING, TYPOGRAPHY}`.
//!
//! Spec §3.2 100% self-rolled / spec §8 no new crate deps / spec §10 zero
//! allocation hot-path: every helper here returns `Copy` rects, no `Vec`,
//! no `String`.

use bento_nano_style::tokens::{RADIUS, SPACING, TYPOGRAPHY};
use bento_nano_style::{BorderRadius, Rect};
use bento_nano_zone::Zone;

/// Layout slot inside the collapsed pill (icon chip, label band, count
/// badge). Caller paints whatever fill + text suits the accent / palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZonePillLayout {
    /// The pill outer rectangle in logical DIPs. Hit-test region.
    pub rect: Rect,
    /// Drop-shadow band (Wave B `SHADOW.zen` outer offset). Painted under
    /// the main pill rect.
    pub shadow_outer: Rect,
    /// Soft surface lift (Wave B `SHADOW.zen_inner`). Painted under the
    /// pill but above `shadow_outer`.
    pub shadow_inner: Rect,
    /// Icon chip rectangle (left-aligned circle / square).
    pub icon: Rect,
    /// Label band (one line of zone title).
    pub label: Rect,
    /// Count badge (item count or stack member count).
    pub badge: Rect,
    /// Wave H2 — status dot at the top-right corner of the pill. Renderer
    /// paints it only when the zone has items (`count > 0`); empty pills
    /// suppress it. Mirrors the Tauri 1.2.4 "filled" indicator.
    pub status_dot: Rect,
    /// Pill corner radius — Wave B `RADIUS.capsule` (24 DIPs).
    pub radius: BorderRadius,
    /// Badge corner radius — Wave B `RADIUS.badge` (10 DIPs).
    pub badge_radius: BorderRadius,
}

/// Default pill height in DIPs. Tauri reference: 36 DIPs (Wave A
/// `zone-collapsed-pill.md`). Pinned constant — Wave A baseline.
pub const PILL_HEIGHT: f32 = 36.0;

/// Minimum total width before the label is clipped.
pub const PILL_MIN_WIDTH: f32 = 96.0;

/// Default visible label width before truncation (~12 ASCII chars at
/// TYPOGRAPHY.md). Keeps the pill horizontally compact next to the badge.
pub const PILL_LABEL_DEFAULT_WIDTH: f32 = 108.0;

/// Icon chip side length in DIPs.
pub const PILL_ICON_SIZE: f32 = 22.0;

/// Count badge minimum width (fits 3-digit count without truncation).
pub const PILL_BADGE_MIN_WIDTH: f32 = 28.0;

/// Count badge height in DIPs.
pub const PILL_BADGE_HEIGHT: f32 = 20.0;

/// Drop-shadow outer offset matching `bento_nano_style::tokens::SHADOW.zen`
/// (y=8, blur=32). Renderer maps this to a translated rect since D2D's
/// shadow effect isn't always available.
pub const PILL_SHADOW_OUTER_DY: f32 = 8.0;

/// Drop-shadow inner lift matching `SHADOW.zen_inner` (y=2, blur=8).
pub const PILL_SHADOW_INNER_DY: f32 = 2.0;

/// Wave H2 — diameter of the top-right "has items" status dot in DIPs.
/// Sized to read at 100 % DPI without crowding the pill chrome (six DIPs
/// is the smallest legible filled disc against PALETTE_DARK.surface_zen).
pub const PILL_STATUS_DOT_SIZE: f32 = 6.0;

/// Wave H2 — inset of the status dot from the pill's top-right corner.
/// Keeps the disc clear of the badge and the capsule curvature.
pub const PILL_STATUS_DOT_INSET: f32 = 6.0;

/// Wave G2 — total wall-clock duration of the capsule expand/shrink morph.
/// Capped at ≤200ms per the parent G2 prompt; 160ms feels snappy for hover.
pub const ZONE_PILL_ANIM_DURATION_MS: u32 = 160;

/// Ease-out cubic — matches `stack_tray::ease_out_cubic`. Decelerating curve
/// so the capsule snaps quickly then settles, mirroring Tauri's CSS
/// `cubic-bezier(0.2, 0.8, 0.4, 1.0)` close enough for a 160ms tween.
#[inline]
pub fn ease_out_cubic_progress(progress: f32) -> f32 {
    let inv = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

/// Linearly interpolate between the collapsed pill rect and the expanded
/// zone rect using a 0..1 morph factor. `morph = 0` → pill, `morph = 1` →
/// expanded body. Pure / allocation-free.
pub fn morph_pill_to_rect(pill: Rect, expanded: Rect, morph: f32) -> Rect {
    let t = morph.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Rect {
        x: pill.x * inv + expanded.x * t,
        y: pill.y * inv + expanded.y * t,
        width: pill.width * inv + expanded.width * t,
        height: pill.height * inv + expanded.height * t,
    }
}

/// Morph the pill corner radius (capsule, 24px) toward the expanded surface
/// radius supplied by `expanded_radius`. Used by the renderer so the chrome
/// "uncurls" smoothly during the expand transition.
pub fn morph_pill_radius(pill_radius: f32, expanded_radius: f32, morph: f32) -> f32 {
    let t = morph.clamp(0.0, 1.0);
    pill_radius * (1.0 - t) + expanded_radius * t
}

/// Build a pill layout anchored at `(zone.x, zone.y)`. `count` is the badge
/// number (item count for a regular zone or stack-member count for an
/// anchor). The returned `rect` is the pill's outer hit-test region.
///
/// Pure / allocation-free / `Copy` output. Safe to call every frame.
pub fn pill_layout_for_zone(zone: &Zone, count: usize) -> ZonePillLayout {
    let pad_horizontal = SPACING.md; // 12 DIPs left/right inset
    let pad_inner = SPACING.s6; // 6 DIPs between icon/label/badge
    let badge_width = badge_width_for_count(count);
    let label_width = PILL_LABEL_DEFAULT_WIDTH;
    let total_width = (pad_horizontal * 2.0)
        + PILL_ICON_SIZE
        + pad_inner
        + label_width
        + pad_inner
        + badge_width;
    let width = total_width.max(PILL_MIN_WIDTH);
    let x = zone.x as f32;
    let y = zone.y as f32;
    let rect = Rect {
        x,
        y,
        width,
        height: PILL_HEIGHT,
    };
    let shadow_outer = Rect {
        x: rect.x,
        y: rect.y + PILL_SHADOW_OUTER_DY,
        width: rect.width,
        height: rect.height,
    };
    let shadow_inner = Rect {
        x: rect.x,
        y: rect.y + PILL_SHADOW_INNER_DY,
        width: rect.width,
        height: rect.height,
    };
    let icon_y = rect.y + (PILL_HEIGHT - PILL_ICON_SIZE) * 0.5;
    let icon = Rect {
        x: rect.x + pad_horizontal,
        y: icon_y,
        width: PILL_ICON_SIZE,
        height: PILL_ICON_SIZE,
    };
    let label_x = icon.x + icon.width + pad_inner;
    let label_h = TYPOGRAPHY.md.size_px * TYPOGRAPHY.md.line_height;
    let label = Rect {
        x: label_x,
        y: rect.y + (PILL_HEIGHT - label_h) * 0.5,
        width: label_width,
        height: label_h,
    };
    let badge_y = rect.y + (PILL_HEIGHT - PILL_BADGE_HEIGHT) * 0.5;
    let badge = Rect {
        x: label.x + label.width + pad_inner,
        y: badge_y,
        width: badge_width,
        height: PILL_BADGE_HEIGHT,
    };
    // Wave H2 — status dot inset from the pill's top-right corner. The
    // renderer paints it only when `count > 0` so empty zones stay clean.
    let status_dot = Rect {
        x: rect.right() - PILL_STATUS_DOT_INSET - PILL_STATUS_DOT_SIZE,
        y: rect.y + PILL_STATUS_DOT_INSET,
        width: PILL_STATUS_DOT_SIZE,
        height: PILL_STATUS_DOT_SIZE,
    };
    ZonePillLayout {
        rect,
        shadow_outer,
        shadow_inner,
        icon,
        label,
        badge,
        status_dot,
        radius: BorderRadius::all(RADIUS.capsule),
        badge_radius: BorderRadius::all(RADIUS.badge),
    }
}

/// Smallest badge width that fits `count` digits (plus default-min padding).
pub fn badge_width_for_count(count: usize) -> f32 {
    let digits = digit_count(count);
    let per_digit = TYPOGRAPHY.xs.size_px * 0.62;
    let raw = (digits as f32) * per_digit + SPACING.md;
    raw.max(PILL_BADGE_MIN_WIDTH)
}

/// True when `(x, y)` falls within the pill's hit-test region (the outer
/// `rect`, not the shadow extents).
pub fn pill_hit(layout: &ZonePillLayout, x: f32, y: f32) -> bool {
    rect_contains(layout.rect, x, y)
}

fn digit_count(value: usize) -> u32 {
    if value < 10 {
        1
    } else if value < 100 {
        2
    } else if value < 1000 {
        3
    } else {
        4
    }
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use bento_nano_zone::ZoneId;

    fn fixture(x: i32, y: i32) -> Zone {
        Zone::new(ZoneId(1), Cow::Borrowed("Docs"), x, y, 160, 120)
    }

    #[test]
    fn pill_layout_uses_tauri_capsule_radius() {
        // Wave B SSoT — pill must consume RADIUS.capsule (24) and
        // RADIUS.badge (10); the renderer must not bake other literals.
        let layout = pill_layout_for_zone(&fixture(0, 0), 4);
        assert_eq!(layout.radius, BorderRadius::all(RADIUS.capsule));
        assert_eq!(layout.badge_radius, BorderRadius::all(RADIUS.badge));
    }

    #[test]
    fn pill_layout_anchors_at_zone_origin() {
        let layout = pill_layout_for_zone(&fixture(120, 240), 4);
        assert_eq!(layout.rect.x, 120.0);
        assert_eq!(layout.rect.y, 240.0);
        assert_eq!(layout.rect.height, PILL_HEIGHT);
    }

    #[test]
    fn pill_width_grows_for_larger_count_badges() {
        let small = pill_layout_for_zone(&fixture(0, 0), 4);
        let huge = pill_layout_for_zone(&fixture(0, 0), 999);
        assert!(huge.rect.width >= small.rect.width);
    }

    #[test]
    fn pill_hit_routes_inside_outer_rect() {
        let layout = pill_layout_for_zone(&fixture(100, 100), 4);
        let cx = layout.rect.x + layout.rect.width * 0.5;
        let cy = layout.rect.y + layout.rect.height * 0.5;
        assert!(pill_hit(&layout, cx, cy));
        // Below the pill (in the shadow band but outside the rect) is NOT
        // a hit — empty pixels stay transparent.
        assert!(!pill_hit(&layout, cx, layout.rect.bottom() + 1.0));
        // Far left of the pill is NOT a hit.
        assert!(!pill_hit(&layout, layout.rect.x - 4.0, cy));
    }

    #[test]
    fn pill_icon_label_badge_share_vertical_centerline() {
        let layout = pill_layout_for_zone(&fixture(0, 0), 4);
        let mid = layout.rect.y + layout.rect.height * 0.5;
        let icon_mid = layout.icon.y + layout.icon.height * 0.5;
        let badge_mid = layout.badge.y + layout.badge.height * 0.5;
        assert!((icon_mid - mid).abs() < 0.5);
        assert!((badge_mid - mid).abs() < 0.5);
    }

    #[test]
    fn pill_badge_width_minimum_holds() {
        assert!(badge_width_for_count(0) >= PILL_BADGE_MIN_WIDTH);
        assert!(badge_width_for_count(7) >= PILL_BADGE_MIN_WIDTH);
        assert!(badge_width_for_count(999) >= PILL_BADGE_MIN_WIDTH);
    }

    #[test]
    fn pill_shadow_offsets_match_token_zen() {
        // Wave B tokens::SHADOW.zen.offset_y == 8.0, zen_inner.offset_y == 2.0.
        let layout = pill_layout_for_zone(&fixture(0, 0), 4);
        assert_eq!(layout.shadow_outer.y - layout.rect.y, PILL_SHADOW_OUTER_DY);
        assert_eq!(layout.shadow_inner.y - layout.rect.y, PILL_SHADOW_INNER_DY);
    }

    #[test]
    fn pill_total_width_respects_minimum() {
        let layout = pill_layout_for_zone(&fixture(0, 0), 1);
        assert!(layout.rect.width >= PILL_MIN_WIDTH);
    }

    #[test]
    fn pill_status_dot_sits_at_top_right_inside_pill() {
        // Wave H2 — status dot is a small filled disc anchored near the
        // pill's top-right corner; the renderer paints it only when the
        // zone has at least one item, so geometry must hold for any count.
        let layout = pill_layout_for_zone(&fixture(40, 60), 0);
        assert_eq!(layout.status_dot.width, PILL_STATUS_DOT_SIZE);
        assert_eq!(layout.status_dot.height, PILL_STATUS_DOT_SIZE);
        assert!(layout.status_dot.x >= layout.rect.x);
        assert!(layout.status_dot.right() <= layout.rect.right());
        assert!(layout.status_dot.y >= layout.rect.y);
        assert!(layout.status_dot.bottom() <= layout.rect.bottom());
        let dx_from_right = layout.rect.right() - layout.status_dot.right();
        assert!((dx_from_right - PILL_STATUS_DOT_INSET).abs() < 0.5);
        let dy_from_top = layout.status_dot.y - layout.rect.y;
        assert!((dy_from_top - PILL_STATUS_DOT_INSET).abs() < 0.5);
    }

    #[test]
    fn morph_pill_to_rect_returns_pill_when_morph_zero() {
        let pill = Rect { x: 10.0, y: 10.0, width: 96.0, height: 36.0 };
        let expanded = Rect { x: 10.0, y: 10.0, width: 240.0, height: 180.0 };
        let r = morph_pill_to_rect(pill, expanded, 0.0);
        assert_eq!(r, pill);
    }

    #[test]
    fn morph_pill_to_rect_returns_expanded_when_morph_one() {
        let pill = Rect { x: 10.0, y: 10.0, width: 96.0, height: 36.0 };
        let expanded = Rect { x: 10.0, y: 10.0, width: 240.0, height: 180.0 };
        let r = morph_pill_to_rect(pill, expanded, 1.0);
        assert_eq!(r, expanded);
    }

    #[test]
    fn morph_pill_to_rect_interpolates_componentwise() {
        let pill = Rect { x: 0.0, y: 0.0, width: 100.0, height: 40.0 };
        let expanded = Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        let r = morph_pill_to_rect(pill, expanded, 0.5);
        assert_eq!(r.width, 150.0);
        assert_eq!(r.height, 120.0);
    }

    #[test]
    fn morph_pill_to_rect_clamps_out_of_range_morph() {
        let pill = Rect { x: 0.0, y: 0.0, width: 100.0, height: 40.0 };
        let expanded = Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        assert_eq!(morph_pill_to_rect(pill, expanded, -1.0), pill);
        assert_eq!(morph_pill_to_rect(pill, expanded, 2.0), expanded);
    }

    #[test]
    fn ease_out_cubic_progress_endpoints_and_monotonic() {
        assert!(ease_out_cubic_progress(0.0).abs() < f32::EPSILON);
        assert!((ease_out_cubic_progress(1.0) - 1.0).abs() < f32::EPSILON);
        // Midpoint is past 0.5 due to deceleration curve.
        assert!(ease_out_cubic_progress(0.5) > 0.5);
        // Monotonic across the range.
        let mut prev = -0.001_f32;
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let v = ease_out_cubic_progress(t);
            assert!(v >= prev, "ease_out_cubic must be monotonic ({prev} -> {v})");
            prev = v;
        }
    }

    #[test]
    fn morph_pill_radius_interpolates_between_endpoints() {
        assert_eq!(morph_pill_radius(24.0, 12.0, 0.0), 24.0);
        assert_eq!(morph_pill_radius(24.0, 12.0, 1.0), 12.0);
        let mid = morph_pill_radius(24.0, 12.0, 0.5);
        assert!((mid - 18.0).abs() < 0.001);
    }

    #[test]
    fn pill_anim_duration_within_200ms_cap() {
        // Wave G2 prompt mandates ≤200ms.
        assert!(ZONE_PILL_ANIM_DURATION_MS <= 200);
    }
}
