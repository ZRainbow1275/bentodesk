//! Settings commands.
//!
//! `update_settings` not only persists the new values but also triggers
//! OS-level side effects for settings that control runtime behaviour:
//!   - launch_at_startup  → Windows Registry (Run key)
//!   - show_in_taskbar    → Window extended styles (WS_EX_APPWINDOW / TOOLWINDOW)
//!   - icon_cache_size    → Resize the in-memory LRU cache
//!   - ghost_layer_enabled → Attach / detach the desktop overlay

use std::path::Path;

use tauri::{Emitter, Manager, State};

use crate::config::settings::{AppSettings, SettingsUpdate};
use crate::AppState;

/// System-protected directory prefixes that must not be used as a desktop path.
const PROTECTED_PREFIXES: &[&str] = &[
    r"c:\windows",
    r"c:\program files",
    r"c:\program files (x86)",
    r"c:\programdata",
    r"c:\$recycle.bin",
    r"c:\system volume information",
];

/// Validate that a `desktop_path` value is a real, existing directory and is
/// not located under a Windows system-protected directory tree.
/// Strip the Windows extended-length path prefix (`\\?\`).
fn strip_unc(s: &str) -> &str {
    s.strip_prefix(r"\\?\").unwrap_or(s)
}

fn validate_desktop_path(desktop_path: &str) -> Result<(), String> {
    let path = Path::new(desktop_path);

    if !path.exists() {
        return Err(format!("Desktop path does not exist: {desktop_path}"));
    }
    if !path.is_dir() {
        return Err(format!("Desktop path is not a directory: {desktop_path}"));
    }

    // Canonicalize to resolve symlinks / junctions, then compare lowercase.
    let canonical =
        std::fs::canonicalize(path).map_err(|e| format!("Cannot resolve desktop path: {e}"))?;
    let canonical_lower = strip_unc(&canonical.to_string_lossy())
        .to_lowercase()
        .replace('/', "\\");

    for prefix in PROTECTED_PREFIXES {
        if canonical_lower.starts_with(prefix) {
            return Err(format!(
                "Desktop path must not be inside a system-protected directory ({prefix}): {desktop_path}"
            ));
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    updates: SettingsUpdate,
) -> Result<AppSettings, String> {
    // Snapshot the full settings object so a startup-task failure can roll the
    // save back atomically instead of leaving the backend on a partial state
    // while the frontend still sees a failed save.
    let previous_settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    // Validate desktop_path before applying the update.
    if let Some(ref dp) = updates.desktop_path {
        validate_desktop_path(dp)?;
    }

    let result = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.apply_update(updates);
        settings.clone()
    };
    state.persist_settings();

    // ── Side effects ─────────────────────────────────────────────

    // 1. Launch at Startup — Task Scheduler
    //    Reconfigure whenever any startup-related setting changes.
    let startup_changed = result.launch_at_startup != previous_settings.launch_at_startup
        || result.startup_high_priority != previous_settings.startup_high_priority
        || result.crash_restart_enabled != previous_settings.crash_restart_enabled
        || result.crash_max_retries != previous_settings.crash_max_retries
        || result.crash_window_secs != previous_settings.crash_window_secs;
    if startup_changed {
        if let Err(e) = apply_launch_at_startup(&state) {
            tracing::error!("Failed to update launch-at-startup: {e}; reverting settings update");
            {
                let mut settings = state.settings.lock().map_err(|err| err.to_string())?;
                *settings = previous_settings.clone();
            }
            state.persist_settings();
            return Err(format!("Failed to update launch-at-startup: {e}"));
        }
    }

    // 2. Show in Taskbar — toggle WS_EX_APPWINDOW / WS_EX_TOOLWINDOW
    if result.show_in_taskbar != previous_settings.show_in_taskbar {
        if let Err(e) = apply_show_in_taskbar(&state.app_handle, result.show_in_taskbar) {
            tracing::error!("Failed to update show-in-taskbar: {e}");
        }
    }

    // 3. Icon Cache Size — resize the LRU cache
    if result.icon_cache_size != previous_settings.icon_cache_size {
        state.icon_cache.resize(result.icon_cache_size as usize);
        tracing::info!("Icon cache resized to {}", result.icon_cache_size);
    }

    // 4. Ghost Layer — attach / detach overlay
    if result.ghost_layer_enabled != previous_settings.ghost_layer_enabled {
        if result.ghost_layer_enabled {
            if let Err(e) =
                crate::ghost_layer::manager::GhostLayerManager::attach(&state.app_handle)
            {
                tracing::error!("Failed to attach ghost layer: {e}");
            }
        } else if let Err(e) =
            crate::ghost_layer::manager::GhostLayerManager::detach(&state.app_handle)
        {
            tracing::error!("Failed to detach ghost layer: {e}");
        }
    }

    // Notify the frontend so all components can react to settings changes.
    if let Err(e) = state.app_handle.emit("settings_changed", &result) {
        tracing::warn!("Failed to emit settings_changed event: {}", e);
    }

    Ok(result)
}

// ─── Launch at Startup via Task Scheduler ─────────────────────────────────

/// Configure or remove the BentoDesk Task Scheduler entry.
///
/// Delegates to [`crate::startup::configure`] which manages `schtasks.exe`.
/// On first call, also cleans up the legacy `HKCU\...\Run` registry value.
fn apply_launch_at_startup(state: &AppState) -> Result<(), String> {
    let (enabled, high_priority, use_guardian, crash_max, crash_window) = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        (
            s.launch_at_startup,
            s.startup_high_priority,
            s.crash_restart_enabled,
            s.crash_max_retries,
            s.crash_window_secs,
        )
    };

    // Clean up legacy registry key (idempotent, safe to call every time).
    if let Err(e) = crate::startup::cleanup_legacy_registry() {
        tracing::warn!("Legacy registry cleanup failed: {e}");
    }

    let app_exe = std::env::current_exe().map_err(|e| format!("Cannot get exe path: {e}"))?;

    let guardian_exe = app_exe
        .parent()
        .map(|p| p.join("guardian.exe"))
        .unwrap_or_else(|| std::path::PathBuf::from("guardian.exe"));

    let app_data = tauri::Manager::path(&state.app_handle)
        .app_data_dir()
        .map_err(|e| format!("Cannot determine app data dir: {e}"))?;

    let crash_settings = crate::startup::CrashSettings {
        max_retries: crash_max,
        window_secs: crash_window,
    };

    crate::startup::configure(
        enabled,
        high_priority,
        use_guardian,
        &app_exe,
        &guardian_exe,
        &app_data,
        &crash_settings,
    )
    .map_err(|e| e.to_string())
}

// ─── Show in Taskbar via Window Extended Styles ──────────────────────────

/// Toggle the window's taskbar visibility by flipping `WS_EX_APPWINDOW` and
/// `WS_EX_TOOLWINDOW` extended styles.
///
/// - `show = true`  → set `WS_EX_APPWINDOW`, clear `WS_EX_TOOLWINDOW`
/// - `show = false` → clear `WS_EX_APPWINDOW`, set `WS_EX_TOOLWINDOW`
fn apply_show_in_taskbar(handle: &tauri::AppHandle, show: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_SHOWNOACTIVATE,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    let window = handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    let hwnd = window
        .hwnd()
        .map_err(|e| format!("Failed to get HWND: {e}"))?;
    let hwnd = HWND(hwnd.0);

    // Bypass the overlay subclass so hide/show are not blocked
    let _guard = crate::ghost_layer::manager::bypass_subclass_guard();

    unsafe {
        // Hide first so the taskbar entry is removed/re-added cleanly
        let _ = ShowWindow(hwnd, SW_HIDE);

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

        let new_style = if show {
            (ex_style | WS_EX_APPWINDOW.0 as isize) & !(WS_EX_TOOLWINDOW.0 as isize)
        } else {
            (ex_style | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize)
        };

        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }

    tracing::info!(
        "Taskbar visibility: {}",
        if show { "shown" } else { "hidden" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_desktop_path (P2-5: system-protected directory guard) ──

    #[test]
    fn accepts_valid_existing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = validate_desktop_path(&tmp.path().to_string_lossy());
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_nonexistent_path() {
        let result = validate_desktop_path(r"C:\ThisPathDoesNotExistAtAll_12345");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn rejects_file_path_not_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("notadir.txt");
        std::fs::write(&file, "data").unwrap();

        let result = validate_desktop_path(&file.to_string_lossy());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));
    }

    #[test]
    fn rejects_windows_system_directory() {
        // C:\Windows should exist on all Windows machines and be protected
        let result = validate_desktop_path(r"C:\Windows");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("system-protected"));
    }

    #[test]
    fn rejects_program_files_directory() {
        let result = validate_desktop_path(r"C:\Program Files");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("system-protected"));
    }

    #[test]
    fn rejects_program_files_x86_directory() {
        let result = validate_desktop_path(r"C:\Program Files (x86)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("system-protected"));
    }

    #[test]
    fn protected_prefixes_cover_all_known_system_dirs() {
        let expected = vec![
            r"c:\windows",
            r"c:\program files",
            r"c:\program files (x86)",
            r"c:\programdata",
            r"c:\$recycle.bin",
            r"c:\system volume information",
        ];
        for prefix in &expected {
            assert!(
                PROTECTED_PREFIXES.contains(prefix),
                "Missing protected prefix: {prefix}"
            );
        }
    }

    #[test]
    fn protected_prefix_check_is_case_insensitive() {
        // The function canonicalizes and lowercases before checking prefixes,
        // so C:\WINDOWS should be caught by the c:\windows prefix.
        // We test the inner logic: canonical_lower.starts_with(prefix).
        let canonical = r"C:\Windows\Temp";
        let canonical_lower = canonical.to_lowercase().replace('/', "\\");
        assert!(canonical_lower.starts_with(r"c:\windows"));
    }
}
