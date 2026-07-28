//! Win32 DPI APIs resolved dynamically via `GetProcAddress` (soft-load).
//!
//! ## Why this module exists (Mc-1a, the EXE-LOADS-EVERYWHERE fix)
//!
//! The per-monitor DPI APIs (`SetProcessDpiAwarenessContext`,
//! `SetProcessDpiAwareness`, `GetDpiForWindow`, `GetDpiForSystem`) only exist on
//! recent Windows:
//!
//! - `GetDpiForWindow` / `GetDpiForSystem` — Windows 10 **1607** and later.
//! - `SetProcessDpiAwarenessContext` — Windows 10 **1703** and later.
//! - `SetProcessDpiAwareness` (shcore) — Windows 8.1 and later.
//!
//! A *static* `use windows_sys::...HiDpi::{...}` import creates a PE
//! import-table entry that the loader resolves at process start. On Windows 10
//! pre-1607/1703, Windows 8.1/8/7, or stripped SKUs, `user32.dll` does not
//! export those symbols, so the loader aborts with
//! `STATUS_ENTRYPOINT_NOT_FOUND` (`0xC0000139`) **before `main` runs** — making
//! any in-code "fallback chain" unreachable. Resolving each symbol with
//! `GetProcAddress` keeps the import table free of these symbols, so the EXE
//! loads everywhere and the cascade degrades gracefully at runtime.
//!
//! ## Spec compliance
//!
//! - §8 — no new crate; every symbol below rides the already-enabled
//!   `windows-sys` feature surface (`Win32_System_LibraryLoader`,
//!   `Win32_Graphics_Gdi`, `Win32_UI_HiDpi`, `Win32_Foundation`).
//! - §10 — each resolved function pointer is cached in a `OnceLock`, so
//!   `GetProcAddress` runs at most once per symbol per process.
//! - §11 — zero panic-shaped operations; a null `GetProcAddress` result always
//!   falls through to the next tier, never `unwrap`/`expect`/`panic!`.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{FARPROC, HMODULE, HWND};
use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryA};

/// `GetDeviceCaps` index for horizontal logical pixels-per-inch (`LOGPIXELSX`).
const LOGPIXELSX: i32 = 88;

/// Universal hard fallback when every API path is unavailable: 96 DPI = 100 %.
const DEFAULT_DPI: u32 = 96;

/// `DPI_AWARENESS_CONTEXT` raw handle for `PER_MONITOR_AWARE_V2`.
///
/// `DPI_AWARENESS_CONTEXT` is an opaque `HANDLE`/`isize`; the documented
/// pseudo-handle values are negative sentinels passed by value.
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

/// `DPI_AWARENESS_CONTEXT` raw handle for `PER_MONITOR_AWARE`.
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE: isize = -3;

/// `PROCESS_DPI_AWARENESS` value for `PROCESS_PER_MONITOR_DPI_AWARE`.
const PROCESS_PER_MONITOR_DPI_AWARE: i32 = 2;

// ─── Soft-loaded function pointer signatures ──────────────────────────────

type FnSetProcessDpiAwarenessContext = unsafe extern "system" fn(isize) -> i32;
type FnSetProcessDpiAwareness = unsafe extern "system" fn(i32) -> i32;
type FnSetProcessDpiAware = unsafe extern "system" fn() -> i32;
type FnGetDpiForWindow = unsafe extern "system" fn(HWND) -> u32;
type FnGetDpiForSystem = unsafe extern "system" fn() -> u32;

// ─── Module handle resolution (cached) ────────────────────────────────────

/// Resolve `user32.dll`'s module handle (always loaded in a GUI process).
fn user32() -> HMODULE {
    static HANDLE: OnceLock<isize> = OnceLock::new();
    let raw = *HANDLE.get_or_init(|| {
        // SAFETY: `GetModuleHandleW` with a valid null-terminated wide string
        // queries an already-loaded module; it never dereferences caller
        // memory and returns null when the module is absent.
        let h = unsafe { GetModuleHandleW(windows_sys::w!("user32.dll")) };
        h as isize
    });
    raw as HMODULE
}

