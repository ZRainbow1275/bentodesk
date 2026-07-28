use super::{
    FROSTED_BACKDROP_DOWNSAMPLE, FROSTED_BACKDROP_SATURATION_DARK,
    FROSTED_BACKDROP_SATURATION_LIGHT, FROSTED_BACKDROP_STDDEV, FROSTED_FALLBACK_MIN_ALPHA,
    ORDINARY_LARGE_PILL_SHADOW_OPACITY, ORDINARY_MEDIUM_PILL_SHADOW_OPACITY,
    STACK_CAPSULE_BLOOMED_OPACITY, STACK_CAPSULE_BLOOMED_RECEDES_MS, STACK_CAPSULE_BLOOMED_SCALE,
    STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS, STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE,
    STACK_CAPSULE_EMERGE_START_SCALE, collapsed_zen_surface_color, d2d_gradient_stop, fade_color,
    frosted_backdrop_saturation_changed, frosted_backdrop_saturation_for_palette,
    frosted_backdrop_saturation_recapture_needed, frosted_fallback_underlay,
    frosted_group_backdrop_opacity, lerp_color, lerp_shadow_stack, opaque_auxiliary_surface,
    ordinary_zone_pill_chrome_radius, ordinary_zone_pill_shadow_stack,
    scale_about_rect_center_matrix, scale_rect_about_center, stack_capsule_badge_chrome,
    stack_capsule_bloom_shadow_stack, stack_capsule_bloom_text_transform,
    stack_capsule_bloom_visual, stack_capsule_bloom_visual_for_app,
    stack_capsule_bloomed_target_shadow_stack, stack_capsule_emerge_visual,
    stack_capsule_glass_sheen_colors, stack_capsule_has_preview, stack_capsule_hover_border_color,
    stack_capsule_hover_shadow_stack, stack_capsule_hover_target_shadow_stack,
    stack_capsule_hover_translate_y, stack_capsule_is_locked, stack_capsule_locked_opacity,
    stack_capsule_presented_emerge_visual, stack_capsule_preview_indicator_width,
    stack_capsule_preview_shadow_stack, stack_capsule_sheen_gradient_props,
    stack_capsule_show_preview_indicator, stack_capsule_visual_shadow_stack,
    title_shrink_signature, translate_rect, vertical_gradient_props, with_alpha,
};
use crate::AppState;
use crate::business::stack_tray::StackTrayState;
use crate::business::zen_capsule::CapsuleSize;
use crate::zone_pill_geometry;
use bento_nano_platform::WindowKind;
use bento_nano_style::{BorderRadius, Color, Rect, Shadow, ShadowStack};
use bento_nano_zone::{Zone, ZoneId};

#[test]
fn shared_frosted_backdrop_tracks_tauri_zen_blur_under_memory_budget() {
    assert_eq!(FROSTED_BACKDROP_DOWNSAMPLE, 4);
    assert!((FROSTED_BACKDROP_STDDEV - 5.0).abs() < f32::EPSILON);
    assert!((FROSTED_BACKDROP_SATURATION_DARK - 1.6).abs() < f32::EPSILON);
    assert!((FROSTED_BACKDROP_SATURATION_LIGHT - 1.3).abs() < f32::EPSILON);
    assert!(
        (frosted_backdrop_saturation_for_palette(bento_nano_style::tokens::PALETTE_DARK) - 1.6)
            .abs()
            < f32::EPSILON
    );
    assert!(
        (frosted_backdrop_saturation_for_palette(bento_nano_style::tokens::PALETTE_LIGHT) - 1.3)
            .abs()
            < f32::EPSILON
    );
    assert!(!frosted_backdrop_saturation_changed(1.6, 1.6));
    assert!(frosted_backdrop_saturation_changed(1.6, 1.3));
    assert!(frosted_backdrop_saturation_recapture_needed(
        WindowKind::Main,
        1.6,
        1.3
    ));
    assert!(!frosted_backdrop_saturation_recapture_needed(
        WindowKind::Settings,
        1.6,
        1.3
    ));
}

