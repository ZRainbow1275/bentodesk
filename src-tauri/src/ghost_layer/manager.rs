//! Manages the desktop overlay layer for BentoDesk.
//!
//! Architecture (inspired by Rainmeter and Stardock Fences):
//!
//! The overlay uses `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` to hide from
//! Alt-Tab and prevent focus stealing. It positions itself at `HWND_BOTTOM`
//! of the normal z-order — above the desktop shell (Progman) but below all
//! regular application windows.
//!
//! A WndProc subclass intercepts `WM_WINDOWPOSCHANGING` to prevent Windows
//! from pushing the overlay behind the desktop when the user clicks elsewhere.
//! The subclass uses a bypass flag (`BYPASS_SUBCLASS`) so that our own
//! show/hide/reposition calls are not blocked.
//!
//! Window decorations (title bar, borders) are explicitly removed via
//! `GWL_STYLE` manipulation as a defensive measure complementing Tauri's
//! `decorations: false` configuration.
//!
//! Click-through is handled by the frontend's cursor polling which toggles
//! `setIgnoreCursorEvents`: passthrough ON by default (clicks reach desktop
//! icons), passthrough OFF when the cursor hovers a zone (BentoDesk captures).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, SetWindowPos,
    GWL_EXSTYLE, GWL_STYLE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_LAYERED,
    WS_POPUP, WS_CAPTION, WS_THICKFRAME, WS_SYSMENU,
    WS_MINIMIZEBOX, WS_MAXIMIZEBOX, WS_BORDER, WS_DLGFRAME,
    SWP_NOZORDER, SWP_NOACTIVATE, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
    SW_SHOWNOACTIVATE, SW_HIDE,
    WM_WINDOWPOSCHANGING, WM_MOUSEACTIVATE, WINDOWPOS,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
    DWMWA_NCRENDERING_POLICY,
};
use windows::Win32::UI::Controls::MARGINS;

use tauri::AppHandle;

use crate::error::BentoDeskError;

/// Subclass ID for our overlay WndProc subclass.
const OVERLAY_SUBCLASS_ID: usize = 0xBE470;

/// `HWND_BOTTOM` — places a window at the bottom of the z-order.
/// Defined as HWND(1) per Win32 API documentation. Above the desktop shell
/// (Progman) but below all application windows.
///
/// Implemented as a `const fn` so the sentinel is computed once at compile
/// time and the call site stays a simple identifier. Clippy's
/// `manual_dangling_ptr` suggestion (`ptr::dangling_mut()`) would replace
/// the documented sentinel `1` with `align_of::<c_void>()` and silently
/// break z-ordering, so it is suppressed at the definition.
#[allow(clippy::manual_dangling_ptr)]
const fn hwnd_bottom() -> HWND {
    HWND(1 as *mut std::ffi::c_void)
}

/// `SWP_SHOWWINDOW` flag (0x0040) — set by Windows in `WINDOWPOS.flags`
/// when the window is about to be shown. Not re-exported by the windows crate
/// in all feature sets, so we define the raw value.
const SWP_SHOWWINDOW_RAW: u32 = 0x0040;

/// `SWP_HIDEWINDOW` flag (0x0080) — set by Windows in `WINDOWPOS.flags`
/// when the window is about to be hidden.
const SWP_HIDEWINDOW_RAW: u32 = 0x0080;

/// Global visibility flag — the single source of truth for whether the main
/// window is currently shown. Used by the tray toggle instead of querying
/// `window.is_visible()` which can return stale state when we bypass Tauri's
/// show/hide with direct `ShowWindow` calls.
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Global HWND storage so that show/hide can use direct Win32 calls.
/// Stored as `usize` because raw pointers are not `Send + Sync`.
static MAIN_HWND: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

/// Global AppHandle storage so the WndProc subclass can dispatch power
/// resume events to `power::handle_resume` without unsafe casts.
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// When `true`, the WndProc subclass allows z-order changes.
/// Set to `true` before our own `SetWindowPos` / `ShowWindow` calls, then
/// immediately reset to `false`. This prevents the subclass from blocking
/// intentional z-order adjustments (show, hide, reposition).
static BYPASS_SUBCLASS: AtomicBool = AtomicBool::new(false);

/// Query whether the main window is currently visible.
pub fn is_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Relaxed)
}

