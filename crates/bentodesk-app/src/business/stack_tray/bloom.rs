use super::*;

pub fn stack_bloom_petal_size(member_count: usize) -> StackBloomPetalSize {
    if member_count <= 4 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_PX,
            height: BLOOM_PETAL_HEIGHT_PX,
            icon_size: BLOOM_PETAL_ICON_PX,
        }
    } else if member_count <= 8 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_MEDIUM_PX,
            height: BLOOM_PETAL_HEIGHT_MEDIUM_PX,
            icon_size: BLOOM_PETAL_ICON_MEDIUM_PX,
        }
    } else if member_count <= 16 {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_DENSE_PX,
            height: BLOOM_PETAL_HEIGHT_DENSE_PX,
            icon_size: BLOOM_PETAL_ICON_DENSE_PX,
        }
    } else {
        StackBloomPetalSize {
            width: BLOOM_PETAL_WIDTH_COMPACT_PX,
            height: BLOOM_PETAL_HEIGHT_COMPACT_PX,
            icon_size: BLOOM_PETAL_ICON_COMPACT_PX,
        }
    }
}

pub fn stack_bloom_petal_content_layout(
    petal_rect: Rect,
    icon_size: f32,
    scale: f32,
) -> StackBloomPetalContentLayout {
    let scale = scale.max(0.01);
    let pad_x = BLOOM_PETAL_PADDING_X_PX * scale;
    let pad_y = BLOOM_PETAL_PADDING_Y_PX * scale;
    let gap = BLOOM_PETAL_CONTENT_GAP_PX * scale;
    let content_width = (petal_rect.width - pad_x * 2.0).max(0.0);
    let content_height = (petal_rect.height - pad_y * 2.0).max(0.0);
    let icon_side = icon_size.min(content_width).min(content_height).max(0.0);
    let name_line_height = BLOOM_PETAL_NAME_FONT_PX * BLOOM_PETAL_NAME_LINE_HEIGHT * scale;
    let max_title_height = name_line_height * BLOOM_PETAL_NAME_MAX_LINES as f32;
    let available_title_height = (content_height - icon_side - gap).max(0.0);
    // DWrite needs the full two-line box (28.75 DIP for the Tauri 11.5/1.25
    // role). The CSS flex box can borrow the sub-pixel remainder from its
    // vertical padding; give native layout the same one-DIP tolerance instead
    // of trimming a long title on its first line.
    let title_height = (available_title_height + scale).min(max_title_height + scale);
    let stack_height = icon_side
        + if title_height > 0.0 {
            gap + title_height
        } else {
            0.0
        };
    let stack_top = petal_rect.y + pad_y + ((content_height - stack_height) * 0.5).max(0.0);
    let icon_rect = Rect {
        x: petal_rect.x + (petal_rect.width - icon_side) * 0.5,
        y: stack_top,
        width: icon_side,
        height: icon_side,
    };
    let title_rect = Rect {
        x: petal_rect.x + pad_x,
        y: icon_rect.bottom() + gap,
        width: content_width,
        height: title_height,
    };
    StackBloomPetalContentLayout {
        icon_rect,
        title_rect,
    }
}

pub fn stack_bloom_frames(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at(viewport, anchor, member_count, 1.0)
}

pub fn stack_bloom_frames_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at_with_motion(viewport, anchor, member_count, reveal_progress, false)
}

pub fn stack_bloom_exit_frames_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at_with_motion(viewport, anchor, member_count, exit_progress, true)
}