#[test]
fn degraded_frosted_surface_stays_dense_but_translucent() {
    let zen = bento_nano_style::tokens::PALETTE_DARK.surface_zen;
    let underlay = frosted_fallback_underlay(zen).expect("zen needs fallback underlay");
    let composed_alpha = zen.a + underlay.a * (1.0 - zen.a);
    assert!((composed_alpha - FROSTED_FALLBACK_MIN_ALPHA).abs() < f32::EPSILON);
    assert!(composed_alpha > zen.a);
    assert!(composed_alpha < 1.0);
    assert_eq!(underlay.r, zen.r);
    assert_eq!(underlay.g, zen.g);
    assert_eq!(underlay.b, zen.b);

    let already_dense = with_alpha(zen, FROSTED_FALLBACK_MIN_ALPHA);
    assert!(frosted_fallback_underlay(already_dense).is_none());
}

#[test]
fn auxiliary_panel_fallback_is_solid_while_rounded_corners_stay_clear() {
    let token = Color::rgba(0.05, 0.06, 0.08, 0.82);
    let fallback = opaque_auxiliary_surface(token);
    assert_eq!(fallback.r, token.r);
    assert_eq!(fallback.g, token.g);
    assert_eq!(fallback.b, token.b);
    assert_eq!(fallback.a, 1.0);
}

#[test]
fn frosted_group_opacity_preserves_css_layer_coefficients() {
    let tint_alpha = 0.55;
    let group_opacity = 0.50;
    let faded_tint_alpha = tint_alpha * group_opacity;
    let backdrop_opacity = frosted_group_backdrop_opacity(tint_alpha, group_opacity);

    assert!(
        ((1.0 - faded_tint_alpha) * backdrop_opacity - group_opacity * (1.0 - tint_alpha)).abs()
            < 1e-6
    );
    assert!(
        ((1.0 - faded_tint_alpha) * (1.0 - backdrop_opacity) - (1.0 - group_opacity)).abs() < 1e-6
    );
    assert_eq!(frosted_group_backdrop_opacity(tint_alpha, 0.0), 0.0);
    assert_eq!(frosted_group_backdrop_opacity(tint_alpha, 1.0), 1.0);
    assert_eq!(frosted_group_backdrop_opacity(1.0, 0.5), 0.0);
}

/// Frosted-backdrop — the capsule↔panel morph cross-fades
/// `surface_zen → surface_expanded` along the shared morph. Pin the endpoints and
/// the midpoint, INCLUDING the alpha channel. The endpoint must match the
/// settled expanded-panel renderer so the morph cannot over-darken before
/// the steady panel path takes over.
#[test]
fn lerp_color_endpoints_and_midpoint() {
    // surface_zen (#121218 @ 0x8C) → surface_expanded (#0C0C12 @ 0xD1), the
    // exact Tauri dark tokens the morph blends between.
    let zen = Color::from_u8(0x12, 0x12, 0x18, 0x8C);
    let expanded = Color::from_u8(0x0C, 0x0C, 0x12, 0xD1);

    // t = 0 → exactly the start colour.
    let at0 = lerp_color(zen, expanded, 0.0);
    assert_eq!(at0, zen);
    // t = 1 → exactly the end colour.
    let at1 = lerp_color(zen, expanded, 1.0);
    assert_eq!(at1, expanded);
    assert_eq!(at1, bento_nano_style::tokens::PALETTE_DARK.surface_expanded);

    // t = 0.5 → per-channel midpoint, alpha included.
    let mid = lerp_color(zen, expanded, 0.5);
    let eps = 1e-6_f32;
    assert!((mid.r - (zen.r + expanded.r) * 0.5).abs() < eps);
    assert!((mid.g - (zen.g + expanded.g) * 0.5).abs() < eps);
    assert!((mid.b - (zen.b + expanded.b) * 0.5).abs() < eps);
    assert!((mid.a - (zen.a + expanded.a) * 0.5).abs() < eps);
    // The alpha genuinely moves (0x8C/255 .. 0xD1/255 midpoint).
    let expected_a = (0x8C as f32 / 255.0 + 0xD1 as f32 / 255.0) * 0.5;
    assert!((mid.a - expected_a).abs() < eps);
}

/// Out-of-range `t` clamps to `[0, 1]` so malformed/transient state can
/// never over/under-saturate the morph tint.
#[test]
fn lerp_color_clamps_t() {
    let a = Color::rgba(0.0, 0.0, 0.0, 0.0);
    let b = Color::rgba(1.0, 1.0, 1.0, 1.0);
    // t < 0 → clamp to start.
    assert_eq!(lerp_color(a, b, -0.5), a);
    // t > 1 → clamp to end.
    assert_eq!(lerp_color(a, b, 1.5), b);
}

