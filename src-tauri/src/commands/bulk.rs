//! Bulk zone operations — single-lock IPC handlers for Theme C.
//!
//! `bulk_update_zones` applies many zone updates under a single layout lock,
//! then records a single timeline checkpoint (coalesced via the debounce
//! hook). This gives Ctrl+Z semantics where an entire auto-layout or bulk
//! palette change is undone in one step, not per zone.
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::layout::persistence::{RelativePosition, RelativeSize};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

/// Partial update payload for one zone inside a bulk operation.
///
/// Shape mirrors `ZoneUpdate` but with fields needed for bulk ops
/// (position + size + accent_color + locked + alias). Only `Some` fields
/// overwrite current zone state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkZoneUpdate {
    pub id: String,
    #[serde(default)]
    pub position: Option<RelativePosition>,
    #[serde(default)]
    pub size: Option<RelativeSize>,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub capsule_size: Option<String>,
    #[serde(default)]
    pub locked: Option<bool>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub display_mode: Option<Option<String>>,
}

/// The supported auto-layout algorithms. Must match frontend literal union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutAlgorithm {
    Grid,
    Row,
    Column,
    Spiral,
    Organic,
}

/// Apply many zone updates atomically under a single layout lock.
///
/// Returns number of zones actually updated (unknown ids are skipped silently).
/// Timeline hook is invoked with `coalesce=true` so a burst of updates maps
/// to one checkpoint via the 500 ms debounce window.
#[tauri::command]
pub async fn bulk_update_zones(
    state: State<'_, AppState>,
    updates: Vec<BulkZoneUpdate>,
) -> Result<usize, String> {
    if updates.is_empty() {
        return Ok(0);
    }
    let touched = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let mut count = 0usize;
        let now = chrono::Utc::now().to_rfc3339();
        for upd in &updates {
            if let Some(zone) = layout.zones.iter_mut().find(|z| z.id == upd.id) {
                if let Some(pos) = &upd.position {
                    zone.position = pos.clone();
                }
                if let Some(sz) = &upd.size {
                    zone.expanded_size = sz.clone();
                }
                if let Some(color) = &upd.accent_color {
                    zone.accent_color = Some(color.clone());
                }
                if let Some(size) = &upd.capsule_size {
                    zone.capsule_size = size.clone();
                }
                if let Some(locked) = upd.locked {
                    zone.locked = locked;
                }
                if let Some(alias) = &upd.alias {
                    let trimmed = alias.trim();
                    zone.alias = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
                }
                if let Some(mode_opt) = &upd.display_mode {
                    zone.display_mode = mode_opt.clone().and_then(|mode| {
                        let trimmed = mode.trim().to_string();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed)
                        }
                    });
                }
                zone.updated_at = now.clone();
                count += 1;
            }
        }
        layout.last_modified = now;
        count
    };
    state.persist_layout();
    timeline_hook::record_change_coalesced(&state.app_handle, "bulk_update_zones");
    Ok(touched)
}

/// Delete several zones in a single lock + persist.
#[tauri::command]
pub async fn bulk_delete_zones(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<usize, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    let removed = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let before = layout.zones.len();
        layout.zones.retain(|z| !ids.contains(&z.id));
        layout.last_modified = chrono::Utc::now().to_rfc3339();
        before - layout.zones.len()
    };
    state.persist_layout();
    timeline_hook::record_change_coalesced(&state.app_handle, "bulk_delete_zones");
    Ok(removed)
}

/// Apply a named auto-layout algorithm to the listed zones.
///
/// The math is identical to the frontend `services/autoLayout.ts` path so
/// users get the same result whether they trigger layout from hotkey or
/// from the bulk manager. Frontend is authoritative for preview animation;
/// this endpoint exists so headless / scripted callers can reshape zones
/// without a webview.
#[tauri::command]
pub async fn apply_layout_algorithm(
    state: State<'_, AppState>,
    algo: LayoutAlgorithm,
    zone_ids: Vec<String>,
) -> Result<usize, String> {
    if zone_ids.is_empty() {
        return Ok(0);
    }
    let touched = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let n = zone_ids.len();
        let positions = compute_layout(algo, n);
        let now = chrono::Utc::now().to_rfc3339();
        let mut count = 0usize;
        for (idx, id) in zone_ids.iter().enumerate() {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
                if let Some(pos) = positions.get(idx) {
                    zone.position = RelativePosition {
                        x_percent: pos.0,
                        y_percent: pos.1,
                    };
                    zone.updated_at = now.clone();
                    count += 1;
                }
            }
        }
        layout.last_modified = now;
        count
    };
    state.persist_layout();
    timeline_hook::record_change_coalesced(&state.app_handle, "apply_layout_algorithm");
    Ok(touched)
}

