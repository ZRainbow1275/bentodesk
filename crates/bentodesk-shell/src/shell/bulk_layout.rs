//! Whole-arrangement layout fitting for BulkManager and related producers.

use super::*;

pub(super) fn apply_bulk_layout_algorithm(
    app: &mut AppState,
    ids: &[ZoneId],
    algorithm: BulkLayoutAlgorithm,
) -> (usize, usize) {
    if ids.is_empty() {
        return (0, 0);
    }

    // The Tauri percentage solver returns anchor points, but the selected-stack
    // renderer paints fixed-size collapsed capsules at those anchors. Mapping a
    // 95% point against the full viewport therefore puts the capsule's right or
    // bottom edge off-screen. Resolve the renderer-owned capsule footprint
    // first, then fit the whole arrangement before mutating any Zone.
    let mut footprints = Vec::with_capacity(ids.len());
    let mut matched = 0usize;
    let mut fallback = (1, 1);
    for id in ids {
        let footprint = app.zones.get(*id).map(|zone| {
            matched += 1;
            let (_, _, width, height) =
                bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, zone);
            let footprint = (width.max(1), height.max(1));
            fallback.0 = fallback.0.max(footprint.0);
            fallback.1 = fallback.1.max(footprint.1);
            footprint
        });
        footprints.push(footprint);
    }
    if matched == 0 {
        return (0, 0);
    }
    let footprints: Vec<_> = footprints
        .into_iter()
        .map(|footprint| footprint.unwrap_or(fallback))
        .collect();
    let origins = compute_bulk_layout_origins(algorithm, &footprints, app.viewport);

    let mut changed = 0usize;
    for (index, id) in ids.iter().enumerate() {
        let Some(zone) = app.zones.get_mut(*id) else {
            continue;
        };
        let Some((next_x, next_y)) = origins.get(index).copied() else {
            continue;
        };
        if zone.locked {
            continue;
        }
        if zone.x != next_x || zone.y != next_y {
            zone.x = next_x;
            zone.y = next_y;
            changed += 1;
        }
    }
    (changed, matched)
}

fn compute_bulk_layout_origins(
    algorithm: BulkLayoutAlgorithm,
    footprints: &[(i32, i32)],
    viewport: bentodesk_style::Size,
) -> Vec<(i32, i32)> {
    if footprints.is_empty() {
        return Vec::new();
    }
    let width = viewport.width.floor().max(1.0) as i32;
    let height = viewport.height.floor().max(1.0) as i32;
    let widths: Vec<_> = footprints.iter().map(|footprint| footprint.0).collect();
    let heights: Vec<_> = footprints.iter().map(|footprint| footprint.1).collect();

    match algorithm {
        BulkLayoutAlgorithm::Row => {
            let centers = bulk_layout_track_centers(&widths, width);
            let top = bulk_layout_margin(height, heights.iter().copied().max().unwrap_or(1));
            footprints
                .iter()
                .zip(centers)
                .map(|(&(item_width, item_height), center)| {
                    (
                        bulk_layout_origin(center, item_width, width),
                        top.clamp(0, (height - item_height).max(0)),
                    )
                })
                .collect()
        }
        BulkLayoutAlgorithm::Column => {
            let centers = bulk_layout_track_centers(&heights, height);
            let left = bulk_layout_margin(width, widths.iter().copied().max().unwrap_or(1));
            footprints
                .iter()
                .zip(centers)
                .map(|(&(item_width, item_height), center)| {
                    (
                        left.clamp(0, (width - item_width).max(0)),
                        bulk_layout_origin(center, item_height, height),
                    )
                })
                .collect()
        }
        BulkLayoutAlgorithm::Grid => {
            let columns = (footprints.len() as f64).sqrt().ceil() as usize;
            let rows = footprints.len().div_ceil(columns);
            let mut column_widths = vec![1; columns];
            let mut row_heights = vec![1; rows];
            for (index, &(item_width, item_height)) in footprints.iter().enumerate() {
                let column = index % columns;
                let row = index / columns;
                column_widths[column] = column_widths[column].max(item_width);
                row_heights[row] = row_heights[row].max(item_height);
            }
            let column_centers = bulk_layout_track_centers(&column_widths, width);
            let row_centers = bulk_layout_track_centers(&row_heights, height);
            footprints
                .iter()
                .enumerate()
                .map(|(index, &(item_width, item_height))| {
                    (
                        bulk_layout_origin(column_centers[index % columns], item_width, width),
                        bulk_layout_origin(row_centers[index / columns], item_height, height),
                    )
                })
                .collect()
        }
        BulkLayoutAlgorithm::Spiral | BulkLayoutAlgorithm::Organic => {
            let points = compute_bulk_layout_positions(algorithm, footprints.len());
            bulk_layout_pattern_origins(&points, footprints, width, height)
        }
    }
}

