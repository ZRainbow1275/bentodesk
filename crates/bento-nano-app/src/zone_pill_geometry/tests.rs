//! Unit + state-machine tests for `zone_pill_geometry` (split out of the
//! production module to honour the §15 800-line budget). `super::*` resolves
//! to the parent module so the private bezier solver + `reached` helper stay
//! reachable from these tests.

use super::*;
use crate::business::zen_capsule::CapsuleSize;
use bento_nano_zone::ZoneId;
use std::borrow::Cow;

fn fixture(x: i32, y: i32) -> Zone {
    Zone::new(ZoneId(1), Cow::Borrowed("Docs"), x, y, 160, 120)
}

/// M2② — fixture with an explicit per-zone capsule appearance so the
/// size/shape wiring in `pill_layout_for_zone` can be exercised.
fn fixture_appearance(size: &'static str, shape: &'static str) -> Zone {
    let mut z = fixture(0, 0);
    z.set_capsule_size(size);
    z.set_capsule_shape(shape);
    z
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
fn pill_default_medium_height_is_tauri_48() {
    // M2② — default zone (medium/pill) resolves to Tauri's 48px box height,
    // which is also PILL_HEIGHT (the Medium fallback constant).
    let layout = pill_layout_for_zone(&fixture(0, 0), 4);
    assert!((layout.rect.height - 48.0).abs() < 0.01);
    assert!((layout.rect.height - PILL_HEIGHT).abs() < 0.01);
}

#[test]
fn pill_size_tier_drives_height_and_icon() {
    // M2② — size token → Tauri height (36/48/56) + icon (14/18/22).
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.rect.height - 36.0).abs() < 0.01);
    assert!((medium.rect.height - 48.0).abs() < 0.01);
    assert!((large.rect.height - 56.0).abs() < 0.01);
    // Icon chip side length follows the tier.
    assert!((small.icon.width - 14.0).abs() < 0.01);
    assert!((medium.icon.width - 18.0).abs() < 0.01);
    assert!((large.icon.width - 22.0).abs() < 0.01);
    // Larger tier ⇒ taller pill grows ~33% (36 → 48 is +33.3%).
    assert!(large.rect.height > medium.rect.height);
    assert!(medium.rect.height > small.rect.height);
}

#[test]
fn pill_shape_drives_corner_radius() {
    // M2② — per-shape Tauri border-radius wired into layout.radius.
    let pill = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let rounded = pill_layout_for_zone(&fixture_appearance("medium", "rounded"), 4);
    let minimal = pill_layout_for_zone(&fixture_appearance("medium", "minimal"), 4);
    let square = pill_layout_for_zone(&fixture_appearance("medium", "square"), 4);
    assert_eq!(pill.radius, BorderRadius::all(24.0));
    assert_eq!(rounded.radius, BorderRadius::all(12.0));
    assert_eq!(minimal.radius, BorderRadius::all(8.0));
    assert_eq!(square.radius, BorderRadius::all(4.0));
}

#[test]
fn pill_circle_shape_is_square_disc() {
    // M2② — Tauri circle pill is a 1:1 icon-only disc (aspect-ratio:1 +
    // border-radius:50%). Box is square at the per-tier circle diameter
    // (medium 52) and the radius is height/2 so it reads as a perfect circle.
    let circle = pill_layout_for_zone(&fixture_appearance("medium", "circle"), 4);
    assert!((circle.rect.width - 52.0).abs() < 0.01);
    assert!((circle.rect.height - 52.0).abs() < 0.01);
    assert_eq!(circle.rect.width, circle.rect.height);
    assert_eq!(circle.radius, BorderRadius::all(26.0));
    // Icon is centred inside the disc.
    let icon_cx = circle.icon.x + circle.icon.width * 0.5;
    let icon_cy = circle.icon.y + circle.icon.height * 0.5;
    let disc_cx = circle.rect.x + circle.rect.width * 0.5;
    let disc_cy = circle.rect.y + circle.rect.height * 0.5;
    assert!((icon_cx - disc_cx).abs() < 0.01);
    assert!((icon_cy - disc_cy).abs() < 0.01);
    // Label + badge collapse to zero width (Tauri display:none).
    assert_eq!(circle.label.width, 0.0);
    assert_eq!(circle.badge.width, 0.0);
}

