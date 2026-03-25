//! Restore desktop icon positions via COM `IFolderView::SelectAndPositionItems`.
//!
//! Matches saved icons to current desktop icons by display name, then
//! repositions each one. Handles auto-arrange detection and DPI scaling.

use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    IEnumIDList, IFolderView, IFolderView2, IShellFolder, SHGDN_NORMAL, SVGIO_ALLVIEW,
    StrRetToStrW,
};
use windows_core::Interface;

use super::{IconPosition, SavedIconLayout};
use crate::error::BentoDeskError;
use crate::layout::resolution;

/// SVSI_POSITIONITEM = 0x80000000 — position the item without changing selection.
const SVSI_POSITIONITEM: u32 = 0x80000000;

/// FWF_AUTOARRANGE flag value for IFolderView2::SetCurrentFolderFlags.
const FWF_AUTOARRANGE: u32 = 0x00000001;

/// FWF_SNAPTOGRID flag value.
const FWF_SNAPTOGRID: u32 = 0x00400000;

/// Restore desktop icon positions from a saved layout.
///
/// # Algorithm
/// 1. Check auto-arrange: if enabled, temporarily disable it.
/// 2. Enumerate current desktop icons to get fresh PIDLs.
/// 3. Match saved icons to current icons by display name.
/// 4. Call `SelectAndPositionItems` for each matched icon.
/// 5. Re-enable auto-arrange if it was previously on.
///
/// Icons that no longer exist on the desktop are silently skipped.
/// Icons on the desktop that were not in the saved layout keep their current positions.
///
/// # DPI Handling
/// If the current DPI differs from the saved DPI, positions are scaled proportionally.
pub(crate) fn restore_icon_positions(
    folder_view: &IFolderView,
    saved: &SavedIconLayout,
) -> std::result::Result<RestoreResult, BentoDeskError> {
    let mut result = RestoreResult::default();

    // Check and handle auto-arrange
    let auto_arrange_was_on = is_auto_arrange_enabled(folder_view);
    if auto_arrange_was_on {
        tracing::warn!("Auto-arrange is enabled — temporarily disabling for restore");
        if let Err(e) = set_auto_arrange(folder_view, false) {
            tracing::error!("Failed to disable auto-arrange: {e}");
            return Err(BentoDeskError::IconPositionError(
                "Cannot restore icon positions: auto-arrange is enabled and could not be disabled"
                    .to_string(),
            ));
        }
        result.auto_arrange_toggled = true;
    }

    // Calculate DPI scale factor if DPI changed
    let current_dpi = resolution::get_dpi_scale();
    let dpi_scale = if (current_dpi - saved.dpi).abs() > 0.01 {
        tracing::info!(
            "DPI changed: saved={:.2}, current={:.2}, scale factor={:.4}",
            saved.dpi,
            current_dpi,
            current_dpi / saved.dpi
        );
        current_dpi / saved.dpi
    } else {
        1.0
    };

    // Build a lookup map: name → saved position
    let saved_positions: std::collections::HashMap<&str, &IconPosition> = saved
        .icons
        .iter()
        .map(|icon| (icon.name.as_str(), icon))
        .collect();

    // Enumerate current desktop icons and match by name
    unsafe {
        let shell_folder: IShellFolder = folder_view.GetFolder().map_err(|e| {
            BentoDeskError::IconPositionError(format!("Failed to get IShellFolder: {e}"))
        })?;

        let enumerator: IEnumIDList = folder_view.Items(SVGIO_ALLVIEW).map_err(|e| {
            BentoDeskError::IconPositionError(format!("Failed to enumerate items: {e}"))
        })?;

        loop {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            let mut fetched: u32 = 0;

            // SAFETY: Next() writes into our slice of 1 PIDL pointer.
            let hr = enumerator.Next(
                std::slice::from_mut(&mut pidl),
                Some(&mut fetched),
            );

            if hr.is_err() || fetched == 0 || pidl.is_null() {
                break;
            }

            // Get the display name of this current desktop icon
            let name = match get_display_name_for_restore(&shell_folder, pidl) {
                Ok(n) => n,
                Err(_) => {
                    // SAFETY: PIDL allocated by COM, must be freed.
                    CoTaskMemFree(Some(pidl as *const _));
                    result.skipped += 1;
                    continue;
                }
            };

            // Look up saved position by name
            if let Some(saved_pos) = saved_positions.get(name.as_str()) {
                // Apply DPI scaling if needed
                let target_x = if dpi_scale != 1.0 {
                    (saved_pos.x as f64 * dpi_scale).round() as i32
                } else {
                    saved_pos.x
                };
                let target_y = if dpi_scale != 1.0 {
                    (saved_pos.y as f64 * dpi_scale).round() as i32
                } else {
                    saved_pos.y
                };

                let pt = POINT {
                    x: target_x,
                    y: target_y,
                };

                // SAFETY: SelectAndPositionItems positions the icon at the given POINT.
                // We pass the PIDL as a single-element array.
                let apidl: [*const ITEMIDLIST; 1] = [pidl as *const _];
                let hr = folder_view.SelectAndPositionItems(
                    1,
                    apidl.as_ptr(),
                    Some(&pt),
                    SVSI_POSITIONITEM,
                );

                match hr {
                    Ok(()) => {
                        tracing::trace!(
                            "Restored '{}' to ({}, {})",
                            name,
                            target_x,
                            target_y
                        );
                        result.restored += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to position '{}': {e}", name);
                        result.failed += 1;
                    }
                }
            } else {
                tracing::trace!("Icon '{}' not in saved layout, keeping current position", name);
                result.skipped += 1;
            }

            // SAFETY: PIDL allocated by COM's IEnumIDList::Next, must be freed.
            CoTaskMemFree(Some(pidl as *const _));
        }
    }

    tracing::info!(
        "Icon restore complete: {} restored, {} skipped, {} failed",
        result.restored,
        result.skipped,
        result.failed
    );

    Ok(result)
}