#[test]
fn vertical_gradient_props_follow_rect_top_to_bottom() {
    let rect = Rect {
        x: 12.0,
        y: 34.0,
        width: 160.0,
        height: 48.0,
    };
    let props = vertical_gradient_props(rect);
    assert_eq!(props.startPoint.x, rect.x);
    assert_eq!(props.startPoint.y, rect.y);
    assert_eq!(props.endPoint.x, rect.x);
    assert_eq!(props.endPoint.y, rect.bottom());
}

#[test]
fn stack_capsule_gradient_props_follow_tauri_135deg_contract() {
    let rect = Rect {
        x: 12.0,
        y: 34.0,
        width: 160.0,
        height: 48.0,
    };
    let props = stack_capsule_sheen_gradient_props(rect);
    assert_eq!(props.startPoint.x, rect.x);
    assert_eq!(props.startPoint.y, rect.y);
    assert_eq!(props.endPoint.x, rect.right());
    assert_eq!(props.endPoint.y, rect.bottom());
}

#[test]
fn d2d_gradient_stop_clamps_position_and_keeps_rgba() {
    let color = Color::from_u8(0x12, 0x16, 0x22, 0xD1);
    let stop = d2d_gradient_stop(1.5, color);
    assert_eq!(stop.position, 1.0);
    assert_eq!(stop.color.r, color.r);
    assert_eq!(stop.color.g, color.g);
    assert_eq!(stop.color.b, color.b);
    assert_eq!(stop.color.a, color.a);
}

#[test]
fn collapsed_zen_surface_ignores_hover_to_match_tauri_css() {
    let idle = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 0.0);
    let hover = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 1.0);
    let overshoot = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 2.0);
    assert_eq!(idle, bento_nano_style::tokens::PALETTE_DARK.surface_zen);
    assert_eq!(hover, idle);
    assert_eq!(overshoot, idle);
}

#[test]
fn ordinary_pill_shadow_attenuates_medium_and_large_without_changing_geometry() {
    let idle = bento_nano_style::tokens::SHADOW.zen;
    let medium = ordinary_zone_pill_shadow_stack(CapsuleSize::Medium, idle);
    let large = ordinary_zone_pill_shadow_stack(CapsuleSize::Large, idle);

    assert_eq!(medium.len(), idle.len());
    assert_eq!(medium.inner().offset_x, idle.inner().offset_x);
    assert_eq!(medium.inner().offset_y, idle.inner().offset_y);
    assert_eq!(medium.inner().blur, idle.inner().blur);
    assert_eq!(medium.inner().spread, idle.inner().spread);
    assert_eq!(medium.outer().offset_x, idle.outer().offset_x);
    assert_eq!(medium.outer().offset_y, idle.outer().offset_y);
    assert_eq!(medium.outer().blur, idle.outer().blur);
    assert_eq!(medium.outer().spread, idle.outer().spread);
    assert!(
        (medium.inner().color.a - idle.inner().color.a * ORDINARY_MEDIUM_PILL_SHADOW_OPACITY).abs()
            < f32::EPSILON
    );
    assert!(
        (medium.outer().color.a - idle.outer().color.a * ORDINARY_MEDIUM_PILL_SHADOW_OPACITY).abs()
            < f32::EPSILON
    );

    assert_eq!(
        ordinary_zone_pill_shadow_stack(CapsuleSize::Small, idle),
        idle
    );
    assert_eq!(large.len(), idle.len());
    assert_eq!(large.inner().offset_x, idle.inner().offset_x);
    assert_eq!(large.inner().offset_y, idle.inner().offset_y);
    assert_eq!(large.inner().blur, idle.inner().blur);
    assert_eq!(large.inner().spread, idle.inner().spread);
    assert_eq!(large.outer().offset_x, idle.outer().offset_x);
    assert_eq!(large.outer().offset_y, idle.outer().offset_y);
    assert_eq!(large.outer().blur, idle.outer().blur);
    assert_eq!(large.outer().spread, idle.outer().spread);
    assert!(
        (large.inner().color.a - idle.inner().color.a * ORDINARY_LARGE_PILL_SHADOW_OPACITY).abs()
            < f32::EPSILON
    );
    assert!(
        (large.outer().color.a - idle.outer().color.a * ORDINARY_LARGE_PILL_SHADOW_OPACITY).abs()
            < f32::EPSILON
    );
}