#[test]
fn pill_unknown_appearance_tokens_fall_back_to_medium_pill() {
    // Forward-compat: garbage tokens must not panic — they resolve to the
    // medium/pill default (48px, radius 24).
    let layout = pill_layout_for_zone(&fixture_appearance("xl", "hexagon"), 4);
    assert!((layout.rect.height - 48.0).abs() < 0.01);
    assert_eq!(layout.radius, BorderRadius::all(24.0));
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
    let label_mid = layout.label.y + layout.label.height * 0.5;
    let badge_mid = layout.badge.y + layout.badge.height * 0.5;
    assert!((icon_mid - mid).abs() < 0.5);
    assert!((label_mid - mid).abs() < 0.5);
    assert!((badge_mid - mid).abs() < 0.5);
}

#[test]
fn pill_label_height_tracks_capsule_title_font_tier() {
    // The paint path uses CapsuleSize::title_font_px(); geometry must keep the
    // same line box or small/large labels drift vertically while the DWrite run
    // is drawn at the right size.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);

    assert!((small.label.height - CapsuleSize::Small.title_font_px() * 1.4).abs() < 0.01);
    assert!((medium.label.height - CapsuleSize::Medium.title_font_px() * 1.4).abs() < 0.01);
    assert!((large.label.height - CapsuleSize::Large.title_font_px() * 1.4).abs() < 0.01);

    for layout in [small, medium, large] {
        let mid = layout.rect.y + layout.rect.height * 0.5;
        let label_mid = layout.label.y + layout.label.height * 0.5;
        assert!((label_mid - mid).abs() < 0.01);
    }
}

#[test]
fn pill_badge_width_minimum_holds() {
    assert!(badge_width_for_count(0) >= PILL_BADGE_MIN_WIDTH);
    assert!(badge_width_for_count(7) >= PILL_BADGE_MIN_WIDTH);
    assert!(badge_width_for_count(999) >= PILL_BADGE_MIN_WIDTH);
    // G5 — per-tier variant also respects the floor on every tier.
    for s in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
        assert!(badge_width_for_size_count(s, 0) >= PILL_BADGE_MIN_WIDTH);
        assert!(badge_width_for_size_count(s, 999) >= PILL_BADGE_MIN_WIDTH);
    }
}

#[test]
fn pill_padding_is_per_tier_asymmetric() {
    // G5 — icon left-anchored at the tier's LEFT padding (Tauri --spacing-xl
    // medium = 20, small 12, large 28); badge right-anchored at the tier's
    // RIGHT padding (medium 16, small 12, large 20).
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    // icon.x - rect.x == pad_left
    assert!((small.icon.x - small.rect.x - 12.0).abs() < 0.01);
    assert!((medium.icon.x - medium.rect.x - 20.0).abs() < 0.01);
    assert!((large.icon.x - large.rect.x - 28.0).abs() < 0.01);
    // rect.right() - badge.right() == pad_right
    assert!((small.rect.right() - small.badge.right() - 12.0).abs() < 0.01);
    assert!((medium.rect.right() - medium.badge.right() - 16.0).abs() < 0.01);
    assert!((large.rect.right() - large.badge.right() - 20.0).abs() < 0.01);
}

#[test]
fn pill_inner_gap_is_per_tier() {
    // G5 — gap between icon and label equals the tier inner-gap (small 8,
    // medium 12, large 16), NOT the pre-G5 flat 6.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.label.x - (small.icon.x + small.icon.width) - 8.0).abs() < 0.01);
    assert!((medium.label.x - (medium.icon.x + medium.icon.width) - 12.0).abs() < 0.01);
    assert!((large.label.x - (large.icon.x + large.icon.width) - 16.0).abs() < 0.01);
}

