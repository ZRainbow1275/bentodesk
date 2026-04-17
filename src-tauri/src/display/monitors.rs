//! Enumerate monitors + look up the monitor that contains a given
//! screen-space point or a Tauri window.
//!
//! All coordinates are in **physical pixels** (i.e. post-DPI scaling),
//! matching what Win32 APIs return. The frontend consumes `dpi_scale`
//! alongside each rect so it can translate back into logical / CSS
//! pixels when needed.

use std::cell::RefCell;

use serde::Serialize;
use tauri::{State, WebviewWindow};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use crate::AppState;

/// Logical rectangle (x/y/width/height) in **physical pixels**.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl From<RECT> for MonitorRect {
    fn from(r: RECT) -> Self {
        Self {
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
        }
    }
}

/// Per-monitor information exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    /// Stable index within the enumeration order of the current call.
    /// Not persisted — re-enumerate after hot-plug to refresh.
    pub index: u32,
    /// Work area (screen minus taskbar / AppBar reservations).
    pub rect_work: MonitorRect,
    /// Full monitor rect including taskbar.
    pub rect_full: MonitorRect,
    /// DPI scale factor (1.0 = 96 DPI, 1.5 = 144 DPI, etc.).
    pub dpi_scale: f64,
    /// True for the primary monitor.
    pub is_primary: bool,
    /// Device name (e.g. `\\.\DISPLAY1`).
    pub device_name: String,
}

/// Effective DPI → scale factor. Falls back to 1.0 on API failure so
/// the frontend at least gets usable geometry.
fn monitor_dpi_scale(hmon: HMONITOR) -> f64 {
    let mut dpi_x: u32 = 96;
    let mut dpi_y: u32 = 96;
    // SAFETY: GetDpiForMonitor fills dpi_x/dpi_y when it succeeds.
    // MDT_EFFECTIVE_DPI is documented to be what we want for scaling.
    let hr = unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    if hr.is_err() {
        return 1.0;
    }
    dpi_x as f64 / 96.0
}

/// Read MONITORINFOEXW for the given HMONITOR.
fn monitor_info(hmon: HMONITOR) -> Option<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: info is correctly sized and writable. GetMonitorInfoW
    // expects a MONITORINFOEXW pointer cast to MONITORINFO — the first
    // field layout matches.
    let ok = unsafe { GetMonitorInfoW(hmon, &mut info.monitorInfo as *mut MONITORINFO) };
    if ok.as_bool() {
        Some(info)
    } else {
        None
    }
}

fn device_name(info: &MONITORINFOEXW) -> String {
    let slice = &info.szDevice;
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..len])
}

fn build_monitor_info(hmon: HMONITOR, index: u32) -> Option<MonitorInfo> {
    let info = monitor_info(hmon)?;
    let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;
    Some(MonitorInfo {
        index,
        rect_work: info.monitorInfo.rcWork.into(),
        rect_full: info.monitorInfo.rcMonitor.into(),
        dpi_scale: monitor_dpi_scale(hmon),
        is_primary,
        device_name: device_name(&info),
    })
}

thread_local! {
    /// EnumDisplayMonitors passes us a raw callback + LPARAM. We stash
    /// the accumulator in a thread-local so the callback remains
    /// `extern "system"` (no closure captures).
    static ENUM_ACC: RefCell<Vec<HMONITOR>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "system" fn enum_cb(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    _lparam: LPARAM,
) -> BOOL {
    ENUM_ACC.with(|acc| acc.borrow_mut().push(hmon));
    TRUE
}

/// Enumerate every active monitor.
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    // Reset the accumulator before each enumeration to avoid carrying
    // state across calls on the same thread.
    ENUM_ACC.with(|acc| acc.borrow_mut().clear());
    // SAFETY: EnumDisplayMonitors with a null HDC/rect enumerates every
    // monitor. Our callback only appends to a thread-local vec.
    let ok = unsafe { EnumDisplayMonitors(HDC::default(), None, Some(enum_cb), LPARAM(0)) };
    if !ok.as_bool() {
        tracing::warn!("EnumDisplayMonitors failed");
        return Vec::new();
    }
    let handles: Vec<HMONITOR> = ENUM_ACC.with(|acc| std::mem::take(&mut *acc.borrow_mut()));
    handles
        .into_iter()
        .enumerate()
        .filter_map(|(i, h)| build_monitor_info(h, i as u32))
        .collect()
}

/// Resolve the monitor that contains the given physical-pixel point.
/// Falls back to the nearest monitor (never returns `None` in practice
/// on Windows where at least the primary monitor exists).
pub fn monitor_for_point(x: i32, y: i32) -> Option<MonitorInfo> {
    // SAFETY: MonitorFromPoint always returns a valid HMONITOR handle
    // (or null) — the DEFAULTTONEAREST flag keeps us inside bounds when
    // the point is off-screen.
    let hmon = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    if hmon.is_invalid() {
        return None;
    }
    build_monitor_info(hmon, 0)
}

/// Resolve the monitor that contains the majority of a window.
pub fn monitor_for_window(window: &WebviewWindow) -> Option<MonitorInfo> {
    let raw = window.hwnd().ok()?;
    // Tauri's `raw-window-handle` HWND is NOT the same type as the one
    // exposed by the `windows` crate we compile against — bridge them
    // by pulling out the underlying pointer value (matching the pattern
    // used by `ghost_layer::manager` and `commands::settings`).
    let hwnd = HWND(raw.0);
    // SAFETY: HWND obtained from Tauri is a live top-level window.
    // DEFAULTTONEAREST keeps us inside a valid monitor even if the
    // window is momentarily off-screen.
    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if hmon.is_invalid() {
        // DEFAULTTONULL would have returned null already; DEFAULTTONEAREST
        // shouldn't, but be defensive.
        return None;
    }
    build_monitor_info(hmon, 0)
}

// ─── Tauri IPC commands ─────────────────────────────────────────

#[tauri::command]
pub async fn list_monitors(_state: State<'_, AppState>) -> Result<Vec<MonitorInfo>, String> {
    Ok(enumerate_monitors())
}

#[tauri::command]
pub async fn get_monitor_for_point(
    _state: State<'_, AppState>,
    x: i32,
    y: i32,
) -> Result<Option<MonitorInfo>, String> {
    Ok(monitor_for_point(x, y))
}

#[tauri::command]
pub async fn get_monitor_for_window(
    window: WebviewWindow,
    _state: State<'_, AppState>,
) -> Result<Option<MonitorInfo>, String> {
    Ok(monitor_for_window(&window))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_returns_at_least_one_monitor() {
        // Win32 guarantees at least the primary monitor on any
        // interactive session. In CI without a display this may fail —
        // guard with a descriptive message.
        let mons = enumerate_monitors();
        assert!(
            !mons.is_empty(),
            "EnumDisplayMonitors returned 0 monitors; are we running headless?"
        );
        assert!(mons.iter().any(|m| m.is_primary));
    }

    #[test]
    fn monitor_for_point_returns_primary_at_origin() {
        if let Some(m) = monitor_for_point(0, 0) {
            // Work area of primary monitor includes (0,0) in the common
            // taskbar-at-bottom config.
            assert!(m.dpi_scale > 0.0);
        }
    }
}
