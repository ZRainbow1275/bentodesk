//! Plugin management commands.
//!
//! Provides IPC handlers for listing, installing, uninstalling, and toggling
//! plugins from the frontend.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::plugins::{self, InstalledPlugin};
use crate::storage;

/// Resolve the app data directory from the Tauri app handle.
fn app_data_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    Ok(storage::state_data_dir(app_handle))
}

/// List all installed plugins.
#[tauri::command]
pub async fn list_plugins(app_handle: AppHandle) -> Result<Vec<InstalledPlugin>, String> {
    let app_data = app_data_dir(&app_handle)?;
    let registry = plugins::PluginRegistry::load(&app_data).map_err(|e| e.to_string())?;
    Ok(registry.plugins)
}

/// Install a plugin from a `.bdplugin` file path.
#[tauri::command]
pub async fn install_plugin(
    app_handle: AppHandle,
    path: String,
) -> Result<InstalledPlugin, String> {
    let zip_path = PathBuf::from(&path);
    let app_data = app_data_dir(&app_handle)?;

    let installed =
        plugins::loader::install_from_zip(&zip_path, &app_data).map_err(|e| e.to_string())?;

    if let Err(e) = app_handle.emit("plugin_installed", &installed) {
        tracing::warn!("Failed to emit plugin_installed event: {e}");
    }

    Ok(installed)
}

/// Uninstall a plugin by ID.
#[tauri::command]
pub async fn uninstall_plugin(app_handle: AppHandle, id: String) -> Result<(), String> {
    let app_data = app_data_dir(&app_handle)?;

    plugins::loader::uninstall(&id, &app_data).map_err(|e| e.to_string())?;

    if let Err(e) = app_handle.emit("plugin_uninstalled", &id) {
        tracing::warn!("Failed to emit plugin_uninstalled event: {e}");
    }

    Ok(())
}

/// Toggle a plugin's enabled state.
#[tauri::command]
pub async fn toggle_plugin(
    app_handle: AppHandle,
    id: String,
    enabled: bool,
) -> Result<InstalledPlugin, String> {
    let app_data = app_data_dir(&app_handle)?;

    let updated =
        plugins::loader::toggle_enabled(&id, enabled, &app_data).map_err(|e| e.to_string())?;

    if let Err(e) = app_handle.emit("plugin_toggled", &updated) {
        tracing::warn!("Failed to emit plugin_toggled event: {e}");
    }

    Ok(updated)
}
