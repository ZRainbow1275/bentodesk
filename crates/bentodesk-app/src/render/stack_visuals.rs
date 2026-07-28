use super::*;

/// Frosted-backdrop (2026-06-01) — straight per-channel colour lerp used by the
/// capsule↔panel morph to cross-fade `surface_zen → surface_expanded` along the
/// shared structural morph. `t` is clamped to
/// `[0, 1]`; every channel — including alpha — is interpolated linearly.
///
/// Deliberately a STRAIGHT lerp (not the premultiplied `Lerp for Color` in
/// `bentodesk-style`): both endpoints here are visible translucent surface
/// tints with similar hue, so the simple per-channel blend matches the CSS
/// `background` transition Tauri runs (which interpolates the rgba components
/// directly) and keeps the helper trivially testable. Free function so the
/// math is unit-tested without a GPU-backed `Renderer`.
#[inline]
pub(super) fn vertical_gradient_props(
    rect: bentodesk_style::Rect,
) -> D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
        startPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        endPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.bottom(),
        },
    }
}

#[inline]
pub(super) fn stack_capsule_sheen_gradient_props(
    rect: bentodesk_style::Rect,
) -> D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
        startPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        endPoint: D2D_POINT_2F {
            x: rect.right(),
            y: rect.bottom(),
        },
    }
}

#[inline]
pub(super) fn d2d_gradient_stop(position: f32, color: Color) -> D2D1_GRADIENT_STOP {
    D2D1_GRADIENT_STOP {
        position: position.clamp(0.0, 1.0),
        color: D2D1_COLOR_F {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        },
    }
}

#[inline]
pub(super) fn translate_rect(
    rect: bentodesk_style::Rect,
    dx: f32,
    dy: f32,
) -> bentodesk_style::Rect {
    bentodesk_style::Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        ..rect
    }
}

#[inline]
pub(super) fn scale_rect_about_center(
    rect: bentodesk_style::Rect,
    center_rect: bentodesk_style::Rect,
    scale: f32,
) -> bentodesk_style::Rect {
    let scale = scale.max(0.0);
    let origin_x = center_rect.x + center_rect.width * 0.5;
    let origin_y = center_rect.y + center_rect.height * 0.5;
    let rect_cx = rect.x + rect.width * 0.5;
    let rect_cy = rect.y + rect.height * 0.5;
    let next_w = rect.width * scale;
    let next_h = rect.height * scale;
    let next_cx = origin_x + (rect_cx - origin_x) * scale;
    let next_cy = origin_y + (rect_cy - origin_y) * scale;
    bentodesk_style::Rect {
        x: next_cx - next_w * 0.5,
        y: next_cy - next_h * 0.5,
        width: next_w,
        height: next_h,
    }
}

#[inline]
pub(super) fn scale_about_rect_center_matrix(
    base_scale: f32,
    center_rect: bentodesk_style::Rect,
    scale: f32,
) -> windows::Foundation::Numerics::Matrix3x2 {
    let scale = scale.max(0.0);
    let origin_x = center_rect.x + center_rect.width * 0.5;
    let origin_y = center_rect.y + center_rect.height * 0.5;
    let combined = base_scale * scale;
    windows::Foundation::Numerics::Matrix3x2 {
        M11: combined,
        M12: 0.0,
        M21: 0.0,
        M22: combined,
        M31: origin_x * (1.0 - scale) * base_scale,
        M32: origin_y * (1.0 - scale) * base_scale,
    }
}

#[inline]
pub(super) fn stack_capsule_bloom_text_transform(
    base_scale: f32,
    center_rect: bentodesk_style::Rect,
    visual_scale: f32,
) -> Option<windows::Foundation::Numerics::Matrix3x2> {
    let visual_scale = visual_scale.max(0.0);
    if (visual_scale - 1.0).abs() <= f32::EPSILON {
        return None;
    }
    Some(scale_about_rect_center_matrix(
        base_scale,
        center_rect,
        visual_scale,
    ))
}

