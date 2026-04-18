//! Tauri commands for the Context Capsule feature (Theme E2-a).

use tauri::AppHandle;

use crate::context_capsule::{self, ContextCapsule, RestoreResult};

#[tauri::command]
pub async fn capture_context(
    app: AppHandle,
    name: String,
    icon: Option<String>,
) -> Result<ContextCapsule, String> {
    context_capsule::capture(&app, name, icon).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_contexts(app: AppHandle) -> Result<Vec<ContextCapsule>, String> {
    Ok(context_capsule::list(&app))
}

#[tauri::command]
pub async fn restore_context(app: AppHandle, id: String) -> Result<RestoreResult, String> {
    context_capsule::restore(&app, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_context(app: AppHandle, id: String) -> Result<(), String> {
    context_capsule::delete(&app, &id).map_err(|e| e.to_string())
}
