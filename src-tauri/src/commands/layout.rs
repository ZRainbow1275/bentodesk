//! Layout / snapshot commands.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use tauri::State;

use crate::layout::{
    persistence::LayoutData,
    resolution,
    snapshot::{DesktopSnapshot, SnapshotManager},
};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct LayoutNormalizeReport {
    pub normalized_zone_ids: Vec<String>,
    /// Items the layout-restore path could not resolve to a single
    /// authoritative on-disk location (spec G's `AmbiguousDisplayName` and
    /// `Unrecognised` outcomes). Aggregated across both startup normalize
    /// and per-zone restore so the UI can surface a single skipped count.
    /// Defaults to empty when no items were skipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedRestoreItem>,
}

/// One item that the spec G identity ladder refused to restore. Carries
/// the reason so the caller can decide between "log + ignore" and
/// "surface to the user via toast".
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SkippedRestoreItem {
    pub item_id: String,
    pub item_name: String,
    pub reason: SkippedRestoreReason,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkippedRestoreReason {
    /// Multiple desktop / hidden files share the same display name and the
    /// item carried no `original_path` / `hidden_path` to disambiguate.
    AmbiguousDisplayName,
    /// No identifier resolves: paths point at vanished files and no
    /// desktop entry shares the display name.
    Unrecognised,
}

/// Save the current layout as a named snapshot.
#[tauri::command]
pub async fn save_snapshot(
    state: State<'_, AppState>,
    name: String,
) -> Result<DesktopSnapshot, String> {
    let layout = state.layout.lock().map_err(|e| e.to_string())?;
    let res = resolution::get_current_resolution();
    let dpi = resolution::get_dpi_scale();

    let snapshot = DesktopSnapshot {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        resolution: res,
        dpi,
        zones: layout.zones.clone(),
        captured_at: chrono::Utc::now().to_rfc3339(),
    };

    // Save to disk (best-effort: log errors but return the snapshot)
    let snapshots_dir = crate::storage::state_data_dir(&state.app_handle).join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    if let Err(e) = manager.save(&snapshot) {
        tracing::warn!("Failed to persist snapshot: {}", e);
    }

    Ok(snapshot)
}

/// Load a snapshot by ID, replacing the current layout.
#[tauri::command]
pub async fn load_snapshot(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let snapshots_dir = crate::storage::state_data_dir(&state.app_handle).join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    let snapshot = manager.load(&id).map_err(|e| e.to_string())?;

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        layout.zones = snapshot.zones;
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    Ok(())
}

/// List all saved snapshots, sorted by capture date (newest first).
#[tauri::command]
pub async fn list_snapshots(_state: State<'_, AppState>) -> Result<Vec<DesktopSnapshot>, String> {
    let snapshots_dir = crate::storage::state_data_dir(&_state.app_handle).join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    manager.list().map_err(|e| e.to_string())
}

/// Delete a snapshot by ID.
#[tauri::command]
pub async fn delete_snapshot(_state: State<'_, AppState>, id: String) -> Result<(), String> {
    let snapshots_dir = crate::storage::state_data_dir(&_state.app_handle).join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    manager.delete(&id).map_err(|e| e.to_string())
}

/// Aggregate result of `reconcile_all_zone_items` across every zone in
/// the live layout. Mirrors the per-zone counters of
/// [`crate::hidden_items::ReconcileReport`] but sums across the whole
/// layout so the frontend can decide in a single check whether it needs
/// to re-fetch zone state.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LayoutReconcileReport {
    pub reconciled_count: u32,
    pub already_managed_count: u32,
    pub missing_count: u32,
    pub unknown_count: u32,
    /// Zone IDs whose items were mutated during the pass. The frontend
    /// uses this to decide whether to re-call `list_zones` / refresh UI.
    pub touched_zone_ids: Vec<String>,
}

