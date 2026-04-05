//! Zone CRUD commands.

use tauri::State;

use crate::guardrails;
use crate::hidden_items;
use crate::layout::persistence::{BentoZone, RelativePosition, RelativeSize, ZoneUpdate};
use crate::AppState;

#[tauri::command]
pub async fn create_zone(
    state: State<'_, AppState>,
    name: String,
    icon: String,
    position: RelativePosition,
    expanded_size: RelativeSize,
) -> Result<BentoZone, String> {
    // Guardrail: verify zone count stays within the safety envelope.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_create_zone(&layout, &settings)?;
    }

    let zone = BentoZone {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        icon,
        position,
        expanded_size,
        items: Vec::new(),
        accent_color: None,
        sort_order: 0,
        auto_group: None,
        grid_columns: 4,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        capsule_size: "medium".to_string(),
        capsule_shape: "pill".to_string(),
    };

    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let mut zone = zone;
        zone.sort_order = layout.zones.len() as i32;
        layout.zones.push(zone.clone());
        layout.last_modified = chrono::Utc::now().to_rfc3339();
        zone
    };
    state.persist_layout();

    Ok(result)
}

#[tauri::command]
pub async fn update_zone(
    state: State<'_, AppState>,
    id: String,
    updates: ZoneUpdate,
) -> Result<BentoZone, String> {
    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == id)
            .ok_or_else(|| format!("Zone not found: {id}"))?;

        if let Some(name) = updates.name {
            zone.name = name;
        }
        if let Some(icon) = updates.icon {
            zone.icon = icon;
        }
        if let Some(position) = updates.position {
            zone.position = position;
        }
        if let Some(expanded_size) = updates.expanded_size {
            zone.expanded_size = expanded_size;
        }
        if let Some(accent_color) = updates.accent_color {
            zone.accent_color = Some(accent_color);
        }
        if let Some(grid_columns) = updates.grid_columns {
            zone.grid_columns = grid_columns;
        }
        if let Some(auto_group) = updates.auto_group {
            zone.auto_group = Some(auto_group);
        }
        if let Some(capsule_size) = updates.capsule_size {
            zone.capsule_size = capsule_size;
        }
        if let Some(capsule_shape) = updates.capsule_shape {
            zone.capsule_shape = capsule_shape;
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        let cloned = zone.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        cloned
    };
    state.persist_layout();

    Ok(result)
}

#[tauri::command]
pub async fn delete_zone(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // SAFETY: Restore all zone items (move from .bentodesk/ back to Desktop)
    // BEFORE removing from layout. This ensures files become visible again.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == id)
            .ok_or_else(|| format!("Zone not found: {id}"))?;

        let items = zone.items.clone();
        drop(layout); // Release lock before doing file I/O

        let restored = hidden_items::restore_zone_items(&state.app_handle, &items);
        tracing::info!(
            "delete_zone '{}': restored {}/{} hidden items before deletion",
            id,
            restored,
            items.len()
        );
    }

    // Clean up the now-empty zone subdirectory
    hidden_items::cleanup_zone_dir(&state.app_handle, &id);

    // Now remove the zone from layout
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let len_before = layout.zones.len();
        layout.zones.retain(|z| z.id != id);
        if layout.zones.len() == len_before {
            return Err(format!("Zone not found: {id}"));
        }
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    Ok(())
}

#[tauri::command]
pub async fn list_zones(state: State<'_, AppState>) -> Result<Vec<BentoZone>, String> {
    let layout = state.layout.lock().map_err(|e| e.to_string())?;
    Ok(layout.zones.clone())
}

#[tauri::command]
pub async fn reorder_zones(
    state: State<'_, AppState>,
    zone_ids: Vec<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        for (i, id) in zone_ids.iter().enumerate() {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
                zone.sort_order = i as i32;
            }
        }
        layout.zones.sort_by_key(|z| z.sort_order);
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    Ok(())
}
