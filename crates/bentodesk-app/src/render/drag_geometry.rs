use super::*;

pub(super) fn active_item_drag_visual(app: &AppState) -> Option<ActiveItemDragVisual> {
    let drag = app.item_drag.borrow();
    let candidate = drag.as_ref()?;
    if !candidate.is_internal_dragging {
        return None;
    }
    Some(ActiveItemDragVisual {
        zone_id: candidate.zone_id,
        item_id: candidate.item_id,
        last_x: candidate.last_x as f32,
        last_y: candidate.last_y as f32,
    })
}

pub(super) fn hit_test_render_zone(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    // Z-order (2026-06-02) — mirror the two-layer draw stack: test `on_top`
    // (expanded/morphing) zones BEFORE `!on_top` (pills) so a point inside an
    // expanded panel resolves to the panel, never a pill drawn behind it. Within
    // each layer keep the existing reverse/topmost order. Uses the shared
    // `AppState::zone_on_top` SSoT so this drag-drop targeting can't drift from
    // the painted stack. (Drop targeting keys off the full stored zone rect.)
    for on_top_layer in [true, false] {
        for zone in app.zones.iter().rev() {
            if !zone.is_visible() || zone.is_stacked_child() {
                continue;
            }
            if app.zone_on_top(zone) != on_top_layer {
                continue;
            }
            let left = zone.x as f32;
            let top = zone.y as f32;
            let right = left + zone.w as f32;
            let bottom = top + zone.h as f32;
            if x >= left && x < right && y >= top && y < bottom {
                return Some(zone.id);
            }
        }
    }
    None
}

