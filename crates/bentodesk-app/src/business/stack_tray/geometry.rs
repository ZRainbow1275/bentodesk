use super::*;

pub fn stack_tray_visible_rows(member_count: usize) -> usize {
    member_count.min(TRAY_VISIBLE_ROW_LIMIT)
}

pub fn stack_tray_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let visible_rows = stack_tray_visible_rows(member_count);
    let height = (TRAY_HEADER_HEIGHT_PX + visible_rows as f32 * TRAY_ROW_STRIDE_PX + TRAY_INSET_PX)
        .max(TRAY_MIN_HEIGHT_PX);
    let anchor_right = anchor.x as f32 + anchor.w as f32;
    let right_candidate = anchor_right + TRAY_GAP_PX;
    let left_candidate = anchor.x as f32 - TRAY_GAP_PX - TRAY_WIDTH_PX;
    let x = if right_candidate + TRAY_WIDTH_PX + TRAY_VIEWPORT_MARGIN_PX <= viewport.width {
        right_candidate
    } else {
        left_candidate
    };
    let max_x =
        (viewport.width - TRAY_WIDTH_PX - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - height - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: (anchor.y as f32).clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: TRAY_WIDTH_PX,
        height,
    }
}

pub fn stack_tray_row_rect(viewport: Size, anchor: &Zone, member_count: usize, row: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.y + TRAY_HEADER_HEIGHT_PX + row as f32 * TRAY_ROW_STRIDE_PX,
        width: tray.width - TRAY_INSET_PX * 2.0,
        height: TRAY_ROW_HEIGHT_PX,
    }
}

pub fn stack_tray_detach_rect(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    row: usize,
) -> Rect {
    let row_rect = stack_tray_row_rect(viewport, anchor, member_count, row);
    Rect {
        x: row_rect.right() - TRAY_DETACH_BUTTON_WIDTH_PX - 6.0,
        y: row_rect.y + 7.0,
        width: TRAY_DETACH_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn stack_tray_dissolve_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.right()
            - TRAY_INSET_PX
            - TRAY_CLOSE_BUTTON_WIDTH_PX
            - TRAY_GAP_PX
            - TRAY_DISSOLVE_BUTTON_WIDTH_PX,
        y: tray.y + 9.0,
        width: TRAY_DISSOLVE_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

/// Whether the side FocusedZonePreview pane should be visible for this tray state.
pub fn focused_preview_visible(anchor_zone_id: ZoneId, selected_member_id: ZoneId) -> bool {
    selected_member_id != anchor_zone_id
}

pub fn stack_tray_header_title_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.y + 10.0,
        width: TRAY_HEADER_TITLE_WIDTH_PX,
        height: TRAY_HEADER_TITLE_HEIGHT_PX,
    }
}

pub fn stack_tray_header_count_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let title = stack_tray_header_title_rect(viewport, anchor, member_count);
    let dissolve = stack_tray_dissolve_rect(viewport, anchor, member_count);
    let x = title.right() + TRAY_GAP_PX;
    let max_width = (dissolve.x - TRAY_GAP_PX - x).max(0.0);
    Rect {
        x,
        y: title.y,
        width: stack_tray_header_count_badge_width(member_count).min(max_width),
        height: TRAY_HEADER_COUNT_HEIGHT_PX,
    }
}

pub fn stack_tray_header_count_badge_width(member_count: usize) -> f32 {
    let text_width = stack_tray_header_count_label_len(member_count) as f32
        * TRAY_HEADER_COUNT_BADGE_DIGIT_WIDTH_PX;
    (text_width + TRAY_HEADER_COUNT_BADGE_PAD_X_PX * 2.0).max(TRAY_HEADER_COUNT_BADGE_MIN_WIDTH_PX)
}

pub fn stack_tray_header_count_label_len(member_count: usize) -> usize {
    if member_count >= 1000 {
        4
    } else if member_count >= 100 {
        3
    } else if member_count >= 10 {
        2
    } else {
        1
    }
}

pub fn stack_tray_member_meta_count_rect(row_rect: Rect) -> Rect {
    Rect {
        x: row_rect.x + TRAY_MEMBER_TEXT_X_PX,
        y: row_rect.y + TRAY_MEMBER_META_Y_PX,
        width: TRAY_MEMBER_META_COUNT_WIDTH_PX,
        height: TRAY_MEMBER_META_HEIGHT_PX,
    }
}

