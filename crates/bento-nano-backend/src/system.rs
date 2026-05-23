//! T-092 — system info surface.
//!
//! Composes the `SystemInfo` payload that the Settings UI's "About" / "System"
//! card renders. The 1.x source coupled this module to `tauri::AppState` (for
//! `settings.desktop_path`), to `crate::layout::resolution` (for screen
//! resolution + DPI), and to `crate::drag_drop::drag_manager` (the
//! `start_drag` Tauri command).
//!
//! The nano port:
//!
//! - Drops `start_drag` — that lives in [`crate::drag_drop`] (T-083) and the
//!   dispatcher invokes the COM-thread spawn directly.
//! - Drops the `resolution` / `dpi` fields — those come from
//!   `crate::layout::resolution` (T-097, ships in this same wave) and the
//!   dispatcher composes them into the final IPC `SystemInfoPayload` so this
//!   module stays free of layout concerns.
//! - Replaces `tauri::AppState` + `settings.read()` with a direct
//!   `desktop_path: Option<&str>` parameter on the public entry points so
//!   the settings model never crosses crate boundaries (master plan §11).
//! - Replaces `windows` 0.58 typed bindings with `windows-sys` 0.59 raw
//!   bindings (smaller binary, single COM crate already in scope).
//! - Reuses [`crate::desktop_sources::user_desktop_dir`] instead of pulling
//!   the forbidden `dirs` crate.
//!
//! ## Public surface
//!
//! - [`SystemInfo`] / [`MemoryInfo`] / [`DesktopSourceInfo`] — DTOs the IPC
//!   surface returns. Serde-derived per ΔB ruling.
//! - [`get_system_info`] — full snapshot (OS version, memory, desktop sources,
//!   WebView2 version, current Desktop path).
//! - [`get_memory_usage`] — lighter-weight call for the memory-only sparkline.
//! - [`get_desktop_sources`] — desktop-source list for reactive refresh after
//!   the user toggles OneDrive Desktop backup in Settings.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::desktop_sources;

// ─── DTOs ────────────────────────────────────────────────────────────

/// System information payload exposed to the Settings UI's "About" card.
///
/// Resolution + DPI come from [`crate::layout::resolution`] in the dispatcher
/// composition layer; this module deliberately stays layout-free so the
/// settings card can paint without instantiating the layout subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemInfo {
    /// `"Windows 10.0.22631"` style. Derived from `RtlGetVersion` (best
    /// effort — gracefully falls back to the `OS` env var on failure).
    pub os_version: SmolStr,
    /// Current user's Desktop path (resolved through `SHGetKnownFolderPath`,
    /// honours OneDrive redirection automatically).
    pub desktop_path: String,
    /// Every active Desktop source (user / public / OneDrive / settings
    /// override) annotated with its kind.
    pub desktop_sources: Vec<DesktopSourceInfo>,
    /// Installed WebView2 runtime version, from registry. `None` when
    /// WebView2 is not installed (e.g. fresh Windows 10).
    pub webview2_version: Option<SmolStr>,
    /// Current process memory.
    pub memory_usage: MemoryInfo,
}

/// A single legitimate Desktop source location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSourceInfo {
    pub path: String,
    /// One of `"user"` / `"public"` / `"onedrive"` / `"custom"`.
    pub kind: SmolStr,
    /// Whether the watcher is attached to this source. Currently every source
    /// returned by [`desktop_sources::all_desktop_dirs`] is watched; the field
    /// keeps room for future per-source disable toggles.
    pub watched: bool,
}

/// Process memory information from `GetProcessMemoryInfo`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryInfo {
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

// ─── Public entry points ─────────────────────────────────────────────

