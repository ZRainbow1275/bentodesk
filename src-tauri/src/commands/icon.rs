//! Icon extraction commands.

use base64::Engine as _;
use tauri::State;

use crate::icon::protocol::extract_and_cache;
use crate::AppState;

#[tauri::command]
pub async fn get_icon_url(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let hash = extract_and_cache(&state.icon_cache, &path).map_err(|e| e.to_string())?;

    // Read PNG bytes from cache and return as data: URL to bypass WebView2 caching
    // issues with the custom bentodesk:// protocol.
    let png_data = state
        .icon_cache
        .get(&hash)
        .ok_or_else(|| "Icon was evicted from cache immediately after insertion".to_string())?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{encoded}"))
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