#[test]
fn ordinary_pill_chrome_radius_caps_at_half_the_visible_height() {
    let rect = Rect {
        x: 10.0,
        y: 20.0,
        width: 160.0,
        height: 48.0,
    };
    assert_eq!(
        ordinary_zone_pill_chrome_radius(rect, BorderRadius::all(24.0)),
        BorderRadius::all(24.0)
    );
    let large_rect = Rect {
        width: 220.0,
        height: 56.0,
        ..rect
    };
    assert_eq!(
        ordinary_zone_pill_chrome_radius(large_rect, BorderRadius::all(28.0)),
        BorderRadius::all(28.0)
    );
    let tiny = Rect {
        height: 6.0,
        ..rect
    };
    assert_eq!(
        ordinary_zone_pill_chrome_radius(tiny, BorderRadius::all(24.0)),
        BorderRadius::all(3.0)
    );
}

#[test]
fn morph_shadow_stack_preserves_both_endpoints() {
    let from = ShadowStack::one(Shadow::drop(0.0, 0.0, 0.0, Color::rgba(1.0, 0.0, 0.0, 0.5)));
    let to = ShadowStack::two(
        Shadow::drop(1.0, 2.0, 0.0, Color::rgba(0.0, 1.0, 0.0, 0.4)),
        Shadow::drop(3.0, 4.0, 0.0, Color::rgba(0.0, 0.0, 1.0, 0.3)),
    );

    let start = lerp_shadow_stack(from, to, 0.0);
    assert_eq!(start.inner(), from.inner());
    assert_eq!(start.outer().color.a, 0.0);
    assert_eq!(lerp_shadow_stack(from, to, 1.0), to);
}

#[test]
fn stack_capsule_sheen_matches_tauri_stackwrapper_alpha_stops() {
    let (start, end) = stack_capsule_glass_sheen_colors();
    assert_eq!(start.r, 1.0);
    assert_eq!(start.g, 1.0);
    assert_eq!(start.b, 1.0);
    assert_eq!(end.r, 1.0);
    assert_eq!(end.g, 1.0);
    assert_eq!(end.b, 1.0);
    assert!((start.a - 0.08).abs() < f32::EPSILON);
    assert!((end.a - 0.02).abs() < f32::EPSILON);
}

#[test]
fn stack_capsule_locked_chrome_matches_tauri_css() {
    assert!((stack_capsule_locked_opacity(false) - 1.0).abs() < f32::EPSILON);
    assert!((stack_capsule_locked_opacity(true) - 0.9).abs() < f32::EPSILON);

    let pal = bento_nano_style::tokens::PALETTE_DARK;
    let unlocked = stack_capsule_badge_chrome(pal, false);
    assert_eq!(unlocked.fill, with_alpha(pal.text_primary, 0.08));
    assert_eq!(unlocked.text, pal.text_primary);

    let locked = stack_capsule_badge_chrome(pal, true);
    assert_eq!(locked.fill, Color::from_u8(0xF5, 0x9E, 0x0B, 0x24));
    assert_eq!(locked.text, Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF));
}

#[test]
fn stack_capsule_locked_rule_matches_tauri_any_zone_locked() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
    app.zones
        .add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));
    app.zones
        .add(Zone::new(ZoneId(3), "child-b", 100, 80, 120, 90));
    let member_ids = [ZoneId(1), ZoneId(2), ZoneId(3)];

    {
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        assert!(!stack_capsule_is_locked(&app, anchor, &member_ids));
    }

    app.zones.get_mut(ZoneId(2)).expect("child").locked = true;
    {
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        assert!(stack_capsule_is_locked(&app, anchor, &member_ids));
    }

    app.zones.get_mut(ZoneId(2)).expect("child").locked = false;
    app.zones.get_mut(ZoneId(1)).expect("anchor").locked = true;
    let anchor = app.zones.get(ZoneId(1)).expect("anchor");
    assert!(stack_capsule_is_locked(&app, anchor, &member_ids));
}