/// Resolve `shcore.dll`'s module handle, loading it on demand.
///
/// `shcore.dll` does not exist on Windows 7, so this can legitimately return
/// null — callers MUST treat null as "skip this tier".
fn shcore() -> HMODULE {
    static HANDLE: OnceLock<isize> = OnceLock::new();
    let raw = *HANDLE.get_or_init(|| {
        // SAFETY: `LoadLibraryA` with a valid null-terminated ASCII string
        // increments the module ref-count and returns null on failure (e.g.
        // Windows 7 where shcore.dll is absent). The handle is intentionally
        // never freed — it lives for the process lifetime.
        let h = unsafe { LoadLibraryA(windows_sys::s!("shcore.dll")) };
        h as isize
    });
    raw as HMODULE
}

/// Resolve a symbol from `module`, returning the raw `FARPROC`.
///
/// Returns `None` when `module` is null or the symbol is not exported.
fn resolve(module: HMODULE, name: &[u8]) -> FARPROC {
    if module.is_null() {
        return None;
    }
    debug_assert_eq!(name.last(), Some(&0), "symbol name must be null-terminated");
    // SAFETY: `module` is a live module handle and `name` is a null-terminated
    // ASCII byte string (asserted above in debug). `GetProcAddress` returns
    // null on a missing symbol rather than faulting.
    unsafe { GetProcAddress(module, name.as_ptr()) }
}

// ─── GetDeviceCaps universal fallback ─────────────────────────────────────

