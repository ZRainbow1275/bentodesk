//! Tauri commands for desktop icon position save/restore.

use tauri::State;

use crate::icon_positions;
use crate::AppState;

/// Save the current desktop icon layout, replacing any existing backup.
#[tauri::command]
pub async fn save_icon_layout(state: State<'_, AppState>) -> Result<(), String> {
    let layout = icon_positions::save_layout().map_err(|e| e.to_string())?;

    // Persist to disk
    let data_dir = icon_positions::data_dir(&state.app_handle);
    icon_positions::persist_to_file(&layout, &data_dir).map_err(|e| e.to_string())?;

    // Update in-memory backup
    let mut backup = state.icon_backup.lock().map_err(|e| e.to_string())?;
    *backup = Some(layout);

    Ok(())
}

/// Restore desktop icon positions from the current backup.
#[tauri::command]
pub async fn restore_icon_layout(state: State<'_, AppState>) -> Result<(), String> {
    let backup = state.icon_backup.lock().map_err(|e| e.to_string())?;

    let layout = match backup.as_ref() {
        Some(l) => l.clone(),
        None => {
            // Try loading from disk as fallback
            let data_dir = icon_positions::data_dir(&state.app_handle);
            match icon_positions::load_from_file(&data_dir) {
                Ok(Some(l)) => l,
                Ok(None) => return Err("No icon layout backup available".to_string()),
                Err(e) => return Err(format!("Failed to load icon backup: {e}")),
            }
        }
    };
    drop(backup); // Release lock before the potentially slow restore

    icon_positions::restore_layout(&layout).map_err(|e| e.to_string())
}
