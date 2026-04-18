//! IPC surface for the Tauri updater plugin (Theme A — A1).
//!
//! Thin wrappers over [`crate::updater`]: they exist so
//! `tauri::generate_handler![...]` can pick them up without dragging the
//! entire `updater` module surface through the command registry.

use tauri::AppHandle;

use crate::updater::{self, UpdateInfo};

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    updater::check_for_updates(app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<(), String> {
    updater::download_update(app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_update_and_restart(app: AppHandle) -> Result<(), String> {
    updater::install_update_and_restart(app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn skip_update_version(app: AppHandle, version: String) -> Result<(), String> {
    updater::skip_update_version(&app, version).map_err(|e| e.to_string())
}
