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
}
