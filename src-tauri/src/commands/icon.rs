//! Icon extraction commands.
//!
//! Theme B — the old base64 data URL path has been removed. `get_icon_url`
//! now primes the tiered cache and returns the `bentodesk://icon/{hash}`
//! URL. The WebView2 frontend sets this directly on `<img src>` which
//! triggers a streaming fetch against [`icon::protocol::handle_icon_request`]
//! (zero base64 transcoding, zero large data-URL strings in JS heap).

use tauri::{AppHandle, State};

use crate::icon::custom_icons::{self, CustomIcon};
use crate::icon::protocol::extract_and_cache;
use crate::icon::stats::IconCacheStatsSnapshot;
use crate::AppState;

#[tauri::command]
pub async fn get_icon_url(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let hash = extract_and_cache(&state.icon_cache, &path).map_err(|e| e.to_string())?;
    Ok(format!("bentodesk://icon/{hash}"))
}

#[tauri::command]
pub async fn preload_icons(state: State<'_, AppState>, paths: Vec<String>) -> Result<(), String> {
    for path in &paths {
        if let Err(e) = extract_and_cache(&state.icon_cache, path) {
            tracing::warn!("Failed to preload icon for {}: {}", path, e);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn clear_icon_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.icon_cache.clear();
    tracing::info!("Icon cache cleared");
    Ok(())
}

#[tauri::command]
pub async fn get_icon_cache_stats(
    state: State<'_, AppState>,
) -> Result<IconCacheStatsSnapshot, String> {
    Ok(state.icon_cache.stats())
}

// ─── Custom icon commands (Theme E1) ───────────────────────────

#[tauri::command]
pub async fn upload_custom_icon(
    app: AppHandle,
    kind: String,
    bytes: Vec<u8>,
    name: String,
) -> Result<String, String> {
    custom_icons::upload(&app, &kind, bytes, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_custom_icons(app: AppHandle) -> Result<Vec<CustomIcon>, String> {
    Ok(custom_icons::list(&app))
}

#[tauri::command]
pub async fn delete_custom_icon(app: AppHandle, uuid: String) -> Result<(), String> {
    custom_icons::delete(&app, &uuid).map_err(|e| e.to_string())
}