#[inline]
pub(super) fn scale_border_radius(radius: BorderRadius, scale: f32) -> BorderRadius {
    let scale = scale.max(0.0);
    BorderRadius {
        top_left: radius.top_left * scale,
        top_right: radius.top_right * scale,
        bottom_right: radius.bottom_right * scale,
        bottom_left: radius.bottom_left * scale,
    }
}

#[inline]
pub(super) fn scale_shadow(shadow: Shadow, scale: f32) -> Shadow {
    let scale = scale.max(0.0);
    Shadow {
        offset_x: shadow.offset_x * scale,
        offset_y: shadow.offset_y * scale,
        blur: shadow.blur * scale,
        spread: shadow.spread * scale,
        ..shadow
    }
}

#[inline]
pub(super) fn scale_shadow_stack(stack: ShadowStack, scale: f32) -> ShadowStack {
    match stack.len() {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(scale_shadow(stack.inner(), scale)),
        _ => ShadowStack::two(
            scale_shadow(stack.inner(), scale),
            scale_shadow(stack.outer(), scale),
        ),
    }
}

#[inline]
pub(super) fn fade_shadow(shadow: Shadow, opacity: f32) -> Shadow {
    Shadow {
        color: fade_color(shadow.color, opacity),
        ..shadow
    }
}

#[inline]
pub(super) fn fade_shadow_stack(stack: ShadowStack, opacity: f32) -> ShadowStack {
    match stack.len() {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(fade_shadow(stack.inner(), opacity)),
        _ => ShadowStack::two(
            fade_shadow(stack.inner(), opacity),
            fade_shadow(stack.outer(), opacity),
        ),
    }
}

#[inline]
pub(super) fn stack_capsule_hover_translate_y(hover_t: f32) -> f32 {
    -hover_t.clamp(0.0, 1.0)
}

#[inline]
pub(super) fn stack_capsule_bloom_visual_for_app(
    app: &AppState,
    anchor_id: ZoneId,
    member_count: usize,
) -> StackCapsuleBloomVisual {
    let bloom_allowed = stack_surface_allows_bloom(app);
    if !bloom_allowed {
        return stack_capsule_bloom_visual(0.0, member_count, false);
    }
    let state_anchor = app.stack_bloom_anchor.get();
    if state_anchor != Some(anchor_id) {
        return stack_capsule_bloom_visual(0.0, member_count, false);
    }
    let leaving = app.stack_bloom_leaving.get();
    stack_capsule_bloom_visual(app.stack_bloom_progress.get(), member_count, leaving)
}

#[inline]
pub(super) fn stack_capsule_bloom_visual(
    progress: f32,
    member_count: usize,
    leaving: bool,
) -> StackCapsuleBloomVisual {
    let progress = progress.clamp(0.0, 1.0);
    let recede_t = if leaving {
        1.0 - zone_pill_geometry::ease_out_back_progress(progress).clamp(0.0, 1.0)
    } else {
        let reveal_ms = stack_tray::stack_bloom_reveal_duration_ms(member_count) as f32;
        let local = (progress * reveal_ms / STACK_CAPSULE_BLOOMED_RECEDES_MS).clamp(0.0, 1.0);
        zone_pill_geometry::ease_out_back_progress(local).clamp(0.0, 1.0)
    };
    StackCapsuleBloomVisual {
        recede_t,
        scale: 1.0 + (STACK_CAPSULE_BLOOMED_SCALE - 1.0) * recede_t,
        opacity: 1.0 + (STACK_CAPSULE_BLOOMED_OPACITY - 1.0) * recede_t,
    }
}