#[test]
fn pill_badge_height_is_per_tier() {
    // G5 — badge box height scales 14/16/20 per tier (was flat 20).
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.badge.height - 14.0).abs() < 0.01);
    assert!((medium.badge.height - 16.0).abs() < 0.01);
    assert!((large.badge.height - 20.0).abs() < 0.01);
}

#[test]
fn pill_circle_icon_uses_circle_override_size() {
    // G5 — circle icon uses the circle-only override (22 small+medium, 28
    // large), NOT the base per-tier icon_px (14/18/22).
    let small = pill_layout_for_zone(&fixture_appearance("small", "circle"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "circle"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "circle"), 4);
    assert!((small.icon.width - 22.0).abs() < 0.01);
    assert!((medium.icon.width - 22.0).abs() < 0.01);
    assert!((large.icon.width - 28.0).abs() < 0.01);
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
    let pill = Rect {
        x: 10.0,
        y: 10.0,
        width: 96.0,
        height: 36.0,
    };
    let expanded = Rect {
        x: 10.0,
        y: 10.0,
        width: 240.0,
        height: 180.0,
    };
    let r = morph_pill_to_rect(pill, expanded, 0.0);
    assert_eq!(r, pill);
}

#[test]
fn morph_pill_to_rect_returns_expanded_when_morph_one() {
    let pill = Rect {
        x: 10.0,
        y: 10.0,
        width: 96.0,
        height: 36.0,
    };
    let expanded = Rect {
        x: 10.0,
        y: 10.0,
        width: 240.0,
        height: 180.0,
    };
    let r = morph_pill_to_rect(pill, expanded, 1.0);
    assert_eq!(r, expanded);
}

#[test]
fn morph_pill_to_rect_interpolates_componentwise() {
    let pill = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 40.0,
    };
    let expanded = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 200.0,
    };
    let r = morph_pill_to_rect(pill, expanded, 0.5);
    assert_eq!(r.width, 150.0);
    assert_eq!(r.height, 120.0);
}

#[test]
fn morph_pill_to_rect_clamps_negative_but_allows_overshoot() {
    // M3 — lower bound pins to the pill; upper bound is intentionally NOT
    // clamped so the easeOutBack overshoot (morph > 1.0) can extrapolate
    // the rect past the expanded target mid-flight.
    let pill = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 40.0,
    };
    let expanded = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 200.0,
    };
    assert_eq!(morph_pill_to_rect(pill, expanded, -1.0), pill);
    // morph = 1.1 → width extrapolates 10% past expanded (100 + 1.1*100).
    let over = morph_pill_to_rect(pill, expanded, 1.1);
    assert!((over.width - 210.0).abs() < 0.01, "got {}", over.width);
}

