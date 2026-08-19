use super::*;

/// Wave C (05-20 visual parity) — effective hit-test rectangle for `zone`.
///
/// V-13 (2026-05-21) — the hit-rect MUST mirror the rect the renderer is
/// currently painting (paint–hit parity to within 1 DIP). Three cases,
/// matching `Renderer::draw_zones` precisely:
///
/// 1. **Pill-morph present** (a per-Zone `PillMorph` entry exists and the Zone
///    is not a stack anchor) — the
///    renderer paints `morph_pill_to_rect(pill, expanded, eased)` so the
///    hit-rect lerps in lockstep. Without this, a single mouse-move tick
///    after hover starts snapped the hit-rect to the full expanded body
///    while the visual was still a pill — clicks/hover registered in the
///    invisible "phantom" zone box surrounding the pill.
/// 2. **Collapsed pill** (body not visible through `zone_pill_body_visible`) —
///    pill rect from `zone_pill_geometry` is the only clickable region.
/// 3. **Expanded body** (body visible through `zone_pill_body_visible`) — full
///    stored `(x, y, w, h)` rectangle is authoritative.
///
/// Pure / allocation-free.
fn effective_zone_hit_rect(app: &AppState, zone: &Zone, now_ms: u32) -> Rect {
    app.zone_effective_rect_at(zone, now_ms)
}

// -----------------------------------------------------------------------------
// Phase 2.1 Ruling D — zone hit-testing helpers.
// -----------------------------------------------------------------------------

/// DIP edge length of each directional resize corner box.
/// Issue #25 expands the target to 24 DIP without changing resize geometry.
pub const ZONE_RESIZE_CORNER: f32 = 24.0;
/// Continuous edge target thickness between the four corner targets.
pub const ZONE_RESIZE_EDGE: f32 = 8.0;

fn resize_handle_for_panel(panel: Rect, x: f32, y: f32) -> Option<bentodesk_app::ZoneResizeHandle> {
    use bentodesk_app::ZoneResizeHandle;

    if !rect_contains(panel, x, y) {
        return None;
    }
    let left_corner = x < panel.x + ZONE_RESIZE_CORNER;
    let right_corner = x >= panel.right() - ZONE_RESIZE_CORNER;
    let top_corner = y < panel.y + ZONE_RESIZE_CORNER;
    let bottom_corner = y >= panel.bottom() - ZONE_RESIZE_CORNER;

    // Corners win their overlap with the continuous edge strips.
    if left_corner && top_corner {
        return Some(ZoneResizeHandle::TopLeft);
    }
    if right_corner && top_corner {
        return Some(ZoneResizeHandle::TopRight);
    }
    if left_corner && bottom_corner {
        return Some(ZoneResizeHandle::BottomLeft);
    }
    if right_corner && bottom_corner {
        return Some(ZoneResizeHandle::BottomRight);
    }

    if x < panel.x + ZONE_RESIZE_EDGE {
        Some(ZoneResizeHandle::Left)
    } else if x >= panel.right() - ZONE_RESIZE_EDGE {
        Some(ZoneResizeHandle::Right)
    } else if y < panel.y + ZONE_RESIZE_EDGE {
        Some(ZoneResizeHandle::Top)
    } else if y >= panel.bottom() - ZONE_RESIZE_EDGE {
        Some(ZoneResizeHandle::Bottom)
    } else {
        None
    }
}

/// Topmost (= last drawn = highest z) zone whose effective surface contains
/// `(x, y)`. Z-order (2026-06-02): mirror the two-layer draw stack in
/// `Renderer::draw_zones` — test `on_top` (expanded/morphing) zones BEFORE
/// `!on_top` (collapsed pills), so a point inside an expanded panel resolves to
/// the panel, never to a pill drawn behind it (which would otherwise mis-target
/// the buried pill and make the panel collapse/flicker on hover). Within each
/// layer keep the existing reverse/topmost order (newer zones win over older).
/// Uses the shared `AppState::zone_on_top` SSoT so the hit stack and the paint
/// stack can't drift.
pub fn hit_test_zone(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    let now_ms = app.geometry_frame_now_ms.get();
    for on_top_layer in [true, false] {
        for z in app.zones.iter().rev() {
            if !z.is_visible() || z.is_stacked_child() {
                continue;
            }
            if app.zone_on_top_at(z, now_ms) != on_top_layer {
                continue;
            }
            let rect = effective_zone_hit_rect(app, z, now_ms);
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(z.id);
            }
        }
    }
    None
}

/// Topmost item card under `(x, y)`, returning its owning zone, item id,
/// and effective filesystem path. Geometry mirrors `Renderer::draw_zones` so
/// drag-out hit-testing stays aligned with what the user sees.
pub fn hit_test_zone_item(app: &AppState, x: f32, y: f32) -> Option<(ZoneId, ZoneItemId, String)> {
    hit_test_zone_item_ref(app, x, y)
        .map(|(zone_id, item_id, path)| (zone_id, item_id, path.to_owned()))
}