/// Read the system DPI via the GDI `GetDeviceCaps(LOGPIXELSX)` path, which is
/// available on every Windows version back to XP. Returns [`DEFAULT_DPI`] if
/// the device context cannot be acquired.
fn gdi_system_dpi() -> u32 {
    // SAFETY: `GetDC(null)` returns a DC for the entire screen (or null on
    // failure). We pass that DC to `GetDeviceCaps` and always `ReleaseDC` it.
    unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        if hdc.is_null() {
            return DEFAULT_DPI;
        }
        let dpi = GetDeviceCaps(hdc, LOGPIXELSX);
        // ReleaseDC for a screen DC obtained via GetDC(null) is required.
        ReleaseDC(std::ptr::null_mut(), hdc);
        if dpi > 0 { dpi as u32 } else { DEFAULT_DPI }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────

/// Declare the best available per-monitor DPI awareness for this process.
///
/// Call **once** at process start, before creating any window. Cascade,
/// returning silently after the first tier that succeeds:
///
/// 1. `user32!SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` (Win10 1703+)
/// 2. `user32!SetProcessDpiAwarenessContext(PER_MONITOR_AWARE)` (Win10 1607+)
/// 3. `shcore!SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE)` (Win8.1+)
/// 4. `user32!SetProcessDPIAware()` (Vista+, always present)
///
/// Never panics: an unresolved symbol simply advances to the next tier. (An
/// embedded application manifest may have already set awareness, in which case
/// these calls are harmless no-ops.)
pub fn set_process_dpi_awareness() {
    // Tier 1 + 2 — user32!SetProcessDpiAwarenessContext.
    if let Some(p) = resolve(user32(), b"SetProcessDpiAwarenessContext\0") {
        // SAFETY: transmuting a non-null FARPROC to the documented signature
        // of the resolved export. The call has no pointer preconditions; the
        // argument is a by-value pseudo-handle sentinel.
        let f: FnSetProcessDpiAwarenessContext = unsafe { std::mem::transmute(p) };
        // SAFETY: process-init best-effort; return value indicates success.
        if unsafe { f(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } != 0 {
            return;
        }
        // SAFETY: same export, fallback context value.
        if unsafe { f(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) } != 0 {
            return;
        }
    }

    // Tier 3 — shcore!SetProcessDpiAwareness (returns HRESULT; S_OK == 0).
    if let Some(p) = resolve(shcore(), b"SetProcessDpiAwareness\0") {
        // SAFETY: transmuting a non-null FARPROC to the documented signature.
        let f: FnSetProcessDpiAwareness = unsafe { std::mem::transmute(p) };
        // SAFETY: process-init best-effort; HRESULT S_OK (0) means success.
        if unsafe { f(PROCESS_PER_MONITOR_DPI_AWARE) } == 0 {
            return;
        }
    }

    // Tier 4 — user32!SetProcessDPIAware (Vista+, always present).
    if let Some(p) = resolve(user32(), b"SetProcessDPIAware\0") {
        // SAFETY: transmuting a non-null FARPROC to the documented signature.
        let f: FnSetProcessDpiAware = unsafe { std::mem::transmute(p) };
        // SAFETY: process-init best-effort; return value discarded — this is
        // the terminal fallback tier.
        let _ = unsafe { f() };
    }
}

/// Per-window DPI, soft-loaded with graceful degradation.
///
/// `user32!GetDpiForWindow(hwnd)` → `user32!GetDpiForSystem()` →
/// `GetDeviceCaps(LOGPIXELSX)` → hard fallback [`DEFAULT_DPI`]. Never panics.
// HWND is the Win32 ABI's opaque window handle (`*mut c_void`); we never
// dereference it in Rust — it is handed straight to `GetDpiForWindow`, which the
// OS resolves on its side and tolerates stale/invalid handles by returning 0.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn get_dpi_for_window(hwnd: HWND) -> u32 {
    static FP: OnceLock<Option<FnGetDpiForWindow>> = OnceLock::new();
    let cached = *FP.get_or_init(|| {
        resolve(user32(), b"GetDpiForWindow\0").map(|p| {
            // SAFETY: transmuting a non-null FARPROC to the documented signature.
            unsafe { std::mem::transmute::<FARPROC, FnGetDpiForWindow>(Some(p)) }
        })
    });
    if let Some(f) = cached {
        // SAFETY: `hwnd` is an opaque handle from the caller; `GetDpiForWindow`
        // tolerates stale/invalid HWNDs by returning 0 rather than faulting.
        let dpi = unsafe { f(hwnd) };
        if dpi != 0 {
            return dpi;
        }
    }
    get_dpi_for_system()
}

/// System DPI, soft-loaded with graceful degradation.
///
/// `user32!GetDpiForSystem()` → `GetDeviceCaps(LOGPIXELSX)` → hard fallback
/// [`DEFAULT_DPI`]. Never panics.
pub fn get_dpi_for_system() -> u32 {
    static FP: OnceLock<Option<FnGetDpiForSystem>> = OnceLock::new();
    let cached = *FP.get_or_init(|| {
        resolve(user32(), b"GetDpiForSystem\0").map(|p| {
            // SAFETY: transmuting a non-null FARPROC to the documented signature.
            unsafe { std::mem::transmute::<FARPROC, FnGetDpiForSystem>(Some(p)) }
        })
    });
    if let Some(f) = cached {
        // SAFETY: `GetDpiForSystem` reads process/system DPI state and has no
        // pointer preconditions.
        let dpi = unsafe { f() };
        if dpi != 0 {
            return dpi;
        }
    }
    gdi_system_dpi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_dpi_is_at_least_baseline() {
        // On this dev machine the real user32!GetDpiForSystem resolves, so the
        // value is the live system DPI (>= 96). On any path it must never be
        // below the 96 baseline.
        assert!(get_dpi_for_system() >= DEFAULT_DPI);
    }

    #[test]
    fn window_dpi_falls_back_to_system_for_null_hwnd() {
        // A null HWND makes GetDpiForWindow return 0, which must cascade to the
        // system DPI (>= 96), never below baseline and never panic.
        assert!(get_dpi_for_window(std::ptr::null_mut()) >= DEFAULT_DPI);
    }

    #[test]
    fn gdi_fallback_is_at_least_baseline() {
        // The universal GDI path is always available on this machine.
        assert!(gdi_system_dpi() >= DEFAULT_DPI);
    }

    #[test]
    fn set_awareness_never_panics() {
        // Idempotent + must not panic regardless of which tier resolves.
        set_process_dpi_awareness();
    }
}
