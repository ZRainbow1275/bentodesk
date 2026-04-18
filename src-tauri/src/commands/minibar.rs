//! Tauri commands for Pin-to-Top Mini Bar (Theme E2-c).

use tauri::AppHandle;

use crate::minibar;

#[tauri::command]
pub async fn pin_zone_as_minibar(app: AppHandle, zone_id: String) -> Result<String, String> {
    minibar::pin_zone(&app, &zone_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unpin_minibar(app: AppHandle, window_label: String) -> Result<(), String> {
    minibar::unpin(&app, &window_label).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_pinned_minibars() -> Result<Vec<String>, String> {
    Ok(minibar::list())
}
