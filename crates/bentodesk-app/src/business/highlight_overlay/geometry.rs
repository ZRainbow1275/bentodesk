use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemFlowCard {
    pub item_id: ZoneItemId,
    pub rect: Rect,
    pub grid_x: i32,
    pub grid_y: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemFlowLayout {
    pub cards: SmallVec<[ItemFlowCard; 16]>,
    pub content_bottom: f32,
    pub max_scroll: f32,
    pub resolved_scroll: f32,
    panel: Rect,
    requested_columns: i32,
    columns: i32,
    cell_width: f32,
    first_row_top: f32,
    row_tops: SmallVec<[f32; 16]>,
    row_heights: SmallVec<[f32; 16]>,
}

impl ItemFlowLayout {
    #[inline]
    pub fn card_for(&self, item_id: ZoneItemId) -> Option<ItemFlowCard> {
        self.cards
            .iter()
            .copied()
            .find(|card| card.item_id == item_id)
    }

    #[inline]
    pub fn hit_card(&self, x: f32, y: f32) -> Option<ItemFlowCard> {
        self.cards.iter().copied().find(|card| {
            card.rect.width > 0.0
                && card.rect.height > 0.0
                && x >= card.rect.x
                && x < card.rect.right()
                && y >= card.rect.y
                && y < card.rect.bottom()
        })
    }

    /// Persisted grid coordinate beneath a pointer in this exact variable-row
    /// layout. Vertical gaps remain associated with the row above, matching the
    /// former fixed-stride floor behavior; space below the final row maps to an
    /// append row. The returned coordinate is converted back into the Zone's
    /// configured column space.
    pub fn grid_position_for_point(&self, x: f32, y: f32) -> Option<(i32, i32)> {
        if self.panel.width <= 0.0 || self.panel.height <= 0.0 {
            return None;
        }
        let stride = (self.cell_width + item_grid::ITEM_GRID_COLUMN_GAP_PX).max(1.0);
        let column =
            ((x - self.panel.x - expanded_zone_grid::HEADER_INSET_X) / stride).floor() as i32;
        let column = column.clamp(0, self.columns - 1);
        let content_y = y + self.resolved_scroll;
        let row = if self.row_tops.is_empty() {
            ((content_y - self.first_row_top)
                / (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX))
                .floor()
                .max(0.0) as i32
        } else if content_y < self.row_tops[0] {
            0
        } else {
            self.row_tops
                .iter()
                .zip(&self.row_heights)
                .position(|(top, height)| {
                    content_y < *top + *height + item_grid::ITEM_GRID_ROW_GAP_PX
                })
                .map_or(self.row_tops.len() as i32, |index| index as i32)
        };
        let effective_slot = row * self.columns + column;
        Some((
            effective_slot % self.requested_columns,
            effective_slot / self.requested_columns,
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct ItemFlowSeed {
    item_id: ZoneItemId,
    row: i32,
    column: i32,
    width: f32,
}

/// Variable-height CSS-style item flow. Every consumer receives the same card
/// rectangles and the same centrally resolved scroll value.
pub fn item_flow_layout_in_panel<'a>(
    zone: &Zone,
    panel: Rect,
    item_top_offset: f32,
    stored_scroll: f32,
    items: impl IntoIterator<Item = &'a ZoneItem>,
) -> ItemFlowLayout {
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    )
    .max(1);
    let columns_i = columns as i32;
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let inset = expanded_zone_grid::HEADER_INSET_X;
    let usable_width = (panel.width - inset * 2.0 - gap * (columns as f32 - 1.0)).max(0.0);
    let cell_width = usable_width / columns as f32;
    let mut seeds = SmallVec::<[ItemFlowSeed; 16]>::new();
    let mut row_heights = SmallVec::<[f32; 16]>::new();
    let mut slot = 0_i32;

    for item in items {
        let (placed_slot, next_slot) = flow_slots(slot, item.is_wide, columns);
        slot = next_slot;
        let row = placed_slot / columns_i;
        let column = placed_slot % columns_i;
        let span = bounded_column_span(item.is_wide, columns) as f32;
        let x = panel.x + inset + column as f32 * (cell_width + gap);
        let width =
            (cell_width * span + gap * (span - 1.0)).min((panel.right() - inset - x).max(0.0));
        let required =
            item_grid::responsive_item_metrics(width, item.name.as_ref()).required_height;
        while row_heights.len() <= row as usize {
            row_heights.push(item_grid::ITEM_GRID_ROW_HEIGHT_PX);
        }
        row_heights[row as usize] = row_heights[row as usize].max(required);
        seeds.push(ItemFlowSeed {
            item_id: item.id,
            row,
            column,
            width,
        });
    }

    let first_top = panel.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + item_top_offset.max(0.0);
    let mut row_tops = SmallVec::<[f32; 16]>::new();
    let mut next_top = first_top;
    for height in &row_heights {
        row_tops.push(next_top);
        next_top += *height + item_grid::ITEM_GRID_ROW_GAP_PX;
    }
    let content_bottom = row_heights
        .last()
        .map(|_| next_top - item_grid::ITEM_GRID_ROW_GAP_PX + bentodesk_style::tokens::SPACING.lg)
        .unwrap_or(first_top);
    let visible_bottom = panel.bottom() - item_grid::ITEM_GRID_BOTTOM_RESIZE_INSET_PX;
    let max_scroll = (content_bottom - visible_bottom).max(0.0);
    let resolved_scroll = if stored_scroll.is_finite() {
        stored_scroll.clamp(0.0, max_scroll)
    } else {
        0.0
    };
    let mut cards = SmallVec::<[ItemFlowCard; 16]>::new();
    for seed in seeds {
        let x = panel.x + inset + seed.column as f32 * (cell_width + gap);
        cards.push(ItemFlowCard {
            item_id: seed.item_id,
            rect: Rect {
                x,
                y: row_tops[seed.row as usize] - resolved_scroll,
                width: seed.width,
                height: row_heights[seed.row as usize],
            },
            grid_x: seed.column,
            grid_y: seed.row,
        });
    }
    ItemFlowLayout {
        cards,
        content_bottom,
        max_scroll,
        resolved_scroll,
        panel,
        requested_columns: zone.grid_columns.max(1) as i32,
        columns: columns_i,
        cell_width,
        first_row_top: first_top,
        row_tops,
        row_heights,
    }
}

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
    item_card_rect_for_item_scrolled_in_panel(
        zone,
        item,
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        },
        scroll_offset,
    )
}

/// Scrolled item-card geometry inside the resolved visible panel rectangle.
pub fn item_card_rect_for_item_scrolled_in_panel(
    zone: &Zone,
    item: &ZoneItem,
    panel: Rect,
    scroll_offset: f32,
) -> Rect {
    let mut rect = item_card_rect_for_item_in_panel(zone, item, panel);
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
    item_card_rect_for_flow_slot_scrolled_in_panel(
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
        scroll_offset,
    )
}

/// Scrolled CSS-grid flow geometry inside the resolved visible panel.
pub fn item_card_rect_for_flow_slot_scrolled_in_panel(
    zone: &Zone,
    panel: Rect,
    slot: i32,
    is_wide: bool,
    item_top_offset: f32,
    scroll_offset: f32,
) -> (Rect, i32) {
    let (mut rect, next_slot) =
        item_card_rect_for_flow_slot_in_panel(zone, panel, slot, is_wide, item_top_offset);
    rect.y -= scroll_offset.max(0.0);
    rect.height = item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    (rect, next_slot)
}

/// Axis-aligned viewport for expanded Zone items. The normal content viewport
/// begins at the 48-DIP header seam; inline search consumes another 44 DIPs.
pub fn item_content_clip_rect(zone: &Zone, item_top_offset: f32) -> Rect {
    item_content_clip_rect_in_panel(
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w.max(0) as f32,
            height: zone.h.max(0) as f32,
        },
        item_top_offset,
    )
}