fn bulk_layout_margin(total: i32, max_item: i32) -> i32 {
    let nominal = (f64::from(total.max(1)) * 0.05).round() as i32;
    nominal.min((total - max_item).max(0) / 2)
}

/// Track centres for row/column/grid layouts.
///
/// When every capsule fits, this produces equal edge gaps. If the intrinsic
/// run is wider/taller than the viewport, it switches to equal centre spacing:
/// overlap is preferable to moving any Zone outside the desktop work area.
fn bulk_layout_track_centers(items: &[i32], total: i32) -> Vec<f64> {
    if items.is_empty() {
        return Vec::new();
    }
    let max_item = items.iter().copied().max().unwrap_or(1).max(1);
    let margin = bulk_layout_margin(total, max_item);
    if items.len() == 1 {
        return vec![f64::from(margin) + f64::from(items[0].max(1)) * 0.5];
    }

    let content = (total - margin * 2).max(0);
    let sum: i64 = items.iter().map(|item| i64::from((*item).max(1))).sum();
    if sum <= i64::from(content) {
        let gap = (f64::from(content) - sum as f64) / (items.len() - 1) as f64;
        let mut cursor = f64::from(margin);
        return items
            .iter()
            .map(|item| {
                let item = f64::from((*item).max(1));
                let center = cursor + item * 0.5;
                cursor += item + gap;
                center
            })
            .collect();
    }

    let first = f64::from(margin) + f64::from(max_item) * 0.5;
    let last = f64::from(total - margin) - f64::from(max_item) * 0.5;
    let step = ((last - first).max(0.0)) / (items.len() - 1) as f64;
    (0..items.len())
        .map(|index| first + index as f64 * step)
        .collect()
}

fn bulk_layout_origin(center: f64, item: i32, total: i32) -> i32 {
    (center - f64::from(item.max(1)) * 0.5)
        .round()
        .clamp(0.0, f64::from((total - item).max(0))) as i32
}

fn bulk_layout_pattern_origins(
    points: &[(f64, f64)],
    footprints: &[(i32, i32)],
    width: i32,
    height: i32,
) -> Vec<(i32, i32)> {
    let center_x = f64::from(width) * 0.5;
    let center_y = f64::from(height) * 0.5;
    // One isotropic DIP scale keeps a round/spiral arrangement round on a
    // widescreen desktop instead of stretching it into an ellipse.
    let dip_per_percent = f64::from(width.min(height)) / 100.0;
    let max_width = footprints
        .iter()
        .map(|footprint| footprint.0)
        .max()
        .unwrap_or(1);
    let max_height = footprints
        .iter()
        .map(|footprint| footprint.1)
        .max()
        .unwrap_or(1);
    let margin_x = f64::from(bulk_layout_margin(width, max_width));
    let margin_y = f64::from(bulk_layout_margin(height, max_height));
    let mut scale = 1.0_f64;

    for (&(point_x, point_y), &(item_width, item_height)) in points.iter().zip(footprints.iter()) {
        let dx = (point_x - 50.0) * dip_per_percent;
        let dy = (point_y - 50.0) * dip_per_percent;
        let half_width = f64::from(item_width.max(1)) * 0.5;
        let half_height = f64::from(item_height.max(1)) * 0.5;
        if dx > 0.0 {
            scale =
                scale.min(((f64::from(width) - margin_x - half_width - center_x) / dx).max(0.0));
        } else if dx < 0.0 {
            scale = scale.min(((center_x - margin_x - half_width) / -dx).max(0.0));
        }
        if dy > 0.0 {
            scale =
                scale.min(((f64::from(height) - margin_y - half_height - center_y) / dy).max(0.0));
        } else if dy < 0.0 {
            scale = scale.min(((center_y - margin_y - half_height) / -dy).max(0.0));
        }
    }
    scale = scale.clamp(0.0, 1.0);

    points
        .iter()
        .zip(footprints.iter())
        .map(|(&(point_x, point_y), &(item_width, item_height))| {
            let x = center_x + (point_x - 50.0) * dip_per_percent * scale;
            let y = center_y + (point_y - 50.0) * dip_per_percent * scale;
            (
                bulk_layout_origin(x, item_width, width),
                bulk_layout_origin(y, item_height, height),
            )
        })
        .collect()
}

