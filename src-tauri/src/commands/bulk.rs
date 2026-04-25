//! Bulk zone operations — single-lock IPC handlers for Theme C.
//!
//! `bulk_update_zones` applies many zone updates under a single layout lock,
//! then records a single timeline checkpoint (coalesced via the debounce
//! hook). This gives Ctrl+Z semantics where an entire auto-layout or bulk
//! palette change is undone in one step, not per zone.
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::layout::persistence::{LayoutData, RelativePosition, RelativeSize};
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
    /// BulkManager v2 fifth field — sets the zone's icon glyph. Empty
    /// strings are treated as "no change requested" so a UI control that
    /// round-trips an empty input does not blank the icon.
    #[serde(default)]
    pub icon: Option<String>,
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
        apply_bulk_updates(&mut layout, &updates)
    };
    state.persist_layout();
    timeline_hook::record_change_coalesced(&state.app_handle, "bulk_update_zones");
    Ok(touched)
}

/// Mutate layout in place to apply bulk updates. Returns the number of
/// matched updates (unknown ids are skipped silently).
///
/// Pure (no IO, no Tauri state) so the lock-mutate-persist contract can be
/// unit-tested without spinning up an `AppState`.
pub(crate) fn apply_bulk_updates(layout: &mut LayoutData, updates: &[BulkZoneUpdate]) -> usize {
    let mut count = 0usize;
    let now = chrono::Utc::now().to_rfc3339();
    for upd in updates {
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
            if let Some(icon) = &upd.icon {
                let trimmed = icon.trim();
                if !trimmed.is_empty() {
                    zone.icon = trimmed.to_string();
                }
            }
            zone.updated_at = now.clone();
            count += 1;
        }
    }
    layout.last_modified = now;
    count
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
        apply_layout_algorithm_to_layout(&mut layout, algo, &zone_ids)
    };
    state.persist_layout();
    timeline_hook::record_change_coalesced(&state.app_handle, "apply_layout_algorithm");
    Ok(touched)
}