#[test]
fn current_morph_rect_matches_old_inline_formula() {
    // #2 step 8 (2026-06-02) — `current_morph_rect` is the single SSoT for the
    // raw→easeOutBack→(flip)→morph_pill_to_rect math that the paint path and
    // BOTH hit-geometry sites (effective_zone_chrome_rect /
    // effective_zone_hit_rect) used to inline. Pin it bit-identical to that
    // former inline formula at several `t`, in both directions, so paint == hit
    // can never drift.
    let pill = Rect {
        x: 10.0,
        y: 20.0,
        width: 96.0,
        height: 36.0,
    };
    let expanded = Rect {
        x: 10.0,
        y: 20.0,
        width: 320.0,
        height: 240.0,
    };
    for &raw in &[0.05_f32, 0.25, 0.5, 0.75, 0.95] {
        for &expanding in &[true, false] {
            // Old inline formula (verbatim from the pre-consolidation sites).
            let eased = ease_out_back_progress(raw);
            let morph_old = if expanding { eased } else { 1.0 - eased };
            let rect_old = morph_pill_to_rect(pill, expanded, morph_old);
            // New shared helper.
            let (morph_new, rect_new) = current_morph_rect(pill, expanded, raw, expanding);
            assert_eq!(
                morph_new, morph_old,
                "morph drift @raw={raw} exp={expanding}"
            );
            assert_eq!(rect_new.x, rect_old.x);
            assert_eq!(rect_new.y, rect_old.y);
            assert_eq!(rect_new.width, rect_old.width);
            assert_eq!(rect_new.height, rect_old.height);
        }
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
fn pill_anim_duration_matches_tauri_spring_expand() {
    // M3 (2026-05-29) — Tauri `.spring-expand` animates width/height/--rad
    // over 0.5s (`animations.css:41-43`). The pre-M3 160ms value was a
    // stand-in; the live morph is now 1:1 with the Tauri transition.
    assert_eq!(ZONE_PILL_ANIM_DURATION_MS, 500);
}

// --- M3 easeOutBack cubic-bezier solver --------------------------------

#[test]
fn ease_out_back_endpoints_are_exact() {
    // Must land on EXACTLY 0.0 and 1.0 — no Newton residual at the
    // boundaries (the settle has to hit the token target precisely).
    assert_eq!(ease_out_back_progress(0.0), 0.0);
    assert_eq!(ease_out_back_progress(1.0), 1.0);
    // Out-of-range clamps to the endpoints.
    assert_eq!(ease_out_back_progress(-0.5), 0.0);
    assert_eq!(ease_out_back_progress(2.0), 1.0);
}

#[test]
fn ease_out_back_overshoots_past_one_midflight() {
    // cubic-bezier(0.34,1.56,0.64,1) bulges ~10% past 1.0 before settling.
    // The peak sits around the input region 0.6..0.85.
    let mut peak = 0.0_f32;
    let mut i = 0;
    while i <= 100 {
        let v = ease_out_back_progress(i as f32 / 100.0);
        if v > peak {
            peak = v;
        }
        i += 1;
    }
    // Overshoot present and in the ~5-15% band (10% nominal).
    assert!(peak > 1.05, "expected overshoot > 1.05, got {peak}");
    assert!(peak < 1.20, "overshoot unexpectedly large: {peak}");
}

#[test]
fn ease_out_back_pinned_samples() {
    // Sampled progress at the report checkpoints. The curve front-loads
    // hard, peaks ~1.098 around t≈0.573, then settles EXACTLY to 1.0.
    // Exact values verified against the reference solver:
    //   t=0.00 → 0.000000   t=0.25 → 0.816289   t=0.50 → 1.087401
    //   t=0.70 → 1.075776   t=1.00 → 1.000000
    assert_eq!(ease_out_back_progress(0.0), 0.0);
    assert_eq!(ease_out_back_progress(1.0), 1.0);
    assert!((ease_out_back_progress(0.25) - 0.816_289).abs() < 1e-3);
    assert!((ease_out_back_progress(0.5) - 1.087_401).abs() < 1e-3);
    assert!((ease_out_back_progress(0.7) - 1.075_776).abs() < 1e-3);
    // The 0.5 and 0.7 samples sit ABOVE 1.0 — the overshoot zone.
    assert!(ease_out_back_progress(0.5) > 1.0);
    assert!(ease_out_back_progress(0.7) > 1.0);
}

#[test]
fn ease_out_back_x_inversion_round_trips() {
    // bezier_solve_x must invert bezier_axis on the x-axis to f32 epsilon.
    let mut i = 0;
    while i <= 20 {
        let x = i as f32 / 20.0;
        let u = bezier_solve_x(x, BEZIER_P1X, BEZIER_P2X);
        let back = bezier_axis(u, BEZIER_P1X, BEZIER_P2X);
        assert!((back - x).abs() < 1e-4, "x={x} round-tripped to {back}");
        i += 1;
    }
}

#[test]
fn morph_pill_to_rect_with_back_curve_overshoots_then_settles() {
    // The overshoot WILL grow the rect past the expanded target mid-flight
    // (correct — Tauri does it) but settle EXACTLY on target at t=1.
    let pill = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 40.0,
    };
    let expanded = Rect {
        x: 0.0,
        y: 0.0,
        width: 300.0,
        height: 200.0,
    };
    // Mid-flight overshoot: find the largest interpolated width.
    let mut max_w = 0.0_f32;
    let mut i = 0;
    while i <= 100 {
        let p = ease_out_back_progress(i as f32 / 100.0);
        let r = morph_pill_to_rect(pill, expanded, p);
        if r.width > max_w {
            max_w = r.width;
        }
        i += 1;
    }
    assert!(
        max_w > expanded.width,
        "expected width overshoot, got {max_w}"
    );
    // Settle is exact at t=1.
    let settled = morph_pill_to_rect(pill, expanded, ease_out_back_progress(1.0));
    assert_eq!(settled, expanded);
}

// --- B2 CSS-standard `ease` color curve --------------------------------

#[test]
fn ease_standard_endpoints_exact_and_no_overshoot() {
    // Endpoints land on EXACTLY 0.0 / 1.0 (no Newton residual); out-of-range
    // clamps to the endpoints.
    assert_eq!(ease_standard_progress(0.0), 0.0);
    assert_eq!(ease_standard_progress(1.0), 1.0);
    assert_eq!(ease_standard_progress(-0.5), 0.0);
    assert_eq!(ease_standard_progress(2.0), 1.0);
    // Unlike the easeOutBack size curve, the CSS `ease` color curve must NEVER
    // overshoot past 1.0 (P1.y=0.1, P2.y=1.0 — both <= 1).
    let mut i = 0;
    while i <= 100 {
        assert!(ease_standard_progress(i as f32 / 100.0) <= 1.0 + 1e-6);
        i += 1;
    }
}

#[test]
fn ease_standard_pinned_samples_and_monotonic() {
    // cubic-bezier(0.25, 0.1, 0.25, 1) sampled against the reference solver:
    //   t=0.25 → 0.408511   t=0.50 → 0.802403   t=0.75 → 0.960459
    assert!((ease_standard_progress(0.25) - 0.408_511).abs() < 1e-3);
    assert!((ease_standard_progress(0.50) - 0.802_403).abs() < 1e-3);
    assert!((ease_standard_progress(0.75) - 0.960_459).abs() < 1e-3);
    // Monotone non-decreasing across the range.
    let mut prev = -0.001_f32;
    let mut i = 0;
    while i <= 100 {
        let v = ease_standard_progress(i as f32 / 100.0);
        assert!(
            v >= prev - 1e-6,
            "ease_standard must be monotonic ({prev} -> {v})"
        );
        prev = v;
        i += 1;
    }
}

// --- A3 HoverScheduler grace state machine -----------------------------

const EXPAND_DELAY: u32 = 150;
const COLLAPSE_DELAY: u32 = 300;

fn zid(n: u64) -> ZoneId {
    ZoneId(n)
}

#[test]
fn enter_arms_expand_intent_and_fires_after_delay() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    assert!(s.is_pending());
    // Before the delay elapses — nothing fires, zone not yet expanded.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY - 1), HoverAction::None);
    assert_eq!(s.expanded_zone(), None);
    // At the deadline — expand fires exactly once.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::Expand(zid(1)));
    assert_eq!(s.expanded_zone(), Some(zid(1)));
    // Subsequent polls don't re-fire.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY + 50), HoverAction::None);
}

