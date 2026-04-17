//! System info, memory, and drag commands.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::desktop_sources;
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
    /// All active Desktop sources watched by BentoDesk (user / public / OneDrive
    /// / settings override). Populated by `desktop_sources::all_desktop_dirs`.
    pub desktop_sources: Vec<DesktopSourceInfo>,
    pub webview2_version: Option<String>,
    pub memory_usage: MemoryInfo,
}

/// A single legitimate Desktop source location, annotated with its kind so the
/// frontend can render a meaningful label and icon.
#[derive(Debug, Clone, Serialize)]
pub struct DesktopSourceInfo {
    pub path: String,
    /// One of "user" / "public" / "onedrive" / "custom".
    pub kind: String,
    /// Whether the watcher is attached to this source. Currently every source
    /// returned by `all_desktop_dirs` is watched, but the field keeps room for
    /// future per-source disable toggles.
    pub watched: bool,
}

/// Process memory information.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

/// Classify a canonicalized Desktop path against the well-known sources so the
/// frontend can show a meaningful label.
fn classify_source(path: &Path, custom: Option<&str>) -> String {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string()
    };
    let key = norm(path);

    if let Some(user) = dirs::desktop_dir() {
        if norm(&user) == key {
            return "user".to_string();
        }
    }

    if let Some(pub_var) = std::env::var_os("PUBLIC") {
        let pub_desktop = PathBuf::from(pub_var).join("Desktop");
        if norm(&pub_desktop) == key {
            return "public".to_string();
        }
    }
    // Heuristic: shared Desktop always lives under \Users\Public\ on Windows.
    if key.contains(r"\users\public\") {
        return "public".to_string();
    }

    for var in &["OneDrive", "OneDriveConsumer"] {
        if let Some(val) = std::env::var_os(var) {
            let od_desktop = PathBuf::from(val).join("Desktop");
            if norm(&od_desktop) == key {
                return "onedrive".to_string();
            }
        }
    }
    if key.contains(r"\onedrive") {
        return "onedrive".to_string();
    }

    if let Some(c) = custom {
        if !c.trim().is_empty() {
            let c_path = PathBuf::from(c);
            if norm(&c_path) == key {
                return "custom".to_string();
            }
        }
    }

    // Reaching the fallback means the path didn't match user / public /
    // OneDrive / settings.desktop_path. Most likely cause: dirs::desktop_dir()
    // returned None (very rare on Windows) so the user-desktop check above
    // silently skipped. Log a warning so this doesn't masquerade as a real
    // "custom override" entry in the Settings UI.
    tracing::warn!(
        "classify_source falling back to \"custom\" for path {:?} (custom override = {:?})",
        path,
        custom,
    );
    "custom".to_string()
}

fn collect_desktop_sources(custom: Option<&str>) -> Vec<DesktopSourceInfo> {
    desktop_sources::all_desktop_dirs(custom)
        .into_iter()
        .map(|p| DesktopSourceInfo {
            kind: classify_source(&p, custom),
            path: p.to_string_lossy().to_string(),
            watched: true,
        })
        .collect()
}

#[tauri::command]
pub async fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    let res = resolution::get_current_resolution();
    let dpi = resolution::get_dpi_scale();
    let desktop_path = dirs::desktop_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mem = query_memory_info();

    let custom = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.desktop_path.clone()
    };
    let sources = collect_desktop_sources(Some(&custom));

    Ok(SystemInfo {
        os_version: query_os_version(),
        resolution: res,
        dpi,
        desktop_path,
        desktop_sources: sources,
        webview2_version: query_webview2_version(),
        memory_usage: mem,
    })
}

/// Expose the active Desktop source list to the frontend for reactive refresh
/// (e.g. after the user toggles OneDrive Desktop backup from the Settings
/// panel). Lighter-weight than a full `get_system_info` round-trip.
#[tauri::command]
pub async fn get_desktop_sources(
    state: State<'_, AppState>,
) -> Result<Vec<DesktopSourceInfo>, String> {
    let custom = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.desktop_path.clone()
    };
    Ok(collect_desktop_sources(Some(&custom)))
}

/// Query the Windows version string via `RtlGetVersion`.
///
/// Returns e.g. "Windows 10.0.22631" instead of the vague "OS" env var.
fn query_os_version() -> String {
    use windows::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};

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
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

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
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
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