/// Set a single desktop icon's position by display name.
///
/// Enumerates current desktop icons, finds the one matching `name`,
/// and repositions it to `(x, y)` using `SelectAndPositionItems`.
///
/// # Safety
/// The caller must hold a valid COM guard for the duration of this call.
pub(crate) fn set_icon_position_by_name(
    folder_view: &IFolderView,
    name: &str,
    x: i32,
    y: i32,
) -> std::result::Result<(), BentoDeskError> {
    unsafe {
        let shell_folder: IShellFolder = folder_view.GetFolder().map_err(|e| {
            BentoDeskError::IconPositionError(format!("Failed to get IShellFolder: {e}"))
        })?;

        let enumerator: IEnumIDList = folder_view.Items(SVGIO_ALLVIEW).map_err(|e| {
            BentoDeskError::IconPositionError(format!("Failed to enumerate items: {e}"))
        })?;

        loop {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            let mut fetched: u32 = 0;

            // SAFETY: Next() writes into our slice of 1 PIDL pointer.
            let hr = enumerator.Next(
                std::slice::from_mut(&mut pidl),
                Some(&mut fetched),
            );

            if hr.is_err() || fetched == 0 || pidl.is_null() {
                break;
            }

            let icon_name = match get_display_name_for_restore(&shell_folder, pidl) {
                Ok(n) => n,
                Err(_) => {
                    // SAFETY: PIDL allocated by COM, must be freed.
                    CoTaskMemFree(Some(pidl as *const _));
                    continue;
                }
            };

            if icon_name == name {
                let pt = POINT { x, y };
                // SAFETY: SelectAndPositionItems positions the icon at the given POINT.
                let apidl: [*const ITEMIDLIST; 1] = [pidl as *const _];
                let result = folder_view.SelectAndPositionItems(
                    1,
                    apidl.as_ptr(),
                    Some(&pt),
                    SVSI_POSITIONITEM,
                );
                // SAFETY: PIDL allocated by COM, must be freed.
                CoTaskMemFree(Some(pidl as *const _));

                return match result {
                    Ok(()) => {
                        tracing::info!("Set icon '{}' position to ({}, {})", name, x, y);
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!("Failed to set position for '{}': {e}", name);
                        Err(BentoDeskError::IconPositionError(format!(
                            "Failed to position icon '{}': {e}",
                            name
                        )))
                    }
                };
            }

            // SAFETY: PIDL allocated by COM's IEnumIDList::Next, must be freed.
            CoTaskMemFree(Some(pidl as *const _));
        }
    }

    tracing::warn!("Icon '{}' not found on desktop, cannot set position", name);
    Ok(()) // Not an error — icon may not be visible yet after restore
}