pub(super) fn drop_preview_rect_for_zone(
    zone: &Zone,
    drag: Option<ActiveItemDragVisual>,
    is_wide: bool,
    scroll_offset: f32,
    item_top_offset: f32,
) -> Option<bentodesk_style::Rect> {
    let drag = drag?;
    let (grid_x, grid_y) = item_grid_position_for_zone(
        zone,
        drag.last_x,
        drag.last_y,
        scroll_offset,
        item_top_offset,
    );
    let mut rect = item_card_rect_for_grid(zone, grid_x, grid_y, is_wide);
    rect.y += item_top_offset - scroll_offset;
    rect.height = item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

pub(super) fn item_grid_position_for_zone(
    zone: &Zone,
    x: f32,
    y: f32,
    scroll_offset: f32,
    item_top_offset: f32,
) -> (i32, i32) {
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    // P3.5 (1:1) — mirror the paint-side horizontal grid inset (`HEADER_INSET_X`
    // = 16 per side) so the drag-position hit math stays in lockstep with the
    // painted card rects (`highlight_overlay::item_card_rect_for_grid`).
    let inset_x = expanded_zone_grid::HEADER_INSET_X;
    let columns =
        item_grid::effective_column_count(zone.w as f32, zone.grid_columns.max(1), inset_x).max(1)
            as i32;
    let columns_f = columns as f32;
    let cell_w = ((zone.w as f32 - inset_x * 2.0) - gap * (columns_f - 1.0)).max(44.0) / columns_f;
    let col_stride = cell_w + gap;
    let row_stride = item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX;
    let raw_col = ((x - zone.x as f32 - inset_x) / col_stride).floor() as i32;
    let raw_row =
        ((y + scroll_offset - zone.y as f32 - item_grid::ITEM_GRID_TOP_OFFSET_PX - item_top_offset)
            / row_stride)
            .floor() as i32;
    (raw_col.clamp(0, columns - 1), raw_row.max(0))
}

pub(super) fn item_card_rect_for_grid(
    zone: &Zone,
    grid_x: i32,
    grid_y: i32,
    is_wide: bool,
) -> bentodesk_style::Rect {
    highlight_overlay::item_card_rect_for_grid(zone, grid_x, grid_y, is_wide)
}

pub(super) fn item_card_rect_for_item(zone: &Zone, item: &ZoneItem) -> bentodesk_style::Rect {
    highlight_overlay::item_card_rect_for_item(zone, item)
}

pub(super) fn source_drag_item(
    app: &AppState,
    drag: ActiveItemDragVisual,
) -> Option<(&Zone, &ZoneItem)> {
    let zone = app.zones.get(drag.zone_id)?;
    let item = zone.item(drag.item_id)?;
    Some((zone, item))
}

pub(super) fn drag_ghost_rect(
    app: &AppState,
    drag: ActiveItemDragVisual,
    source_rect: bentodesk_style::Rect,
) -> bentodesk_style::Rect {
    let width = source_rect.width.max(64.0);
    let height = source_rect.height.max(48.0);
    let max_x = (app.viewport.width - width).max(0.0);
    let max_y = (app.viewport.height - height).max(0.0);
    bentodesk_style::Rect {
        x: (drag.last_x - width * 0.5).clamp(0.0, max_x),
        y: (drag.last_y - 18.0).clamp(0.0, max_y),
        width,
        height,
    }
}

pub(super) fn inset_rect(rect: bentodesk_style::Rect, inset: f32) -> bentodesk_style::Rect {
    bentodesk_style::Rect {
        x: rect.x + inset,
        y: rect.y + inset,
        width: (rect.width - inset * 2.0).max(0.0),
        height: (rect.height - inset * 2.0).max(0.0),
    }
}

pub(super) fn centered_square_rect(
    rect: bentodesk_style::Rect,
    size: f32,
) -> bentodesk_style::Rect {
    let size = size.max(0.0).min(rect.width).min(rect.height);
    bentodesk_style::Rect {
        x: rect.x + (rect.width - size) * 0.5,
        y: rect.y + (rect.height - size) * 0.5,
        width: size,
        height: size,
    }
}

#[inline]
pub(super) fn stack_bloom_active_transition_t(now_ms: u32, started_ms: u32) -> f32 {
    let raw = now_ms.wrapping_sub(started_ms) as f32 / STACK_BLOOM_ACTIVE_TRANSITION_MS as f32;
    animator::ease_in_out_quad(raw.clamp(0.0, 1.0))
}

/// Return the active petal's crisp outer-halo spread and alpha.
///
/// This deliberately models only the CSS spread rings. Reintroducing the
/// reference's blurred black elevation layers would recreate R13-01's broad
/// dark cloud in the native renderer.
#[inline]
pub(super) fn stack_bloom_active_pulse(
    now_ms: u32,
    started_ms: u32,
    many_members: bool,
) -> (f32, f32) {
    if many_members {
        return (4.0, 0.18);
    }
    let elapsed = now_ms.wrapping_sub(started_ms);
    if elapsed <= STACK_BLOOM_ACTIVE_PULSE_DELAY_MS {
        return (5.5, 0.16);
    }
    let phase = (elapsed - STACK_BLOOM_ACTIVE_PULSE_DELAY_MS) % STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS;
    let phase = phase as f32 / STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS as f32;
    let triangle = if phase <= 0.5 {
        phase * 2.0
    } else {
        (1.0 - phase) * 2.0
    };
    let t = animator::ease_in_out_quad(triangle);
    (5.5 + 1.5 * t, 0.16 + 0.06 * t)
}

// =============================================================================
// M6c — pure effect geometry (testable, no GPU). The 3 render primitives
// (`draw_scanline_overlay` / `draw_neon_glow` / `draw_text_chromatic_title`)
// delegate their math here so it can be unit-tested without a live D2D target
// (§3.4: no offscreen render harness exists). Every helper is allocation-free
// stack-`f32` math (§10) and panic-free (§11).
// =============================================================================

/// M6c scanline — the number of 1-DIP-tall lit bands a full-viewport overlay
/// of height `vh` paints at period `period`. Bands sit at `y = k * period` for
/// `k = 0..count`, so `count = ceil(vh / period)`. A non-positive period or
/// height yields 0 (the overlay no-ops). Pure (§10), panic-free (§11).
pub(super) fn scanline_band_count(vh: f32, period: f32) -> usize {
    if vh <= 0.0 || period <= 0.0 {
        return 0;
    }
    (vh / period).ceil() as usize
}

/// W13-B — retain only zero-blur outline/ring geometry from a shadow token.
/// Blurred layers return `None`; drawing them as larger solid fills creates a
/// dark halo rather than a Gaussian shadow.
pub(super) fn crisp_shadow_rect(
    base: bentodesk_style::Rect,
    layer: bentodesk_style::Shadow,
) -> Option<bentodesk_style::Rect> {
    if layer.color.a <= 0.0 || layer.blur > 0.5 {
        return None;
    }
    let grow = layer.spread.max(0.0);
    Some(bentodesk_style::Rect {
        x: base.x + layer.offset_x - grow,
        y: base.y + layer.offset_y - grow,
        width: base.width + grow * 2.0,
        height: base.height + grow * 2.0,
    })
}

/// M6c neon — grow a base rect by `blur` on all four sides (the `drop-shadow(0
/// 0 Npx)` symmetric bloom: 0,0 offset, grown by the blur radius). Mirrors the
/// `draw_shadow_stack` grow-and-fill idiom. Pure (§10).
pub(super) fn neon_glow_rect(base: bentodesk_style::Rect, blur: f32) -> bentodesk_style::Rect {
    let grow = blur.max(0.0);
    bentodesk_style::Rect {
        x: base.x - grow,
        y: base.y - grow,
        width: base.width + grow * 2.0,
        height: base.height + grow * 2.0,
    }
}

/// M6c chromatic — the two channel-copy x-origins for an `h1`/`h2` glyph run:
/// red at `base_x + dx`, cyan at `base_x - dx` (Tauri `text-shadow 1px 0` /
/// `-1px 0`). Returns `(red_x, cyan_x)`. Pure (§10).
pub(super) fn chromatic_split_offsets(base_x: f32, dx: f32) -> (f32, f32) {
    (base_x + dx, base_x - dx)
}

/// M6c neon (morph path) — lerp one neon glow `Shadow` layer from its collapsed
/// endpoint `a` to its expanded endpoint `b` by `t` (clamped 0..=1). Blur and
/// every colour channel interpolate so the capsule<->panel morph grows the
/// bloom smoothly with no pop at either endpoint. Pure (§10).
pub(super) fn lerp_neon_layer(
    a: bentodesk_style::Shadow,
    b: bentodesk_style::Shadow,
    t: f32,
) -> bentodesk_style::Shadow {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    bentodesk_style::Shadow::drop(
        0.0,
        0.0,
        lerp(a.blur, b.blur),
        Color {
            r: lerp(a.color.r, b.color.r),
            g: lerp(a.color.g, b.color.g),
            b: lerp(a.color.b, b.color.b),
            a: lerp(a.color.a, b.color.a),
        },
    )
}
