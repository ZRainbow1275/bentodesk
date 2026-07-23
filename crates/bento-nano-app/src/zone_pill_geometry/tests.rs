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
    // Small/medium keep the baseline; Large is compacted to 50 DIPs after
    // hand-test feedback while preserving its 200-DIP width.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.rect.height - 36.0).abs() < 0.01);
    assert!((medium.rect.height - 48.0).abs() < 0.01);
    assert!((large.rect.height - 50.0).abs() < 0.01);
    // Icon chip side length follows Tauri's fixed ZoneIcon wrapper.
    assert!((small.icon.width - 18.0).abs() < 0.01);
    assert!((medium.icon.width - 18.0).abs() < 0.01);
    assert!((large.icon.width - 18.0).abs() < 0.01);
    // Size tiers remain ordered even though Large no longer looks oversized.
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
    assert_eq!(square.radius, BorderRadius::ZERO);
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
fn pill_width_matches_tauri_capsule_tiers() {
    // Tauri `hitTest.ts` / `StackWrapper.tsx` — the outer capsule box is fixed
    // at 120 / 160 / 200 DIPs and content fits inside rather than expanding it.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.rect.width - CapsuleSize::Small.width_px()).abs() < 0.01);
    assert!((medium.rect.width - CapsuleSize::Medium.width_px()).abs() < 0.01);
    assert!((large.rect.width - CapsuleSize::Large.width_px()).abs() < 0.01);
    assert_eq!(small.rect.width, 120.0);
    assert_eq!(medium.rect.width, 160.0);
    assert_eq!(large.rect.width, 200.0);
}

#[test]
fn pill_width_stays_fixed_when_badge_count_grows() {
    let one_digit = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let three_digits = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 999);
    assert_eq!(three_digits.rect.width, one_digit.rect.width);
    assert!(three_digits.badge.width >= one_digit.badge.width);
    assert!(three_digits.label.width <= one_digit.label.width);
    assert!(three_digits.badge.right() <= three_digits.rect.right());
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
fn pill_content_is_centered_across_sizes_and_glyph_profiles() {
    for size in ["small", "medium", "large"] {
        for icon in ["copy", "code", "none"] {
            let mut zone = fixture_appearance(size, "pill");
            zone.title = Cow::Borrowed("Benchmark Zone 4");
            zone.set_icon(Cow::Borrowed(icon));
            let layout = pill_layout_for_zone(&zone, 10);
            let mid = layout.rect.y + layout.rect.height * 0.5;
            for slot_mid in [
                layout.icon.y + layout.icon.height * 0.5,
                layout.label.y + layout.label.height * 0.5,
                layout.badge.y + layout.badge.height * 0.5,
            ] {
                assert!((slot_mid - mid).abs() < 0.01, "size={size} icon={icon}");
            }
        }
    }
}

#[test]
fn pill_label_height_tracks_capsule_title_font_tier() {
    for (wire, size) in [
        ("small", CapsuleSize::Small),
        ("medium", CapsuleSize::Medium),
        ("large", CapsuleSize::Large),
    ] {
        let layout = pill_layout_for_zone(&fixture_appearance(wire, "pill"), 4);
        assert!((layout.label.height - size.title_font_px() * 1.4).abs() < 0.01);
        assert!((size.title_font_px() - 13.0).abs() < 0.01);
        let mid = layout.rect.y + layout.rect.height * 0.5;
        assert!((layout.label.y + layout.label.height * 0.5 - mid).abs() < 0.01);
    }
}

#[test]
fn pill_title_role_is_stable_across_size_glyph_and_label_content() {
    for size in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
        for has_glyph in [false, true] {
            assert!((pill_title_font_px_for(size, has_glyph) - 13.0).abs() < 0.01);
            for title in ["ai", "浏览器", "Benchmark Zone 4"] {
                assert!((pill_title_font_px_for_text(size, has_glyph, title) - 13.0).abs() < 0.01);
            }
            assert!((pill_title_tracking_px_for(size, has_glyph) - 0.3).abs() < 0.01);
            assert!((pill_title_alpha_for(size, has_glyph) - 1.0).abs() < 0.001);
        }
    }
}

