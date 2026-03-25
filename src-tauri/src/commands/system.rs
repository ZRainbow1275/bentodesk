//! System info, memory, and drag commands.

use serde::Serialize;
use tauri::State;

use crate::drag_drop::drag_manager;
use crate::layout::resolution::{self, Resolution};
use crate::AppState;

/// System information exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub os_version: String,
    pub resolution: Resolution,
    pub dpi: f64,
    pub desktop_path: String,
    pub webview2_version: Option<String>,
    pub memory_usage: MemoryInfo,
}

/// Process memory information.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let res = resolution::get_current_resolution();
    let dpi = resolution::get_dpi_scale();
    let desktop_path = dirs::desktop_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mem = query_memory_info();

    Ok(SystemInfo {
        os_version: query_os_version(),
        resolution: res,
        dpi,
        desktop_path,
        webview2_version: query_webview2_version(),
        memory_usage: mem,
    })
}

/// Query the Windows version string via `RtlGetVersion`.
///
/// Returns e.g. "Windows 10.0.22631" instead of the vague "OS" env var.
fn query_os_version() -> String {
    use windows::Win32::System::SystemInformation::{
        GetVersionExW, OSVERSIONINFOW,
    };

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: GetVersionExW is a well-documented API that fills the provided struct.
    // We set dwOSVersionInfoSize to the correct struct size.
    let ok = unsafe { GetVersionExW(&mut info) };
    if ok.is_ok() {
        format!(
            "Windows {}.{}.{}",
            info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
        )
    } else {
        std::env::var("OS").unwrap_or_else(|_| "Windows".to_string())
    }
}

/// Query the installed WebView2 runtime version from the registry.
///
/// WebView2 stores its version at:
/// `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`
fn query_webview2_version() -> Option<String> {
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let subkey: Vec<u16> = OsStr::new(
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    )
    .encode_wide()
    .chain(std::iter::once(0))
    .collect();

    let value_name: Vec<u16> = OsStr::new("pv")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut buf = vec![0u16; 128];
    let mut buf_size = (buf.len() * 2) as u32;

    // SAFETY: RegGetValueW reads a registry value into the provided buffer.
    // We pass valid null-terminated wide strings and a correctly sized buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            windows::core::PCWSTR(subkey.as_ptr()),
            windows::core::PCWSTR(value_name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut buf_size),
        )
    };

    if status.is_ok() {
        let len = (buf_size as usize / 2).saturating_sub(1); // Remove null terminator
        let version = String::from_utf16_lossy(&buf[..len]);
        if version.is_empty() {
            None
        } else {
            Some(version)
        }
    } else {
        None
    }
}

#[tauri::command]
pub async fn start_drag(
    _state: State<'_, AppState>,
    file_paths: Vec<String>,
) -> Result<String, String> {
    // OLE drag-drop must run on a thread that owns a COM STA.
    // Spawn a dedicated thread to avoid blocking the async runtime.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = drag_manager::start_drag_operation(&file_paths);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|e| format!("Drag thread communication error: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_memory_usage() -> Result<MemoryInfo, String> {
    Ok(query_memory_info())
}

/// Query current process memory via Win32 `GetProcessMemoryInfo`.
fn query_memory_info() -> MemoryInfo {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that does not need closing.
    // GetProcessMemoryInfo fills the provided struct with memory counters.
    unsafe {
        let process = GetCurrentProcess();
        let _ = GetProcessMemoryInfo(
            process,
            &mut pmc,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        );
    }

    MemoryInfo {
        working_set_bytes: pmc.WorkingSetSize,
        peak_working_set_bytes: pmc.PeakWorkingSetSize,
    }
}