/// Build the full system-info snapshot. `desktop_path` is the user's
/// `settings.desktop_path` override, or `None` to skip the "custom" source.
pub fn get_system_info(desktop_path: Option<&str>) -> SystemInfo {
    let desktop = desktop_sources::user_desktop_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    SystemInfo {
        os_version: query_os_version(),
        desktop_path: desktop,
        desktop_sources: collect_desktop_sources(desktop_path),
        webview2_version: query_webview2_version(),
        memory_usage: query_memory_info(),
    }
}

/// Lighter-weight memory-only snapshot for the Settings card sparkline.
pub fn get_memory_usage() -> MemoryInfo {
    query_memory_info()
}

/// Active Desktop source list. Lighter than [`get_system_info`] — used after
/// the user toggles OneDrive Desktop backup in Settings.
pub fn get_desktop_sources(desktop_path: Option<&str>) -> Vec<DesktopSourceInfo> {
    collect_desktop_sources(desktop_path)
}

// ─── Internals ───────────────────────────────────────────────────────

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

/// Lowercased + slash-normalised + trim-trailing-backslash key for
/// case-insensitive comparison. Matches the same algorithm used internally
/// by [`crate::desktop_sources`] so the classifier sees the same shape the
/// deduper saw.
fn norm_key(p: &Path) -> String {
    p.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

/// Classify a canonicalised Desktop path against the well-known sources so
/// the Settings UI can render a meaningful label and icon.
///
/// Falls back to `"custom"` only when the path doesn't match user / public /
/// OneDrive / `settings.desktop_path`. Logs a `tracing::warn!` so callers can
/// distinguish a real custom override from a misclassification.
fn classify_source(path: &Path, custom: Option<&str>) -> SmolStr {
    let key = norm_key(path);

    if let Some(user) = desktop_sources::user_desktop_dir() {
        if norm_key(&user) == key {
            return SmolStr::new_static("user");
        }
    }

    if let Some(pub_var) = std::env::var_os("PUBLIC") {
        let pub_desktop = PathBuf::from(pub_var).join("Desktop");
        if norm_key(&pub_desktop) == key {
            return SmolStr::new_static("public");
        }
    }
    // Heuristic: shared Desktop always lives under \Users\Public\ on Windows.
    if key.contains(r"\users\public\") {
        return SmolStr::new_static("public");
    }

    for var in &["OneDrive", "OneDriveConsumer"] {
        if let Some(val) = std::env::var_os(var) {
            let od_desktop = PathBuf::from(val).join("Desktop");
            if norm_key(&od_desktop) == key {
                return SmolStr::new_static("onedrive");
            }
        }
    }
    if key.contains(r"\onedrive") {
        return SmolStr::new_static("onedrive");
    }

    if let Some(c) = custom {
        if !c.trim().is_empty() {
            let c_path = PathBuf::from(c);
            if norm_key(&c_path) == key {
                return SmolStr::new_static("custom");
            }
        }
    }

    tracing::warn!(
        "classify_source falling back to \"custom\" for path {:?} (custom override = {:?})",
        path,
        custom,
    );
    SmolStr::new_static("custom")
}

/// Query the Windows version string via `GetVersionExW`.
///
/// Returns e.g. `"Windows 10.0.22631"`. On modern Windows the returned
/// version may be capped without an app-manifest, but matches 1.x parity.
/// Falls back to the `OS` env var (typically `"Windows_NT"`) on hard failure.
fn query_os_version() -> SmolStr {
    use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};

    let mut info: OSVERSIONINFOW = unsafe { core::mem::zeroed() };
    info.dwOSVersionInfoSize = core::mem::size_of::<OSVERSIONINFOW>() as u32;

    // SAFETY: `GetVersionExW` reads `dwOSVersionInfoSize` first to decide
    // how many bytes to fill. We zeroed the struct and set the size field
    // before the call. The function is documented as deprecated but never
    // invalid; on failure (return 0) the struct is left zeroed and we fall
    // back to the env var.
    let ok = unsafe { GetVersionExW(&mut info) };
    if ok != 0 {
        let s = format!(
            "Windows {}.{}.{}",
            info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
        );
        SmolStr::from(s)
    } else {
        SmolStr::from(std::env::var("OS").unwrap_or_else(|_| "Windows".to_string()))
    }
}