/// Tauri `spring-emerge`: 0% scale(.96)/opacity(0), 60%
/// scale(1.02)/opacity(1), 100% scale(1)/opacity(1), with the same spring
/// bezier applied to each keyframe interval.
#[inline]
pub(super) fn stack_capsule_emerge_visual(progress: f32) -> StackCapsuleEmergeVisual {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= STACK_CAPSULE_EMERGE_OVERSHOOT_AT {
        let local = progress / STACK_CAPSULE_EMERGE_OVERSHOOT_AT;
        let eased = zone_pill_geometry::ease_out_back_progress(local);
        StackCapsuleEmergeVisual {
            scale: STACK_CAPSULE_EMERGE_START_SCALE
                + (STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE - STACK_CAPSULE_EMERGE_START_SCALE) * eased,
            opacity: eased.clamp(0.0, 1.0),
        }
    } else {
        let local = (progress - STACK_CAPSULE_EMERGE_OVERSHOOT_AT)
            / (1.0 - STACK_CAPSULE_EMERGE_OVERSHOOT_AT);
        let eased = zone_pill_geometry::ease_out_back_progress(local);
        StackCapsuleEmergeVisual {
            scale: STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE
                + (1.0 - STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE) * eased,
            opacity: 1.0,
        }
    }
}

#[inline]
pub(super) fn stack_capsule_presented_emerge_visual(progress: f32) -> StackCapsuleEmergeVisual {
    stack_capsule_emerge_visual(progress.max(STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS))
}

#[inline]
pub(super) fn stack_capsule_bloomed_target_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 14.0, 36.0, Color::rgba(0.0, 0.0, 0.0, 0.22)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::rgba(1.0, 1.0, 1.0, 0.06),
        },
    )
}

#[inline]
pub(super) fn stack_capsule_bloom_shadow_stack(
    idle: ShadowStack,
    hover_t: f32,
    recede_t: f32,
) -> ShadowStack {
    let base = stack_capsule_hover_shadow_stack(idle, hover_t);
    let t = recede_t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return base;
    }
    let target = stack_capsule_bloomed_target_shadow_stack();
    let base_outer = if base.len() >= 2 {
        base.outer()
    } else {
        Shadow::NONE
    };
    ShadowStack::two(
        lerp_shadow(base.inner(), target.inner(), t),
        lerp_shadow(base_outer, target.outer(), t),
    )
}

#[inline]
pub(super) fn stack_capsule_bloom_border_color(
    pal: bentodesk_style::tokens::PaletteTauri,
    hover_t: f32,
    recede_t: f32,
) -> Color {
    lerp_color(
        stack_capsule_hover_border_color(pal, hover_t),
        Color::rgba(1.0, 1.0, 1.0, 0.18),
        recede_t.clamp(0.0, 1.0),
    )
}

#[inline]
pub(super) fn stack_capsule_hover_target_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 18.0, 42.0, Color::rgba(0.0, 0.0, 0.0, 0.24)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::rgba(1.0, 1.0, 1.0, 0.04),
        },
    )
}

#[inline]
pub(super) fn stack_capsule_preview_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 18.0, 42.0, Color::rgba(0.0, 0.0, 0.0, 0.24)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: STACK_CAPSULE_PREVIEW_RING,
        },
    )
}

#[inline]
pub(super) fn stack_capsule_visual_shadow_stack(
    idle: ShadowStack,
    hover_t: f32,
    recede_t: f32,
    has_preview: bool,
) -> ShadowStack {
    if has_preview {
        return stack_capsule_preview_shadow_stack();
    }
    stack_capsule_bloom_shadow_stack(idle, hover_t, recede_t)
}

#[inline]
pub(super) fn lerp_shadow(a: Shadow, b: Shadow, t: f32) -> Shadow {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Shadow {
        offset_x: a.offset_x * inv + b.offset_x * t,
        offset_y: a.offset_y * inv + b.offset_y * t,
        blur: a.blur * inv + b.blur * t,
        spread: a.spread * inv + b.spread * t,
        color: lerp_color(a.color, b.color, t),
    }
}

#[inline]
pub(super) fn lerp_shadow_stack(a: ShadowStack, b: ShadowStack, t: f32) -> ShadowStack {
    let len = a.len().max(b.len());
    let layer = |stack: ShadowStack, index: usize| {
        stack.layers().get(index).copied().unwrap_or(Shadow::NONE)
    };
    match len {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(lerp_shadow(layer(a, 0), layer(b, 0), t)),
        _ => ShadowStack::two(
            lerp_shadow(layer(a, 0), layer(b, 0), t),
            lerp_shadow(layer(a, 1), layer(b, 1), t),
        ),
    }
}

