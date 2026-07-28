//! Platform-layer error type.
//!
//! Spec §11: zero `unwrap` / `expect` / `panic` in production code paths.
//! Every fallible Win32 / DXGI / D2D / DComp / DWrite call returns `Result`.
//! Spec §8 forbids `thiserror` — we hand-roll `Display` / `Error`.

use core::fmt;

/// Failure surface for every platform call. Hand-rolled enum; no `thiserror`,
/// no `anyhow` (spec §8 forbidden list).
#[derive(Debug)]
pub enum PlatformError {
    /// Plain Win32 call (no HRESULT) failed; `code` from `GetLastError()`.
    Win32 { ctx: &'static str, code: u32 },
    /// COM call returned a non-S_OK HRESULT.
    Hresult { ctx: &'static str, hr: i32 },
    /// `Option<ComInterface>` out param was unexpectedly None.
    Null { ctx: &'static str },
    /// Lazy-initialised global (`OnceLock`) lookup raced and lost; the inner
    /// message identifies which singleton (D3D / D2D / DWrite / DComp).
    Init(&'static str),
    /// Mc-2b — the GPU device was lost (TDR / GPU reset / driver upgrade): a
    /// `Present`/`ResizeBuffers` returned `DXGI_ERROR_DEVICE_REMOVED`/`_RESET`/
    /// `_HUNG`. The caller routes this into `recover_device_chain` rather than
    /// treating it as a fatal HRESULT.
    DeviceLost,
    /// SVG path parser rejected an unsupported command or malformed number.
    Svg(&'static str),
    /// Storage codec rejected a file (bad magic, version mismatch, truncated
    /// stream). Caller may still recover by starting fresh — Ruling 1 says
    /// magic / version mismatch returns `Storage`, not panic.
    Storage(&'static str),
    /// `std::io` failure during a storage call (file not found / permission
    /// denied / etc). Inner ErrorKind preserved as a stable code rather
    /// than an owned String, keeping the enum `Copy`-cheap to clone.
    StorageIo {
        ctx: &'static str,
        kind: std::io::ErrorKind,
    },
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatformError::Win32 { ctx, code } => {
                write!(f, "Win32 call failed: {ctx} (last error = 0x{code:08X})")
            }
            PlatformError::Hresult { ctx, hr } => {
                write!(f, "HRESULT failure: {ctx} (hr = 0x{hr:08X})")
            }
            PlatformError::Null { ctx } => {
                write!(f, "Null pointer / missing optional from {ctx}")
            }
            PlatformError::Init(msg) => {
                write!(f, "Initialization order violated: {msg}")
            }
            PlatformError::DeviceLost => {
                write!(f, "GPU device lost (DXGI device removed / reset / hung)")
            }
            PlatformError::Svg(msg) => write!(f, "Invalid SVG path: {msg}"),
            PlatformError::Storage(msg) => write!(f, "Invalid storage payload: {msg}"),
            PlatformError::StorageIo { ctx, kind } => {
                write!(f, "Storage I/O failure: {ctx} (kind = {kind:?})")
            }
        }
    }
}

impl std::error::Error for PlatformError {}

/// `DXGI_ERROR_DEVICE_REMOVED` — the GPU was physically removed, the driver was
/// upgraded, or a TDR removed the device.
const DXGI_ERROR_DEVICE_REMOVED: i32 = 0x887A_0005u32 as i32;
/// `DXGI_ERROR_DEVICE_RESET` — the device failed because of a badly-formed
/// command (effectively a device loss for our purposes).
const DXGI_ERROR_DEVICE_RESET: i32 = 0x887A_0007u32 as i32;
/// `DXGI_ERROR_DEVICE_HUNG` — the device hung (often the cause of a TDR).
const DXGI_ERROR_DEVICE_HUNG: i32 = 0x887A_0006u32 as i32;
/// `D2DERR_RECREATE_TARGET` — the canonical D2D device-loss surface returned by
/// `ID2D1DeviceContext::EndDraw`; the render target (and its device) must be
/// recreated.
const D2DERR_RECREATE_TARGET: i32 = 0x8899_000Cu32 as i32;

/// Convert a `windows::core::Result<T>` directly into `Result<T, PlatformError>`.
///
/// Mc-2b — device-loss centralisation: BEFORE the generic `Hresult` mapping we
/// classify the four device-lost HRESULTs (DXGI removed / reset / hung, plus the
/// D2D `D2DERR_RECREATE_TARGET` that `EndDraw` returns) into
/// [`PlatformError::DeviceLost`]. This routes *every* COM call funnelled through
/// `ok()` — `EndDraw`, DComp `Commit`, etc. — into `recover_device_chain` instead
/// of burning a fatal generic HRESULT. `present()` / `resize()` already classify
/// and return `DeviceLost` early, so they never reach here for those codes — this
/// is purely additive, no double-handling.
///
/// §10 hot-path discipline: the classification is a handful of integer compares
/// on the error path only — no allocation, no `format!`.
#[inline]
pub fn ok<T>(ctx: &'static str, r: windows::core::Result<T>) -> Result<T, PlatformError> {
    r.map_err(|e| {
        let hr = e.code().0;
        if hr == DXGI_ERROR_DEVICE_REMOVED
            || hr == DXGI_ERROR_DEVICE_RESET
            || hr == DXGI_ERROR_DEVICE_HUNG
            || hr == D2DERR_RECREATE_TARGET
        {
            PlatformError::DeviceLost
        } else {
            PlatformError::Hresult { ctx, hr }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `windows::core::Result<()>` carrying the given HRESULT as an error.
    fn err_with(code: i32) -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            code,
        )))
    }

    #[test]
    fn ok_classifies_device_lost_hresults() {
        // Mc-2b — the four device-lost codes (DXGI removed/reset/hung +
        // D2DERR_RECREATE_TARGET from EndDraw) must surface as DeviceLost so the
        // shell chokepoint can recreate the device chain.
        for code in [
            0x887A_0005u32 as i32, // DXGI_ERROR_DEVICE_REMOVED
            0x887A_0007u32 as i32, // DXGI_ERROR_DEVICE_RESET
            0x887A_0006u32 as i32, // DXGI_ERROR_DEVICE_HUNG
            0x8899_000Cu32 as i32, // D2DERR_RECREATE_TARGET (EndDraw)
        ] {
            match ok("t", err_with(code)) {
                Err(PlatformError::DeviceLost) => {}
                other => panic!("0x{code:08X} should map to DeviceLost, got {other:?}"),
            }
        }
    }

    #[test]
    fn ok_keeps_non_device_lost_as_hresult() {
        // E_FAIL is a genuine failure, not a device loss — it must still route
        // through the generic Hresult arm unchanged.
        match ok("ctx", err_with(0x8000_4005u32 as i32)) {
            Err(PlatformError::Hresult { ctx, hr }) => {
                assert_eq!(ctx, "ctx");
                assert_eq!(hr, 0x8000_4005u32 as i32);
            }
            other => panic!("E_FAIL should map to Hresult, got {other:?}"),
        }
    }
}