pub(super) fn percent_to_logical(percent: f64, total: f32) -> i32 {
    ((percent.clamp(0.0, 100.0) / 100.0) * f64::from(total.max(1.0))).round() as i32
}

pub(super) fn compute_bulk_layout_positions(
    algorithm: BulkLayoutAlgorithm,
    count: usize,
) -> Vec<(f64, f64)> {
    if count == 0 {
        return Vec::new();
    }
    const MARGIN: f64 = 5.0;
    const USABLE: f64 = 100.0 - MARGIN * 2.0;
    match algorithm {
        BulkLayoutAlgorithm::Grid => {
            let columns = (count as f64).sqrt().ceil() as usize;
            let rows = (count as f64 / columns as f64).ceil() as usize;
            let cell_width = USABLE / columns as f64;
            let cell_height = USABLE / rows as f64;
            (0..count)
                .map(|index| {
                    let column = index % columns;
                    let row = index / columns;
                    (
                        MARGIN + column as f64 * cell_width,
                        MARGIN + row as f64 * cell_height,
                    )
                })
                .collect()
        }
        BulkLayoutAlgorithm::Row => {
            let cell_width = USABLE / count as f64;
            (0..count)
                .map(|index| (MARGIN + index as f64 * cell_width, MARGIN))
                .collect()
        }
        BulkLayoutAlgorithm::Column => {
            let cell_height = USABLE / count as f64;
            (0..count)
                .map(|index| (MARGIN, MARGIN + index as f64 * cell_height))
                .collect()
        }
        BulkLayoutAlgorithm::Spiral => {
            let center_x = 50.0;
            let center_y = 50.0;
            let start_radius = 5.0;
            let radius_step = 4.0 / std::f64::consts::TAU;
            let arc_step = 4.0;
            let mut theta: f64 = 0.0;
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
                let radius = start_radius + radius_step * theta;
                out.push((
                    center_x + radius * theta.cos(),
                    center_y + radius * theta.sin(),
                ));
                theta += arc_step / radius.max(start_radius);
            }
            out
        }
        BulkLayoutAlgorithm::Organic => {
            let mut positions: Vec<(f64, f64)> = (0..count)
                .map(|index| {
                    let golden_angle = 137.508_f64.to_radians();
                    let theta = index as f64 * golden_angle;
                    let radius = 2.0 * (index as f64).sqrt();
                    (50.0 + radius * theta.cos(), 50.0 + radius * theta.sin())
                })
                .collect();
            let mut previous = positions.clone();
            let repulsion: f64 = 8000.0;
            let edge_pull: f64 = 0.05;
            let dt: f64 = 0.16;
            for _ in 0..150 {
                for i in 0..count {
                    let mut force_x = 0.0;
                    let mut force_y = 0.0;
                    for j in 0..count {
                        if i == j {
                            continue;
                        }
                        let dx = positions[i].0 - positions[j].0;
                        let dy = positions[i].1 - positions[j].1;
                        let distance_squared = dx * dx + dy * dy + 1.0;
                        let scale = repulsion / (distance_squared * distance_squared.sqrt());
                        force_x += dx * scale;
                        force_y += dy * scale;
                    }
                    force_x += (50.0 - positions[i].0) * edge_pull;
                    force_y += (50.0 - positions[i].1) * edge_pull;
                    let velocity_x = (positions[i].0 - previous[i].0) * 0.85;
                    let velocity_y = (positions[i].1 - previous[i].1) * 0.85;
                    previous[i] = positions[i];
                    positions[i].0 = (positions[i].0 + velocity_x + force_x * dt * dt)
                        .clamp(MARGIN, 100.0 - MARGIN);
                    positions[i].1 = (positions[i].1 + velocity_y + force_y * dt * dt)
                        .clamp(MARGIN, 100.0 - MARGIN);
                }
            }
            positions
        }
    }
}