#[test]
fn leave_before_expand_clears_intent() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    // Cursor leaves before the intent elapses.
    s.on_leave(1_000 + EXPAND_DELAY / 2, COLLAPSE_DELAY, true);
    // No expand should ever fire — there was nothing expanded to collapse.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::None);
    assert_eq!(s.poll(10_000), HoverAction::None);
    assert_eq!(s.expanded_zone(), None);
    assert!(!s.is_pending());
}

#[test]
fn expand_sets_lock_window() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::Expand(zid(1)));
    // Leave immediately after expand — collapse must defer to the lock,
    // NOT fire at now + collapse_delay (which is earlier than the lock).
    let leave = 1_000 + EXPAND_DELAY; // == expand tick
    s.on_leave(leave, COLLAPSE_DELAY, true);
    // collapse_delay (300) < EXPAND_LOCK_MS (550), so the lock wins.
    let lock_until = leave + EXPAND_LOCK_MS;
    // At now + collapse_delay the lock has NOT elapsed — no collapse yet.
    assert_eq!(s.poll(leave + COLLAPSE_DELAY), HoverAction::None);
    // At the lock deadline the collapse fires.
    assert_eq!(s.poll(lock_until), HoverAction::Collapse(zid(1)));
    assert_eq!(s.expanded_zone(), None);
}

