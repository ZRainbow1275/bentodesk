//! Layout / snapshot commands.

use tauri::State;

use crate::layout::resolution;
use crate::layout::snapshot::{DesktopSnapshot, SnapshotManager};
use crate::AppState;

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
    let snapshots_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("BentoDesk")
        .join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    if let Err(e) = manager.save(&snapshot) {
        tracing::warn!("Failed to persist snapshot: {}", e);
    }

    Ok(snapshot)
}

/// Load a snapshot by ID, replacing the current layout.
#[tauri::command]
pub async fn load_snapshot(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let snapshots_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("BentoDesk")
        .join("snapshots");
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
pub async fn list_snapshots(
    _state: State<'_, AppState>,
) -> Result<Vec<DesktopSnapshot>, String> {
    let snapshots_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("BentoDesk")
        .join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    manager.list().map_err(|e| e.to_string())
}

/// Delete a snapshot by ID.
#[tauri::command]
pub async fn delete_snapshot(_state: State<'_, AppState>, id: String) -> Result<(), String> {
    let snapshots_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("BentoDesk")
        .join("snapshots");
    let manager = SnapshotManager::new(snapshots_dir);
    manager.delete(&id).map_err(|e| e.to_string())
}
