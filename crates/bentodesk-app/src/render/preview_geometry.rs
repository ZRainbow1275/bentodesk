use super::*;

pub(super) fn timeline_detail_thumbnail_rect(
    panel: bentodesk_style::Rect,
    detail_x: f32,
    detail_w: f32,
) -> bentodesk_style::Rect {
    let y = panel.y + timeline_panel::RUNTIME_ROW_TOP_PX + 86.0;
    let max_h = (panel.bottom() - y - 18.0).max(64.0);
    let max_w = detail_w.clamp(0.0, timeline_panel::THUMBNAIL_MAX_WIDTH);
    let mut width = max_w;
    let mut height = (width / timeline_panel::THUMBNAIL_ASPECT_RATIO).min(max_h);
    if height * timeline_panel::THUMBNAIL_ASPECT_RATIO < width {
        width = height * timeline_panel::THUMBNAIL_ASPECT_RATIO;
    }
    if width < 1.0 || height < 1.0 {
        width = 0.0;
        height = 0.0;
    }
    bentodesk_style::Rect {
        x: detail_x,
        y,
        width,
        height,
    }
}

pub(super) fn snapshot_row_preview_rect(row: bentodesk_style::Rect) -> bentodesk_style::Rect {
    let height = (row.height - 8.0).max(0.0);
    let width = (height * timeline_panel::THUMBNAIL_ASPECT_RATIO).min(76.0);
    bentodesk_style::Rect {
        x: (row.right() - width - 8.0).max(row.x + 8.0),
        y: row.y + 4.0,
        width,
        height,
    }
}

pub(super) fn snapshot_zone_thumbnail_rect(
    zone: &SnapshotZone,
    thumbnail: bentodesk_style::Rect,
) -> Option<bentodesk_style::Rect> {
    if !zone.visible {
        return None;
    }
    let canvas = inset_rect(thumbnail, 8.0);
    if canvas.width <= 0.0 || canvas.height <= 0.0 {
        return None;
    }
    let x = canvas.x + canvas.width * percent_ratio(zone.position.x_percent);
    let y = canvas.y + canvas.height * percent_ratio(zone.position.y_percent);
    let right_limit = canvas.right();
    let bottom_limit = canvas.bottom();
    if x >= right_limit || y >= bottom_limit {
        return None;
    }
    let width = (canvas.width * percent_ratio(zone.expanded_size.w_percent))
        .max(3.0)
        .min(right_limit - x);
    let height = (canvas.height * percent_ratio(zone.expanded_size.h_percent))
        .max(3.0)
        .min(bottom_limit - y);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(bentodesk_style::Rect {
        x,
        y,
        width,
        height,
    })
}

pub(super) fn percent_ratio(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 100.0) as f32 * 0.01
    } else {
        0.0
    }
}

pub(super) fn grid_columns_label(columns: u32, zh: bool) -> &'static str {
    match (columns, zh) {
        (2, true) => "2 列",
        (3, true) => "3 列",
        (4, true) => "4 列",
        (5, true) => "5 列",
        (6, true) => "6 列",
        (_, true) => "4 列",
        (2, false) => "2 columns",
        (3, false) => "3 columns",
        (4, false) => "4 columns",
        (5, false) => "5 columns",
        (6, false) => "6 columns",
        (_, false) => "4 columns",
    }
}
