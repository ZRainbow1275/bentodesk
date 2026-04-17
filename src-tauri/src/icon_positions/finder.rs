//! Locate the desktop `IFolderView` COM interface.
//!
//! The COM chain is:
//! `IShellWindows` → `FindWindowSW(CSIDL_DESKTOP)` → `IServiceProvider`
//! → `IShellBrowser` → `IShellView` → `IFolderView`
//!
//! This is the Microsoft-recommended approach (Raymond Chen / The Old New Thing)
//! and works from Windows XP through Windows 11.

use windows::core::Result;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IServiceProvider, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    IFolderView, IFolderView2, IShellBrowser, IShellWindows, ShellWindows, SWC_DESKTOP,
    SWFO_NEEDDISPATCH,
};
use windows_core::Interface;

use crate::error::BentoDeskError;

/// RAII guard for COM initialization on the current thread.
///
/// Calls `CoInitializeEx(COINIT_APARTMENTTHREADED)` on creation and
/// `CoUninitialize` on drop, ensuring balanced init/uninit even on error paths.
pub(crate) struct ComGuard {
    _initialized: bool,
}

impl ComGuard {
    /// Initialize COM on the current thread (STA).
    ///
    /// Returns `Ok(guard)` on success. The caller must keep the guard alive
    /// for the duration of COM usage.
    pub fn new() -> std::result::Result<Self, BentoDeskError> {
        // SAFETY: CoInitializeEx is safe to call and will return S_OK or
        // S_FALSE (already initialized). We treat both as success.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| {
                    BentoDeskError::IconPositionError(format!("COM initialization failed: {e}"))
                })?;
        }
        Ok(Self { _initialized: true })
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        // SAFETY: Balanced CoUninitialize for the CoInitializeEx in new().
        unsafe { CoUninitialize() };
    }
}

/// SID_STopLevelBrowser GUID used to query the shell browser service.
///
/// {4C96BE40-915C-11CF-99D3-00AA004AE837}
const SID_S_TOP_LEVEL_BROWSER: windows::core::GUID = windows::core::GUID::from_values(
    0x4C96BE40,
    0x915C,
    0x11CF,
    [0x99, 0xD3, 0x00, 0xAA, 0x00, 0x4A, 0xE8, 0x37],
);

/// Acquire the desktop `IFolderView` via the Shell COM interfaces.
///
/// The returned `IFolderView` can enumerate desktop icons and read/write
/// their positions. The caller is responsible for keeping the [`ComGuard`]
/// alive for the lifetime of the returned interface.
///
/// # Errors
/// Returns `BentoDeskError::IconPositionError` if any step in the COM
/// chain fails (e.g. Explorer not running).
pub(crate) fn find_desktop_folder_view(
) -> std::result::Result<(ComGuard, IFolderView), BentoDeskError> {
    let guard = ComGuard::new()?;

    let folder_view = unsafe { find_folder_view_inner() }.map_err(|e| {
        BentoDeskError::IconPositionError(format!("Failed to acquire desktop IFolderView: {e}"))
    })?;

    Ok((guard, folder_view))
}

/// Acquire `IFolderView2` (extends `IFolderView` with auto-arrange control).
pub(crate) fn find_desktop_folder_view2(
) -> std::result::Result<(ComGuard, IFolderView2), BentoDeskError> {
    let guard = ComGuard::new()?;

    let folder_view = unsafe { find_folder_view_inner() }.map_err(|e| {
        BentoDeskError::IconPositionError(format!("Failed to acquire desktop IFolderView: {e}"))
    })?;

    // SAFETY: IFolderView2 extends IFolderView; cast via QueryInterface.
    let folder_view2: IFolderView2 = folder_view.cast().map_err(|e| {
        BentoDeskError::IconPositionError(format!(
            "Failed to cast IFolderView to IFolderView2: {e}"
        ))
    })?;

    Ok((guard, folder_view2))
}

/// Inner implementation: walk the COM chain to get `IFolderView`.
///
/// # Safety
/// All COM calls are inherently unsafe. The caller must have initialized COM
/// on this thread via [`ComGuard::new`].
unsafe fn find_folder_view_inner() -> Result<IFolderView> {
    // Step 1: Create IShellWindows
    let shell_windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;

    // Step 2: FindWindowSW for the desktop
    let mut hwnd: i32 = 0;

    // CSIDL_DESKTOP = 0x0000
    let loc = windows_core::VARIANT::from(0i32);
    let empty = windows_core::VARIANT::default();

    let disp =
        shell_windows.FindWindowSW(&loc, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;

    // Step 3: QI for IServiceProvider, then query SID_STopLevelBrowser → IShellBrowser
    let service_provider: IServiceProvider = disp.cast()?;
    let browser: IShellBrowser = service_provider.QueryService(&SID_S_TOP_LEVEL_BROWSER)?;

    // Step 4: Get the active shell view
    let view = browser.QueryActiveShellView()?;

    // Step 5: QI for IFolderView
    let folder_view: IFolderView = view.cast()?;

    Ok(folder_view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com_guard_initializes_and_drops() {
        // Verify COM init/uninit doesn't panic.
        // This test will only pass on Windows with a desktop shell running.
        let guard = ComGuard::new();
        assert!(guard.is_ok());
        drop(guard);
    }

    #[test]
    fn find_desktop_folder_view_succeeds() {
        // This test requires a running Windows desktop shell (Explorer).
        // It will fail in headless CI environments.
        let result = find_desktop_folder_view();
        if std::env::var("CI").is_ok() {
            println!("CI environment, skipping desktop test: {:?}", result.err());
        } else {
            assert!(
                result.is_ok(),
                "Failed to find desktop IFolderView: {:?}",
                result.err()
            );
        }
    }
}