#[test]
fn pill_short_and_long_titles_share_one_readable_line_box() {
    let mut ai = fixture_appearance("large", "pill");
    ai.title = Cow::Borrowed("ai");
    ai.set_icon(Cow::Borrowed("code"));

    let short = pill_layout_for_zone(&ai, 8);
    ai.title = Cow::Borrowed("Benchmark Zone 4");
    let long = pill_layout_for_zone(&ai, 8);

    assert!((short.label.height - 13.0 * 1.4).abs() < 0.01);
    assert_eq!(short.label.height, long.label.height);
}

#[test]
fn pill_layout_uses_display_alias_for_title_metrics() {
    let mut zone = fixture_appearance("large", "pill");
    zone.title = Cow::Borrowed("Compiler");
    zone.set_icon(Cow::Borrowed("code"));
    let canonical = pill_layout_for_zone(&zone, 4);

    zone.set_alias(Some(Cow::Borrowed("浏览器")));
    let aliased = pill_layout_for_zone(&zone, 4);

    assert_eq!(zone.display_title(), "浏览器");
    assert_eq!(aliased.label.height, canonical.label.height);
}

#[test]
fn pill_large_long_ascii_code_glyph_keeps_reference_content_profile() {
    let mut compiler = fixture_appearance("large", "pill");
    compiler.title = Cow::Borrowed("Compiler");
    compiler.set_icon(Cow::Borrowed("code"));
    let compiler = pill_layout_for_zone(&compiler, 4);

    assert!(icon_name_has_visible_glyph("code"));
    assert!(!pill_uses_visible_glyph_content_metrics(
        CapsuleSize::Large,
        "code",
        "Compiler"
    ));
    assert!(pill_uses_visible_glyph_content_metrics(
        CapsuleSize::Large,
        "code",
        "ai"
    ));
    assert!(pill_uses_visible_glyph_content_metrics(
        CapsuleSize::Large,
        "copy",
        "浏览器"
    ));
    assert!(pill_uses_visible_glyph_content_metrics(
        CapsuleSize::Medium,
        "code",
        "Compiler"
    ));
    assert!(pill_uses_visible_glyph_content_metrics(
        CapsuleSize::Large,
        "folder",
        "Docs"
    ));

    let pill_mid = compiler.rect.y + compiler.rect.height * 0.5;
    assert!((compiler.icon.width - 18.0).abs() < 0.01);
    assert!(
        (compiler.icon.x - compiler.rect.x - PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX).abs() < 0.01
    );
    assert!(
        (compiler.label.x
            - compiler.rect.x
            - (PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX
                + 18.0
                + PILL_LARGE_VISIBLE_GLYPH_INNER_GAP_PX))
            .abs()
            < 0.01
    );
    assert!((compiler.icon.y + compiler.icon.height * 0.5 - pill_mid).abs() < 0.01);
    assert!((compiler.label.y + compiler.label.height * 0.5 - pill_mid).abs() < 0.01);
    assert!((compiler.badge.y + compiler.badge.height * 0.5 - pill_mid).abs() < 0.01);
    assert!((compiler.label.height - CapsuleSize::Large.title_font_px() * 1.4).abs() < 0.01);
    assert!((compiler.badge.height - PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX).abs() < 0.01);
    assert!(
        (compiler.rect.right() - compiler.badge.right() - PILL_LARGE_NO_GLYPH_BADGE_RIGHT_INSET_PX)
            .abs()
            < 0.01
    );
    assert!(
        (compiler.badge.width - badge_width_for_size_count(CapsuleSize::Large, 4)).abs() < 0.01,
        "a visible code glyph must not inherit the no-glyph badge width expansion"
    );
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
    // medium = 20, small 12). V21-C21 keeps the source large token at 28 in
    // `CapsuleSize::pad_lr_px`, but visible-glyph large capsules use the
    // video-observed 21-DIP slot so Browser's left icon/title run matches the
    // 2026-06-02 component crop. Badge remains right-anchored to the tier's
    // RIGHT padding (medium 16, small 12). C27 keeps the source large right
    // token at 20, but anchors rendered Large badges to video-observed insets:
    // Browser and the source-tier Large profile need different small moves.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    let mut large_no_glyph_zone = fixture_appearance("large", "pill");
    large_no_glyph_zone.set_icon(Cow::Borrowed("none"));
    let large_no_glyph = pill_layout_for_zone(&large_no_glyph_zone, 4);
    // icon.x - rect.x == pad_left
    assert!((small.icon.x - small.rect.x - 12.0).abs() < 0.01);
    assert!((medium.icon.x - medium.rect.x - 20.0).abs() < 0.01);
    assert!((large.icon.x - large.rect.x - PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX).abs() < 0.01);
    // rect.right() - badge.right() == pad_right
    assert!((small.rect.right() - small.badge.right() - 12.0).abs() < 0.01);
    assert!((medium.rect.right() - medium.badge.right() - 16.0).abs() < 0.01);
    assert!(
        (large.rect.right() - large.badge.right() - PILL_LARGE_VISIBLE_GLYPH_BADGE_RIGHT_INSET_PX)
            .abs()
            < 0.01
    );
    assert!(
        (large_no_glyph.rect.right()
            - large_no_glyph.badge.right()
            - PILL_LARGE_NO_GLYPH_BADGE_RIGHT_INSET_PX)
            .abs()
            < 0.01
    );
}