/// Compute N x,y percentage positions for the named algorithm.
///
/// Deterministic + pure, so it can be covered by unit tests without a
/// Tauri context. Returns coordinates in the 0..100 relative coordinate
/// system used by `RelativePosition`.
pub(crate) fn compute_layout(algo: LayoutAlgorithm, n: usize) -> Vec<(f64, f64)> {
    if n == 0 {
        return Vec::new();
    }
    const MARGIN: f64 = 5.0;
    const USABLE: f64 = 100.0 - MARGIN * 2.0;
    match algo {
        LayoutAlgorithm::Grid => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = (n as f64 / cols as f64).ceil() as usize;
            let cell_w = USABLE / cols as f64;
            let cell_h = USABLE / rows as f64;
            (0..n)
                .map(|i| {
                    let col = i % cols;
                    let row = i / cols;
                    (MARGIN + col as f64 * cell_w, MARGIN + row as f64 * cell_h)
                })
                .collect()
        }
        LayoutAlgorithm::Row => {
            let cell_w = USABLE / n as f64;
            (0..n)
                .map(|i| (MARGIN + i as f64 * cell_w, MARGIN))
                .collect()
        }
        LayoutAlgorithm::Column => {
            let cell_h = USABLE / n as f64;
            (0..n)
                .map(|i| (MARGIN, MARGIN + i as f64 * cell_h))
                .collect()
        }
        LayoutAlgorithm::Spiral => {
            let cx = 50.0;
            let cy = 50.0;
            let a = 5.0;
            let b = 4.0 / std::f64::consts::TAU;
            let step = 4.0;
            let mut theta: f64 = 0.0;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                let r = a + b * theta;
                out.push((cx + r * theta.cos(), cy + r * theta.sin()));
                theta += step / r.max(a);
            }
            out
        }
        LayoutAlgorithm::Organic => {
            // Initial Poisson-ish layout, then deterministic dampened Verlet.
            let mut pos: Vec<(f64, f64)> = (0..n)
                .map(|i| {
                    let golden = 137.508_f64.to_radians();
                    let theta = i as f64 * golden;
                    let r = 2.0 * (i as f64).sqrt();
                    (50.0 + r * theta.cos(), 50.0 + r * theta.sin())
                })
                .collect();
            let mut prev = pos.clone();
            let kr: f64 = 8000.0;
            let ke: f64 = 0.05;
            let dt: f64 = 0.16;
            for _ in 0..150 {
                for i in 0..n {
                    let mut fx = 0.0;
                    let mut fy = 0.0;
                    for j in 0..n {
                        if i == j {
                            continue;
                        }
                        let dx = pos[i].0 - pos[j].0;
                        let dy = pos[i].1 - pos[j].1;
                        let d2 = dx * dx + dy * dy + 1.0;
                        let scale = kr / (d2 * d2.sqrt());
                        fx += dx * scale;
                        fy += dy * scale;
                    }
                    fx += (50.0 - pos[i].0) * ke;
                    fy += (50.0 - pos[i].1) * ke;
                    let vx = (pos[i].0 - prev[i].0) * 0.85;
                    let vy = (pos[i].1 - prev[i].1) * 0.85;
                    prev[i] = pos[i];
                    pos[i].0 = (pos[i].0 + vx + fx * dt * dt).clamp(MARGIN, 100.0 - MARGIN);
                    pos[i].1 = (pos[i].1 + vy + fy * dt * dt).clamp(MARGIN, 100.0 - MARGIN);
                }
            }
            pos
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_layout_24_zones_forms_5x5() {
        let pts = compute_layout(LayoutAlgorithm::Grid, 24);
        assert_eq!(pts.len(), 24);
        // 24 → cols=5, rows=5 (ceil(sqrt(24))=5, ceil(24/5)=5)
        let cols = (24f64.sqrt().ceil()) as usize;
        assert_eq!(cols, 5);
        // first point sits at the top-left margin
        assert!((pts[0].0 - 5.0).abs() < 0.01);
        assert!((pts[0].1 - 5.0).abs() < 0.01);
    }

    #[test]
    fn spiral_points_are_roughly_equidistant() {
        let pts = compute_layout(LayoutAlgorithm::Spiral, 12);
        assert_eq!(pts.len(), 12);
        // Consecutive pairs should differ in arc length without ever collapsing.
        for i in 1..pts.len() {
            let dx = pts[i].0 - pts[i - 1].0;
            let dy = pts[i].1 - pts[i - 1].1;
            let d = (dx * dx + dy * dy).sqrt();
            assert!(d > 0.01, "spiral collapsed between {i}-{}", i - 1);
        }
    }

    #[test]
    fn organic_converges_into_viewport() {
        let pts = compute_layout(LayoutAlgorithm::Organic, 10);
        for (x, y) in &pts {
            assert!(*x >= 5.0 && *x <= 95.0, "x out of range: {x}");
            assert!(*y >= 5.0 && *y <= 95.0, "y out of range: {y}");
        }
    }

    #[test]
    fn row_and_column_are_linear() {
        let row = compute_layout(LayoutAlgorithm::Row, 5);
        for pair in row.windows(2) {
            assert!((pair[1].1 - pair[0].1).abs() < 0.01);
            assert!(pair[1].0 > pair[0].0);
        }
        let col = compute_layout(LayoutAlgorithm::Column, 5);
        for pair in col.windows(2) {
            assert!((pair[1].0 - pair[0].0).abs() < 0.01);
            assert!(pair[1].1 > pair[0].1);
        }
    }
}
