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
            PlatformError::Svg(msg) => write!(f, "Invalid SVG path: {msg}"),
            PlatformError::Storage(msg) => write!(f, "Invalid storage payload: {msg}"),
            PlatformError::StorageIo { ctx, kind } => {
                write!(f, "Storage I/O failure: {ctx} (kind = {kind:?})")
            }
        }
    }
}

impl std::error::Error for PlatformError {}

/// Convert a `windows::core::Result<T>` directly into `Result<T, PlatformError>`.
#[inline]
pub fn ok<T>(ctx: &'static str, r: windows::core::Result<T>) -> Result<T, PlatformError> {
    r.map_err(|e| PlatformError::Hresult {
        ctx,
        hr: e.code().0,
    })
}