#[test]
fn pill_inner_gap_is_per_tier() {
    // G5 — gap between icon and label equals the tier inner-gap (small 8,
    // medium 12). C22 keeps the source large token at 16, but visible-glyph
    // large capsules use the video-observed 11-DIP gap so Browser's icon/title
    // spacing matches the 2026-06-02 component crop.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    assert!((small.label.x - (small.icon.x + small.icon.width) - 8.0).abs() < 0.01);
    assert!((medium.label.x - (medium.icon.x + medium.icon.width) - 12.0).abs() < 0.01);
    assert!(
        (large.label.x - (large.icon.x + large.icon.width) - PILL_LARGE_VISIBLE_GLYPH_INNER_GAP_PX)
            .abs()
            < 0.01
    );
}

#[test]
fn pill_none_icon_keeps_video_observed_slot_without_painting_glyph() {
    // Explicit `""` / `"none"` wire values retain a residual layout slot while
    // suppressing glyph paint in `draw_icon_glyph`. N187 proved the reference
    // Compiler itself is not such a value; this test protects only the generic
    // persisted no-glyph fallback.
    let mut with_icon = fixture_appearance("large", "pill");
    with_icon.set_icon(Cow::Borrowed("folder"));
    let mut no_icon = fixture_appearance("large", "pill");
    no_icon.set_icon(Cow::Borrowed("none"));

    let with_icon = pill_layout_for_zone(&with_icon, 4);
    let no_icon = pill_layout_for_zone(&no_icon, 4);

    assert!(icon_name_has_visible_glyph("folder"));
    assert!(!icon_name_has_visible_glyph(""));
    assert!(!icon_name_has_visible_glyph("none"));
    assert_eq!(no_icon.rect, with_icon.rect);
    assert!(
        (with_icon.icon.x - with_icon.rect.x - PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX).abs() < 0.01
    );
    assert!((no_icon.icon.x - no_icon.rect.x - 28.0).abs() < 0.01);
    assert!(
        (with_icon.label.x
            - (with_icon.icon.x + with_icon.icon.width)
            - PILL_LARGE_VISIBLE_GLYPH_INNER_GAP_PX)
            .abs()
            < 0.01
    );
    assert!((no_icon.icon.width - PILL_NO_GLYPH_ICON_SLOT_PX).abs() < 0.01);
    assert!((with_icon.icon.width - 18.0).abs() < 0.01);
    assert!(
        (with_icon.badge.x
            - no_icon.badge.x
            - (PILL_LARGE_NO_GLYPH_BADGE_RIGHT_INSET_PX
                - PILL_LARGE_VISIBLE_GLYPH_BADGE_RIGHT_INSET_PX
                + PILL_LARGE_NO_GLYPH_BADGE_WIDTH_EXTRA_PX))
            .abs()
            < 0.01
    );
    assert!(
        (no_icon.badge.width - with_icon.badge.width - PILL_LARGE_NO_GLYPH_BADGE_WIDTH_EXTRA_PX)
            .abs()
            < 0.01
    );
    assert!((with_icon.badge.height - PILL_LARGE_VISIBLE_GLYPH_BADGE_HEIGHT_PX).abs() < 0.01);
    assert!((no_icon.badge.height - PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX).abs() < 0.01);
    let expected_badge_y_delta =
        (PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX - PILL_LARGE_VISIBLE_GLYPH_BADGE_HEIGHT_PX) * 0.5;
    assert!((with_icon.badge.y - no_icon.badge.y - expected_badge_y_delta).abs() < 0.01);
    assert!(no_icon.label.x > no_icon.rect.x + 28.0 + 16.0);
    assert!((no_icon.label.x - with_icon.label.x).abs() < 0.01);
    assert!(
        (no_icon.label.x - no_icon.rect.x - (28.0 + PILL_NO_GLYPH_ICON_SLOT_PX + 16.0)).abs()
            < 0.01
    );
}