fn stack_bloom_frames_at_with_motion(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    motion_progress: f32,
    exiting: bool,
) -> SmallVec<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]> {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    let mut frames = SmallVec::<[StackBloomFrame; BLOOM_VISIBLE_PETAL_LIMIT]>::new();
    if visible_count == 0 {
        return frames;
    }

    let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, member_count).rect;
    let petal = stack_bloom_petal_size(member_count);
    let single_row_width = visible_count as f32 * petal.width
        + visible_count.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
    let available_width = (viewport.width - BLOOM_VIEWPORT_INSET_PX * 2.0).max(0.0);

    if single_row_width > available_width {
        let petals_per_row = ((available_width + BLOOM_PETAL_GAP_PX)
            / (petal.width + BLOOM_PETAL_GAP_PX))
            .floor()
            .max(1.0) as usize;
        let total_rows = visible_count.div_ceil(petals_per_row);
        let total_height = total_rows as f32 * petal.height
            + total_rows.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
        let grid_top = stack_bloom_row_top(viewport, capsule, total_height);
        for index in 0..visible_count {
            let row = index / petals_per_row;
            let col = index % petals_per_row;
            let row_start = row * petals_per_row;
            let row_end = (row_start + petals_per_row).min(visible_count);
            let petals_in_row = row_end - row_start;
            let row_width = petals_in_row as f32 * petal.width
                + petals_in_row.saturating_sub(1) as f32 * BLOOM_PETAL_GAP_PX;
            let row_left = stack_bloom_row_left(viewport, capsule, row_width);
            let final_rect = Rect {
                x: row_left + col as f32 * (petal.width + BLOOM_PETAL_GAP_PX),
                y: grid_top + row as f32 * (petal.height + BLOOM_PETAL_GAP_PX),
                width: petal.width,
                height: petal.height,
            };
            frames.push(stack_bloom_motion_frame(
                viewport,
                capsule,
                final_rect,
                index,
                visible_count,
                motion_progress,
                exiting,
            ));
        }
        return frames;
    }

    let row_left = stack_bloom_row_left(viewport, capsule, single_row_width);
    let row_top = stack_bloom_row_top(viewport, capsule, petal.height);
    for index in 0..visible_count {
        let final_rect = Rect {
            x: row_left + index as f32 * (petal.width + BLOOM_PETAL_GAP_PX),
            y: row_top,
            width: petal.width,
            height: petal.height,
        };
        frames.push(stack_bloom_motion_frame(
            viewport,
            capsule,
            final_rect,
            index,
            visible_count,
            motion_progress,
            exiting,
        ));
    }
    frames
}

pub fn stack_bloom_petal_rects(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_petal_rects_at(viewport, anchor, member_count, 1.0)
}

pub fn stack_bloom_petal_rects_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_frames_at(viewport, anchor, member_count, reveal_progress)
        .iter()
        .map(|frame| frame.rect)
        .collect()
}

pub fn stack_bloom_exit_petal_rects_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
) -> SmallVec<[Rect; BLOOM_VISIBLE_PETAL_LIMIT]> {
    stack_bloom_exit_frames_at(viewport, anchor, member_count, exit_progress)
        .iter()
        .map(|frame| frame.rect)
        .collect()
}

pub fn stack_bloom_hit_test(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_hit_test_at(viewport, anchor, member_count, 1.0, x, y)
}

pub fn stack_bloom_hit_test_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    reveal_progress: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_petal_rects_at(viewport, anchor, member_count, reveal_progress)
        .iter()
        .position(|rect| rect_contains(inflate_rect(*rect, BLOOM_PETAL_HIT_INFLATE_PX), x, y))
}

pub fn stack_bloom_exit_hit_test_at(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    exit_progress: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    stack_bloom_exit_petal_rects_at(viewport, anchor, member_count, exit_progress)
        .iter()
        .position(|rect| rect_contains(inflate_rect(*rect, BLOOM_PETAL_HIT_INFLATE_PX), x, y))
}

/// Map a visible Bloom slot to a real member. The final slot is reserved for
/// Tauri's `+N more` indicator when the stack exceeds the 24-slot cap.
pub fn stack_bloom_member_index_for_petal(
    member_count: usize,
    petal_index: usize,
) -> Option<usize> {
    let visible_count = member_count.min(BLOOM_VISIBLE_PETAL_LIMIT);
    if petal_index >= visible_count
        || (member_count > BLOOM_VISIBLE_PETAL_LIMIT && petal_index + 1 == visible_count)
    {
        return None;
    }
    Some(petal_index)
}