/// Result summary from a restore operation.
#[derive(Debug, Default)]
pub(crate) struct RestoreResult {
    /// Number of icons successfully repositioned.
    pub restored: u32,
    /// Number of icons skipped (not in saved layout or name unreadable).
    pub skipped: u32,
    /// Number of icons that failed to reposition.
    pub failed: u32,
    /// Whether auto-arrange was temporarily toggled off.
    pub auto_arrange_toggled: bool,
}

/// Check whether auto-arrange is currently enabled.
///
/// `IFolderView::GetAutoArrange` returns `S_OK` (Ok(())) when auto-arrange
/// is ON, and `S_FALSE` (Err) when it is OFF.
fn is_auto_arrange_enabled(folder_view: &IFolderView) -> bool {
    // SAFETY: GetAutoArrange is a read-only query on the folder view.
    unsafe { folder_view.GetAutoArrange().is_ok() }
}

/// Enable or disable auto-arrange via `IFolderView2::SetCurrentFolderFlags`.
fn set_auto_arrange(folder_view: &IFolderView, enable: bool) -> std::result::Result<(), BentoDeskError> {
    // SAFETY: Cast to IFolderView2 for the SetCurrentFolderFlags method.
    let view2: IFolderView2 = folder_view.cast().map_err(|e| {
        BentoDeskError::IconPositionError(format!("Failed to cast to IFolderView2: {e}"))
    })?;

    let mask = FWF_AUTOARRANGE | FWF_SNAPTOGRID;
    let flags = if enable { mask } else { 0 };

    // SAFETY: SetCurrentFolderFlags is a standard COM call to change view flags.
    unsafe {
        view2
            .SetCurrentFolderFlags(mask, flags)
            .map_err(|e| {
                BentoDeskError::IconPositionError(format!(
                    "SetCurrentFolderFlags failed: {e}"
                ))
            })?;
    }

    tracing::info!(
        "Auto-arrange/snap-to-grid {}",
        if enable { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Get display name for matching during restore.
///
/// # Safety
/// `pidl` must be a valid PIDL. Caller frees it.
unsafe fn get_display_name_for_restore(
    shell_folder: &IShellFolder,
    pidl: *mut ITEMIDLIST,
) -> std::result::Result<String, BentoDeskError> {
    let mut str_ret = STRRET::default();

    // SAFETY: GetDisplayNameOf writes into our STRRET.
    shell_folder
        .GetDisplayNameOf(pidl, SHGDN_NORMAL, &mut str_ret)
        .map_err(|e| {
            BentoDeskError::IconPositionError(format!("GetDisplayNameOf failed: {e}"))
        })?;

    // SAFETY: StrRetToStrW converts STRRET to a COM-allocated PWSTR.
    let mut psz_name = windows_core::PWSTR::null();
    StrRetToStrW(&mut str_ret, Some(pidl as *const _), &mut psz_name)
        .map_err(|e| {
            BentoDeskError::IconPositionError(format!("StrRetToStrW failed: {e}"))
        })?;

    let name = psz_name.to_string().map_err(|e| {
        BentoDeskError::IconPositionError(format!("UTF-16 conversion failed: {e}"))
    })?;

    // SAFETY: psz_name was allocated by StrRetToStrW via CoTaskMemAlloc.
    CoTaskMemFree(Some(psz_name.as_ptr() as *const _));

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_result_defaults_to_zero() {
        let r = RestoreResult::default();
        assert_eq!(r.restored, 0);
        assert_eq!(r.skipped, 0);
        assert_eq!(r.failed, 0);
        assert!(!r.auto_arrange_toggled);
    }
}