#[test]
fn pill_badge_height_is_per_tier() {
    // C28 — small/medium keep source metrics. Large visible-glyph Browser uses
    // the 16-DIP video-observed span, while the source-tier profile keeps the
    // C23 17-DIP span reused by N188.
    let small = pill_layout_for_zone(&fixture_appearance("small", "pill"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "pill"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "pill"), 4);
    let mut large_no_glyph_zone = fixture_appearance("large", "pill");
    large_no_glyph_zone.set_icon(Cow::Borrowed("none"));
    let large_no_glyph = pill_layout_for_zone(&large_no_glyph_zone, 4);
    assert!((small.badge.height - 14.0).abs() < 0.01);
    assert!((medium.badge.height - 16.0).abs() < 0.01);
    assert!((large.badge.height - PILL_LARGE_VISIBLE_GLYPH_BADGE_HEIGHT_PX).abs() < 0.01);
    assert!((large_no_glyph.badge.height - PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX).abs() < 0.01);
    assert!((pill_badge_height_for(CapsuleSize::Large, true) - 16.0).abs() < 0.01);
    assert!((pill_badge_height_for(CapsuleSize::Large, false) - 17.0).abs() < 0.01);
    assert!(
        (pill_badge_width_for_size_count(CapsuleSize::Large, false, 4)
            - badge_width_for_size_count(CapsuleSize::Large, 4)
            - PILL_LARGE_NO_GLYPH_BADGE_WIDTH_EXTRA_PX)
            .abs()
            < 0.01
    );
    assert!(
        (pill_badge_width_for_size_count(CapsuleSize::Large, true, 4)
            - badge_width_for_size_count(CapsuleSize::Large, 4))
        .abs()
            < 0.01
    );
}