#[test]
fn leave_during_lock_defers_collapse_to_lock_until() {
    let mut s = HoverScheduler::new();
    // Force-expand at t=2000 so the lock window is [2000, 2550].
    s.mark_expanded(zid(7), 2_000);
    assert_eq!(s.expanded_zone(), Some(zid(7)));
    // Leave at t=2100 (inside the lock). base = 2100+300 = 2400 < 2550.
    s.on_leave(2_100, COLLAPSE_DELAY, true);
    // 2400 is inside the lock — must NOT collapse.
    assert_eq!(s.poll(2_400), HoverAction::None);
    // 2550 (lock_until) — collapse fires.
    assert_eq!(
        s.poll(2_000 + EXPAND_LOCK_MS),
        HoverAction::Collapse(zid(7))
    );
}

#[test]
fn leave_after_lock_sets_collapse_at_now_plus_delay() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(3), 1_000); // lock [1000, 1550]
    // Leave well after the lock has expired (t=5000). base = 5300.
    s.on_leave(5_000, COLLAPSE_DELAY, true);
    // Before now+delay — nothing.
    assert_eq!(s.poll(5_000 + COLLAPSE_DELAY - 1), HoverAction::None);
    // At now+delay — collapse fires (lock long gone, so base wins).
    assert_eq!(
        s.poll(5_000 + COLLAPSE_DELAY),
        HoverAction::Collapse(zid(3))
    );
}

#[test]
fn reenter_before_collapse_cancels_grace() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(2), 5_000); // lock expires at 5550
    s.on_leave(6_000, COLLAPSE_DELAY, true); // collapse pending at 6300
    assert!(s.is_pending());
    // Cursor re-enters the same zone before the grace elapses.
    s.on_enter(zid(2), 6_100, EXPAND_DELAY);
    // Collapse must be cancelled — no Collapse ever fires.
    assert_eq!(s.poll(6_300), HoverAction::None);
    assert_eq!(s.poll(10_000), HoverAction::None);
    // Zone stays expanded.
    assert_eq!(s.expanded_zone(), Some(zid(2)));
}

#[test]
fn always_mode_leave_does_not_collapse() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(4), 1_000);
    // auto_collapse = false (ALWAYS display mode): leave is a no-op for
    // the collapse path (mirrors Tauri BentoZone.tsx:589).
    s.on_leave(5_000, COLLAPSE_DELAY, false);
    assert_eq!(s.poll(5_000 + COLLAPSE_DELAY), HoverAction::None);
    assert_eq!(s.poll(100_000), HoverAction::None);
    assert_eq!(s.expanded_zone(), Some(zid(4)));
}

#[test]
fn reset_clears_all_pending_and_expanded() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 0, EXPAND_DELAY);
    s.mark_expanded(zid(1), 0);
    s.reset();
    assert!(!s.is_pending());
    assert_eq!(s.expanded_zone(), None);
    assert_eq!(s.poll(100_000), HoverAction::None);
}

#[test]
fn reached_handles_tick_wraparound() {
    // Deadline just before the u32 wrap; "now" just after — reached.
    let deadline = u32::MAX - 10;
    let now = 5_u32; // wrapped past the deadline by 15ms
    assert!(reached(now, deadline));
    // Now still before the deadline — not reached.
    assert!(!reached(deadline - 100, deadline));
}
