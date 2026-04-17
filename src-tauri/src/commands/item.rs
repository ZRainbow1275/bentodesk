//! Item management commands.

use tauri::State;

use crate::error::BentoDeskError;
use crate::guardrails;
use crate::hidden_items;
use crate::icon::protocol::extract_and_cache_fresh;
use crate::icon_positions;
use crate::layout::persistence::{BentoItem, GridPosition, ItemType};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

#[tauri::command]
pub async fn add_item(
    state: State<'_, AppState>,
    zone_id: String,
    path: String,
) -> Result<BentoItem, String> {
    // Security: validate that the file resides on any legitimate Desktop source
    // (user / public / OneDrive / override). Prevents a compromised frontend
    // from moving arbitrary system files into .bentodesk/.
    {
        let desktop_path = {
            let settings = state.settings.lock().map_err(|e| e.to_string())?;
            settings.desktop_path.clone()
        };
        if !hidden_items::is_desktop_path_with_custom(&path, Some(&desktop_path)) {
            let allowed_sources: Vec<String> =
                crate::desktop_sources::all_desktop_dirs(Some(&desktop_path))
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
            // The variant centralises the JSON envelope so any future command
            // that needs the same OUTSIDE_DESKTOP UX (e.g. a future drag-into
            // command) can return the same shape without re-deriving it.
            return Err(BentoDeskError::OutsideDesktop {
                path: path.clone(),
                allowed_sources,
            }
            .to_ipc_string());
        }
    }

    // Guardrail: verify item count stays within the safety envelope.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_add_items(&layout, &settings, &zone_id, 1)?;
    }

    let file_path = std::path::Path::new(&path);

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let item_type = if file_path.is_dir() {
        ItemType::Folder
    } else {
        match ext {
            "lnk" => ItemType::Shortcut,
            "exe" | "msi" => ItemType::Application,
            _ => ItemType::File,
        }
    };

    let name = if ext == "lnk" || ext == "url" {
        // Strip shortcut extension from display name
        file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    } else {
        file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }
    .unwrap_or_else(|| path.clone());

    // Extract and cache the icon (before hiding, so the file is still accessible).
    // Use fresh extraction to invalidate any stale generic icon from a prior cache.
    let icon_hash = extract_and_cache_fresh(&state.icon_cache, &path).map_err(|e| e.to_string())?;

    // Look up the icon's current desktop position before hiding.
    // The display name shown on the desktop matches the file_name (without
    // extension for .lnk/.url, with extension for everything else).
    let (icon_x, icon_y) = {
        let backup = state.icon_backup.lock().ok();
        let display_name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match backup.as_ref().and_then(|b| b.as_ref()) {
            Some(layout) => icon_positions::lookup_icon_position(layout, &display_name)
                .map(|(x, y)| (Some(x), Some(y)))
                .unwrap_or((None, None)),
            None => (None, None),
        }
    };

    // Hide the file from the Desktop by moving it into .bentodesk/{zone_id}/ subfolder.
    let (original_path, hidden_path) =
        match hidden_items::hide_file(&state.app_handle, &path, &zone_id) {
            Some((orig, hidden)) => (Some(orig), Some(hidden)),
            None => (None, None),
        };

    let item = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let row = zone.items.len() as u32 / zone.grid_columns;
        let col = zone.items.len() as u32 % zone.grid_columns;

        // path stores the file's CURRENT location (hidden_path if hidden succeeded,
        // original path otherwise). This ensures open_file and icon extraction work.
        let effective_path = hidden_path.clone().unwrap_or_else(|| path.clone());
        let item = BentoItem {
            id: uuid::Uuid::new_v4().to_string(),
            zone_id: zone_id.clone(),
            item_type,
            name,
            path: effective_path,
            icon_hash,
            grid_position: GridPosition {
                col,
                row,
                col_span: 1,
            },
            is_wide: false,
            added_at: chrono::Utc::now().to_rfc3339(),
            original_path,
            hidden_path,
            icon_x,
            icon_y,
            file_missing: false,
        };

        zone.items.push(item.clone());
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        item
    };
    state.persist_layout();

    // Sync zone metadata to manifest for complete backup
    hidden_items::sync_zone_metadata(&state.app_handle);

    timeline_hook::record_change(&state.app_handle, "item_add");

    Ok(item)
}