pub fn stack_bloom_overflow_count(member_count: usize) -> usize {
    if member_count > BLOOM_VISIBLE_PETAL_LIMIT {
        member_count - (BLOOM_VISIBLE_PETAL_LIMIT - 1)
    } else {
        0
    }
}

pub fn stack_tray_hit_test(
    viewport: Size,
    anchor: &Zone,
    member_count: usize,
    x: f32,
    y: f32,
) -> Option<StackTrayPointerHit> {
    if rect_contains(stack_tray_close_rect(viewport, anchor, member_count), x, y) {
        return Some(StackTrayPointerHit::Close);
    }
    if rect_contains(
        stack_tray_dissolve_rect(viewport, anchor, member_count),
        x,
        y,
    ) {
        return Some(StackTrayPointerHit::Dissolve);
    }
    for row in 0..stack_tray_visible_rows(member_count) {
        if rect_contains(
            stack_tray_detach_rect(viewport, anchor, member_count, row),
            x,
            y,
        ) {
            return Some(StackTrayPointerHit::Detach(row));
        }
        if rect_contains(
            stack_tray_row_rect(viewport, anchor, member_count, row),
            x,
            y,
        ) {
            return Some(StackTrayPointerHit::Row(row));
        }
    }
    None
}

fn inflate_rect(rect: Rect, amount: f32) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.0,
        height: rect.height + amount * 2.0,
    }
}

fn stack_bloom_motion_frame(
    viewport: Size,
    capsule: Rect,
    final_rect: Rect,
    index: usize,
    visible_count: usize,
    motion_progress: f32,
    exiting: bool,
) -> StackBloomFrame {
    let start_center_x = capsule.x + capsule.width / 2.0;
    let start_center_y = capsule.y + capsule.height / 2.0;
    let final_center_x = final_rect.x + final_rect.width / 2.0;
    let final_center_y = final_rect.y + final_rect.height / 2.0;
    let (center_x, center_y, scale, alpha, progress) = if exiting {
        let elapsed_ms =
            motion_progress.clamp(0.0, 1.0) * stack_bloom_exit_duration_ms(visible_count) as f32;
        let exit_delay_ms = stack_bloom_exit_delay_ms(index, visible_count);
        let local_exit = if elapsed_ms <= exit_delay_ms {
            0.0
        } else {
            ((elapsed_ms - exit_delay_ms) / BLOOM_PETAL_EXIT_DURATION_MS as f32).clamp(0.0, 1.0)
        };
        let eased = zone_pill_geometry::ease_stack_bloom_exit_progress(local_exit);
        let remaining = (1.0 - eased).clamp(0.0, 1.0);
        (
            lerp(final_center_x, start_center_x, eased),
            lerp(final_center_y, start_center_y, eased),
            lerp(1.0, BLOOM_EXIT_SCALE, eased),
            remaining,
            remaining,
        )
    } else {
        let elapsed_ms =
            motion_progress.clamp(0.0, 1.0) * stack_bloom_reveal_duration_ms(visible_count) as f32;
        let reveal_delay_ms = stack_bloom_entry_delay_ms(index, visible_count);
        let local_reveal = if elapsed_ms <= reveal_delay_ms {
            0.0
        } else {
            ((elapsed_ms - reveal_delay_ms) / BLOOM_PETAL_ENTER_DURATION_MS as f32).clamp(0.0, 1.0)
        };
        let eased = zone_pill_geometry::ease_out_back_progress(local_reveal).max(0.0);
        let progress = eased.clamp(0.0, 1.0);
        (
            lerp(start_center_x, final_center_x, eased),
            lerp(start_center_y, final_center_y, eased),
            BLOOM_MOTION_MIN_SCALE + (1.0 - BLOOM_MOTION_MIN_SCALE) * eased,
            (BLOOM_MOTION_MIN_ALPHA + (1.0 - BLOOM_MOTION_MIN_ALPHA) * progress).clamp(0.0, 1.0),
            progress,
        )
    };
    let rect = clamp_rect_to_viewport(
        rect_from_center(
            center_x,
            center_y,
            final_rect.width * scale,
            final_rect.height * scale,
        ),
        viewport,
    );
    let connector = Rect {
        x: capsule.x + capsule.width * 0.5,
        y: capsule.y + capsule.height * 0.5,
        width: 0.0,
        height: 0.0,
    };

    StackBloomFrame {
        rect,
        connector,
        progress,
        scale,
        alpha,
    }
}

