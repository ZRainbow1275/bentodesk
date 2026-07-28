//! System-DPI query, soft-loaded via `GetProcAddress` (Mc-1a).
//!
//! ## Why duplicated from `bento-nano-platform::dpi`
//!
//! `bento-nano-backend` must stay self-contained — it does **not** depend on
//! `bento-nano-platform` (master plan layer 2.5 boundary). So the same
//! soft-load logic lives here independently rather than re-using the platform
//! module. This is a deliberate, documented duplication (spec §8 forbids adding
//! the platform dep just to share ~30 lines).
//!
//! ## Why soft-load at all
//!
//! `user32!GetDpiForSystem` only exists on Windows 10 1607+. A *static* import
//! (`use windows_sys::...HiDpi::GetDpiForSystem`) creates a PE import-table
//! entry the loader resolves at process start; on older Windows that aborts the
//! whole EXE with `STATUS_ENTRYPOINT_NOT_FOUND` before `main`. Resolving the
//! symbol with `GetProcAddress` keeps the import table clean and degrades to
//! the universal `GetDeviceCaps(LOGPIXELSX)` GDI path.
//!
//! Spec: §8 no new crate (rides the already-enabled `windows-sys`
//! `Win32_System_LibraryLoader` / `Win32_Graphics_Gdi` features); §10 the
//! resolved pointer is cached in a `OnceLock`; §11 zero panic-shaped ops — a
//! null resolve falls through to GDI, then to the 96-DPI baseline.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{FARPROC, HMODULE};
use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

/// `GetDeviceCaps` index for horizontal logical pixels-per-inch (`LOGPIXELSX`).
const LOGPIXELSX: i32 = 88;

/// Universal hard fallback when every API path is unavailable: 96 DPI = 100 %.
const DEFAULT_DPI: u32 = 96;

type FnGetDpiForSystem = unsafe extern "system" fn() -> u32;

/// GDI fallback present on every Windows version back to XP.
fn gdi_system_dpi() -> u32 {
    // SAFETY: `GetDC(null)` returns a screen DC (or null on failure); the DC is
    // passed to `GetDeviceCaps` and always released afterward via `ReleaseDC`.
    unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        if hdc.is_null() {
            return DEFAULT_DPI;
        }
        let dpi = GetDeviceCaps(hdc, LOGPIXELSX);
        ReleaseDC(std::ptr::null_mut(), hdc);
        if dpi > 0 { dpi as u32 } else { DEFAULT_DPI }
    }
}

/// Current system DPI (96 baseline). `user32!GetDpiForSystem()` resolved once
/// via `GetProcAddress`; on absence (Win10 <1607 / 8.1 / 7) falls back to the
/// universal `GetDeviceCaps(LOGPIXELSX)` path, then to [`DEFAULT_DPI`].
pub(crate) fn system_dpi() -> u32 {
    static FP: OnceLock<Option<FnGetDpiForSystem>> = OnceLock::new();
    let cached = *FP.get_or_init(|| {
        // SAFETY: `GetModuleHandleW` with a valid null-terminated wide string
        // queries the already-loaded `user32.dll`; returns null if absent.
        let user32: HMODULE = unsafe { GetModuleHandleW(windows_sys::w!("user32.dll")) };
        if user32.is_null() {
            return None;
        }
        // SAFETY: `user32` is live and the name is a null-terminated ASCII
        // string; `GetProcAddress` returns null on a missing export.
        let proc: FARPROC = unsafe { GetProcAddress(user32, windows_sys::s!("GetDpiForSystem")) };
        proc.map(|p| {
            // SAFETY: transmuting a non-null FARPROC to the documented
            // signature of `user32!GetDpiForSystem`.
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
        assert!(system_dpi() >= DEFAULT_DPI);
    }
}
