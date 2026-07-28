use super::*;

/// Shared item-card geometry for Search/Suggestor target mapping and renderer
/// painting. This must stay aligned with the item-grid renderer so the overlay
/// lands on the same real item cards that the user sees.
pub fn item_card_rect_for_grid(zone: &Zone, grid_x: i32, grid_y: i32, is_wide: bool) -> Rect {
    let requested_columns = zone.grid_columns.max(1);
    // P3.5 (1:1) — horizontal grid inset is `--spacing-lg` (16) per side, the
    // same `HEADER_INSET_X` the header band uses, so column 1 aligns under the
    // header icon (was a fixed 8). The available width subtracts `16 × 2`.
    let columns = effective_grid_columns(zone);
    let columns_i = columns.max(1) as i32;
    let requested_columns_i = requested_columns.max(1) as i32;
    let linear_index = grid_y.max(0) * requested_columns_i + grid_x.max(0);
    let effective_grid_x = linear_index % columns_i;
    let effective_grid_y = linear_index / columns_i;
    item_card_rect_for_effective_grid(zone, effective_grid_x, effective_grid_y, columns, is_wide)
}

/// Shared item-card geometry for a concrete zone item. Unlike
/// [`item_card_rect_for_grid`], this mirrors CSS Grid auto-placement: items are
/// placed in `zone.items` order and an `is_wide` card consumes two column slots,
/// so the next visible item cannot paint into the same lane.
pub fn item_card_rect_for_item(zone: &Zone, item: &ZoneItem) -> Rect {
    item_card_rect_for_item_in_panel(
        zone,
        item,
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        },
    )
}

/// CSS-grid flow geometry for a visible item subset (for example inline Zone
/// search results). Returns the painted rectangle and the next free slot, so
/// renderer and hit-testing can reflow the same filtered sequence without
/// cloning or mutating the persisted Zone.
pub fn item_card_rect_for_flow_slot(
    zone: &Zone,
    slot: i32,
    is_wide: bool,
    item_top_offset: f32,
) -> (Rect, i32) {
    item_card_rect_for_flow_slot_in_panel(
        zone,
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        },
        slot,
        is_wide,
        item_top_offset,
    )
}

/// Tauri's `.bento-panel__content` is a real vertical scroll container. These
/// helpers keep the un-clipped 78-DIP card geometry and translate it by the
/// live scroll offset; renderer clipping and shell hit-testing then share the
/// same coordinates instead of shrinking inaccessible bottom rows.
pub fn item_card_rect_for_item_scrolled(zone: &Zone, item: &ZoneItem, scroll_offset: f32) -> Rect {
    let mut rect = item_card_rect_for_item(zone, item);
    rect.y -= scroll_offset.max(0.0);
    rect.height = item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    rect
}

pub fn item_card_rect_for_flow_slot_scrolled(
    zone: &Zone,
    slot: i32,
    is_wide: bool,
    item_top_offset: f32,
    scroll_offset: f32,
) -> (Rect, i32) {
    let (mut rect, next_slot) = item_card_rect_for_flow_slot(zone, slot, is_wide, item_top_offset);
    rect.y -= scroll_offset.max(0.0);
    rect.height = item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    (rect, next_slot)
}

/// Axis-aligned viewport for expanded Zone items. The normal content viewport
/// begins at the 48-DIP header seam; inline search consumes another 44 DIPs.
pub fn item_content_clip_rect(zone: &Zone, item_top_offset: f32) -> Rect {
    let top = zone.y as f32 + expanded_zone_grid::HEADER_BAND_HEIGHT + item_top_offset.max(0.0);
    Rect {
        x: zone.x as f32,
        y: top,
        width: zone.w.max(0) as f32,
        height: (zone.y as f32 + zone.h.max(0) as f32 - top).max(0.0),
    }
}

/// Maximum scroll for the currently visible item flow. `is_wide_items` is the
/// filtered item sequence (all items for normal mode, query matches for inline
/// search), so no temporary vector or cloned Zone is needed.
pub fn item_flow_max_scroll(
    zone: &Zone,
    item_top_offset: f32,
    is_wide_items: impl IntoIterator<Item = bool>,
) -> f32 {
    let columns = effective_grid_columns(zone).max(1) as i32;
    let mut slot = 0_i32;
    let mut last_row = None;
    for is_wide in is_wide_items {
        let span = bounded_column_span(is_wide, columns as u32);
        let column = slot % columns;
        if column + span > columns {
            slot += columns - column;
        }
        last_row = Some(slot / columns);
        slot += span;
    }
    let Some(last_row) = last_row else {
        return 0.0;
    };
    let last_card_bottom = zone.y as f32
        + item_grid::ITEM_GRID_TOP_OFFSET_PX
        + item_top_offset.max(0.0)
        + last_row as f32 * (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX)
        + item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    let content_bottom = last_card_bottom + bentodesk_style::tokens::SPACING.lg;
    (content_bottom - (zone.y + zone.h) as f32).max(0.0)
}