#[test]
fn stack_capsule_preview_rule_matches_tauri_has_preview_state() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
    app.zones
        .add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));

    assert!(!stack_capsule_has_preview(&app, ZoneId(1)));
    app.stack_tray
        .borrow_mut()
        .replace(StackTrayState::new(ZoneId(1), ZoneId(1)));
    assert!(!stack_capsule_has_preview(&app, ZoneId(1)));

    app.stack_tray
        .borrow_mut()
        .replace(StackTrayState::new(ZoneId(1), ZoneId(2)));
    assert!(stack_capsule_has_preview(&app, ZoneId(1)));
    assert!(!stack_capsule_has_preview(&app, ZoneId(2)));
}

#[test]
fn stack_capsule_preview_chrome_matches_tauri_css() {
    let preview = stack_capsule_preview_shadow_stack();
    assert_eq!(preview.inner().offset_x, 0.0);
    assert_eq!(preview.inner().offset_y, 18.0);
    assert_eq!(preview.inner().blur, 42.0);
    assert_eq!(preview.inner().spread, 0.0);
    assert!((preview.inner().color.a - 0.24).abs() < f32::EPSILON);

    let ring = preview.outer();
    assert_eq!(ring.offset_x, 0.0);
    assert_eq!(ring.offset_y, 0.0);
    assert_eq!(ring.blur, 0.0);
    assert_eq!(ring.spread, 1.0);
    assert_eq!(ring.color, Color::from_u8(0x3B, 0x82, 0xF6, 0x6B));

    assert!(stack_capsule_show_preview_indicator(true, 0.0));
    assert!(!stack_capsule_show_preview_indicator(true, 0.01));
    assert!(!stack_capsule_show_preview_indicator(false, 0.0));

    let zh_width = stack_capsule_preview_indicator_width("预览中");
    let en_width = stack_capsule_preview_indicator_width("Preview open");
    assert!(zh_width >= 34.0);
    assert!(en_width <= 82.0);

    let idle = bento_nano_style::tokens::SHADOW.zen;
    assert_eq!(
        stack_capsule_visual_shadow_stack(idle, 0.0, 0.0, true),
        preview
    );
    assert_eq!(
        stack_capsule_visual_shadow_stack(idle, 1.0, 0.0, false),
        stack_capsule_hover_shadow_stack(idle, 1.0)
    );
}

#[test]
fn stack_capsule_hover_lift_matches_tauri_translate_y_contract() {
    assert_eq!(stack_capsule_hover_translate_y(-1.0), 0.0);
    assert_eq!(stack_capsule_hover_translate_y(0.0), 0.0);
    assert_eq!(stack_capsule_hover_translate_y(0.5), -0.5);
    assert_eq!(stack_capsule_hover_translate_y(1.0), -1.0);
    assert_eq!(stack_capsule_hover_translate_y(2.0), -1.0);

    let rect = Rect {
        x: 12.0,
        y: 34.0,
        width: 160.0,
        height: 48.0,
    };
    let lifted = translate_rect(rect, 0.0, stack_capsule_hover_translate_y(1.0));
    assert_eq!(lifted.x, rect.x);
    assert_eq!(lifted.y, rect.y - 1.0);
    assert_eq!(lifted.width, rect.width);
    assert_eq!(lifted.height, rect.height);
}

#[test]
fn stack_capsule_hover_border_reaches_tauri_literal_white_alpha() {
    let idle = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 0.0);
    let mid = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 0.5);
    let hover = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 1.0);

    assert_eq!(idle, bento_nano_style::tokens::PALETTE_DARK.border_zen);
    assert_eq!(hover.r, 1.0);
    assert_eq!(hover.g, 1.0);
    assert_eq!(hover.b, 1.0);
    assert!((hover.a - 0.18).abs() < f32::EPSILON);
    assert!((mid.a - (idle.a + 0.18) * 0.5).abs() < 1e-6);
}