#[tauri::command]
pub async fn remove_item(
    state: State<'_, AppState>,
    zone_id: String,
    item_id: String,
) -> Result<(), String> {
    // Extract hidden file info and icon position before removing from layout
    let hidden_info: Option<(String, String, Option<i32>, Option<i32>, bool)> = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.items
            .iter()
            .find(|i| i.id == item_id)
            .and_then(|item| match (&item.original_path, &item.hidden_path) {
                (Some(orig), Some(hidden)) => Some((
                    orig.clone(),
                    hidden.clone(),
                    item.icon_x,
                    item.icon_y,
                    item.file_missing,
                )),
                _ => None,
            })
    };

    // Restore hidden file back to Desktop (move from .bentodesk/ to original path).
    // If the file is missing (deleted externally), skip restore and just remove the item.
    if let Some((ref original, ref hidden, icon_x, icon_y, file_missing)) = hidden_info {
        if !file_missing {
            if !hidden_items::restore_file_tracked(&state.app_handle, original, hidden) {
                return Err(format!(
                    "Cannot remove item: failed to restore file. \
                     The file is safe in .bentodesk/ and will be \
                     restored on exit."
                ));
            }
            // Restore succeeded -- optionally set icon position
            if let (Some(x), Some(y)) = (icon_x, icon_y) {
                let display_name = std::path::Path::new(original)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Err(e) = icon_positions::set_single_icon_position(&display_name, x, y) {
                    tracing::warn!(
                        "Failed to restore icon position for '{}': {e}",
                        display_name
                    );
                }
            }
        }
    }

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let len_before = zone.items.len();
        zone.items.retain(|item| item.id != item_id);
        if zone.items.len() == len_before {
            return Err(format!("Item not found: {item_id}"));
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_remove");
    Ok(())
}

#[tauri::command]
pub async fn move_item(
    state: State<'_, AppState>,
    from_zone_id: String,
    to_zone_id: String,
    item_id: String,
) -> Result<(), String> {
    // Guardrail: verify target zone can accept one more item.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_move_item_into_zone(&layout, &settings, &to_zone_id)?;
    }

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;

        // Find and remove from source zone
        let from_zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == from_zone_id)
            .ok_or_else(|| format!("Source zone not found: {from_zone_id}"))?;

        let item_idx = from_zone
            .items
            .iter()
            .position(|i| i.id == item_id)
            .ok_or_else(|| format!("Item not found: {item_id}"))?;

        let mut item = from_zone.items.remove(item_idx);
        from_zone.updated_at = chrono::Utc::now().to_rfc3339();

        // Add to target zone
        let to_zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == to_zone_id)
            .ok_or_else(|| format!("Target zone not found: {to_zone_id}"))?;

        // Physically move the hidden file to the new zone's subdirectory.
        // Clone the hidden_path string first to avoid borrow conflicts.
        if let (Some(hidden_str), true) = (item.hidden_path.clone(), item.original_path.is_some()) {
            let new_dir = hidden_items::zone_hidden_dir(&state.app_handle, &to_zone_id);
            if let Some(filename) = std::path::Path::new(&hidden_str).file_name() {
                let new_hidden = new_dir.join(filename);
                let old_path = std::path::Path::new(&hidden_str);
                // Avoid moving to the same location
                if old_path != new_hidden && old_path.exists() {
                    match std::fs::rename(old_path, &new_hidden) {
                        Ok(()) => {
                            let new_hidden_str = new_hidden.to_string_lossy().to_string();
                            item.hidden_path = Some(new_hidden_str.clone());
                            item.path = new_hidden_str;
                            tracing::info!(
                                "Moved hidden file between zones: {} -> {:?}",
                                hidden_str,
                                new_hidden
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to move hidden file between zones ({} -> {:?}): {}",
                                hidden_str,
                                new_hidden,
                                e
                            );
                            // Non-fatal: item stays in old zone dir but is logically in new zone
                        }
                    }
                }
            }
        }

        item.zone_id = to_zone_id;
        let row = to_zone.items.len() as u32 / to_zone.grid_columns;
        let col = to_zone.items.len() as u32 % to_zone.grid_columns;
        item.grid_position = GridPosition {
            col,
            row,
            col_span: if item.is_wide { 2 } else { 1 },
        };

        to_zone.items.push(item);
        to_zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_move");

    Ok(())
}

#[tauri::command]
pub async fn reorder_items(
    state: State<'_, AppState>,
    zone_id: String,
    item_ids: Vec<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let mut reordered = Vec::with_capacity(item_ids.len());
        for id in &item_ids {
            if let Some(item) = zone.items.iter().find(|i| &i.id == id) {
                reordered.push(item.clone());
            }
        }
        // Add any items not in the reorder list (shouldn't happen, but defensive)
        for item in &zone.items {
            if !item_ids.contains(&item.id) {
                reordered.push(item.clone());
            }
        }

        // Recalculate grid positions
        for (i, item) in reordered.iter_mut().enumerate() {
            let idx = i as u32;
            item.grid_position = GridPosition {
                col: idx % zone.grid_columns,
                row: idx / zone.grid_columns,
                col_span: if item.is_wide { 2 } else { 1 },
            };
        }

        zone.items = reordered;
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_reorder");

    Ok(())
}

#[tauri::command]
pub async fn toggle_item_wide(
    state: State<'_, AppState>,
    zone_id: String,
    item_id: String,
) -> Result<BentoItem, String> {
    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone_idx = layout
            .zones
            .iter()
            .position(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let item_idx = layout.zones[zone_idx]
            .items
            .iter()
            .position(|i| i.id == item_id)
            .ok_or_else(|| format!("Item not found: {item_id}"))?;

        let item = &mut layout.zones[zone_idx].items[item_idx];
        item.is_wide = !item.is_wide;
        item.grid_position.col_span = if item.is_wide { 2 } else { 1 };
        let result = item.clone();

        layout.zones[zone_idx].updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        result
    };
    state.persist_layout();

    Ok(result)
}