/// Temporarily bypass the WndProc subclass z-order protection.
///
/// Used by other modules (e.g. resolution monitor) that need to call
/// `SetWindowPos` on the overlay window without being blocked.
///
/// Returns a guard that restores the bypass flag on drop.
pub fn bypass_subclass_guard() -> BypassGuard {
    BYPASS_SUBCLASS.store(true, Ordering::Release);
    BypassGuard
}

/// RAII guard that resets `BYPASS_SUBCLASS` to `false` on drop.
pub struct BypassGuard;

impl Drop for BypassGuard {
    fn drop(&mut self) {
        BYPASS_SUBCLASS.store(false, Ordering::Release);
    }
}

/// Show the main window without activating it (no focus steal).
///
/// After showing, re-asserts the window at `HWND_BOTTOM` so it stays above
/// the desktop shell but below all regular windows. The bypass flag is set
/// so the subclass does not block the z-order adjustment.
pub fn show_window() {
    if let Some(&raw_hwnd) = MAIN_HWND.get() {
        let hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);
        unsafe {
            // Bypass the subclass so our show + z-order set are not blocked.
            BYPASS_SUBCLASS.store(true, Ordering::Release);

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            // Re-assert HWND_BOTTOM z-order: above desktop, below all apps.
            let _ = SetWindowPos(
                hwnd,
                hwnd_bottom(),
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

            BYPASS_SUBCLASS.store(false, Ordering::Release);
        }
        WINDOW_VISIBLE.store(true, Ordering::Relaxed);
        tracing::debug!("Main window shown (NOACTIVATE, HWND_BOTTOM)");
    }
}

/// Hide the main window.
pub fn hide_window() {
    if let Some(&raw_hwnd) = MAIN_HWND.get() {
        let hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);
        unsafe {
            // Bypass the subclass so hide is not blocked.
            BYPASS_SUBCLASS.store(true, Ordering::Release);
            let _ = ShowWindow(hwnd, SW_HIDE);
            BYPASS_SUBCLASS.store(false, Ordering::Release);
        }
        WINDOW_VISIBLE.store(false, Ordering::Relaxed);
        tracing::debug!("Main window hidden");
    }
}

/// Reposition the overlay window to cover the current work area.
///
/// Called by the resolution monitor when the display changes. Uses the
/// bypass flag so the subclass does not block the reposition.
pub fn reposition_to_work_area() {
    if let Some(&raw_hwnd) = MAIN_HWND.get() {
        let hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);

        use windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
        use windows::Win32::Foundation::RECT;

        let mut work_area = RECT::default();
        unsafe {
            let _ = SystemParametersInfoW(
                windows::Win32::UI::WindowsAndMessaging::SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut RECT as *mut _),
                windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }

        let x = work_area.left;
        let y = work_area.top;
        let w = work_area.right - work_area.left;
        let h = work_area.bottom - work_area.top;

        unsafe {
            BYPASS_SUBCLASS.store(true, Ordering::Release);
            let _ = SetWindowPos(
                hwnd,
                hwnd_bottom(),
                x, y, w, h,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            BYPASS_SUBCLASS.store(false, Ordering::Release);
        }

        tracing::info!("Overlay repositioned to work area: {}x{} at ({},{})", w, h, x, y);
    }
}

/// WndProc subclass callback — protects the overlay's z-order.
///
/// Strategy (inspired by Rainmeter's "On Desktop" mode):
/// - Always add `SWP_NOACTIVATE` to prevent focus stealing.
/// - When `BYPASS_SUBCLASS` is false (i.e. Windows initiated the change,
///   not our code), block z-order changes by adding `SWP_NOZORDER`.
///   This prevents clicking the desktop from pushing our window behind.
/// - When `BYPASS_SUBCLASS` is true, allow all changes so our own
///   show/hide/reposition calls work correctly.
/// - Always allow show/hide operations (SWP_SHOWWINDOW / SWP_HIDEWINDOW)
///   regardless of bypass state.
///
/// SAFETY: Called by Windows as a subclass procedure. The `lparam` for
/// `WM_WINDOWPOSCHANGING` points to a valid `WINDOWPOS` struct per MSDN.
// Non-client area message constants
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCPAINT: u32 = 0x0085;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCACTIVATE: u32 = 0x0086;

