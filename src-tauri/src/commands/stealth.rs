//! Stealth (dotfolder visibility) IPC commands.
//!
//! Exposes the runtime state of the Win32 stealth subsystem to the frontend
//! Settings panel (`StealthModeCard`) and lets the user force a re-apply
//! when OneDrive / AV locks caused a deferred failure.

use serde::Serialize;
use tauri::AppHandle;

use crate::hidden_items::{self, AttrGuard, StealthStatus};

/// Return the current stealth subsystem status. Used by the UI to render
/// the "Desktop Stealth Mode" card: applied / retry queue / last error /
/// schema version / mirror health.
#[tauri::command]
pub fn get_stealth_status() -> StealthStatus {
    hidden_items::get_stealth_status()
}

/// Force a fresh `AttrGuard::sweep_root()` pass. Surfaced in the UI as the
/// "Re-apply" button for users who want to retry after fixing a OneDrive
/// exclusion or closing a blocking AV scan.
#[tauri::command]
pub fn reapply_stealth(app: AppHandle) -> StealthStatus {
    AttrGuard::startup_sweep(&app);
    hidden_items::get_stealth_status()
}

/// Payload returned by [`check_onedrive_exclusion_needed`]. `needed = true`
/// means the user's Desktop is redirected into OneDrive, in which case the
/// `.bentodesk/` dotfolder will be synced to the cloud unless the user adds
/// an exclusion. The UI renders a warning with a link to `guide_url`.
#[derive(Debug, Clone, Serialize)]
pub struct OneDriveExclusionCheck {
    pub needed: bool,
    /// The detected OneDrive Desktop path, if any.
    pub desktop_path: Option<String>,
    /// The relative folder to exclude inside OneDrive settings.
    pub exclusion_hint: String,
    /// Short guide URL surfaced to the user.
    pub guide_url: String,
}

/// Check whether the current Desktop is served by OneDrive and therefore
/// needs a sync exclusion for `.bentodesk/`. Does **not** mutate anything;
/// the user always performs the OneDrive configuration themselves because
/// it lives in a Microsoft UI we cannot automate reliably.
#[tauri::command]
pub fn check_onedrive_exclusion_needed(app: AppHandle) -> OneDriveExclusionCheck {
    use tauri::Manager;
    let desktop_path = {
        let state = app.state::<crate::AppState>();
        let settings = match state.settings.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        settings.desktop_path.clone()
    };

    // Heuristic: either the user's resolved Desktop contains "OneDrive" in
    // the path (common for both personal and business tenants) or the
    // `OneDrive*` env var resolves to a parent that the Desktop lives under.
    let path_lower = desktop_path.to_lowercase();
    let mut needed = path_lower.contains("onedrive");

    if !needed {
        for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
            if let Some(root) = std::env::var_os(var) {
                let root_str = root.to_string_lossy().to_lowercase();
                if !root_str.is_empty() && path_lower.starts_with(&root_str) {
                    needed = true;
                    break;
                }
            }
        }
    }

    OneDriveExclusionCheck {
        needed,
        desktop_path: if needed { Some(desktop_path) } else { None },
        exclusion_hint: ".bentodesk".to_string(),
        guide_url: "https://support.microsoft.com/onedrive-choose-folders-to-sync".to_string(),
    }
}
