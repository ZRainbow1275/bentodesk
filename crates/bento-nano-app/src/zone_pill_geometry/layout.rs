use super::*;

/// Build a pill layout anchored at `(zone.x, zone.y)`. `count` is the badge
/// number (item count for a regular zone or stack-member count for an
/// anchor). The returned `rect` is the pill's outer hit-test region.
///
/// M2② (2026-05-29) — the pill now honours the per-zone appearance
/// (`zone.capsule_size` / `zone.capsule_shape`) so the live capsule renders at
/// Tauri-1:1 size + shape (ruling A, Q2 pixel parity):
///
/// * **Size** ([`CapsuleSize`], from `zone.capsule_size`): drives the outer
///   height (small 36 / medium 48 / large 50 after the 2026-07-23 desktop
///   hand-test refinement) while the icon box stays at the actual Tauri
///   `ZoneIcon size={18}` for every tier.
/// * **Shape** ([`CapsuleShape`], from `zone.capsule_shape`): drives the
///   corner radius (`pill` 24 / `rounded` 12 / `minimal` 8 / `circle`
///   height/2 / legacy `square` 4) per Tauri `BentoZone.css:80-99`. A `circle`
///   shape collapses the box to a 1:1 icon-only disc (Tauri
///   `aspect-ratio:1` + `border-radius:50%`), suppressing the label/badge run.
///
/// **V-13 paint–hit parity:** the shell's `effective_zone_hit_rect`
/// (`ui.rs:81`) derives its hit-rect from `pill_layout_for_zone(..).rect`,
/// the SAME call the renderer uses, so per-zone height/shape changes here keep
/// the clickable region pixel-locked to the painted capsule with NO separate
/// constant to bump.
///
/// Pure / allocation-free / `Copy` output. Safe to call every frame — the
/// appearance resolution is two `match`es on `Copy` enums parsed from the
/// already-resident `Cow<str>` tokens (spec §10: no alloc, no `format!`).
pub fn pill_layout_for_zone(zone: &Zone, count: usize) -> ZonePillLayout {
    let size = CapsuleSize::parse(zone.capsule_size.as_ref());
    let shape = CapsuleShape::parse(zone.capsule_shape.as_ref());
    let height = size.height_px();
    let icon_size = size.icon_px();

    // G5 (2026-06-01) — per-tier asymmetric horizontal padding + inner gap,
    // sourced from `CapsuleSize` (Tauri `.zen-capsule` per-tier `padding`/`gap`)
    // instead of the pre-G5 flat `SPACING.md` (12) both sides + `SPACING.s6`
    // (6) inner. See `CapsuleSize::pad_lr_px` / `inner_gap_px`.
    let (pad_left, pad_right) = size.pad_lr_px();
    let pad_inner = size.inner_gap_px();
    let has_visible_glyph = icon_name_has_visible_glyph(zone.icon.as_ref());
    let display_title = zone.display_title();
    let uses_visible_glyph_content_metrics =
        pill_uses_visible_glyph_content_metrics(size, zone.icon.as_ref(), display_title);
    let x = zone.x as f32;
    let y = zone.y as f32;

    // Circle shape — Tauri renders an icon-only 1:1 disc (label + badge are
    // `display:none`). The box is square (width == height == circle diameter)
    // and the icon is centred; the label/badge rects collapse onto the icon
    // slot so downstream paint of those bands is a visual no-op.
    if shape.is_circle() {
        let diameter = size.circle_diameter_px();
        // V21-C4 (2026-06-22) — Tauri's visible SVG/custom icon box is still
        // `ZoneIcon size={18}` in circle mode. The circle CSS font-size
        // override is on the outer span and does not resize `--zone-icon-size`.
        let icon_size = size.circle_icon_px();
        // True 50% disc — radius is half the ACTUAL (square) box, i.e. the
        // circle diameter, not the non-circle tier height. Mirrors Tauri's
        // `border-radius: 50%` on the 1:1 `aspect-ratio:1` circle box.
        let radius_px = diameter * 0.5;
        let rect = Rect {
            x,
            y,
            width: diameter,
            height: diameter,
        };
        let icon = Rect {
            x: rect.x + (diameter - icon_size) * 0.5,
            y: rect.y + (diameter - icon_size) * 0.5,
            width: icon_size,
            height: icon_size,
        };
        // Zero-width label/badge anchored at the centre so the renderer paints
        // nothing visible for them (matches Tauri's `display:none`).
        let centre = Rect {
            x: rect.x + diameter * 0.5,
            y: rect.y + diameter * 0.5,
            width: 0.0,
            height: 0.0,
        };
        return ZonePillLayout {
            rect,
            shadow_outer: Rect {
                x: rect.x,
                y: rect.y + PILL_SHADOW_OUTER_DY,
                width: rect.width,
                height: rect.height,
            },
            shadow_inner: Rect {
                x: rect.x,
                y: rect.y + PILL_SHADOW_INNER_DY,
                width: rect.width,
                height: rect.height,
            },
            icon,
            label: centre,
            badge: centre,
            radius: BorderRadius::all(radius_px),
            badge_radius: BorderRadius::all(RADIUS.badge),
        };
    }

    // Non-circle corner radius from the per-shape Tauri token (resolved
    // against the tier height; circle is handled above with its own radius).
    let radius_px = shape.corner_radius_px(height);
    // Badge box sized by the capsule tier's badge metrics: width from the
    // tier badge font + tier padding, height from `CapsuleSize::badge_height_px`.
    // C18 keeps large capsules visually large while using the reference-frame
    // medium-sized count chip for Browser/Compiler.
    // Badge width stays tied to real glyph presence: N188 needs the existing
    // code-glyph width while reusing only the source-tier content placement.
    let badge_width = pill_badge_width_for_size_count(size, has_visible_glyph, count);
    let badge_height = pill_badge_height_for(size, uses_visible_glyph_content_metrics);
    // The outer `.bento-zone` width is the original Tauri fixed tier box from
    // `CapsuleSize::width_px` (120 / 160 / 200), while the label
    // flexes/shrinks inside it.
    let width = size.width_px().max(PILL_MIN_WIDTH);
    let rect = Rect {
        x,
        y,
        width,
        height,
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
    let content_dy = pill_content_dy_for(size, uses_visible_glyph_content_metrics);
    let icon_y = rect.y + (height - icon_size) * 0.5 + content_dy;
    // G5 — icon left-anchored at the tier's LEFT padding (`--spacing-xl` at
    // medium = 20), not the symmetric 12.
    // V21-C21 — large visible-glyph capsules use the 2026-06-02
    // video-observed Browser slot, while explicit no-glyph capsules keep the
    // CSS 28-DIP left padding plus the C19 residual slot.
    // V21-C19 — explicit no-glyph icons (`""` / `"none"`) suppress paint in
    // `draw_icon_glyph`; retain a 6-DIP layout slot instead of collapsing to
    // zero or reserving the full visible-glyph 18-DIP slot.
    let icon_pad_left = if has_visible_glyph && size == CapsuleSize::Large {
        PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX
    } else {
        pad_left
    };
    let visible_inner_gap = if has_visible_glyph && size == CapsuleSize::Large {
        PILL_LARGE_VISIBLE_GLYPH_INNER_GAP_PX
    } else {
        pad_inner
    };
    let icon_slot_width = if has_visible_glyph {
        icon_size
    } else {
        PILL_NO_GLYPH_ICON_SLOT_PX
    };
    let icon = Rect {
        x: rect.x + icon_pad_left,
        y: icon_y,
        width: icon_slot_width,
        height: icon_size,
    };
    let label_x = icon.x + icon.width + visible_inner_gap;
    // G5.1 (2026-06-08) - layout height must follow the same per-tier title
    // font that the renderer draws (`small=11`, `medium=14`, `large=15`).
    // The previous flat `TYPOGRAPHY.md` line box kept small/large labels
    // vertically offset even after their paint size changed.
    let label_h =
        pill_title_font_px_for_text(size, uses_visible_glyph_content_metrics, display_title)
            * TYPOGRAPHY.md.line_height;
    // G5 — the badge is RIGHT-anchored to the tier's RIGHT padding
    // (`--spacing-lg` at medium = 16); the label fills the gap between the icon
    // run and the badge so a wider badge (3-digit count) eats into the label,
    // matching Tauri's flex layout (icon · title flex:1 · badge).
    let badge_y = rect.y + (height - badge_height) * 0.5 + content_dy;
    let badge_right_inset =
        pill_badge_right_inset_for(size, uses_visible_glyph_content_metrics, pad_right);
    let badge = Rect {
        x: rect.right() - badge_right_inset - badge_width,
        y: badge_y,
        width: badge_width,
        height: badge_height,
    };
    let label = Rect {
        x: label_x,
        y: rect.y + (height - label_h) * 0.5 + content_dy,
        width: (badge.x - visible_inner_gap - label_x).max(0.0),
        height: label_h,
    };
    ZonePillLayout {
        rect,
        shadow_outer,
        shadow_inner,
        icon,
        label,
        badge,
        radius: BorderRadius::all(radius_px),
        badge_radius: BorderRadius::all(RADIUS.badge),
    }
}

/// Build the stack capsule layout anchored at `(zone.x, zone.y)`.
///
/// Tauri renders stacks through `StackCapsule.tsx`, not the ordinary
/// `ZenCapsule`: the visible chrome is a 220x52 grid with up to three
/// overlapped member peek icons, a 28px main icon bubble, a title band, and a
/// 24px member-count badge. This helper is the SSoT for both renderer chrome
/// and shell hit-test so the larger stack capsule cannot drift from its
/// clickable region.
pub fn stack_capsule_layout_for_zone(zone: &Zone, member_count: usize) -> StackCapsuleLayout {
    let rect = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: STACK_CAPSULE_WIDTH_PX,
        height: STACK_CAPSULE_HEIGHT_PX,
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
    let peek_visible_count = member_count.min(STACK_CAPSULE_MAX_PEEK_ICONS);
    let zero_peek = Rect {
        x: rect.x + STACK_CAPSULE_PAD_X_PX,
        y: rect.y + rect.height * 0.5,
        width: 0.0,
        height: 0.0,
    };
    let mut peek_icons = [zero_peek; STACK_CAPSULE_MAX_PEEK_ICONS];
    let peek_stride = STACK_CAPSULE_PEEK_ICON_SIZE_PX - STACK_CAPSULE_PEEK_OVERLAP_PX;
    let peek_y = rect.y + (rect.height - STACK_CAPSULE_PEEK_ICON_SIZE_PX) * 0.5;
    let mut i = 0;
    while i < peek_visible_count {
        peek_icons[i] = Rect {
            x: rect.x + STACK_CAPSULE_PAD_X_PX + i as f32 * peek_stride,
            y: peek_y,
            width: STACK_CAPSULE_PEEK_ICON_SIZE_PX,
            height: STACK_CAPSULE_PEEK_ICON_SIZE_PX,
        };
        i += 1;
    }
    let peek_width = if peek_visible_count == 0 {
        0.0
    } else {
        STACK_CAPSULE_PEEK_ICON_SIZE_PX
            + (peek_visible_count.saturating_sub(1) as f32) * peek_stride
            + STACK_CAPSULE_PEEK_PAD_RIGHT_PX
    };
    let icon_bubble = Rect {
        x: rect.x
            + STACK_CAPSULE_PAD_X_PX
            + peek_width
            + if peek_visible_count > 0 {
                STACK_CAPSULE_GAP_PX
            } else {
                0.0
            },
        y: rect.y + (rect.height - STACK_CAPSULE_MAIN_ICON_BUBBLE_PX) * 0.5,
        width: STACK_CAPSULE_MAIN_ICON_BUBBLE_PX,
        height: STACK_CAPSULE_MAIN_ICON_BUBBLE_PX,
    };
    let icon_glyph = Rect {
        x: icon_bubble.x + (icon_bubble.width - STACK_CAPSULE_MAIN_ICON_GLYPH_PX) * 0.5,
        y: icon_bubble.y + (icon_bubble.height - STACK_CAPSULE_MAIN_ICON_GLYPH_PX) * 0.5,
        width: STACK_CAPSULE_MAIN_ICON_GLYPH_PX,
        height: STACK_CAPSULE_MAIN_ICON_GLYPH_PX,
    };
    let badge_width = stack_capsule_badge_width_for_count(member_count);
    let badge = Rect {
        x: rect.right() - STACK_CAPSULE_PAD_X_PX - badge_width,
        y: rect.y + (rect.height - STACK_CAPSULE_BADGE_HEIGHT_PX) * 0.5,
        width: badge_width,
        height: STACK_CAPSULE_BADGE_HEIGHT_PX,
    };
    let label_x = icon_bubble.right() + STACK_CAPSULE_GAP_PX;
    let label = Rect {
        x: label_x,
        y: rect.y,
        width: (badge.x - STACK_CAPSULE_GAP_PX - label_x).max(0.0),
        height: rect.height,
    };
    StackCapsuleLayout {
        rect,
        shadow_outer,
        shadow_inner,
        peek_icons,
        peek_visible_count,
        icon_bubble,
        icon_glyph,
        label,
        badge,
        radius: BorderRadius::all(STACK_CAPSULE_RADIUS_PX),
        peek_radius: BorderRadius::all(STACK_CAPSULE_PEEK_ICON_SIZE_PX * 0.5),
        icon_radius: BorderRadius::all(STACK_CAPSULE_MAIN_ICON_BUBBLE_PX * 0.5),
        badge_radius: BorderRadius::all(STACK_CAPSULE_BADGE_HEIGHT_PX * 0.5),
    }
}

/// Smallest badge width that fits `count` digits (plus default-min padding).
///
/// G5 (2026-06-01) — legacy helper retained for back-compat / API callers; the
/// live pill now sizes the badge per-tier via [`badge_width_for_size_count`]
/// (Tauri scales both the badge font AND the badge padding by `CapsuleSize`).
/// This fixed-`TYPOGRAPHY.xs` version equals the Medium tier.
pub fn badge_width_for_count(count: usize) -> f32 {
    let digits = digit_count(count);
    let per_digit = TYPOGRAPHY.xs.size_px * 0.62;
    let raw = (digits as f32) * per_digit + SPACING.md;
    raw.max(PILL_BADGE_MIN_WIDTH)
}

/// G5 (2026-06-01) — per-tier badge width: the digit run measured against the
/// tier's badge font ([`CapsuleSize::badge_font_px`]) plus the tier's
/// horizontal badge padding on both sides ([`CapsuleSize::badge_padding_xy`]).
///
/// §10 allocation-free: a digit-count lookup (`digit_count`) times a constant
/// per-digit ratio — NO per-frame DWrite measure, NO `format!`. Clamped to
/// [`PILL_BADGE_MIN_WIDTH`] so a single-digit count still reads as a pill.
pub fn badge_width_for_size_count(size: CapsuleSize, count: usize) -> f32 {
    let digits = digit_count(count);
    // Semibold digit advance ≈ 0.60 em for the YaHei/Segoe numerals at these
    // small sizes; matches the legacy 0.62 ratio used at TYPOGRAPHY.xs (11px).
    let per_digit = size.badge_font_px() * 0.60;
    let (pad_x, _pad_y) = size.badge_padding_xy();
    let raw = (digits as f32) * per_digit + pad_x * 2.0;
    raw.max(PILL_BADGE_MIN_WIDTH)
}

/// Stack capsule badge width from Tauri `min-width: 24px; padding: 0 8px`.
pub fn stack_capsule_badge_width_for_count(count: usize) -> f32 {
    let digits = digit_count(count);
    let per_digit = STACK_CAPSULE_BADGE_FONT_PX * 0.60;
    let raw = digits as f32 * per_digit + STACK_CAPSULE_BADGE_PAD_X_PX * 2.0;
    raw.max(STACK_CAPSULE_BADGE_MIN_WIDTH_PX)
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
