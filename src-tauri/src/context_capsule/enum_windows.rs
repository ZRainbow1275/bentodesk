//! Enumerate visible top-level windows via Win32 `EnumWindows`.
//!
//! Returns a `Vec<LiveWindow>` with the fields needed to:
//!   * Filter shell / system / our own overlay from capture.
//!   * Match captured windows back to live HWNDs on restore.
//!
//! Only windows that are visible, not minimised, with a non-empty title, and
//! whose class is not in the shell blacklist are returned.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, MAX_PATH, RECT};
use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed,
};

/// A single live window snapshot used for matching & capture.
#[derive(Debug, Clone)]
pub struct LiveWindow {
    pub hwnd: isize,
    pub title: String,
    pub class_name: String,
    pub process_name: String,
    pub rect: (i32, i32, i32, i32), // (left, top, right, bottom)
    pub is_maximized: bool,
}

const SHELL_BLACKLIST: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Button",
    "NotifyIconOverflowWindow",
    "Windows.UI.Core.CoreWindow",
    "ApplicationFrameWindow",
    "BentoDesk",
];

/// Enumerate visible top-level windows, filtered to exclude shell / system UI.
pub fn enumerate_windows() -> Vec<LiveWindow> {
    let mut out: Vec<LiveWindow> = Vec::new();
    let ptr = &mut out as *mut Vec<LiveWindow> as isize;
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(ptr));
    }
    out
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out_ptr = lparam.0 as *mut Vec<LiveWindow>;
    if out_ptr.is_null() {
        return BOOL(1);
    }
    let out = &mut *out_ptr;

    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    if IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }

    // Title filter — empty titles are background/tool windows we don't want.
    let title_len = GetWindowTextLengthW(hwnd);
    if title_len == 0 {
        return BOOL(1);
    }
    let mut title_buf = vec![0u16; (title_len as usize) + 1];
    let n = GetWindowTextW(hwnd, &mut title_buf);
    let title = String::from_utf16_lossy(&title_buf[..n as usize]);
    if title.trim().is_empty() {
        return BOOL(1);
    }

    // Class name filter — exclude shell & our own windows.
    let mut class_buf = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buf);
    let class = String::from_utf16_lossy(&class_buf[..class_len as usize]);
    if SHELL_BLACKLIST.iter().any(|c| class.contains(c)) {
        return BOOL(1);
    }

    // Rect — needed for restore.
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return BOOL(1);
    }

    // Process name — used as the strongest matcher when titles drift.
    let mut pid = 0u32;
    let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let process_name = lookup_process_name(pid);

    let is_max = IsZoomed(hwnd).as_bool();

    out.push(LiveWindow {
        hwnd: hwnd.0 as isize,
        title,
        class_name: class,
        process_name,
        rect: (rect.left, rect.top, rect.right, rect.bottom),
        is_maximized: is_max,
    });

    BOOL(1)
}

/// Best-effort: resolve a PID to the basename of its executable.
/// Returns "" on failure — the matcher handles missing names gracefully.
fn lookup_process_name(pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };
        let mut buf = vec![0u16; MAX_PATH as usize];
        let len = GetModuleFileNameExW(handle, None, &mut buf);
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        if len == 0 {
            return String::new();
        }
        let full = OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .into_owned();
        full.rsplit('\\').next().unwrap_or(&full).to_string()
    }
}