#[test]
fn pill_circle_icon_uses_fixed_tauri_zone_icon_size() {
    // V21-C4 — the actual Tauri circle icon box remains the fixed
    // `ZoneIcon size={18}` wrapper; CSS font-size overrides do not resize it.
    let small = pill_layout_for_zone(&fixture_appearance("small", "circle"), 4);
    let medium = pill_layout_for_zone(&fixture_appearance("medium", "circle"), 4);
    let large = pill_layout_for_zone(&fixture_appearance("large", "circle"), 4);
    assert!((small.icon.width - 18.0).abs() < 0.01);
    assert!((medium.icon.width - 18.0).abs() < 0.01);
    assert!((large.icon.width - 18.0).abs() < 0.01);
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
fn pill_layout_exposes_only_tauri_visible_slots() {
    // V21-C1 — the old Wave H2 status-dot geometry drifted from the current
    // renderer and from the ZenCapsule snap: collapsed pills paint icon, title,
    // and count badge only. Keep the layout contract aligned with paint/hit.
    let layout = pill_layout_for_zone(&fixture(40, 60), 4);
    assert!(layout.icon.width > 0.0);
    assert!(layout.label.width > 0.0);
    assert!(layout.badge.width > 0.0);
    assert!(layout.icon.x >= layout.rect.x);
    assert!(layout.badge.right() <= layout.rect.right());
}

#[test]
fn stack_capsule_layout_matches_tauri_stack_capsule_grid() {
    // V21-C5 — Tauri `StackCapsule.css` is not the ordinary 160x48 ZenCapsule:
    // it is a 220x52 grid with member peeks, a 28px main icon bubble, title,
    // and a 24px badge.
    let layout = stack_capsule_layout_for_zone(&fixture(40, 60), 4);

    assert!((layout.rect.x - 40.0).abs() < 0.01);
    assert!((layout.rect.y - 60.0).abs() < 0.01);
    assert!((layout.rect.width - STACK_CAPSULE_WIDTH_PX).abs() < 0.01);
    assert!((layout.rect.height - STACK_CAPSULE_HEIGHT_PX).abs() < 0.01);
    assert_eq!(layout.radius, BorderRadius::all(STACK_CAPSULE_RADIUS_PX));
    assert_eq!(layout.peek_visible_count, STACK_CAPSULE_MAX_PEEK_ICONS);

    assert!((layout.peek_icons[0].x - (layout.rect.x + STACK_CAPSULE_PAD_X_PX)).abs() < 0.01);
    assert!((layout.peek_icons[1].x - layout.peek_icons[0].x - 14.0).abs() < 0.01);
    assert!((layout.peek_icons[2].x - layout.peek_icons[1].x - 14.0).abs() < 0.01);
    for peek in layout.peek_icons {
        assert!((peek.width - STACK_CAPSULE_PEEK_ICON_SIZE_PX).abs() < 0.01);
        assert!((peek.height - STACK_CAPSULE_PEEK_ICON_SIZE_PX).abs() < 0.01);
    }

    assert!((layout.icon_bubble.width - STACK_CAPSULE_MAIN_ICON_BUBBLE_PX).abs() < 0.01);
    assert!((layout.icon_bubble.height - STACK_CAPSULE_MAIN_ICON_BUBBLE_PX).abs() < 0.01);
    assert!((layout.icon_glyph.width - STACK_CAPSULE_MAIN_ICON_GLYPH_PX).abs() < 0.01);
    assert!((layout.icon_glyph.height - STACK_CAPSULE_MAIN_ICON_GLYPH_PX).abs() < 0.01);
    assert!(layout.label.x > layout.icon_bubble.right());
    assert!(layout.label.right() <= layout.badge.x - STACK_CAPSULE_GAP_PX + 0.01);
    assert!((layout.badge.height - STACK_CAPSULE_BADGE_HEIGHT_PX).abs() < 0.01);
    assert!(layout.badge.width >= STACK_CAPSULE_BADGE_MIN_WIDTH_PX);
    assert!((layout.rect.right() - layout.badge.right() - STACK_CAPSULE_PAD_X_PX).abs() < 0.01);
}

#[test]
fn stack_capsule_peek_icons_cap_at_last_three_members() {
    let two = stack_capsule_layout_for_zone(&fixture(0, 0), 2);
    let many = stack_capsule_layout_for_zone(&fixture(0, 0), 8);

    assert_eq!(two.peek_visible_count, 2);
    assert_eq!(many.peek_visible_count, STACK_CAPSULE_MAX_PEEK_ICONS);
    assert!(many.icon_bubble.x > two.icon_bubble.x);
    assert!(many.label.width < two.label.width);
    assert_eq!(two.peek_icons[2].width, 0.0);
    assert!(many.peek_icons[2].width > 0.0);
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
fn current_morph_rect_matches_shared_geometry_timeline_formula() {
    // #2 step 8 (2026-06-02) — `current_morph_rect` is the single SSoT for the
    // content-raw→geometry-raw→monotonic-ease-out→morph_pill_to_rect math used
    // by the paint path and BOTH hit-geometry sites. Pin it bit-identical to
    // that shared formula at several `t`, in both directions, so paint == hit
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
            let eased = ease_out_progress(pill_geometry_progress(raw));
            let from_morph = if expanding { 0.0 } else { 1.0 };
            let target = if expanding { 1.0 } else { 0.0 };
            let morph_expected = from_morph + (target - from_morph) * eased;
            let rect_expected = morph_pill_to_rect(pill, expanded, morph_expected);
            let (morph_actual, rect_actual) =
                current_morph_rect(pill, expanded, from_morph, raw, expanding);
            assert_eq!(
                morph_actual, morph_expected,
                "morph drift @raw={raw} exp={expanding}"
            );
            assert_eq!(rect_actual.x, rect_expected.x);
            assert_eq!(rect_actual.y, rect_expected.y);
            assert_eq!(rect_actual.width, rect_expected.width);
            assert_eq!(rect_actual.height, rect_expected.height);
        }
    }
}

#[test]
fn pill_geometry_and_content_share_fast_release_envelope() {
    assert_eq!(ZONE_PILL_ANIM_DURATION_MS, 300);
    assert_eq!(ZONE_PILL_GEOMETRY_DURATION_MS, 300);
    assert_eq!(pill_geometry_progress(0.0), 0.0);

    let half_geometry = 150.0 / ZONE_PILL_ANIM_DURATION_MS as f32;
    assert!((pill_geometry_progress(half_geometry) - 0.5).abs() < f32::EPSILON);

    let settled_geometry =
        ZONE_PILL_GEOMETRY_DURATION_MS as f32 / ZONE_PILL_ANIM_DURATION_MS as f32;
    assert_eq!(pill_geometry_progress(settled_geometry), 1.0);
    assert_eq!(pill_geometry_progress(1.0), 1.0);
}