/// Axis-aligned item viewport inside the resolved visible panel rectangle.
pub fn item_content_clip_rect_in_panel(panel: Rect, item_top_offset: f32) -> Rect {
    let top = panel.y + expanded_zone_grid::HEADER_BAND_HEIGHT + item_top_offset.max(0.0);
    let bottom = panel.bottom() - item_grid::ITEM_GRID_BOTTOM_RESIZE_INSET_PX;
    Rect {
        x: panel.x,
        y: top,
        width: panel.width.max(0.0),
        height: (bottom - top).max(0.0),
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
    item_flow_max_scroll_in_panel(
        zone,
        Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w.max(0) as f32,
            height: zone.h.max(0) as f32,
        },
        item_top_offset,
        is_wide_items,
    )
}

/// Maximum scroll for the visible item flow inside a resolved panel rectangle.
pub fn item_flow_max_scroll_in_panel(
    zone: &Zone,
    panel: Rect,
    item_top_offset: f32,
    is_wide_items: impl IntoIterator<Item = bool>,
) -> f32 {
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    )
    .max(1) as i32;
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
    let last_card_bottom = panel.y
        + item_grid::ITEM_GRID_TOP_OFFSET_PX
        + item_top_offset.max(0.0)
        + last_row as f32 * (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX)
        + item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    let content_bottom = last_card_bottom + bentodesk_style::tokens::SPACING.lg;
    (content_bottom - (panel.bottom() - item_grid::ITEM_GRID_BOTTOM_RESIZE_INSET_PX)).max(0.0)
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
    let (placed_slot, next_slot) = flow_slots(slot, is_wide, columns);
    let mut rect = item_card_rect_for_effective_grid_in_panel(
        zone.grid_columns.max(1),
        panel,
        placed_slot % columns_i,
        placed_slot / columns_i,
        columns,
        is_wide,
    );
    rect.y += item_top_offset;
    rect.height = rect
        .height
        .min((panel.bottom() - item_grid::ITEM_GRID_BOTTOM_RESIZE_INSET_PX - rect.y).max(0.0));
    (rect, next_slot)
}