fn hit_test_zone_item_ref(app: &AppState, x: f32, y: f32) -> Option<(ZoneId, ZoneItemId, &str)> {
    let topmost = hit_test_zone(app, x, y)?;
    let now_ms = app.geometry_frame_now_ms.get();
    for z in app.zones.iter().rev() {
        if z.id != topmost || !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        // Wave C — collapsed pill mode hides the item grid, so items attached
        // to a non-expanded zone are not hit-testable. #4 (2026-06-02): a
        // collapsed stack anchor now ALSO renders as a compact pill (no item
        // grid), so it is skipped too; an EXPANDED anchor (focused member) uses
        // the normal panel and its items stay reachable via body_visible.
        if !app.zone_pill_body_visible(z) {
            continue;
        }
        let panel = app.zone_effective_rect_at(z, now_ms);
        if x < panel.x || x >= panel.right() || y < panel.y || y >= panel.bottom() {
            continue;
        }
        let search_active = app.zone_search_target.get() == Some(z.id);
        let search_reveal = if search_active {
            app.zone_search_animation_progress_at(now_ms)
        } else {
            0.0
        };
        let item_top_offset = if search_active {
            search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * search_reveal
        } else {
            0.0
        };
        let content_clip =
            highlight_overlay::item_content_clip_rect_in_panel(panel, item_top_offset);
        if !rect_contains(content_clip, x, y) {
            continue;
        }
        let search_state = app.search_bar.borrow();
        let query = search_state.query.as_str();
        let layout = app.resolve_zone_item_flow_layout(
            z,
            panel,
            item_top_offset,
            z.items.iter().filter(|item| {
                !search_active || search_bar::zone_item_matches_query(item.name.as_ref(), query)
            }),
        );
        let hit = layout.hit_card(x, y)?;
        let item = z.item(hit.item_id)?;
        return Some((z.id, item.id, item.path.as_ref()));
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineZoneSearchHit {
    Body,
    Clear,
}

/// Hit-test the active Tauri-parity inline Zone search input.
pub fn hit_test_inline_zone_search(app: &AppState, x: f32, y: f32) -> Option<InlineZoneSearchHit> {
    if app.zone_search_closing.get() {
        return None;
    }
    let zone_id = app.zone_search_target.get()?;
    let zone = app.zones.get(zone_id)?;
    if hit_test_zone(app, x, y) != Some(zone_id) {
        return None;
    }
    let now_ms = app.geometry_frame_now_ms.get();
    if app.zone_pill_morph_in_flight_at(zone, now_ms) {
        return None;
    }
    let zone_rect = app.zone_effective_rect_at(zone, now_ms);
    let final_input = search_bar::zone_inline_rect(zone_rect);
    let reveal = app.zone_search_animation_progress_at(now_ms);
    let input = Rect {
        x: final_input.right() - final_input.width * reveal,
        width: final_input.width * reveal,
        ..final_input
    };
    if !rect_contains(input, x, y) {
        return None;
    }
    let clear = search_bar::zone_inline_clear_rect(zone_rect);
    if !app.search_bar.borrow().query.is_empty() && rect_contains(clear, x, y) {
        Some(InlineZoneSearchHit::Clear)
    } else {
        Some(InlineZoneSearchHit::Body)
    }
}

/// Grid coordinate under `(x, y)` inside `zone_id`, using the same geometry
/// constants as [`hit_test_zone_item`] and `Renderer::draw_zones`. The shell
/// uses this on item mouse-up so a dragged card can produce a real
/// `Command::MoveItem` instead of only an Explorer drag-out.
pub fn item_grid_position_for_point(
    app: &AppState,
    zone_id: ZoneId,
    x: f32,
    y: f32,
) -> Option<(i32, i32)> {
    let z = app.zones.get(zone_id)?;
    let now_ms = app.geometry_frame_now_ms.get();
    let search_active = app.zone_search_target.get() == Some(zone_id);
    let item_top_offset = if search_active {
        // SAFETY: GetTickCount is total and thread-safe.
        search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * app.zone_search_animation_progress_at(now_ms)
    } else {
        0.0
    };
    let panel = if app.zone_pill_morph_at(zone_id, now_ms).is_some() {
        app.zone_effective_rect_at(z, now_ms)
    } else {
        app.zone_expanded_placement(z).panel
    };
    let search_state = app.search_bar.borrow();
    let query = search_state.query.as_str();
    app.resolve_zone_item_flow_layout(
        z,
        panel,
        item_top_offset,
        z.items.iter().filter(|item| {
            !search_active || search_bar::zone_item_matches_query(item.name.as_ref(), query)
        }),
    )
    .grid_position_for_point(x, y)
}

pub fn item_drop_target_for_point(
    app: &AppState,
    zone_id: ZoneId,
    source_item: Option<ZoneItemId>,
    dragged_item: &bentodesk_zone::ZoneItem,
    x: f32,
    y: f32,
) -> Option<(i32, i32, usize)> {
    let zone = app.zones.get(zone_id)?;
    let now_ms = app.geometry_frame_now_ms.get();
    let item_top_offset = if app.zone_search_target.get() == Some(zone_id) {
        search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * app.zone_search_animation_progress_at(now_ms)
    } else {
        0.0
    };
    let scroll_offset = app.zone_content_scroll_offset(zone_id);
    let search_active = app.zone_search_target.get() == Some(zone_id);
    let search_state = app.search_bar.borrow();
    let search_query = search_state.query.as_str();
    let panel = if app.zone_pill_morph_at(zone_id, now_ms).is_some() {
        app.zone_effective_rect_at(zone, now_ms)
    } else {
        app.zone_expanded_placement(zone).panel
    };
    highlight_overlay::item_drop_target_for_item_in_panel(
        zone,
        source_item,
        dragged_item,
        highlight_overlay::ItemDropProjection {
            panel,
            pointer: (x, y),
            item_top_offset,
            stored_scroll: scroll_offset,
        },
        |item| {
            !search_active || search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
        },
    )
    .map(|(grid_x, grid_y, target_index, _)| (grid_x, grid_y, target_index))
}

/// Capture a resize session only for the topmost visible Zone surface.
pub fn zone_resize_session_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<bentodesk_app::ZoneResizeSession> {
    let now_ms = app.geometry_frame_now_ms.get();
    let z = app.zones.get(hit_test_zone(app, x, y)?)?;
    // Wave C — collapsed pills have no resize handle (they auto-size to
    // their badge + label content). Only expanded zones surface a corner.
    if !app.zone_pill_body_visible(z) {
        return None;
    }
    // The transient shell/card layout is still moving. Arming a resize
    // against it would capture a non-final start size and jump on move.
    if app.zone_pill_morph_in_flight_at(z, now_ms) {
        return None;
    }
    let placement = app.zone_expanded_placement(z);
    let handle = resize_handle_for_panel(placement.panel, x, y)?;
    Some(bentodesk_app::ZoneResizeSession {
        id: z.id,
        start_pointer_x: x,
        start_pointer_y: y,
        handle,
        start_panel: placement.panel,
        start_persisted_width: z.w,
        start_persisted_height: z.h,
        start_home_x: z.x,
        start_home_y: z.y,
        start_capsule: app.zone_collapsed_rect(z),
        anchor_right: placement.anchor_right,
        anchor_bottom: placement.anchor_bottom,
    })
}

/// Actionable resize session after applying the same higher-priority pointer
/// surfaces and lock gate used by Main-client mouse down.
pub fn actionable_zone_resize_session_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<bentodesk_app::ZoneResizeSession> {
    let session = zone_resize_session_for_point(app, x, y)?;
    let zone = app.zones.get(session.id)?;
    if zone.locked
        || app.settings_open.get()
        || app.about_open.get()
        || app.active_context_menu.borrow().is_some()
        || stack_overlay_contains(app, x, y)
        || hit_test_inline_zone_search(app, x, y).is_some()
        || hit_test_zone_item_ref(app, x, y).is_some()
        || hit_test_zone_header_button(app, x, y).is_some()
    {
        return None;
    }
    Some(session)
}

/// Topmost directional resize handle under `(x, y)`.
pub fn hit_test_zone_resize_corner(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    zone_resize_session_for_point(app, x, y).map(|session| session.id)
}

/// GROUP-4 (2026-06-01) — the two action buttons in an expanded zone's
/// `PanelHeader`. Mirrors Tauri's `.panel-header__btn` (search) and
/// `.panel-header__btn--close`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderButton {
    /// Magnifier button → opens search for the zone.
    Search,
    /// X button → collapses the expanded panel back to its pill.
    Close,
}

/// Topmost expanded-zone header action button under `(x, y)`. The button
/// rects come from the paint==hit SSoT (`expanded_zone_grid::ExpandedZoneLayout`)
/// so a click lands exactly on the painted 28×28 glyph. Only surfaced when the
/// zone body is visible (collapsed pills have no header buttons). #4 (2026-06-02):
/// an EXPANDED stack anchor (focused member) now paints the normal `PanelHeader`,
/// so its header buttons are reachable too — only the collapsed (pill) state has
/// none.
pub fn hit_test_zone_header_button(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<(ZoneId, HeaderButton)> {
    let topmost = hit_test_zone(app, x, y)?;
    let now_ms = app.geometry_frame_now_ms.get();
    for z in app.zones.iter().rev() {
        if z.id != topmost || !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        if !app.zone_pill_body_visible(z) {
            continue;
        }
        let layout = expanded_zone_grid::expanded_zone_layout_for_rect(
            app.zone_effective_rect_at(z, now_ms),
            z.items.len(),
        );
        if rect_contains(layout.header_close_btn, x, y) {
            return Some((z.id, HeaderButton::Close));
        }
        if rect_contains(layout.header_search_btn, x, y) {
            return Some((z.id, HeaderButton::Search));
        }
    }
    None
}