/// Query the installed WebView2 runtime version from the registry at
/// `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F30...}\pv`.
/// Returns `None` when WebView2 is not installed or the registry call fails.
fn query_webview2_version() -> Option<SmolStr> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

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

    // SAFETY: `RegGetValueW` reads a registry value into the provided buffer.
    // We pass valid null-terminated wide strings for the subkey and value
    // name; `buf_size` is the size in bytes (per the docs, REG_SZ values are
    // counted in bytes including the null terminator). The function returns
    // a non-zero LSTATUS on failure and never writes to the buffer in that
    // case.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            core::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut buf_size,
        )
    };

    if status != ERROR_SUCCESS {
        return None;
    }
    // `buf_size` now holds the byte count including the trailing null.
    let utf16_len = (buf_size as usize / 2).saturating_sub(1);
    let version = String::from_utf16_lossy(&buf[..utf16_len]);
    if version.is_empty() {
        None
    } else {
        Some(SmolStr::from(version))
    }
}

/// Query current process memory via `GetProcessMemoryInfo` (psapi.dll).
fn query_memory_info() -> MemoryInfo {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut pmc: PROCESS_MEMORY_COUNTERS = unsafe { core::mem::zeroed() };
    let cb = core::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

    // SAFETY: `GetCurrentProcess` returns a pseudo-handle that does not need
    // closing. `GetProcessMemoryInfo` fills the struct on success; on failure
    // the struct stays zeroed and we surface an all-zero MemoryInfo, which
    // the UI renders as "—" rather than panicking.
    let ok = unsafe {
        let process = GetCurrentProcess();
        GetProcessMemoryInfo(process, &mut pmc, cb)
    };
    if ok == 0 {
        tracing::warn!("system::query_memory_info: GetProcessMemoryInfo failed");
    }
    MemoryInfo {
        working_set_bytes: pmc.WorkingSetSize,
        peak_working_set_bytes: pmc.PeakWorkingSetSize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_version_starts_with_windows() {
        let s = query_os_version();
        assert!(s.as_str().starts_with("Windows"), "got {s:?}");
    }

    #[test]
    fn memory_info_is_nonzero_for_running_process() {
        let mem = query_memory_info();
        assert!(mem.working_set_bytes > 0, "got {mem:?}");
        assert!(
            mem.peak_working_set_bytes >= mem.working_set_bytes,
            "got {mem:?}"
        );
    }

    #[test]
    fn classify_user_desktop_returns_user_kind() {
        if let Some(user) = desktop_sources::user_desktop_dir() {
            assert_eq!(classify_source(&user, None).as_str(), "user");
        }
    }

    #[test]
    fn classify_public_path_via_heuristic() {
        let path = Path::new(r"C:\Users\Public\Desktop");
        assert_eq!(classify_source(path, None).as_str(), "public");
    }

    #[test]
    fn classify_unknown_falls_back_to_custom() {
        let path = Path::new(r"Z:\some-totally-unrelated-folder");
        assert_eq!(classify_source(path, None).as_str(), "custom");
    }

    #[test]
    fn get_system_info_payload_is_serializable() {
        let info = get_system_info(None);
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: SystemInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, info);
    }

    #[test]
    fn get_memory_usage_round_trips_through_serde() {
        let mem = get_memory_usage();
        let json = serde_json::to_string(&mem).expect("serialize");
        let parsed: MemoryInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, mem);
    }

    #[test]
    fn desktop_sources_list_contains_at_least_one_entry() {
        // Any healthy Windows install has at least the user Desktop or the
        // public Desktop. If neither is reachable, something is very wrong
        // with the test runner's environment — the assertion would catch it.
        let sources = get_desktop_sources(None);
        assert!(!sources.is_empty(), "expected at least one desktop source");
    }
}