/// Canonical persisted grid coordinate beneath a pointer in an arbitrary
/// BentoPanel rectangle. The pointer is resolved in the Zone's configured
/// column space. Drop callers should use [`item_drop_target_for_panel`] so CSS
/// auto-flow order and wide cards are included as well.
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
    let effective_slot = raw_row.max(0) * columns + raw_col.clamp(0, columns - 1);
    let requested_columns = requested_columns.max(1) as i32;
    Some((
        effective_slot % requested_columns,
        effective_slot / requested_columns,
    ))
}

/// Resolve one item drop into the persisted grid point, post-removal insertion
/// index, and the exact rectangle the renderer will use after that insertion.
/// The target index follows the existing CSS-style auto-flow order, including
/// wide-card spans and the Zone's configured column count.
pub fn item_drop_target_for_panel(
    zone: &Zone,
    panel: Rect,
    source_item: Option<ZoneItemId>,
    is_wide: bool,
    pointer: (f32, f32),
    item_top_offset: f32,
    mut item_visible: impl FnMut(&ZoneItem) -> bool,
) -> Option<(i32, i32, usize, Rect)> {
    let (x, y) = pointer;
    let (pointer_x, pointer_y) =
        item_grid_position_for_panel(panel, zone.grid_columns.max(1), x, y, item_top_offset)?;
    let requested_columns = zone.grid_columns.max(1) as i32;
    let pointer_slot = pointer_y * requested_columns + pointer_x;
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    )
    .max(1);
    let mut slot = 0_i32;
    let post_removal_len = zone.items.len().saturating_sub(usize::from(
        source_item.is_some_and(|id| zone.item(id).is_some()),
    ));
    let mut target_index = post_removal_len;
    let mut post_removal_index = 0_usize;
    for item in &zone.items {
        if Some(item.id) == source_item {
            continue;
        }
        if !item_visible(item) {
            post_removal_index += 1;
            continue;
        }
        let (_, next_slot) = flow_slots(slot, item.is_wide, columns);
        if pointer_slot < next_slot {
            target_index = post_removal_index;
            break;
        }
        slot = next_slot;
        target_index = post_removal_index + 1;
        post_removal_index += 1;
    }
    let (placed_slot, _) = flow_slots(slot, is_wide, columns);
    let columns_i = columns as i32;
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
    Some((
        placed_slot % requested_columns,
        placed_slot / requested_columns,
        target_index,
        rect,
    ))
}

/// Full-item drag projection. The preview is laid out after removing the
/// source (for an in-Zone reorder) and inserting the actual dragged ZoneItem,
/// so its name-driven row height and wide span exactly match the committed
/// result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemDropProjection {
    pub panel: Rect,
    pub pointer: (f32, f32),
    pub item_top_offset: f32,
    pub stored_scroll: f32,
}