#[inline]
pub(super) fn stack_capsule_hover_shadow_stack(idle: ShadowStack, hover_t: f32) -> ShadowStack {
    let t = hover_t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return idle;
    }
    let hover = stack_capsule_hover_target_shadow_stack();
    let idle_outer = if idle.len() >= 2 {
        idle.outer()
    } else {
        Shadow::NONE
    };
    ShadowStack::two(
        lerp_shadow(idle.inner(), hover.inner(), t),
        lerp_shadow(idle_outer, hover.outer(), t),
    )
}

#[inline]
pub(super) fn stack_capsule_hover_border_color(
    pal: bentodesk_style::tokens::PaletteTauri,
    hover_t: f32,
) -> Color {
    lerp_color(
        pal.border_zen,
        Color::rgba(1.0, 1.0, 1.0, 0.18),
        hover_t.clamp(0.0, 1.0),
    )
}

#[inline]
pub(super) fn collapsed_zen_surface_color(
    pal: bentodesk_style::tokens::PaletteTauri,
    _hover_t: f32,
) -> Color {
    pal.surface_zen
}

pub(super) const ORDINARY_MEDIUM_PILL_SHADOW_OPACITY: f32 = 0.30;
pub(super) const ORDINARY_LARGE_PILL_SHADOW_OPACITY: f32 = 0.22;

#[inline]
pub(super) fn ordinary_zone_pill_shadow_stack(
    size: crate::business::zen_capsule::CapsuleSize,
    stack: ShadowStack,
) -> ShadowStack {
    match size {
        crate::business::zen_capsule::CapsuleSize::Medium => {
            fade_shadow_stack(stack, ORDINARY_MEDIUM_PILL_SHADOW_OPACITY)
        }
        crate::business::zen_capsule::CapsuleSize::Large => {
            fade_shadow_stack(stack, ORDINARY_LARGE_PILL_SHADOW_OPACITY)
        }
        crate::business::zen_capsule::CapsuleSize::Small => stack,
    }
}

#[inline]
pub(super) fn ordinary_zone_pill_chrome_radius(rect: Rect, radius: BorderRadius) -> BorderRadius {
    let max_radius = rect.height * 0.5;
    BorderRadius {
        top_left: radius.top_left.min(max_radius),
        top_right: radius.top_right.min(max_radius),
        bottom_right: radius.bottom_right.min(max_radius),
        bottom_left: radius.bottom_left.min(max_radius),
    }
}

#[inline]
pub(super) fn frosted_backdrop_saturation_for_palette(
    pal: bentodesk_style::tokens::PaletteTauri,
) -> f32 {
    if pal.is_dark {
        FROSTED_BACKDROP_SATURATION_DARK
    } else {
        FROSTED_BACKDROP_SATURATION_LIGHT
    }
}

#[inline]
pub(super) fn frosted_backdrop_saturation_changed(cached: f32, desired: f32) -> bool {
    (cached - desired).abs() > f32::EPSILON
}

/// Main is desktop-embedded, so a palette saturation change can safely refresh
/// its desktop snapshot. Settings is a full-work-area modal: recapturing while
/// it is visible would photograph its own scrim and recursively darken the
/// panel after every theme switch. It therefore reuses the clean snapshot from
/// open and captures the new saturation on the next reopen.
#[inline]
pub(super) fn frosted_backdrop_saturation_recapture_needed(
    kind: WindowKind,
    cached: f32,
    desired: f32,
) -> bool {
    kind == WindowKind::Main && frosted_backdrop_saturation_changed(cached, desired)
}

#[inline]
pub(super) fn stack_capsule_glass_sheen_colors() -> (Color, Color) {
    (
        with_alpha(Color::WHITE, 0.08),
        with_alpha(Color::WHITE, 0.02),
    )
}

#[inline]
pub(super) fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color {
        r: a.r * inv + b.r * t,
        g: a.g * inv + b.g * t,
        b: a.b * inv + b.b * t,
        a: a.a * inv + b.a * t,
    }
}