#[test]
fn morph_pill_radius_interpolates_between_endpoints() {
    assert_eq!(morph_pill_radius(24.0, 12.0, 0.0), 24.0);
    assert_eq!(morph_pill_radius(24.0, 12.0, 1.0), 12.0);
    let mid = morph_pill_radius(24.0, 12.0, 0.5);
    assert!((mid - 18.0).abs() < 0.001);
}

#[test]
fn pill_anim_duration_keeps_eighteen_frames_at_sixty_hz() {
    assert_eq!(ZONE_PILL_ANIM_DURATION_MS, 300);
    assert_eq!(ZONE_PILL_GEOMETRY_DURATION_MS, 300);
}

#[test]
fn interrupted_segment_duration_scales_without_snapping() {
    assert_eq!(pill_segment_duration_ms(0.0, 1.0), 300);
    assert_eq!(pill_segment_duration_ms(0.5, 1.0), 150);
    assert_eq!(pill_segment_duration_ms(0.95, 1.0), 60);
}

#[test]
fn interrupted_reverse_starts_at_the_current_visual_morph() {
    let current = current_morph_progress(0.0, 0.25, true).clamp(0.0, 1.0);
    assert_eq!(current_morph_progress(current, 0.0, false), current);
    assert_eq!(current_morph_progress(current, 0.0, true), current);
}

#[test]
fn zone_morph_is_monotonic_and_can_drive_all_visual_channels() {
    let mut previous = 0.0;
    for step in 0..=100 {
        let current = current_morph_progress(0.0, step as f32 / 100.0, true);
        assert!((0.0..=1.0).contains(&current));
        assert!(current >= previous, "morph regressed at step {step}");
        previous = current;
    }
    assert_eq!(previous, 1.0);
}

#[test]
fn zen_content_reflows_inside_the_live_morph_rect() {
    let base = pill_layout_for_zone(&fixture(64, 332), 10);
    let expanded = Rect {
        x: 64.0,
        y: 332.0,
        width: 320.0,
        height: 220.0,
    };
    let live = pill_content_layout_in_rect(base, expanded);

    assert_eq!(pill_content_layout_in_rect(base, base.rect), base);
    assert!((live.icon.x - base.icon.x).abs() < f32::EPSILON);
    assert!(live.icon.y > base.icon.y);
    assert!(live.badge.x > base.badge.x);
    assert!(live.badge.y > base.badge.y);
    assert!(live.label.width > base.label.width);
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

// --- A3 HoverScheduler grace state machine -----------------------------

const EXPAND_DELAY: u32 = 90;
const COLLAPSE_DELAY: u32 = 200;

#[test]
fn expand_lock_tracks_the_fast_outer_shell_timeline() {
    assert_eq!(EXPAND_LOCK_MS, 320);
    assert_eq!(EXPAND_LOCK_MS, ZONE_PILL_GEOMETRY_DURATION_MS + 20);
    const { assert!(EXPAND_LOCK_MS > ZONE_PILL_ANIM_DURATION_MS) };
}

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
    // collapse_delay is shorter than EXPAND_LOCK_MS, so the lock wins.
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
    // Force-expand at t=2000 so the lock window starts at that timestamp.
    s.mark_expanded(zid(7), 2_000);
    assert_eq!(s.expanded_zone(), Some(zid(7)));
    // Leave at t=2100 (inside the lock). base = 2100+200 is still before the
    // selected-stack lock deadline.
    s.on_leave(2_100, COLLAPSE_DELAY, true);
    // One millisecond before the lock deadline — must NOT collapse.
    assert_eq!(s.poll(2_000 + EXPAND_LOCK_MS - 1), HoverAction::None);
    // lock_until — collapse fires.
    assert_eq!(
        s.poll(2_000 + EXPAND_LOCK_MS),
        HoverAction::Collapse(zid(7))
    );
}

#[test]
fn leave_after_lock_sets_collapse_at_now_plus_delay() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(3), 1_000);
    // Leave well after the lock has expired (t=5000). base = 5400.
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
    s.mark_expanded(zid(2), 5_000);
    s.on_leave(6_000, COLLAPSE_DELAY, true); // collapse pending at 6400
    assert!(s.is_pending());
    // Cursor re-enters the same zone before the grace elapses.
    s.on_enter(zid(2), 6_100, EXPAND_DELAY);
    // Collapse must be cancelled — no Collapse ever fires.
    assert_eq!(s.poll(6_400), HoverAction::None);
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