pub fn item_drop_target_for_item_in_panel(
    zone: &Zone,
    source_item: Option<ZoneItemId>,
    dragged: &ZoneItem,
    projection: ItemDropProjection,
    mut item_visible: impl FnMut(&ZoneItem) -> bool,
) -> Option<(i32, i32, usize, Rect)> {
    let ItemDropProjection {
        panel,
        pointer,
        item_top_offset,
        stored_scroll,
    } = projection;
    let current = item_flow_layout_in_panel(
        zone,
        panel,
        item_top_offset,
        stored_scroll,
        zone.items.iter().filter(|item| item_visible(item)),
    );
    let requested_columns = zone.grid_columns.max(1) as i32;
    let pointer_grid = current
        .hit_card(pointer.0, pointer.1)
        .map(|card| (card.grid_x, card.grid_y))
        .or_else(|| current.grid_position_for_point(pointer.0, pointer.1))?;
    let pointer_slot = pointer_grid.1 * requested_columns + pointer_grid.0;
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    )
    .max(1);
    let mut slot = 0_i32;
    let mut target_index = zone.items.len().saturating_sub(usize::from(
        source_item.is_some_and(|id| zone.item(id).is_some()),
    ));
    let mut post_removal_index = 0_usize;
    for item in &zone.items {
        if Some(item.id) == source_item {
            continue;
        }
        if !item_visible(item) {
            post_removal_index += 1;
            continue;
        }
        let (_, next_slot) = flow_slots(slot, item.is_wide, columns);
        if pointer_slot < next_slot {
            target_index = post_removal_index;
            break;
        }
        slot = next_slot;
        target_index = post_removal_index + 1;
        post_removal_index += 1;
    }

    let mut projected = SmallVec::<[&ZoneItem; 16]>::new();
    for item in &zone.items {
        if Some(item.id) != source_item {
            projected.push(item);
        }
    }
    projected.insert(target_index.min(projected.len()), dragged);
    let mut projected_visible = SmallVec::<[&ZoneItem; 16]>::new();
    let mut dragged_visible_index = None;
    for item in projected {
        if item_visible(item) {
            if std::ptr::eq(item, dragged) {
                dragged_visible_index = Some(projected_visible.len());
            }
            projected_visible.push(item);
        }
    }
    let dragged_visible_index = dragged_visible_index?;
    let projected_layout = item_flow_layout_in_panel(
        zone,
        panel,
        item_top_offset,
        current.resolved_scroll,
        projected_visible,
    );
    let preview = projected_layout.cards.get(dragged_visible_index).copied()?;
    Some((preview.grid_x, preview.grid_y, target_index, preview.rect))
}

/// Shared item-card geometry for a concrete zone item inside an arbitrary
/// panel rect. Used by the in-flight capsule->panel morph so body content can
/// fade in on the same timeline without cloning or mutating the persisted zone.
pub fn item_card_rect_for_item_in_panel(zone: &Zone, item: &ZoneItem, panel: Rect) -> Rect {
    let columns = item_grid::effective_column_count(
        panel.width,
        zone.grid_columns.max(1),
        expanded_zone_grid::HEADER_INSET_X,
    );
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

/// Shared grid-slot geometry inside the resolved visible panel rectangle.
pub fn item_card_rect_for_grid_in_panel(
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

fn flow_slots(slot: i32, is_wide: bool, columns: u32) -> (i32, i32) {
    let columns_i = columns.max(1) as i32;
    let span = bounded_column_span(is_wide, columns);
    let mut placed_slot = slot.max(0);
    let column = placed_slot % columns_i;
    if column + span > columns_i {
        placed_slot += columns_i - column;
    }
    (placed_slot, placed_slot + span)
}

fn effective_grid_slot_for_item(zone: &Zone, target: &ZoneItem, columns: u32) -> Option<i32> {
    let mut slot = 0_i32;
    for item in &zone.items {
        let (placed_slot, next_slot) = flow_slots(slot, item.is_wide, columns);
        if item.id == target.id {
            return Some(placed_slot);
        }
        slot = next_slot;
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

#[path = "geometry/targets.rs"]
mod targets;
pub use targets::*;