pub fn stack_tray_member_meta_suffix_rect(row_rect: Rect) -> Rect {
    let count = stack_tray_member_meta_count_rect(row_rect);
    let text_width = (row_rect.width - TRAY_MEMBER_TEXT_RESERVED_RIGHT_PX).max(0.0);
    Rect {
        x: count.right() + TRAY_MEMBER_META_GAP_PX,
        y: count.y,
        width: (text_width - TRAY_MEMBER_META_COUNT_WIDTH_PX - TRAY_MEMBER_META_GAP_PX).max(0.0),
        height: count.height,
    }
}

pub fn stack_tray_close_rect(viewport: Size, anchor: &Zone, member_count: usize) -> Rect {
    let tray = stack_tray_rect(viewport, anchor, member_count);
    Rect {
        x: tray.right() - TRAY_INSET_PX - TRAY_CLOSE_BUTTON_WIDTH_PX,
        y: tray.y + 9.0,
        width: TRAY_CLOSE_BUTTON_WIDTH_PX,
        height: TRAY_ACTION_BUTTON_HEIGHT_PX,
    }
}

pub fn stack_tray_status_rect(tray: Rect) -> Rect {
    Rect {
        x: tray.x + TRAY_INSET_PX,
        y: tray.bottom() - TRAY_STATUS_BOTTOM_OFFSET_PX,
        width: tray.width - TRAY_INSET_PX * 2.0,
        height: TRAY_STATUS_HEIGHT_PX,
    }
}

pub fn stack_tray_status_prefix_rect(status_rect: Rect) -> Rect {
    Rect {
        x: status_rect.x,
        y: status_rect.y,
        width: TRAY_STATUS_PREFIX_WIDTH_PX,
        height: status_rect.height,
    }
}

pub fn stack_tray_status_count_rect(status_rect: Rect) -> Rect {
    Rect {
        x: status_rect.x + TRAY_STATUS_PREFIX_WIDTH_PX,
        y: status_rect.y,
        width: TRAY_STATUS_COUNT_WIDTH_PX,
        height: status_rect.height,
    }
}

pub fn stack_tray_status_suffix_rect(status_rect: Rect) -> Rect {
    let x = status_rect.x
        + TRAY_STATUS_PREFIX_WIDTH_PX
        + TRAY_STATUS_COUNT_WIDTH_PX
        + TRAY_STATUS_GAP_PX;
    Rect {
        x,
        y: status_rect.y,
        width: (status_rect.right() - x).max(0.0),
        height: status_rect.height,
    }
}

pub fn focused_preview_meta_number_rect(preview: Rect, index: usize) -> Rect {
    let first_x = preview.x + 16.0;
    let step = PREVIEW_META_NUMBER_WIDTH_PX + PREVIEW_META_MARK_WIDTH_PX + PREVIEW_META_GAP_PX;
    Rect {
        x: first_x + index as f32 * step,
        y: preview.y + 58.0,
        width: PREVIEW_META_NUMBER_WIDTH_PX,
        height: 16.0,
    }
}

pub fn focused_preview_meta_mark_rect(preview: Rect, index: usize) -> Rect {
    let number = focused_preview_meta_number_rect(preview, index);
    Rect {
        x: number.right(),
        y: number.y,
        width: PREVIEW_META_MARK_WIDTH_PX,
        height: number.height,
    }
}

pub fn focused_preview_meta_suffix_rect(preview: Rect) -> Rect {
    let item_number = focused_preview_meta_number_rect(preview, 2);
    let x = item_number.right() + PREVIEW_META_GAP_PX;
    Rect {
        x,
        y: item_number.y,
        width: (preview.right() - 16.0 - x).max(0.0),
        height: item_number.height,
    }
}

pub fn focused_preview_rect(viewport: Size, tray: Rect) -> Rect {
    let right_candidate = tray.right() + PREVIEW_GAP_PX;
    let left_candidate = tray.x - PREVIEW_GAP_PX - PREVIEW_WIDTH_PX;
    let left_available = (tray.x - PREVIEW_GAP_PX - TRAY_VIEWPORT_MARGIN_PX).max(0.0);
    let right_available =
        (viewport.width - TRAY_VIEWPORT_MARGIN_PX - PREVIEW_GAP_PX - tray.right()).max(0.0);
    let right_fits = right_available >= PREVIEW_WIDTH_PX;
    let left_fits = left_available >= PREVIEW_WIDTH_PX;
    let x = if right_fits && (!left_fits || right_available >= left_available) {
        right_candidate
    } else if left_fits {
        left_candidate
    } else if right_available >= left_available {
        right_candidate
    } else {
        left_candidate
    };
    let max_x =
        (viewport.width - PREVIEW_WIDTH_PX - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - PREVIEW_HEIGHT_PX - TRAY_VIEWPORT_MARGIN_PX)
        .max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: tray.y.clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: PREVIEW_WIDTH_PX,
        height: PREVIEW_HEIGHT_PX,
    }
}