/// CSS-grid flow geometry inside an arbitrary panel rectangle. The floating
/// Bloom preview renders the same `BentoPanel` as an expanded Zone in Tauri,
/// so it must use the same requested column count, wide-card spans, gaps and
/// row height rather than a second two-column layout.
pub fn item_card_rect_for_flow_slot_in_panel(
    zone: &Zone,
    panel: Rect,
    slot: i32,
    is_wide: bool,
    item_top_offset: f32,
) -> (Rect, i32) {
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    );
    let columns_i = columns.max(1) as i32;
    let span = bounded_column_span(is_wide, columns);
    let mut placed_slot = slot.max(0);
    let column = placed_slot % columns_i;
    if column + span > columns_i {
        placed_slot += columns_i - column;
    }
    let mut rect = item_card_rect_for_effective_grid_in_panel(
        zone.grid_columns.max(1),
        panel,
        placed_slot % columns_i,
        placed_slot / columns_i,
        columns,
        is_wide,
    );
    rect.y += item_top_offset;
    rect.height = rect.height.min((panel.bottom() - 8.0 - rect.y).max(0.0));
    (rect, placed_slot + span)
}

/// Grid coordinate beneath a pointer in an arbitrary BentoPanel rectangle.
/// Used by both ordinary expanded Zones and the floating Bloom preview so item
/// reorder/cross-zone drag cannot mistake the preview for empty desktop.
pub fn item_grid_position_for_panel(
    panel: Rect,
    requested_columns: u32,
    x: f32,
    y: f32,
    item_top_offset: f32,
) -> Option<(i32, i32)> {
    if panel.width <= 0.0 || panel.height <= 0.0 {
        return None;
    }
    let inset_x = expanded_zone_grid::HEADER_INSET_X;
    let columns = item_grid::effective_column_count(panel.width, requested_columns.max(1), inset_x)
        .max(1) as i32;
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let cell_w =
        ((panel.width - inset_x * 2.0) - gap * (columns as f32 - 1.0)).max(44.0) / columns as f32;
    let raw_col = ((x - panel.x - inset_x) / (cell_w + gap)).floor() as i32;
    let row_stride = item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX;
    let raw_row = ((y - panel.y - item_grid::ITEM_GRID_TOP_OFFSET_PX - item_top_offset)
        / row_stride)
        .floor() as i32;
    Some((raw_col.clamp(0, columns - 1), raw_row.max(0)))
}

/// Shared item-card geometry for a concrete zone item inside an arbitrary
/// panel rect. Used by the in-flight capsule->panel morph so body content can
/// fade in on the same timeline without cloning or mutating the persisted zone.
pub fn item_card_rect_for_item_in_panel(zone: &Zone, item: &ZoneItem, panel: Rect) -> Rect {
    let columns = effective_grid_columns(zone);
    if let Some(slot) = effective_grid_slot_for_item(zone, item, columns) {
        let columns_i = columns.max(1) as i32;
        return item_card_rect_for_effective_grid_in_panel(
            zone.grid_columns.max(1),
            panel,
            slot % columns_i,
            slot / columns_i,
            columns,
            item.is_wide,
        );
    }
    item_card_rect_for_grid_in_panel(zone, item.x, item.y, item.is_wide, panel)
}

fn item_card_rect_for_grid_in_panel(
    zone: &Zone,
    grid_x: i32,
    grid_y: i32,
    is_wide: bool,
    panel: Rect,
) -> Rect {
    let requested_columns = zone.grid_columns.max(1);
    let columns = item_grid::effective_column_count(
        panel.width,
        requested_columns,
        expanded_zone_grid::HEADER_INSET_X,
    );
    let columns_i = columns.max(1) as i32;
    let requested_columns_i = requested_columns.max(1) as i32;
    let linear_index = grid_y.max(0) * requested_columns_i + grid_x.max(0);
    let effective_grid_x = linear_index % columns_i;
    let effective_grid_y = linear_index / columns_i;
    item_card_rect_for_effective_grid_in_panel(
        requested_columns,
        panel,
        effective_grid_x,
        effective_grid_y,
        columns,
        is_wide,
    )
}

fn effective_grid_columns(zone: &Zone) -> u32 {
    item_grid::effective_column_count(
        zone.w as f32,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    )
}

fn bounded_column_span(is_wide: bool, columns: u32) -> i32 {
    item_grid::column_span_for(is_wide).min(columns.max(1)) as i32
}

