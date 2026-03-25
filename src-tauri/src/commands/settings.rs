//! Settings commands.
//!
//! `update_settings` not only persists the new values but also triggers
//! OS-level side effects for settings that control runtime behaviour:
//!   - launch_at_startup  → Windows Registry (Run key)
//!   - show_in_taskbar    → Window extended styles (WS_EX_APPWINDOW / TOOLWINDOW)
//!   - icon_cache_size    → Resize the in-memory LRU cache
//!   - ghost_layer_enabled → Attach / detach the desktop overlay

use tauri::{Emitter, Manager, State};

use crate::config::settings::{AppSettings, SettingsUpdate};
use crate::AppState;

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
    // Capture which toggles changed so we can fire side effects after the lock
    // is released.
    let (old_launch, old_taskbar, old_cache_size, old_ghost) = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        (
            s.launch_at_startup,
            s.show_in_taskbar,
            s.icon_cache_size,
            s.ghost_layer_enabled,
        )
    };

    let result = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.apply_update(updates);
        settings.clone()
    };
    state.persist_settings();

    // ── Side effects ─────────────────────────────────────────────

    // 1. Launch at Startup — Windows Registry
    if result.launch_at_startup != old_launch {
        if let Err(e) = apply_launch_at_startup(result.launch_at_startup) {
            tracing::error!("Failed to update launch-at-startup: {e}");
        }
    }

    // 2. Show in Taskbar — toggle WS_EX_APPWINDOW / WS_EX_TOOLWINDOW
    if result.show_in_taskbar != old_taskbar {
        if let Err(e) = apply_show_in_taskbar(&state.app_handle, result.show_in_taskbar) {
            tracing::error!("Failed to update show-in-taskbar: {e}");
        }
    }

    // 3. Icon Cache Size — resize the LRU cache
    if result.icon_cache_size != old_cache_size {
        state.icon_cache.resize(result.icon_cache_size as usize);
        tracing::info!("Icon cache resized to {}", result.icon_cache_size);
    }

    // 4. Ghost Layer — attach / detach overlay
    if result.ghost_layer_enabled != old_ghost {
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

// ─── Launch at Startup via Windows Registry ──────────────────────────────

/// Add or remove BentoDesk from the Windows `Run` registry key.
///
/// Key: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
/// Value name: `BentoDesk`
fn apply_launch_at_startup(enable: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, RegDeleteValueW, RegSetValueExW, RegCloseKey,
        HKEY_CURRENT_USER, KEY_WRITE, REG_SZ,
    };
    use windows::core::PCWSTR;

    let sub_key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "BentoDesk\0".encode_utf16().collect();

    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();

        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub_key.as_ptr()),
            0,
            KEY_WRITE,
            &mut hkey,
        );
        if status.is_err() {
            return Err(format!("RegOpenKeyExW failed: {status:?}"));
        }

        let result = if enable {
            // Get the current exe path
            let exe_path = std::env::current_exe()
                .map_err(|e| format!("Cannot get exe path: {e}"))?;
            let exe_str = exe_path.to_string_lossy();
            // Quote the path and encode as wide string with null terminator
            let value_data: Vec<u16> = format!("\"{exe_str}\"\0").encode_utf16().collect();
            let byte_len = value_data.len() * 2; // u16 = 2 bytes

            let r = RegSetValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    value_data.as_ptr() as *const u8,
                    byte_len,
                )),
            );

            if r.is_ok() {
                tracing::info!("Registered BentoDesk for startup: {exe_str}");
            }
            r
        } else {
            // Ignore error if the value doesn't exist
            let _ = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
            tracing::info!("Removed BentoDesk from startup");
            windows::Win32::Foundation::WIN32_ERROR(0) // ERROR_SUCCESS
        };

        let _ = RegCloseKey(hkey);
        if result.is_err() {
            Err(format!("Registry operation failed: {result:?}"))
        } else {
            Ok(())
        }
    }
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
        GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow,
        GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
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