/// Place the focused Bloom preview beside the complete visible petal family.
///
/// The original Tauri implementation used only the selected petal as its
/// horizontal anchor. In a multi-member row that lets the preview cover every
/// sibling to the selected petal's right. Keep the selected petal as the
/// vertical/attention anchor, but use the union of all visible petals for side
/// placement. If neither side fits (a wrapped row on a narrow display), move
/// the preview below or above the family before falling back to a clamped side.
/// Paint and hit-test callers pass the same petal slice, so the surface cannot
/// drift from its interactive geometry.
pub fn focused_bloom_preview_rect(
    viewport: Size,
    selected_petal: Rect,
    petals: &[Rect],
    zone: &Zone,
) -> Rect {
    let width = if zone.w > 0 {
        (zone.w as f32).min(FLOATING_PREVIEW_MAX_WIDTH_PX)
    } else {
        FLOATING_PREVIEW_MAX_WIDTH_PX
    };
    let height = if zone.h > 0 {
        (zone.h as f32).min(FLOATING_PREVIEW_MAX_HEIGHT_PX)
    } else {
        FLOATING_PREVIEW_MAX_HEIGHT_PX
    };
    let max_x = (viewport.width - width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX)
        .max(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
    let max_y = (viewport.height - height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX)
        .max(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);

    let family = bloom_petal_family_bounds(selected_petal, petals);
    let right = family.right() + FLOATING_PREVIEW_GAP_PX;
    let left = family.x - FLOATING_PREVIEW_GAP_PX - width;
    let right_fits = right + width <= viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
    let left_fits = left >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;

    let (x, y) = if right_fits || left_fits {
        let x = if right_fits { right } else { left };
        let y =
            if selected_petal.y + height <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX {
                selected_petal.y
            } else {
                selected_petal.bottom() - height
            };
        (x, y.clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_y))
    } else {
        let below = family.bottom() + FLOATING_PREVIEW_GAP_PX;
        let above = family.y - FLOATING_PREVIEW_GAP_PX - height;
        let below_fits = below + height <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
        let above_fits = above >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
        if below_fits || above_fits {
            let x = (selected_petal.x + (selected_petal.width - width) * 0.5)
                .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_x);
            (x, if below_fits { below } else { above })
        } else {
            let right_space = viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX - family.right();
            let left_space = family.x - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX;
            let x = if right_space >= left_space {
                right
            } else {
                left
            }
            .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_x);
            let y = selected_petal
                .y
                .clamp(FLOATING_PREVIEW_VIEWPORT_MARGIN_PX, max_y);
            (x, y)
        }
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn focused_bloom_preview_contains(
    viewport: Size,
    selected_petal: Rect,
    petals: &[Rect],
    zone: &Zone,
    x: f32,
    y: f32,
) -> bool {
    rect_contains(
        focused_bloom_preview_rect(viewport, selected_petal, petals, zone),
        x,
        y,
    )
}

fn bloom_petal_family_bounds(selected_petal: Rect, petals: &[Rect]) -> Rect {
    let mut left = selected_petal.x;
    let mut top = selected_petal.y;
    let mut right = selected_petal.right();
    let mut bottom = selected_petal.bottom();
    for petal in petals {
        left = left.min(petal.x);
        top = top.min(petal.y);
        right = right.max(petal.right());
        bottom = bottom.max(petal.bottom());
    }
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

pub fn focused_bloom_preview_search_rect(preview: Rect) -> Rect {
    Rect {
        x: preview.right() - 70.0,
        y: preview.y + 10.0,
        width: 28.0,
        height: 28.0,
    }
}

pub fn focused_bloom_preview_close_rect(preview: Rect) -> Rect {
    Rect {
        x: preview.right() - 36.0,
        y: preview.y + 10.0,
        width: 28.0,
        height: 28.0,
    }
}

pub fn stack_wrapper_halo_rect(anchor: &Zone, member_count: usize) -> Rect {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    let pad = BLOOM_WRAPPER_BASE_PAD_PX + visible_count as f32 * BLOOM_WRAPPER_MEMBER_PAD_PX;
    Rect {
        x: anchor.x as f32 - pad,
        y: anchor.y as f32 - pad,
        width: anchor.w as f32 + pad * 2.0,
        height: anchor.h as f32 + pad * 2.0,
    }
}