/// Walk every zone in the live layout and reconcile its items against
/// on-disk reality:
/// - Items already hidden under `.bentodesk/{zone_id}/` are no-ops.
/// - Items whose `original_path` still sits on the desktop are physically
///   moved into the zone's hidden subfolder; `hidden_path` is rewritten.
/// - Items whose neither path resolves are flagged `file_missing = true`.
///
/// The layout is persisted exactly once at the end of the pass when any
/// item or counter changed. Idempotent — calling twice in a row is safe.
#[tauri::command]
pub async fn reconcile_all_zone_items(
    state: State<'_, AppState>,
) -> Result<LayoutReconcileReport, String> {
    let app_handle = state.app_handle.clone();
    let mut aggregate = LayoutReconcileReport::default();

    let touched = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let mut touched_zone_ids: Vec<String> = Vec::new();
        for zone in layout.zones.iter_mut() {
            let zone_id = zone.id.clone();
            let report = crate::hidden_items::reconcile_zone_items(
                &app_handle,
                &mut zone.items,
                &zone_id,
            );
            aggregate.reconciled_count += report.reconciled_count;
            aggregate.already_managed_count += report.already_managed_count;
            aggregate.missing_count += report.missing_count;
            aggregate.unknown_count += report.unknown_count;
            // Touched = anything that mutated the zone's items: either a
            // file actually moved, or an item flag flipped to missing.
            if report.reconciled_count > 0
                || report.missing_count > 0
                || report.unknown_count > 0
            {
                touched_zone_ids.push(zone_id);
            }
        }
        if !touched_zone_ids.is_empty() {
            layout.last_modified = chrono::Utc::now().to_rfc3339();
        }
        touched_zone_ids
    };

    if !touched.is_empty() {
        state.persist_layout();
    }

    aggregate.touched_zone_ids = touched;

    tracing::info!(
        "reconcile_all_zone_items: reconciled={} already={} missing={} unknown={} zones_touched={}",
        aggregate.reconciled_count,
        aggregate.already_managed_count,
        aggregate.missing_count,
        aggregate.unknown_count,
        aggregate.touched_zone_ids.len()
    );

    Ok(aggregate)
}

#[tauri::command]
pub async fn normalize_zone_layout(
    state: State<'_, AppState>,
) -> Result<LayoutNormalizeReport, String> {
    let normalized_zone_ids = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let normalized_zone_ids = normalize_layout_data(&mut layout);
        if !normalized_zone_ids.is_empty() {
            layout.last_modified = chrono::Utc::now().to_rfc3339();
        }
        normalized_zone_ids
    };

    if !normalized_zone_ids.is_empty() {
        state.persist_layout();
    }

    Ok(LayoutNormalizeReport {
        normalized_zone_ids,
        skipped: Vec::new(),
    })
}

