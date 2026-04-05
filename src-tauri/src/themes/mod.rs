//! JSON Theme plugin system.
//!
//! Manages theme definitions stored as JSON files on disk. Themes define
//! visual properties (colors, glassmorphism, capsule shape, animation timing)
//! that the frontend applies as CSS custom properties.
//!
//! Built-in themes are embedded at compile time; custom themes are loaded
//! from the `themes/` subdirectory in the app data folder.

pub mod loader;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

// ─── Theme Schema ───────────────────────────────────────────

/// Color palette for a theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub accent: String,
    pub background: String,
    pub text: String,
    pub border: String,
}

/// Capsule (zone pill) shape configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeCapsule {
    pub shape: String,
    pub size: String,
    pub blur_radius: f64,
}

/// Animation timing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeAnimation {
    pub expand_duration_ms: u32,
    pub collapse_duration_ms: u32,
}

/// Glassmorphism effect configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeGlassmorphism {
    pub blur: f64,
    pub opacity: f64,
    pub saturation: f64,
}

/// Complete JSON Theme definition.
///
/// Each theme has a unique string ID (kebab-case), a display name, and
/// grouped visual properties. Built-in themes are shipped with the app;
/// user themes are loaded from `{app_data}/themes/*.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    /// Unique identifier — lowercase kebab-case, e.g. "ocean-blue".
    pub id: String,
    /// Human-readable display name, e.g. "Ocean Blue".
    pub name: String,
    /// Whether this theme ships with the app (cannot be deleted).
    #[serde(default)]
    pub is_builtin: bool,
    /// Core color palette.
    pub colors: ThemeColors,
    /// Capsule shape parameters.
    pub capsule: ThemeCapsule,
    /// Animation durations.
    pub animation: ThemeAnimation,
    /// Glassmorphism backdrop-filter settings.
    pub glassmorphism: ThemeGlassmorphism,
}

// ─── Tauri Commands ─────────────────────────────────────────

/// List all available themes (built-in + user-installed).
#[tauri::command]
pub async fn list_themes(state: State<'_, AppState>) -> Result<Vec<Theme>, String> {
    let themes_dir = loader::themes_dir(&state.app_handle);
    loader::load_all_themes(&themes_dir).map_err(|e| e.to_string())
}

/// Get a single theme by ID.
#[tauri::command]
pub async fn get_theme(state: State<'_, AppState>, id: String) -> Result<Theme, String> {
    let themes_dir = loader::themes_dir(&state.app_handle);
    let all = loader::load_all_themes(&themes_dir).map_err(|e| e.to_string())?;
    all.into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Theme not found: {id}"))
}

/// Get the currently active theme (resolved from settings).
#[tauri::command]
pub async fn get_active_theme(state: State<'_, AppState>) -> Result<Theme, String> {
    let active_id = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.active_theme.clone()
    };

    let themes_dir = loader::themes_dir(&state.app_handle);
    let all = loader::load_all_themes(&themes_dir).map_err(|e| e.to_string())?;

    // If no active theme is set, or the set one is missing, return the first built-in.
    let target_id = active_id.as_deref().unwrap_or("ocean-blue");
    all.into_iter()
        .find(|t| t.id == target_id)
        .or_else(|| {
            let all2 = loader::load_all_themes(&themes_dir).ok()?;
            all2.into_iter().next()
        })
        .ok_or_else(|| "No themes available".to_string())
}

/// Set the active theme by ID. Persists the choice to settings.
#[tauri::command]
pub async fn set_active_theme(state: State<'_, AppState>, id: String) -> Result<Theme, String> {
    // Verify the theme exists before persisting
    let themes_dir = loader::themes_dir(&state.app_handle);
    let all = loader::load_all_themes(&themes_dir).map_err(|e| e.to_string())?;
    let theme = all
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Theme not found: {id}"))?;

    // Update settings
    {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.active_theme = Some(id);
    }
    state.persist_settings();

    // Notify frontend
    if let Err(e) = tauri::Emitter::emit(&state.app_handle, "theme_changed", &theme) {
        tracing::warn!("Failed to emit theme_changed event: {e}");
    }

    Ok(theme)
}