#[test]
fn stack_capsule_hover_shadow_reaches_tauri_hover_box_shadow() {
    let idle_shadow = bento_nano_style::tokens::SHADOW.zen;
    assert_eq!(
        stack_capsule_hover_shadow_stack(idle_shadow, 0.0),
        idle_shadow
    );

    let target = stack_capsule_hover_target_shadow_stack();
    let hover = stack_capsule_hover_shadow_stack(idle_shadow, 1.0);
    assert_eq!(hover, target);

    let dark_drop = hover.inner();
    assert_eq!(dark_drop.offset_x, 0.0);
    assert_eq!(dark_drop.offset_y, 18.0);
    assert_eq!(dark_drop.blur, 42.0);
    assert_eq!(dark_drop.spread, 0.0);
    assert!((dark_drop.color.a - 0.24).abs() < f32::EPSILON);

    let white_ring = hover.outer();
    assert_eq!(white_ring.offset_x, 0.0);
    assert_eq!(white_ring.offset_y, 0.0);
    assert_eq!(white_ring.blur, 0.0);
    assert_eq!(white_ring.spread, 1.0);
    assert_eq!(white_ring.color.r, 1.0);
    assert_eq!(white_ring.color.g, 1.0);
    assert_eq!(white_ring.color.b, 1.0);
    assert!((white_ring.color.a - 0.04).abs() < f32::EPSILON);
}

