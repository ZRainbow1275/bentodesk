//! Desktop file highlight overlay — C2 Rule Preview.
//!
//! Draws pulsing circles at desktop icon positions to preview which files
//! would be affected by a smart-group suggestion.
//!
//! Rendering strategy:
//! - Drawing is performed by the frontend (webview) which already fills the
//!   work area of the primary monitor.
//! - This module resolves `Vec<String>` paths → `Vec<(display_name, x, y)>`
//!   via the shared `icon_backup` layout, then emits a Tauri event
//!   `highlight_desktop_files` carrying the positions and a duration.
//! - A companion event `clear_desktop_highlights` signals the frontend to
//!   remove any live highlights (e.g. on mouse-leave of a suggestion card).
//!
//! Multi-monitor:
//! - Each highlight carries the `monitor_index` of the display that contains
//!   it (resolved via `display::monitors::monitor_for_point`). The frontend
//!   can use this to place overlays on the correct screen; callers that only
//!   care about the primary display can ignore the field.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::display::monitors::MonitorInfo;
use crate::icon_positions::SavedIconLayout;
use crate::AppState;

/// Default highlight duration (milliseconds) if the caller does not specify.
pub const DEFAULT_HIGHLIGHT_DURATION_MS: u64 = 3_000;

/// A single highlight target resolved from a file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightTarget {
    /// Display name of the desktop icon (e.g. "report.pdf").
    pub name: String,
    /// Desktop logical x coordinate of the icon center.
    pub x: i32,
    /// Desktop logical y coordinate of the icon center.
    pub y: i32,
    /// Index of the monitor containing this point (from
    /// `display::monitors::enumerate_monitors`). `None` if no monitor
    /// contains the point (icon off-screen after hot-plug, etc.) — the
    /// frontend should fall back to the primary monitor in that case.
    pub monitor_index: Option<u32>,
}

/// Payload for the `highlight_desktop_files` Tauri event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightPayload {
    pub targets: Vec<HighlightTarget>,
    pub duration_ms: u64,
}

/// Resolve a list of absolute/original file paths to their desktop icon
/// coordinates using the saved icon layout.
///
/// Paths whose icon is not found in the backup are silently skipped — the
/// user may have hidden the file via a zone, or the icon backup may be
/// incomplete.
pub fn resolve_paths(paths: &[String], backup: &SavedIconLayout) -> Vec<HighlightTarget> {
    paths
        .iter()
        .filter_map(|p| {
            let path = std::path::Path::new(p);
            let display_name = path.file_name().map(|n| n.to_string_lossy().to_string())?;
            let (x, y) = crate::icon_positions::lookup_icon_position(backup, &display_name)?;
            let monitor_index: Option<u32> =
                crate::display::monitors::monitor_for_point(x, y).map(|m: MonitorInfo| m.index);
            Some(HighlightTarget {
                name: display_name,
                x,
                y,
                monitor_index,
            })
        })
        .collect()
}

/// Emit a `highlight_desktop_files` event to the frontend.
pub fn emit_highlight(app: &AppHandle, targets: Vec<HighlightTarget>, duration_ms: u64) {
    let payload = HighlightPayload {
        targets,
        duration_ms,
    };
    if let Err(e) = app.emit("highlight_desktop_files", &payload) {
        tracing::warn!("Failed to emit highlight_desktop_files: {e}");
    }
}

/// Emit a `clear_desktop_highlights` event to the frontend.
pub fn emit_clear(app: &AppHandle) {
    if let Err(e) = app.emit("clear_desktop_highlights", ()) {
        tracing::warn!("Failed to emit clear_desktop_highlights: {e}");
    }
}

/// Tauri command: highlight the given desktop files for `duration_ms`
/// milliseconds.
///
/// Resolves paths → icon positions via the in-memory icon backup, then
/// emits `highlight_desktop_files` to the frontend which renders the
/// pulsing circles.
#[tauri::command]
pub async fn highlight_desktop_files(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    duration_ms: Option<u64>,
) -> Result<usize, String> {
    let duration = duration_ms.unwrap_or(DEFAULT_HIGHLIGHT_DURATION_MS);

    let targets = {
        let backup = state.icon_backup.lock().map_err(|e| e.to_string())?;
        match backup.as_ref() {
            Some(layout) => resolve_paths(&paths, layout),
            None => Vec::new(),
        }
    };

    let count = targets.len();
    emit_highlight(&state.app_handle, targets, duration);
    Ok(count)
}

/// Tauri command: clear any live highlight overlays.
#[tauri::command]
pub async fn clear_desktop_highlights(state: tauri::State<'_, AppState>) -> Result<(), String> {
    emit_clear(&state.app_handle);
    Ok(())
}