fn normalize_layout_data(layout: &mut LayoutData) -> Vec<String> {
    let mut changed = BTreeSet::new();

    let before_bounds: Vec<(String, f64, f64, f64, f64)> = layout
        .zones
        .iter()
        .map(|zone| {
            (
                zone.id.clone(),
                zone.position.x_percent,
                zone.position.y_percent,
                zone.expanded_size.w_percent,
                zone.expanded_size.h_percent,
            )
        })
        .collect();
    resolution::clamp_zones_to_screen(layout);
    for (zone, (id, x, y, w, h)) in layout.zones.iter().zip(before_bounds.iter()) {
        if zone.position.x_percent != *x
            || zone.position.y_percent != *y
            || zone.expanded_size.w_percent != *w
            || zone.expanded_size.h_percent != *h
        {
            changed.insert(id.clone());
        }
    }

    layout.zones.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.id.cmp(&b.id))
    });

    for (index, zone) in layout.zones.iter_mut().enumerate() {
        if zone.sort_order != index as i32 {
            zone.sort_order = index as i32;
            changed.insert(zone.id.clone());
        }

        if zone
            .stack_id
            .as_deref()
            .is_some_and(|stack_id| stack_id.trim().is_empty())
        {
            zone.stack_id = None;
            changed.insert(zone.id.clone());
        }

        if zone.stack_id.is_none() && zone.stack_order != 0 {
            zone.stack_order = 0;
            changed.insert(zone.id.clone());
        }
    }

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, zone) in layout.zones.iter().enumerate() {
        if let Some(stack_id) = zone.stack_id.clone() {
            groups.entry(stack_id).or_default().push(index);
        }
    }

    for indices in groups.into_values() {
        if indices.len() < 2 {
            for index in indices {
                let zone = &mut layout.zones[index];
                if zone.stack_id.take().is_some() {
                    changed.insert(zone.id.clone());
                }
                if zone.stack_order != 0 {
                    zone.stack_order = 0;
                    changed.insert(zone.id.clone());
                }
            }
            continue;
        }

        let mut ordered = indices;
        ordered.sort_by(|a, b| {
            layout.zones[*a]
                .stack_order
                .cmp(&layout.zones[*b].stack_order)
                .then(
                    layout.zones[*a]
                        .sort_order
                        .cmp(&layout.zones[*b].sort_order),
                )
                .then(layout.zones[*a].id.cmp(&layout.zones[*b].id))
        });

        for (stack_order, index) in ordered.into_iter().enumerate() {
            let zone = &mut layout.zones[index];
            if zone.stack_order != stack_order as u32 {
                zone.stack_order = stack_order as u32;
                changed.insert(zone.id.clone());
            }
        }
    }

    if !changed.is_empty() {
        let now = chrono::Utc::now().to_rfc3339();
        for zone in &mut layout.zones {
            if changed.contains(&zone.id) {
                zone.updated_at = now.clone();
            }
        }
    }

    changed.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{BentoZone, RelativePosition, RelativeSize};

    fn make_zone(id: &str) -> BentoZone {
        BentoZone {
            id: id.to_string(),
            name: format!("Zone {id}"),
            icon: "Z".to_string(),
            position: RelativePosition {
                x_percent: 95.0,
                y_percent: -10.0,
            },
            expanded_size: RelativeSize {
                w_percent: 30.0,
                h_percent: 120.0,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 99,
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
    fn normalize_layout_clears_singleton_stack_and_clamps_bounds() {
        let mut layout = LayoutData::default();
        let mut zone = make_zone("z-1");
        zone.stack_id = Some("solo".to_string());
        zone.stack_order = 3;
        layout.zones.push(zone);

        let changed = normalize_layout_data(&mut layout);

        assert_eq!(changed, vec!["z-1".to_string()]);
        assert_eq!(layout.zones[0].stack_id, None);
        assert_eq!(layout.zones[0].stack_order, 0);
        assert_eq!(layout.zones[0].position.x_percent, 70.0);
        assert_eq!(layout.zones[0].position.y_percent, 0.0);
        assert_eq!(layout.zones[0].expanded_size.h_percent, 100.0);
    }

    #[test]
    fn normalize_layout_reindexes_stack_and_sort_order() {
        let mut layout = LayoutData::default();
        let mut a = make_zone("a");
        a.sort_order = 5;
        a.stack_id = Some("stack".to_string());
        a.stack_order = 8;
        let mut b = make_zone("b");
        b.sort_order = 1;
        b.stack_id = Some("stack".to_string());
        b.stack_order = 2;
        layout.zones = vec![a, b];

        let changed = normalize_layout_data(&mut layout);

        assert_eq!(changed, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(layout.zones[0].id, "b");
        assert_eq!(layout.zones[0].sort_order, 0);
        assert_eq!(layout.zones[0].stack_order, 0);
        assert_eq!(layout.zones[1].id, "a");
        assert_eq!(layout.zones[1].sort_order, 1);
        assert_eq!(layout.zones[1].stack_order, 1);
    }

    #[test]
    fn normalize_layout_drops_multiple_singleton_stack_badges() {
        // Three zones each holding their own non-overlapping stack_id are all
        // singletons. Each badge must be cleared, none of them should keep a
        // residual stack_order, and the changed report must list every zone.
        let mut layout = LayoutData::default();
        let make = |id: &str, stack: &str| {
            let mut z = make_zone(id);
            z.position.x_percent = 10.0;
            z.position.y_percent = 10.0;
            z.expanded_size.w_percent = 20.0;
            z.expanded_size.h_percent = 20.0;
            z.stack_id = Some(stack.to_string());
            z.stack_order = 4;
            z
        };
        layout.zones = vec![
            make("solo-a", "ghost-a"),
            make("solo-b", "ghost-b"),
            make("solo-c", "ghost-c"),
        ];

        let changed = normalize_layout_data(&mut layout);

        assert_eq!(
            changed,
            vec!["solo-a".to_string(), "solo-b".to_string(), "solo-c".to_string()]
        );
        for zone in &layout.zones {
            assert!(
                zone.stack_id.is_none(),
                "zone {} should drop singleton stack badge",
                zone.id
            );
            assert_eq!(zone.stack_order, 0, "zone {} should reset stack_order", zone.id);
        }
    }

    #[test]
    fn normalize_layout_resolves_duplicate_stack_order_collisions() {
        // Two members of the same stack carry an identical stack_order. The
        // tiebreaker chain (stack_order, sort_order, id) must hand them
        // distinct contiguous slots without dropping anyone.
        let mut layout = LayoutData::default();
        let mut alpha = make_zone("alpha");
        alpha.sort_order = 0;
        alpha.position.x_percent = 5.0;
        alpha.position.y_percent = 5.0;
        alpha.expanded_size.w_percent = 20.0;
        alpha.expanded_size.h_percent = 20.0;
        alpha.stack_id = Some("group".to_string());
        alpha.stack_order = 1;

        let mut bravo = make_zone("bravo");
        bravo.sort_order = 1;
        bravo.position.x_percent = 30.0;
        bravo.position.y_percent = 5.0;
        bravo.expanded_size.w_percent = 20.0;
        bravo.expanded_size.h_percent = 20.0;
        bravo.stack_id = Some("group".to_string());
        bravo.stack_order = 1;

        let mut charlie = make_zone("charlie");
        charlie.sort_order = 2;
        charlie.position.x_percent = 55.0;
        charlie.position.y_percent = 5.0;
        charlie.expanded_size.w_percent = 20.0;
        charlie.expanded_size.h_percent = 20.0;
        charlie.stack_id = Some("group".to_string());
        charlie.stack_order = 1;

        layout.zones = vec![alpha, bravo, charlie];

        let changed = normalize_layout_data(&mut layout);

        let stack_orders: Vec<u32> = layout.zones.iter().map(|z| z.stack_order).collect();
        assert_eq!(stack_orders, vec![0, 1, 2]);
        // Tiebreak by id when stack_order + sort_order tie => alpha < bravo < charlie.
        let ids: Vec<&str> = layout.zones.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "bravo", "charlie"]);
        // alpha shifted 1->0 and charlie shifted 1->2; bravo's slot stayed at 1
        // so it is not reported as changed.
        assert!(changed.contains(&"alpha".to_string()));
        assert!(changed.contains(&"charlie".to_string()));
        assert!(!changed.contains(&"bravo".to_string()));
    }

    #[test]
    fn normalize_layout_clamps_negative_and_overflow_positions() {
        // A legacy zone migrated from a different resolution may have stored
        // out-of-bounds coordinates. After normalize, every zone must sit in
        // the [0,100) interval (with size kept inside [5,100]).
        let mut layout = LayoutData::default();

        let mut negative = make_zone("neg");
        negative.position.x_percent = -50.0;
        negative.position.y_percent = -200.0;
        negative.expanded_size.w_percent = 20.0;
        negative.expanded_size.h_percent = 30.0;

        let mut overflow = make_zone("over");
        overflow.position.x_percent = 250.0;
        overflow.position.y_percent = 1000.0;
        overflow.expanded_size.w_percent = 200.0; // clamp size first
        overflow.expanded_size.h_percent = 250.0;

        layout.zones = vec![negative, overflow];

        let changed = normalize_layout_data(&mut layout);

        assert!(changed.contains(&"neg".to_string()));
        assert!(changed.contains(&"over".to_string()));

        for zone in &layout.zones {
            assert!(
                zone.position.x_percent >= 0.0 && zone.position.x_percent < 100.0,
                "zone {} x_percent {} out of [0,100)",
                zone.id,
                zone.position.x_percent
            );
            assert!(
                zone.position.y_percent >= 0.0 && zone.position.y_percent < 100.0,
                "zone {} y_percent {} out of [0,100)",
                zone.id,
                zone.position.y_percent
            );
            assert!(
                zone.expanded_size.w_percent >= 5.0 && zone.expanded_size.w_percent <= 100.0,
                "zone {} w_percent {} out of [5,100]",
                zone.id,
                zone.expanded_size.w_percent
            );
            assert!(
                zone.expanded_size.h_percent >= 5.0 && zone.expanded_size.h_percent <= 100.0,
                "zone {} h_percent {} out of [5,100]",
                zone.id,
                zone.expanded_size.h_percent
            );
        }
    }

    #[test]
    fn layout_normalize_report_carries_changed_zone_ids() {
        // Build a report from the public `LayoutNormalizeReport` shape and
        // confirm the contract: serialize cleanly, contain only the zone IDs
        // that the normalizer reported as changed.
        let mut layout = LayoutData::default();
        let mut z1 = make_zone("z1");
        z1.position.x_percent = 95.0;
        z1.position.y_percent = -10.0;
        z1.expanded_size.w_percent = 30.0;
        z1.expanded_size.h_percent = 120.0;
        let mut z2 = make_zone("z2");
        z2.position.x_percent = 5.0;
        z2.position.y_percent = 5.0;
        z2.expanded_size.w_percent = 10.0;
        z2.expanded_size.h_percent = 10.0;
        z2.sort_order = 0;
        layout.zones = vec![z1, z2];

        let normalized_zone_ids = normalize_layout_data(&mut layout);
        let report = LayoutNormalizeReport {
            normalized_zone_ids: normalized_zone_ids.clone(),
            skipped: Vec::new(),
        };

        assert_eq!(report.normalized_zone_ids, normalized_zone_ids);
        assert!(report.normalized_zone_ids.contains(&"z1".to_string()));

        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("normalized_zone_ids"),
            "report payload missing field, got {json}"
        );
    }
}