// Power broadcast message constants
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;

unsafe extern "system" fn overlay_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    // ── Complete non-client area suppression ───────────────────────
    //
    // These four handlers work together to completely eliminate the
    // window frame/title bar, even when DWM composition is active:
    //
    // WM_NCCALCSIZE: "entire window is client area" (no room for frame)
    // WM_NCPAINT:    "don't paint any non-client area" (no frame rendering)
    // WM_NCACTIVATE: "don't draw activation state" (no title bar highlight)
    // WM_NCHITTEST:  "everything is HTCLIENT" (no resize handles/caption)

    if msg == WM_NCCALCSIZE {
        // Regardless of wParam, return 0 to tell Windows: the client
        // area occupies the entire window rect. No room for a title bar.
        return LRESULT(0);
    }

    if msg == WM_NCPAINT {
        // Suppress all non-client area painting. Return 0 without
        // calling DefSubclassProc so no frame is drawn by DWM.
        return LRESULT(0);
    }

    if msg == WM_NCACTIVATE {
        // Return TRUE (1) to prevent DWM from drawing the title bar
        // in active/inactive state. This prevents the frame flash.
        return LRESULT(1);
    }

    if msg == WM_NCHITTEST {
        // HTCLIENT = 1: everything is client area. No resize borders,
        // no caption bar, no system menu — the cursor is always "inside".
        return LRESULT(1); // HTCLIENT
    }

    match msg {
        _ if msg == WM_WINDOWPOSCHANGING => {
            let wp = &mut *(lparam.0 as *mut WINDOWPOS);

            // Always prevent activation — the overlay must never steal focus.
            wp.flags |= SWP_NOACTIVATE;

            // Check if this is a show or hide operation — these must always pass
            // through unmodified so ShowWindow(SW_HIDE/SW_SHOWNOACTIVATE) works.
            let is_show_hide =
                (wp.flags.0 & SWP_SHOWWINDOW_RAW) != 0
                    || (wp.flags.0 & SWP_HIDEWINDOW_RAW) != 0;

            if !is_show_hide && !BYPASS_SUBCLASS.load(Ordering::Acquire) {
                // External z-order change (e.g. user clicked desktop) — block it.
                wp.flags |= SWP_NOZORDER;
            }
        }
        _ if msg == WM_MOUSEACTIVATE => {
            return LRESULT(3); // MA_NOACTIVATE
        }
        _ if msg == WM_POWERBROADCAST => {
            // PBT_APMRESUMEAUTOMATIC: system has resumed from sleep/hibernate.
            // Dispatch recovery to the power module on a background thread.
            if wparam.0 == PBT_APMRESUMEAUTOMATIC {
                if let Some(handle) = APP_HANDLE.get() {
                    crate::power::handle_resume(handle.clone());
                }
            }
            // Return TRUE to indicate we processed the message
            return LRESULT(1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Manages the desktop overlay lifecycle.
pub struct GhostLayerManager;

impl GhostLayerManager {
    /// Configure the main window as a non-intrusive desktop overlay.
    ///
    /// Steps:
    /// 1. Strip window decorations (WS_CAPTION, WS_THICKFRAME, etc.)
    /// 2. Set extended styles: TOOLWINDOW | NOACTIVATE | LAYERED
    /// 3. Install WndProc subclass for z-order protection
    /// 4. Disable DWM non-client rendering + extend frame (no border/shadow)
    /// 5. Position at work area with HWND_BOTTOM z-order
    /// 6. Show without activating
    pub fn attach(handle: &AppHandle) -> Result<(), BentoDeskError> {
        use tauri::Manager;

        let window = handle
            .get_webview_window("main")
            .ok_or_else(|| BentoDeskError::GhostLayerError("Main window not found".into()))?;

        let hwnd = window.hwnd().map_err(|e| {
            BentoDeskError::GhostLayerError(format!("Failed to get HWND: {e}"))
        })?;

        let hwnd = HWND(hwnd.0);

        // Store the HWND for later show/hide/reposition calls.
        let _ = MAIN_HWND.set(hwnd.0 as usize);

        // Store the AppHandle so the WndProc subclass can dispatch
        // WM_POWERBROADCAST events to the power module.
        let _ = APP_HANDLE.set(handle.clone());

        // ── Step 0: Use Tauri API to force decorations off ────────────
        //
        // The tauri.conf.json `decorations: false` may not be honored on
        // all systems. Calling the runtime API ensures Tauri internally
        // handles the WS_CAPTION removal with proper sequencing.
        let _ = window.set_decorations(false);

        // ── Diagnostics: log HWND, styles, parent, class name ─────────
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                GetParent, GetClassNameW, GetWindowTextW, IsWindowVisible,
            };
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let parent = GetParent(hwnd).unwrap_or(HWND(std::ptr::null_mut()));

            let mut class_buf = [0u16; 256];
            let class_len = GetClassNameW(hwnd, &mut class_buf);
            let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);

            let mut title_buf = [0u16; 256];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

            let visible = IsWindowVisible(hwnd).as_bool();

            tracing::info!(
                "DIAG: hwnd={:?} class='{}' title='{}' style=0x{:08X} exstyle=0x{:08X} parent={:?} visible={}",
                hwnd.0, class_name, title, style, ex_style, parent.0, visible
            );

            // Also check if there's a parent window that might have the frame
            if !parent.0.is_null() {
                let p_style = GetWindowLongPtrW(parent, GWL_STYLE);
                let p_ex_style = GetWindowLongPtrW(parent, GWL_EXSTYLE);
                let mut p_class = [0u16; 256];
                let p_len = GetClassNameW(parent, &mut p_class);
                let p_class_name = String::from_utf16_lossy(&p_class[..p_len as usize]);
                let mut p_title = [0u16; 256];
                let p_tlen = GetWindowTextW(parent, &mut p_title);
                let p_title_str = String::from_utf16_lossy(&p_title[..p_tlen as usize]);
                tracing::info!(
                    "DIAG PARENT: hwnd={:?} class='{}' title='{}' style=0x{:08X} exstyle=0x{:08X}",
                    parent.0, p_class_name, p_title_str, p_style, p_ex_style
                );
            }
        }

        // ── Step 1: Install WndProc subclass FIRST ────────────────────
        //
        // Must be installed BEFORE any style changes so that our
        // WM_NCCALCSIZE handler is active when Windows re-evaluates
        // the window frame. The WM_NCCALCSIZE → return 0 trick tells
        // Windows "this window has NO non-client area" which is the
        // definitive way to eliminate the title bar and borders —
        // it cannot be overridden by Tauri re-applying styles later.
        //
        // SAFETY: SetWindowSubclass with valid HWND and function pointer.
        unsafe {
            let ok = SetWindowSubclass(
                hwnd,
                Some(overlay_subclass_proc),
                OVERLAY_SUBCLASS_ID,
                0,
            );
            if !ok.as_bool() {
                tracing::warn!("Failed to install overlay WndProc subclass");
            }
        }

        // ── Step 2: Strip ALL window decorations ──────────────────────
        //
        // Set WS_POPUP and clear all frame-related flags.
        // With the subclass intercepting WM_NCCALCSIZE, the non-client
        // area (title bar + borders) is forcibly zeroed out.
        //
        // SAFETY: GetWindowLongPtrW/SetWindowLongPtrW with valid HWND.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let new_style = (style | WS_POPUP.0 as isize)
                & !(WS_CAPTION.0 as isize)
                & !(WS_THICKFRAME.0 as isize)
                & !(WS_SYSMENU.0 as isize)
                & !(WS_MINIMIZEBOX.0 as isize)
                & !(WS_MAXIMIZEBOX.0 as isize)
                & !(WS_BORDER.0 as isize)
                & !(WS_DLGFRAME.0 as isize);
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);

            // Force immediate frame recalculation — triggers WM_NCCALCSIZE
            // which our subclass intercepts to return 0 (no non-client area).
            let _ = SetWindowPos(
                hwnd, None, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }

        // ── Step 3: Set extended styles ───────────────────────────────
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let new_ex_style = ex_style
                | WS_EX_TOOLWINDOW.0 as isize
                | WS_EX_NOACTIVATE.0 as isize
                | WS_EX_LAYERED.0 as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);
        }

        // ── Step 4: Disable DWM border/shadow/rounded corners ─────────
        //
        // SAFETY: DwmSetWindowAttribute/DwmExtendFrameIntoClientArea
        // with valid HWND.
        unsafe {
            // DWMNCRP_DISABLED = 1: no window chrome rendering
            let disabled: u32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &disabled as *const u32 as *const _,
                std::mem::size_of::<u32>() as u32,
            );

            // Disable Windows 11 rounded corners on the overlay window.
            // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_DONOTROUND = 1
            let do_not_round: u32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(33),
                &do_not_round as *const u32 as *const _,
                std::mem::size_of::<u32>() as u32,
            );

            // Full-bleed DWM frame extension with -1 margins.
            // This tells Windows to extend the client area fully into the
            // non-client area, eliminating the 1px border artifact that can
            // appear on Windows 11 builds 22621+ with transparent windows.
            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }

        // ── Step 5: Position at work area with HWND_BOTTOM ────────────
        //
        // Work area = screen minus taskbar. HWND_BOTTOM places the window
        // at the bottom of the normal z-order: above the desktop shell
        // (Progman/WorkerW) but below all application windows.
        //
        // SAFETY: SystemParametersInfoW + SetWindowPos with valid HWND.
        use windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
        use windows::Win32::Foundation::RECT;

        let mut work_area = RECT::default();
        unsafe {
            let _ = SystemParametersInfoW(
                windows::Win32::UI::WindowsAndMessaging::SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut RECT as *mut _),
                windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }

        let x = work_area.left;
        let y = work_area.top;
        let w = work_area.right - work_area.left;
        let h = work_area.bottom - work_area.top;

        unsafe {
            // Bypass subclass so our SetWindowPos is not blocked
            BYPASS_SUBCLASS.store(true, Ordering::Release);
            let _ = SetWindowPos(
                hwnd,
                hwnd_bottom(),
                x, y,
                w, h,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            BYPASS_SUBCLASS.store(false, Ordering::Release);
        }

        // ── Step 6: Show without activating ───────────────────────────
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        WINDOW_VISIBLE.store(true, Ordering::Relaxed);

        tracing::info!(
            "Overlay: {}x{} at ({},{}) — TOOLWINDOW | NOACTIVATE | LAYERED | NO_DECORATIONS | HWND_BOTTOM | SUBCLASSED",
            w, h, x, y
        );

        // ── Step 7: Deferred re-apply ─────────────────────────────────
        //
        // Tauri may re-apply window styles after our setup hook returns.
        // Spawn a thread that waits 500ms and then re-applies the WS_POPUP
        // style + SWP_FRAMECHANGED. The subclass's WM_NCCALCSIZE handler
        // ensures the title bar stays gone even if Tauri restores styles
        // in between, but this second pass is a safety net.
        let hwnd_raw = hwnd.0 as usize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let hwnd = HWND(hwnd_raw as *mut std::ffi::c_void);
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let new_style = (style | WS_POPUP.0 as isize)
                    & !(WS_CAPTION.0 as isize)
                    & !(WS_THICKFRAME.0 as isize)
                    & !(WS_SYSMENU.0 as isize)
                    & !(WS_MINIMIZEBOX.0 as isize)
                    & !(WS_MAXIMIZEBOX.0 as isize)
                    & !(WS_BORDER.0 as isize)
                    & !(WS_DLGFRAME.0 as isize);
                SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);
                BYPASS_SUBCLASS.store(true, Ordering::Release);
                let _ = SetWindowPos(
                    hwnd, None, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
                BYPASS_SUBCLASS.store(false, Ordering::Release);
            }
            let final_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
            tracing::info!("Deferred style re-apply: style=0x{:08X}", final_style);
        });

        Ok(())
    }

    /// Detach the overlay by removing the WndProc subclass.
    pub fn detach(handle: &AppHandle) -> Result<(), BentoDeskError> {
        use tauri::Manager;
        use windows::Win32::UI::Shell::RemoveWindowSubclass;

        if let Some(window) = handle.get_webview_window("main") {
            if let Ok(hwnd) = window.hwnd() {
                let hwnd = HWND(hwnd.0);
                // SAFETY: RemoveWindowSubclass with valid HWND and subclass ID.
                unsafe {
                    let _ = RemoveWindowSubclass(
                        hwnd,
                        Some(overlay_subclass_proc),
                        OVERLAY_SUBCLASS_ID,
                    );
                }
            }
        }

        tracing::info!("Overlay detached (subclass removed)");
        Ok(())
    }
}
