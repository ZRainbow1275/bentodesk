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

/// DIP edge length of the bottom-right resize corner box. Spec: 12 DIP square
/// is the canonical "easy to grab without crowding the zone body".
pub const ZONE_RESIZE_CORNER: f32 = 12.0;

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
    let now_ms = app.geometry_frame_now_ms.get();
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
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
        let scroll_offset = app.zone_content_scroll_offset(z.id);
        let search_query = search_active.then(|| app.search_bar.borrow().query.clone());
        let mut search_slot = 0;
        for item in &z.items {
            if let Some(query) = search_query.as_ref()
                && !search_bar::zone_item_matches_query(item.name.as_ref(), query.as_str())
            {
                continue;
            }
            // P3.8 paint-hit parity: reuse the same item-card rectangle SSoT as
            // the renderer and highlight overlay. This keeps the 16-DIP horizontal
            // inset, 56-DIP grid top, row height, wide-card span, and bottom clamp
            // in lockstep; the old local 8/16/48 math made hit/drag targets drift
            // from the cards the user actually saw.
            let card = if search_active {
                let (card, next_slot) =
                    highlight_overlay::item_card_rect_for_flow_slot_scrolled_in_panel(
                        z,
                        panel,
                        search_slot,
                        item.is_wide,
                        item_top_offset,
                        scroll_offset,
                    );
                search_slot = next_slot;
                card
            } else {
                highlight_overlay::item_card_rect_for_item_scrolled_in_panel(
                    z,
                    item,
                    panel,
                    scroll_offset,
                )
            };
            if card.width > 0.0
                && card.height > 0.0
                && x >= card.x
                && x < card.right()
                && y >= card.y
                && y < card.bottom()
            {
                return Some((z.id, item.id, item.path.to_string()));
            }
        }
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
    let item_top_offset = if app.zone_search_target.get() == Some(zone_id) {
        // SAFETY: GetTickCount is total and thread-safe.
        search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * app.zone_search_animation_progress_at(now_ms)
    } else {
        0.0
    };
    let scroll_offset = app.zone_content_scroll_offset(zone_id);
    let panel = if app.zone_pill_morph_at(zone_id, now_ms).is_some() {
        app.zone_effective_rect_at(z, now_ms)
    } else {
        app.zone_expanded_placement(z).panel
    };
    highlight_overlay::item_grid_position_for_panel(
        panel,
        z.grid_columns,
        x,
        y + scroll_offset,
        item_top_offset,
    )
}

pub fn item_drop_target_for_point(
    app: &AppState,
    zone_id: ZoneId,
    source_item: Option<ZoneItemId>,
    is_wide: bool,
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
    highlight_overlay::item_drop_target_for_panel(
        zone,
        panel,
        source_item,
        is_wide,
        (x, y + scroll_offset),
        item_top_offset,
        |item| {
            !search_active || search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
        },
    )
    .map(|(grid_x, grid_y, target_index, _)| (grid_x, grid_y, target_index))
}

/// Capture the topmost directional resize session under `(x, y)`.
///
/// The handle is opposite the capsule's fixed horizontal/vertical anchor, so
/// all four desktop quadrants resize without translating the anchored edge.
pub fn zone_resize_session_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<bentodesk_app::ZoneResizeSession> {
    let now_ms = app.geometry_frame_now_ms.get();
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        // Wave C — collapsed pills have no resize handle (they auto-size to
        // their badge + label content). Only expanded zones surface the
        // bottom-right resize corner. #4 (2026-06-02): a collapsed stack anchor
        // is now a compact pill too, so it has no resize handle either.
        if !app.zone_pill_body_visible(z) {
            continue;
        }
        // The transient shell/card layout is still moving. Arming a resize
        // against it would capture a non-final start size and jump on move.
        if app.zone_pill_morph_in_flight_at(z, now_ms) {
            continue;
        }
        let placement = app.zone_expanded_placement(z);
        let panel = placement.panel;
        let corner_left = if placement.anchor_right {
            panel.x
        } else {
            panel.right() - ZONE_RESIZE_CORNER
        };
        let corner_right = if placement.anchor_right {
            panel.x + ZONE_RESIZE_CORNER
        } else {
            panel.right()
        };
        let corner_top = if placement.anchor_bottom {
            panel.y
        } else {
            panel.bottom() - ZONE_RESIZE_CORNER
        };
        let corner_bottom = if placement.anchor_bottom {
            panel.y + ZONE_RESIZE_CORNER
        } else {
            panel.bottom()
        };
        if x >= corner_left && x < corner_right && y >= corner_top && y < corner_bottom {
            return Some(bentodesk_app::ZoneResizeSession {
                id: z.id,
                start_pointer_x: x,
                start_pointer_y: y,
                start_visible_width: panel.width,
                start_visible_height: panel.height,
                anchor_right: placement.anchor_right,
                anchor_bottom: placement.anchor_bottom,
            });
        }
    }
    None
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
    let now_ms = app.geometry_frame_now_ms.get();
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
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