/// Apply the algorithm's computed positions to the matching zones in `layout`.
///
/// Pure mutation (no IO) so callers can unit-test the lock-mutate-persist
/// contract end-to-end without a Tauri state context.
pub(crate) fn apply_layout_algorithm_to_layout(
    layout: &mut LayoutData,
    algo: LayoutAlgorithm,
    zone_ids: &[String],
) -> usize {
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
    use crate::layout::persistence::BentoZone;

    fn layout_with(zones: Vec<BentoZone>) -> LayoutData {
        LayoutData {
            zones,
            ..LayoutData::default()
        }
    }

    fn make_zone(id: &str) -> BentoZone {
        BentoZone {
            id: id.to_string(),
            name: format!("Zone {id}"),
            icon: "Z".to_string(),
            position: RelativePosition {
                x_percent: 10.0,
                y_percent: 10.0,
            },
            expanded_size: RelativeSize {
                w_percent: 20.0,
                h_percent: 20.0,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: "2026-04-22T00:00:00Z".to_string(),
            updated_at: "2026-04-22T00:00:00Z".to_string(),
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
            locked: false,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    #[test]
    fn bulk_update_writes_position_size_and_color() {
        let mut layout = layout_with(vec![make_zone("a"), make_zone("b")]);

        let updates = vec![
            BulkZoneUpdate {
                id: "a".to_string(),
                position: Some(RelativePosition {
                    x_percent: 33.0,
                    y_percent: 44.0,
                }),
                size: Some(RelativeSize {
                    w_percent: 18.0,
                    h_percent: 22.0,
                }),
                accent_color: Some("#abcdef".to_string()),
                capsule_size: Some("large".to_string()),
                locked: Some(true),
                alias: Some("  Trimmed  ".to_string()),
                display_mode: Some(Some(" hover ".to_string())),
                icon: Some("  star  ".to_string()),
            },
            BulkZoneUpdate {
                id: "b".to_string(),
                position: None,
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: Some("   ".to_string()),
                display_mode: Some(None),
                icon: None,
            },
        ];

        let touched = apply_bulk_updates(&mut layout, &updates);
        assert_eq!(touched, 2);

        let a = &layout.zones[0];
        assert_eq!(a.position.x_percent, 33.0);
        assert_eq!(a.position.y_percent, 44.0);
        assert_eq!(a.expanded_size.w_percent, 18.0);
        assert_eq!(a.expanded_size.h_percent, 22.0);
        assert_eq!(a.accent_color.as_deref(), Some("#abcdef"));
        assert_eq!(a.capsule_size, "large");
        assert!(a.locked);
        // Whitespace alias must be trimmed before persisting.
        assert_eq!(a.alias.as_deref(), Some("Trimmed"));
        // " hover " trims to "hover".
        assert_eq!(a.display_mode.as_deref(), Some("hover"));
        // Icon "  star  " must trim to "star" before landing on the zone.
        assert_eq!(a.icon, "star");
        // updated_at must shift to "now" — assert it diverged from the seed.
        assert_ne!(a.updated_at, "2026-04-22T00:00:00Z");

        let b = &layout.zones[1];
        // Empty alias collapses to None (clear semantics).
        assert!(b.alias.is_none());
        // display_mode = Some(None) clears the override back to inherit.
        assert!(b.display_mode.is_none());
        // icon=None means no change — the seeded "Z" must survive.
        assert_eq!(b.icon, "Z");
    }

    #[test]
    fn bulk_update_skips_unknown_ids_silently() {
        let mut layout = layout_with(vec![make_zone("real")]);

        let updates = vec![
            BulkZoneUpdate {
                id: "ghost".to_string(),
                position: Some(RelativePosition {
                    x_percent: 50.0,
                    y_percent: 50.0,
                }),
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: None,
                display_mode: None,
                icon: None,
            },
            BulkZoneUpdate {
                id: "real".to_string(),
                position: Some(RelativePosition {
                    x_percent: 70.0,
                    y_percent: 80.0,
                }),
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: None,
                display_mode: None,
                icon: None,
            },
        ];

        let touched = apply_bulk_updates(&mut layout, &updates);
        assert_eq!(touched, 1, "ghost id must not match any zone");
        assert_eq!(layout.zones[0].position.x_percent, 70.0);
        assert_eq!(layout.zones[0].position.y_percent, 80.0);
    }

    /// BulkManager v2 — fifth field is `icon`. The IPC payload must round-
    /// trip a non-empty glyph to the zone, treat whitespace-only as a
    /// no-op (so a half-cleared form does not blank the icon), and leave
    /// the icon untouched when the field is `None`.
    #[test]
    fn bulk_update_writes_icon_field_with_trim_and_empty_guard() {
        let mut layout = layout_with(vec![
            make_zone("with-glyph"),
            make_zone("whitespace-input"),
            make_zone("absent-input"),
        ]);

        let updates = vec![
            BulkZoneUpdate {
                id: "with-glyph".to_string(),
                position: None,
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: None,
                display_mode: None,
                icon: Some("  folder  ".to_string()),
            },
            BulkZoneUpdate {
                id: "whitespace-input".to_string(),
                position: None,
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: None,
                display_mode: None,
                icon: Some("   ".to_string()),
            },
            BulkZoneUpdate {
                id: "absent-input".to_string(),
                position: None,
                size: None,
                accent_color: None,
                capsule_size: None,
                locked: None,
                alias: None,
                display_mode: None,
                icon: None,
            },
        ];

        let touched = apply_bulk_updates(&mut layout, &updates);
        assert_eq!(touched, 3, "every matched id must be touched even if icon was a no-op");

        let by_id = |id: &str| layout.zones.iter().find(|z| z.id == id).unwrap();

        // 1. Non-empty input lands trimmed.
        assert_eq!(by_id("with-glyph").icon, "folder");
        // 2. Whitespace-only input is a no-op — the seeded "Z" survives.
        assert_eq!(by_id("whitespace-input").icon, "Z");
        // 3. Absent input is a no-op — the seeded "Z" survives.
        assert_eq!(by_id("absent-input").icon, "Z");
    }

    /// BulkManager v2 contract — the IPC payload must deserialise even if
    /// the frontend omits `icon` entirely (older clients) and must serialise
    /// back without surprising fields. This guards the wire format.
    #[test]
    fn bulk_update_icon_field_is_serde_optional() {
        // Legacy payload without the icon field must still parse.
        let legacy = r#"{"id":"zone-1"}"#;
        let parsed: BulkZoneUpdate = serde_json::from_str(legacy).unwrap();
        assert!(parsed.icon.is_none(), "missing icon must default to None");

        // New payload with the icon field round-trips.
        let modern = r#"{"id":"zone-1","icon":"sparkles"}"#;
        let parsed: BulkZoneUpdate = serde_json::from_str(modern).unwrap();
        assert_eq!(parsed.icon.as_deref(), Some("sparkles"));

        // Serialising back must include the field when it is Some(...) so the
        // frontend can read it from a returned snapshot.
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(
            json.contains("\"icon\":\"sparkles\""),
            "Some(icon) must serialise visibly, got: {json}"
        );
    }

    #[test]
    fn apply_layout_algorithm_grid_writes_positions_for_matching_zones() {
        let mut layout = layout_with(vec![
            make_zone("z0"),
            make_zone("z1"),
            make_zone("z2"),
            make_zone("z3"),
        ]);
        let ids = vec![
            "z0".to_string(),
            "z1".to_string(),
            "z2".to_string(),
            "z3".to_string(),
        ];

        let touched = apply_layout_algorithm_to_layout(&mut layout, LayoutAlgorithm::Grid, &ids);
        assert_eq!(touched, 4);

        let expected = compute_layout(LayoutAlgorithm::Grid, 4);
        for (i, zone) in layout.zones.iter().enumerate() {
            assert_eq!(zone.position.x_percent, expected[i].0);
            assert_eq!(zone.position.y_percent, expected[i].1);
        }
    }

    #[test]
    fn apply_layout_algorithm_row_keeps_y_constant() {
        let mut layout = layout_with(vec![make_zone("a"), make_zone("b"), make_zone("c")]);
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let touched = apply_layout_algorithm_to_layout(&mut layout, LayoutAlgorithm::Row, &ids);
        assert_eq!(touched, 3);
        let y0 = layout.zones[0].position.y_percent;
        for zone in &layout.zones {
            assert!(
                (zone.position.y_percent - y0).abs() < 0.01,
                "row layout must keep y constant"
            );
        }
        // x must be strictly increasing.
        let xs: Vec<f64> = layout.zones.iter().map(|z| z.position.x_percent).collect();
        for w in xs.windows(2) {
            assert!(w[1] > w[0], "row x must be strictly increasing");
        }
    }

    #[test]
    fn apply_layout_algorithm_column_keeps_x_constant() {
        let mut layout = layout_with(vec![make_zone("a"), make_zone("b"), make_zone("c")]);
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let touched = apply_layout_algorithm_to_layout(&mut layout, LayoutAlgorithm::Column, &ids);
        assert_eq!(touched, 3);
        let x0 = layout.zones[0].position.x_percent;
        for zone in &layout.zones {
            assert!(
                (zone.position.x_percent - x0).abs() < 0.01,
                "column layout must keep x constant"
            );
        }
        let ys: Vec<f64> = layout.zones.iter().map(|z| z.position.y_percent).collect();
        for w in ys.windows(2) {
            assert!(w[1] > w[0], "column y must be strictly increasing");
        }
    }

    #[test]
    fn apply_layout_algorithm_spiral_distributes_distinct_positions() {
        let mut layout =
            layout_with((0..6).map(|i| make_zone(&format!("z{i}"))).collect());
        let ids: Vec<String> = (0..6).map(|i| format!("z{i}")).collect();

        let touched = apply_layout_algorithm_to_layout(&mut layout, LayoutAlgorithm::Spiral, &ids);
        assert_eq!(touched, 6);

        // No two consecutive points should collapse onto the same coordinate.
        for w in layout.zones.windows(2) {
            let dx = w[1].position.x_percent - w[0].position.x_percent;
            let dy = w[1].position.y_percent - w[0].position.y_percent;
            assert!(
                (dx * dx + dy * dy).sqrt() > 0.01,
                "spiral collapsed two consecutive points"
            );
        }
    }

    #[test]
    fn apply_layout_algorithm_organic_stays_inside_viewport() {
        let mut layout =
            layout_with((0..5).map(|i| make_zone(&format!("o{i}"))).collect());
        let ids: Vec<String> = (0..5).map(|i| format!("o{i}")).collect();

        let touched = apply_layout_algorithm_to_layout(&mut layout, LayoutAlgorithm::Organic, &ids);
        assert_eq!(touched, 5);
        for zone in &layout.zones {
            assert!(
                zone.position.x_percent >= 5.0 && zone.position.x_percent <= 95.0,
                "organic x out of margin window: {}",
                zone.position.x_percent
            );
            assert!(
                zone.position.y_percent >= 5.0 && zone.position.y_percent <= 95.0,
                "organic y out of margin window: {}",
                zone.position.y_percent
            );
        }
    }

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
