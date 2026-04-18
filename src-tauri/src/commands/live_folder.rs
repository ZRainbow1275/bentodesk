//! Tauri commands for the Live Folder feature (Theme E2-e).

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::layout::persistence::BentoZone;
use crate::watcher::live_folder;
use crate::AppState;

#[tauri::command]
pub async fn bind_zone_to_folder(
    app: AppHandle,
    zone_id: String,
    folder_path: String,
) -> Result<(), String> {
    let path = PathBuf::from(&folder_path);
    live_folder::bind(&app, &zone_id, &path).map_err(|e| e.to_string())?;

    // Persist the binding on the zone so it survives restart.
    update_zone_live_path(&app, &zone_id, Some(folder_path))
}

#[tauri::command]
pub async fn unbind_zone_folder(app: AppHandle, zone_id: String) -> Result<(), String> {
    live_folder::unbind(&app, &zone_id).map_err(|e| e.to_string())?;
    update_zone_live_path(&app, &zone_id, None)
}

/// Materialise the live folder contents into the zone so the frontend sees
/// current state. MVP: returns the list of `BentoItem`-shaped JSON entries
/// (name / path / is_directory / size) for the frontend to replace `zone.items`.
#[tauri::command]
pub async fn scan_live_folder(path: String) -> Result<Vec<LiveFolderEntry>, String> {
    let dir = PathBuf::from(&path);
    live_folder::validate_folder(&dir).map_err(|e| e.to_string())?;

    let mut entries: Vec<LiveFolderEntry> = Vec::new();
    let read = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for (idx, entry) in read.flatten().enumerate() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let metadata = entry.metadata().ok();
        let is_directory = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339().into())
            .unwrap_or_default();

        entries.push(LiveFolderEntry {
            index: idx as u32,
            name,
            path: path.to_string_lossy().to_string(),
            is_directory,
            size,
            modified_at: modified,
        });
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LiveFolderEntry {
    pub index: u32,
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified_at: String,
}

fn update_zone_live_path(
    app: &AppHandle,
    zone_id: &str,
    path: Option<String>,
) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "AppState unavailable".to_string())?;
    {
        let mut layout = state
            .layout
            .lock()
            .map_err(|e| format!("layout lock poisoned: {e}"))?;
        if let Some(zone) = layout
            .zones
            .iter_mut()
            .find(|z: &&mut BentoZone| z.id == zone_id)
        {
            zone.live_folder_path = path;
            zone.updated_at = chrono::Utc::now().to_rfc3339();
            layout.last_modified = zone.updated_at.clone();
        }
    }
    state.persist_layout();
    Ok(())
}