fn effective_grid_slot_for_item(zone: &Zone, target: &ZoneItem, columns: u32) -> Option<i32> {
    let columns_i = columns.max(1) as i32;
    let mut slot = 0_i32;
    for item in &zone.items {
        let span = bounded_column_span(item.is_wide, columns);
        let col = slot % columns_i;
        if col + span > columns_i {
            slot += columns_i - col;
        }
        if item.id == target.id {
            return Some(slot);
        }
        slot += span;
    }
    None
}

fn item_card_rect_for_effective_grid(
    zone: &Zone,
    effective_grid_x: i32,
    effective_grid_y: i32,
    columns: u32,
    is_wide: bool,
) -> Rect {
    item_card_rect_for_effective_grid_in_panel(
        zone.grid_columns.max(1),
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        },
        effective_grid_x,
        effective_grid_y,
        columns,
        is_wide,
    )
}

fn item_card_rect_for_effective_grid_in_panel(
    _requested_columns: u32,
    panel: Rect,
    effective_grid_x: i32,
    effective_grid_y: i32,
    columns: u32,
    is_wide: bool,
) -> Rect {
    let zone_left = panel.x;
    let zone_top = panel.y;
    let zone_right = panel.right();
    let zone_bottom = panel.bottom();
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let inset_x = expanded_zone_grid::HEADER_INSET_X;
    let columns_f = columns as f32;
    let cell_w = ((panel.width - inset_x * 2.0) - gap * (columns_f - 1.0)).max(44.0) / columns_f;
    let span = bounded_column_span(is_wide, columns) as f32;
    let item_x = zone_left + inset_x + effective_grid_x as f32 * (cell_w + gap);
    // M2③ (05-31, 1:1): grid starts below the 48-DIP `PanelHeader` band via
    // the shared SSoT offset — keeps the painted grid in lockstep with the
    // shell hit-tests that read the same constant.
    let item_y = zone_top
        + item_grid::ITEM_GRID_TOP_OFFSET_PX
        + effective_grid_y as f32
            * (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX);
    Rect {
        x: item_x,
        y: item_y,
        width: (cell_w * span + gap * (span - 1.0)).min((zone_right - inset_x - item_x).max(0.0)),
        height: item_card::CardVariant::Standard
            .height_px()
            .min((zone_bottom - 8.0 - item_y).max(0.0)),
    }
}

/// Full-zone target used by Search zone hits and live-folder hits.
pub fn zone_target_rect(zone: &Zone) -> HighlightRect {
    HighlightRect::new(zone.x as f32, zone.y as f32, zone.w as f32, zone.h as f32)
}

/// Item target used by Search item hits and Suggestor matching-path previews.
pub fn item_target_rect(zone: &Zone, item: &ZoneItem) -> HighlightRect {
    HighlightRect::from_rect(item_card_rect_for_item(zone, item))
}

/// Renderer paint rect after applying the snap.md inset.
pub fn paint_rect(target: HighlightRect) -> Rect {
    let rect = target.to_rect();
    Rect {
        x: rect.x + TARGET_INSET_PX,
        y: rect.y + TARGET_INSET_PX,
        width: (rect.width - (TARGET_INSET_PX * 2.0)).max(0.0),
        height: (rect.height - (TARGET_INSET_PX * 2.0)).max(0.0),
    }
}

/// Clamp an elapsed pulse value into the repeat-loop phase `0.0..=1.0`.
pub fn pulse_phase(elapsed_ms: u32) -> f32 {
    if PULSE_LOOP_MS == 0 {
        return 0.0;
    }
    (elapsed_ms % PULSE_LOOP_MS) as f32 / PULSE_LOOP_MS as f32
}

/// Expanding halo rect for a desktop-icon pulse.
pub fn pulse_halo_rect(target: &HighlightPulse, phase: f32) -> Rect {
    let clamped = phase.clamp(0.0, 1.0);
    let radius =
        PULSE_HALO_MIN_RADIUS_PX + (PULSE_HALO_RADIUS_PX - PULSE_HALO_MIN_RADIUS_PX) * clamped;
    Rect {
        x: target.x - radius,
        y: target.y - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    }
}

/// Solid center dot rect for a desktop-icon pulse.
pub fn pulse_core_rect(target: &HighlightPulse) -> Rect {
    Rect {
        x: target.x - PULSE_CORE_RADIUS_PX,
        y: target.y - PULSE_CORE_RADIUS_PX,
        width: PULSE_CORE_RADIUS_PX * 2.0,
        height: PULSE_CORE_RADIUS_PX * 2.0,
    }
}

/// Target corner radius from explicit active radius tokens.
pub fn target_radius_from_tokens(radius: RadiusTokens) -> BorderRadius {
    radius.lg
}

/// Target corner radius from the process-default theme.
pub fn target_radius() -> BorderRadius {
    target_radius_from_tokens(radius::DEFAULT)
}