fn stack_bloom_row_left(viewport: Size, capsule: Rect, row_width: f32) -> f32 {
    let raw_left = capsule.x + capsule.width / 2.0 - row_width / 2.0;
    let max_left =
        (viewport.width - BLOOM_VIEWPORT_INSET_PX - row_width).max(BLOOM_VIEWPORT_INSET_PX);
    raw_left.clamp(BLOOM_VIEWPORT_INSET_PX, max_left)
}

fn stack_bloom_row_top(viewport: Size, capsule: Rect, row_height: f32) -> f32 {
    let below = capsule.bottom() + BLOOM_PETAL_GAP_BELOW_CAPSULE_PX;
    if below + row_height <= viewport.height - BLOOM_VIEWPORT_INSET_PX {
        return below;
    }
    let above = capsule.y - BLOOM_PETAL_GAP_BELOW_CAPSULE_PX - row_height;
    let max_top =
        (viewport.height - BLOOM_VIEWPORT_INSET_PX - row_height).max(BLOOM_VIEWPORT_INSET_PX);
    above.clamp(BLOOM_VIEWPORT_INSET_PX, max_top)
}

fn rect_from_center(center_x: f32, center_y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x: center_x - width / 2.0,
        y: center_y - height / 2.0,
        width,
        height,
    }
}

fn clamp_rect_to_viewport(rect: Rect, viewport: Size) -> Rect {
    let max_x =
        (viewport.width - rect.width - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    let max_y =
        (viewport.height - rect.height - TRAY_VIEWPORT_MARGIN_PX).max(TRAY_VIEWPORT_MARGIN_PX);
    Rect {
        x: rect.x.clamp(TRAY_VIEWPORT_MARGIN_PX, max_x),
        y: rect.y.clamp(TRAY_VIEWPORT_MARGIN_PX, max_y),
        width: rect.width,
        height: rect.height,
    }
}

pub fn stack_bloom_reveal_duration_ms(member_count: usize) -> u32 {
    let visible_count = member_count.clamp(1, BLOOM_VISIBLE_PETAL_LIMIT) as u32;
    BLOOM_PETAL_ENTER_DURATION_MS
        + (BLOOM_ENTRY_STAGGER_BUDGET_MS * visible_count.saturating_sub(1)) / visible_count
}

pub fn stack_bloom_exit_duration_ms(member_count: usize) -> u32 {
    let visible_count = member_count.clamp(1, BLOOM_VISIBLE_PETAL_LIMIT) as u32;
    let keyframe_with_tail = BLOOM_PETAL_EXIT_DURATION_MS
        + (BLOOM_EXIT_STAGGER_BUDGET_MS * visible_count.saturating_sub(1)) / visible_count;
    keyframe_with_tail.min(BLOOM_EXIT_VISIBLE_DURATION_MS)
}

fn stack_bloom_entry_delay_ms(index: usize, visible_count: usize) -> f32 {
    let count = visible_count.max(1) as f32;
    (BLOOM_ENTRY_STAGGER_BUDGET_MS as f32 / count) * index as f32
}

fn stack_bloom_exit_delay_ms(index: usize, visible_count: usize) -> f32 {
    let count = visible_count.max(1) as f32;
    let reverse_index = visible_count.saturating_sub(1).saturating_sub(index);
    (BLOOM_EXIT_STAGGER_BUDGET_MS as f32 / count) * reverse_index as f32
}

fn lerp(start: f32, end: f32, progress: f32) -> f32 {
    start + (end - start) * progress
}
