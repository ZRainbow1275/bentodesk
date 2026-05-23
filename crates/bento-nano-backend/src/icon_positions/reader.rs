//! Read desktop icon names and positions via COM `IFolderView`.
//!
//! Uses `IFolderView::Items` to enumerate all desktop icons, then
//! `IShellFolder::GetDisplayNameOf` for names and `IFolderView::GetItemPosition`
//! for pixel coordinates.

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    IEnumIDList, IFolderView, IShellFolder, SHGDN_NORMAL, SVGIO_ALLVIEW, StrRetToStrW,
};

use super::{IconPosition, IconPositionError};

/// Read all desktop icon positions from the given `IFolderView`.
///
/// Returns a `Vec<IconPosition>` containing every visible desktop icon's
/// display name and pixel coordinates.
pub(crate) fn read_all_icon_positions(
    folder_view: &IFolderView,
) -> std::result::Result<Vec<IconPosition>, IconPositionError> {
    let mut icons = Vec::new();

    // SAFETY: Caller has initialized COM (ComGuard contract from finder.rs)
    // and `folder_view` is a live IFolderView. Each PIDL we receive from
    // IEnumIDList::Next is freed via CoTaskMemFree before the next iteration.
    unsafe {
        // Get the item count for pre-allocation hint
        let count = folder_view
            .ItemCount(SVGIO_ALLVIEW)
            .map_err(|e| IconPositionError::Com {
                ctx: "IFolderView::ItemCount",
                message: e.to_string(),
            })?;

        tracing::debug!("Desktop has {} icons", count);
        icons.reserve(count as usize);

        // Get the underlying IShellFolder for display name retrieval
        let shell_folder: IShellFolder =
            folder_view
                .GetFolder()
                .map_err(|e| IconPositionError::Com {
                    ctx: "IFolderView::GetFolder",
                    message: e.to_string(),
                })?;

        // Enumerate all items
        let enumerator: IEnumIDList =
            folder_view
                .Items(SVGIO_ALLVIEW)
                .map_err(|e| IconPositionError::Com {
                    ctx: "IFolderView::Items",
                    message: e.to_string(),
                })?;

        loop {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            let mut fetched: u32 = 0;

            // SAFETY: Next() writes into our slice of 1 PIDL pointer.
            let hr = enumerator.Next(std::slice::from_mut(&mut pidl), Some(&mut fetched));

            // S_FALSE or error means no more items
            if hr.is_err() || fetched == 0 || pidl.is_null() {
                break;
            }

            // Get display name
            let name = match get_display_name(&shell_folder, pidl) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!("Skipping icon with unreadable name: {e}");
                    // SAFETY: PIDL was allocated by COM, must be freed.
                    CoTaskMemFree(Some(pidl as *const _));
                    continue;
                }
            };

            // Get position
            let position = folder_view
                .GetItemPosition(pidl)
                .map_err(|e| IconPositionError::Com {
                    ctx: "IFolderView::GetItemPosition",
                    message: format!("for '{name}': {e}"),
                });

            // SAFETY: PIDL was allocated by COM's IEnumIDList::Next, must be freed.
            CoTaskMemFree(Some(pidl as *const _));

            match position {
                Ok(pt) => {
                    tracing::trace!("Icon '{}' at ({}, {})", name, pt.x, pt.y);
                    icons.push(IconPosition {
                        name,
                        x: pt.x,
                        y: pt.y,
                    });
                }
                Err(e) => {
                    tracing::warn!("Skipping icon with unreadable position: {e}");
                }
            }
        }
    }

    tracing::info!("Read {} icon positions from desktop", icons.len());
    Ok(icons)
}

/// Get the display name of a shell item from its PIDL.
///
/// # Safety
/// `pidl` must be a valid PIDL pointer. The caller is responsible for
/// freeing it after this call returns.
unsafe fn get_display_name(
    shell_folder: &IShellFolder,
    pidl: *mut ITEMIDLIST,
) -> std::result::Result<String, IconPositionError> {
    let mut str_ret = STRRET::default();

    // SAFETY: GetDisplayNameOf writes into our STRRET; caller-supplied PIDL
    // is documented contract.
    unsafe {
        shell_folder
            .GetDisplayNameOf(pidl, SHGDN_NORMAL, &mut str_ret)
            .map_err(|e| IconPositionError::Com {
                ctx: "IShellFolder::GetDisplayNameOf",
                message: e.to_string(),
            })?;
    }

    // SAFETY: StrRetToStrW converts STRRET to a COM-allocated PWSTR.
    let mut psz_name = windows_core::PWSTR::null();
    // SAFETY: out-pointer is a stack PWSTR; STRRET was just populated above.
    unsafe {
        StrRetToStrW(&mut str_ret, Some(pidl as *const _), &mut psz_name).map_err(|e| {
            IconPositionError::Com {
                ctx: "StrRetToStrW",
                message: e.to_string(),
            }
        })?;
    }

    // Convert the wide string to a Rust String
    // SAFETY: psz_name now points to a COM-allocated null-terminated UTF-16
    // string; PWSTR::to_string walks until the null terminator.
    let name = unsafe { psz_name.to_string() }.map_err(|e| IconPositionError::Com {
        ctx: "PWSTR::to_string",
        message: e.to_string(),
    })?;

    // SAFETY: psz_name was allocated by StrRetToStrW via CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(psz_name.as_ptr() as *const _)) };

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon_positions::finder;

    #[test]
    fn read_desktop_icons_does_not_panic() {
        // Requires a running Windows desktop with at least one icon.
        if std::env::var("CI").is_ok() {
            println!("CI environment, skipping desktop icon read test");
            return;
        }

        let result = finder::find_desktop_folder_view();
        let Ok((_guard, view)) = result else {
            println!("Could not acquire desktop view, skipping test");
            return;
        };

        let icons = read_all_icon_positions(&view);
        match icons {
            Ok(icons) => {
                println!("Found {} desktop icons", icons.len());
                for icon in &icons {
                    println!("  '{}' at ({}, {})", icon.name, icon.x, icon.y);
                }
            }
            Err(e) => println!("Read failed (acceptable in unusual runners): {e}"),
        }
    }
}