#[test]
fn stack_capsule_bloom_recedes_to_tauri_scale_opacity_by_180ms() {
    let member_count = 2;
    let reveal_ms = crate::business::stack_tray::stack_bloom_reveal_duration_ms(member_count);
    let cutoff_progress = STACK_CAPSULE_BLOOMED_RECEDES_MS / reveal_ms as f32;

    let start = stack_capsule_bloom_visual(0.0, member_count, false);
    assert_eq!(start.recede_t, 0.0);
    assert_eq!(start.scale, 1.0);
    assert_eq!(start.opacity, 1.0);

    let at_cutoff = stack_capsule_bloom_visual(cutoff_progress, member_count, false);
    assert!((at_cutoff.recede_t - 1.0).abs() < 1e-6);
    assert!((at_cutoff.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
    assert!((at_cutoff.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);

    let settled = stack_capsule_bloom_visual(1.0, member_count, false);
    assert!((settled.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
    assert!((settled.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);
}

#[test]
fn stack_capsule_emerge_matches_tauri_spring_keyframes() {
    let start = stack_capsule_emerge_visual(0.0);
    assert_eq!(start.scale, STACK_CAPSULE_EMERGE_START_SCALE);
    assert_eq!(start.opacity, 0.0);

    let overshoot = stack_capsule_emerge_visual(0.60);
    assert!((overshoot.scale - STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE).abs() < 1e-6);
    assert_eq!(overshoot.opacity, 1.0);

    let settled = stack_capsule_emerge_visual(1.0);
    assert_eq!(settled.scale, 1.0);
    assert_eq!(settled.opacity, 1.0);
}

#[test]
fn stack_capsule_first_native_present_is_visible_without_changing_keyframe_endpoint() {
    let first = stack_capsule_presented_emerge_visual(0.0);
    assert!(first.opacity > 0.0);
    assert!(first.opacity < 1.0);
    assert!(first.scale > STACK_CAPSULE_EMERGE_START_SCALE);
    const {
        assert!(
            STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS > 0.0
                && STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS < 0.25
        );
    }

    let settled = stack_capsule_presented_emerge_visual(1.0);
    assert_eq!(settled.scale, 1.0);
    assert_eq!(settled.opacity, 1.0);
}

#[test]
fn stack_capsule_bloom_visual_requires_explicit_bloom_state() {
    let app = AppState::new();
    app.hovered_zone.set(Some(ZoneId(1)));

    let held_after_drop = stack_capsule_bloom_visual_for_app(&app, ZoneId(1), 4);
    assert_eq!(held_after_drop.recede_t, 0.0);
    assert_eq!(held_after_drop.scale, 1.0);
    assert_eq!(held_after_drop.opacity, 1.0);

    app.stack_bloom_anchor.set(Some(ZoneId(1)));
    app.stack_bloom_progress.set(1.0);
    let bloomed = stack_capsule_bloom_visual_for_app(&app, ZoneId(1), 4);
    assert_eq!(bloomed.recede_t, 1.0);
    assert_eq!(bloomed.scale, STACK_CAPSULE_BLOOMED_SCALE);
    assert_eq!(bloomed.opacity, STACK_CAPSULE_BLOOMED_OPACITY);
}

#[test]
fn stack_capsule_bloom_leave_restores_identity_without_anchor_pop() {
    let leaving_start = stack_capsule_bloom_visual(0.0, 5, true);
    assert!((leaving_start.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
    assert!((leaving_start.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);

    let leaving_done = stack_capsule_bloom_visual(1.0, 5, true);
    assert_eq!(leaving_done.recede_t, 0.0);
    assert_eq!(leaving_done.scale, 1.0);
    assert_eq!(leaving_done.opacity, 1.0);
}

#[test]
fn stack_capsule_child_rects_scale_about_capsule_center() {
    let capsule = Rect {
        x: 100.0,
        y: 200.0,
        width: 220.0,
        height: 52.0,
    };
    let child = Rect {
        x: 112.0,
        y: 214.0,
        width: 20.0,
        height: 20.0,
    };
    let scaled = scale_rect_about_center(child, capsule, STACK_CAPSULE_BLOOMED_SCALE);
    assert!((scaled.width - 18.4).abs() < 1e-5);
    assert!((scaled.height - 18.4).abs() < 1e-5);
    assert!(scaled.x > child.x);
    assert!(scaled.y > child.y);
    let capsule_cx = capsule.x + capsule.width * 0.5;
    let before_dx = child.x + child.width * 0.5 - capsule_cx;
    let after_dx = scaled.x + scaled.width * 0.5 - capsule_cx;
    assert!((after_dx - before_dx * STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-5);
}

#[test]
fn stack_capsule_text_transform_scales_without_shrink_width_churn() {
    let capsule = Rect {
        x: 100.0,
        y: 200.0,
        width: 220.0,
        height: 52.0,
    };
    let base_scale = 1.5;
    let matrix =
        stack_capsule_bloom_text_transform(base_scale, capsule, STACK_CAPSULE_BLOOMED_SCALE)
            .expect("bloomed scale should need a transform");
    let direct = scale_about_rect_center_matrix(base_scale, capsule, STACK_CAPSULE_BLOOMED_SCALE);
    assert!((matrix.M11 - direct.M11).abs() < 1e-6);
    assert!((matrix.M22 - direct.M22).abs() < 1e-6);
    assert!((matrix.M31 - direct.M31).abs() < 1e-6);
    assert!((matrix.M32 - direct.M32).abs() < 1e-6);
    assert!((matrix.M11 - 1.38).abs() < 1e-6);
    assert!((matrix.M22 - 1.38).abs() < 1e-6);

    let origin_x = capsule.x + capsule.width * 0.5;
    let origin_y = capsule.y + capsule.height * 0.5;
    assert!((matrix.M31 - origin_x * 0.08 * base_scale).abs() < 1e-5);
    assert!((matrix.M32 - origin_y * 0.08 * base_scale).abs() < 1e-5);
    assert!(stack_capsule_bloom_text_transform(base_scale, capsule, 1.0).is_none());

    let unscaled_fit_width = 132.0;
    let unscaled_sig = title_shrink_signature(
        "Benchmark Zone",
        unscaled_fit_width,
        zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
        zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
        0.0,
    );
    let scaled_sig = title_shrink_signature(
        "Benchmark Zone",
        unscaled_fit_width * STACK_CAPSULE_BLOOMED_SCALE,
        zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
        zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
        0.0,
    );
    assert_ne!(
        unscaled_sig, scaled_sig,
        "changing fit width during bloom would churn the shrink cache"
    );
    assert_eq!(
        unscaled_sig,
        title_shrink_signature(
            "Benchmark Zone",
            unscaled_fit_width,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
            0.0,
        ),
        "text transform keeps the title fit/cache width stable"
    );
}

#[test]
fn stack_capsule_bloom_shadow_and_color_apply_css_opacity() {
    let target = stack_capsule_bloomed_target_shadow_stack();
    let bloomed = stack_capsule_bloom_shadow_stack(bento_nano_style::ShadowStack::NONE, 0.0, 1.0);
    assert_eq!(bloomed, target);
    assert!((bloomed.inner().offset_y - 14.0).abs() < f32::EPSILON);
    assert!((bloomed.inner().blur - 36.0).abs() < f32::EPSILON);
    assert!((bloomed.inner().color.a - 0.22).abs() < f32::EPSILON);
    assert!((bloomed.outer().color.a - 0.06).abs() < f32::EPSILON);

    let faded = fade_color(
        Color::rgba(1.0, 1.0, 1.0, 0.18),
        STACK_CAPSULE_BLOOMED_OPACITY,
    );
    assert!((faded.a - 0.099).abs() < 1e-6);
}
