//! Zone CRUD commands.

use tauri::State;

use crate::guardrails;
use crate::hidden_items;
use crate::layout::persistence::{BentoZone, RelativePosition, RelativeSize, ZoneUpdate};
use crate::timeline::hook as timeline_hook;
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
        stack_id: None,
        stack_order: 0,
        alias: None,
        live_folder_path: None,
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
    timeline_hook::record_change(&state.app_handle, "zone_create");

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
        if let Some(alias_opt) = updates.alias {
            // Inner None clears the alias; inner Some("…") sets it.
            zone.alias = alias_opt.and_then(|s| if s.is_empty() { None } else { Some(s) });
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        let cloned = zone.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        cloned
    };
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_update");

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
    timeline_hook::record_change(&state.app_handle, "zone_delete");
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
    timeline_hook::record_change(&state.app_handle, "zone_reorder");
    Ok(())
}

// ─── D2 Stack Mode commands ──────────────────────────────────

/// Group one or more zones into a single stack. Returns the allocated stack_id.
///
/// Semantics:
/// - Rejects empty `zone_ids` (caller bug).
/// - If any of the zones already belong to different stacks, they are merged
///   into a single new stack with a fresh id (safer than silently preserving
///   a partial stack).
/// - `stack_order` is assigned in the order received (first id → order 0).
#[tauri::command]
pub async fn stack_zones(
    state: State<'_, AppState>,
    zone_ids: Vec<String>,
) -> Result<String, String> {
    if zone_ids.is_empty() {
        return Err("stack_zones called with empty zone_ids".to_string());
    }
    let stack_id = uuid::Uuid::new_v4().to_string();
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        for (i, id) in zone_ids.iter().enumerate() {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
                zone.stack_id = Some(stack_id.clone());
                zone.stack_order = i as u32;
                zone.updated_at = chrono::Utc::now().to_rfc3339();
            }
        }
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_stack");
    Ok(stack_id)
}

/// Dissolve a stack: clears `stack_id` and resets `stack_order` on every
/// member so they become free-standing zones again. No-op if the stack
/// has no members (e.g. it was already dissolved).
#[tauri::command]
pub async fn unstack_zones(state: State<'_, AppState>, stack_id: String) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        for zone in layout.zones.iter_mut() {
            if zone.stack_id.as_deref() == Some(&stack_id) {
                zone.stack_id = None;
                zone.stack_order = 0;
                zone.updated_at = now.clone();
            }
        }
        layout.last_modified = now;
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_unstack");
    Ok(())
}

/// Set or clear the display alias for a zone. Passing `None` clears the
/// alias; passing an empty string is treated as a clear so the UI can
/// round-trip a cleared textbox without a separate "delete" command.
#[tauri::command]
pub async fn set_zone_alias(
    state: State<'_, AppState>,
    zone_id: String,
    alias: Option<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.alias = alias.and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_alias");
    Ok(())
}

/// Move a zone to a new `stack_order` within its stack. Remaining members
/// are re-sequenced to fill the gap so the stack stays contiguous.
#[tauri::command]
pub async fn reorder_stack(
    state: State<'_, AppState>,
    stack_id: String,
    zone_id: String,
    new_order: u32,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        // Collect stack members in current order so we can rebuild.
        let mut members: Vec<(String, u32)> = layout
            .zones
            .iter()
            .filter(|z| z.stack_id.as_deref() == Some(&stack_id))
            .map(|z| (z.id.clone(), z.stack_order))
            .collect();
        if members.is_empty() {
            return Err(format!("Stack not found: {stack_id}"));
        }
        if !members.iter().any(|(id, _)| id == &zone_id) {
            return Err(format!("Zone {zone_id} is not in stack {stack_id}"));
        }
        members.sort_by_key(|(_, order)| *order);
        // Remove the moving zone, then insert at the new position (clamped).
        members.retain(|(id, _)| id != &zone_id);
        let target = (new_order as usize).min(members.len());
        members.insert(target, (zone_id.clone(), 0));
        // Re-sequence and write back.
        let now = chrono::Utc::now().to_rfc3339();
        for (i, (id, _)) in members.iter().enumerate() {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
                zone.stack_order = i as u32;
                zone.updated_at = now.clone();
            }
        }
        layout.last_modified = now;
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "stack_reorder");
    Ok(())
}

#[cfg(test)]
mod stack_tests {
    // NOTE: tests for the in-memory state transitions live here; full IPC
    // integration tests live in Theme D's test matrix.
    use crate::layout::persistence::{BentoZone, RelativePosition, RelativeSize};

    fn sample_zone(id: &str) -> BentoZone {
        BentoZone {
            id: id.to_string(),
            name: "zone".to_string(),
            icon: "folder".to_string(),
            position: RelativePosition {
                x_percent: 0.0,
                y_percent: 0.0,
            },
            expanded_size: RelativeSize {
                w_percent: 25.0,
                h_percent: 40.0,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
            stack_id: None,
            stack_order: 0,
            alias: None,
            live_folder_path: None,
        }
    }

    #[test]
    fn stack_fields_serialize_additively() {
        let mut zone = sample_zone("z1");
        // When None / 0 / None, serialization must omit the fields so older
        // clients ignore them cleanly.
        let json = serde_json::to_string(&zone).unwrap();
        assert!(!json.contains("stack_id"));
        assert!(!json.contains("stack_order"));
        assert!(!json.contains("alias"));
        // When populated, fields must appear.
        zone.stack_id = Some("s1".to_string());
        zone.stack_order = 2;
        zone.alias = Some("MyAlias".to_string());
        let json = serde_json::to_string(&zone).unwrap();
        assert!(json.contains("\"stack_id\":\"s1\""));
        assert!(json.contains("\"stack_order\":2"));
        assert!(json.contains("\"alias\":\"MyAlias\""));
    }

    #[test]
    fn stack_fields_default_on_legacy_payload() {
        // A zone payload missing stack_id/stack_order/alias (pre-1.2) must
        // deserialize with the default values.
        let legacy = r#"{
            "id":"z1","name":"n","icon":"folder",
            "position":{"x_percent":0.0,"y_percent":0.0},
            "expanded_size":{"w_percent":25.0,"h_percent":40.0},
            "items":[],"accent_color":null,"sort_order":0,"auto_group":null,
            "grid_columns":4,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z",
            "capsule_size":"medium","capsule_shape":"pill"
        }"#;
        let zone: BentoZone = serde_json::from_str(legacy).unwrap();
        assert!(zone.stack_id.is_none());
        assert_eq!(zone.stack_order, 0);
        assert!(zone.alias.is_none());
    }
}
