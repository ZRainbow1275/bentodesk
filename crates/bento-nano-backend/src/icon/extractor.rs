#![allow(clippy::doc_lazy_continuation)]
//! T-080 — native icon extraction via the Windows Shell API.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/extractor.rs`. Three
//! mechanical changes from the 1.x source:
//!
//! 1. `image` crate (forbidden by spec §8) → [`super::wic::encode_png`]
//!    + [`super::wic::decode_png_alpha_check`]. Same hand-shake: BGRA
//!    pixels read from the HICON via `GetDIBits`, byte-swapped to RGBA,
//!    encoded to PNG via WIC.
//! 2. `BentoDeskError::IconError { source: windows::core::Error, .. }`
//!    → `IconError::Extract { path, win32_error }`. The
//!    `windows::core::Error` wraps `GetLastError()`; we surface that
//!    raw u32 instead of the wrapped error so the call-site doesn't
//!    need to depend on `windows::core::Error`'s public API.
//! 3. Hash function unchanged — `std::collections::hash_map::DefaultHasher`
//!    (SipHash-1-3 in current rustlib). `compute_icon_hash` returns 16
//!    hex chars matching the 1.x output byte-for-byte for any given
//!    path.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::IconError;
use super::wic;

/// Compute a deterministic 16-hex-char hash for a file path. Used as
/// the icon cache key. `DefaultHasher` is SipHash-1-3 in the current
/// stdlib, sufficient for non-adversarial cache keying.
pub fn compute_icon_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ─── .lnk → target resolver (COM IShellLinkW + IPersistFile) ─────────

/// Resolve a `.lnk` shortcut to its target path using COM `IShellLinkW`.
///
/// Returns `None` if the target cannot be resolved (broken shortcut,
/// non-`.lnk` path, COM init failure). This is the 1.x behaviour
/// preserved verbatim — the only changes are spec §11 unwrap/expect
/// removal and SAFETY comments on every unsafe.
pub fn resolve_lnk_target(lnk_path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{Interface, PCWSTR};

    // SAFETY: CoInitializeEx with COINIT_APARTMENTTHREADED is safe to call
    // from any thread. S_FALSE (already initialized) and RPC_E_CHANGED_MODE
    // are non-fatal; we only Uninitialize when our own Init succeeded.
    let com_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let need_uninit = com_init.is_ok();

    let result = (|| -> Option<String> {
        // SAFETY: CoCreateInstance creates a well-known Shell COM object.
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;

        // SAFETY: QueryInterface for IPersistFile — standard COM cast.
        let persist_file: IPersistFile = Interface::cast(&shell_link).ok()?;

        let wide_path: Vec<u16> = OsStr::new(lnk_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: Load the .lnk file in read-only mode. STGM(0) == STGM_READ.
        unsafe { persist_file.Load(PCWSTR(wide_path.as_ptr()), STGM(0)) }.ok()?;

        // SAFETY: GetPath fills the buffer with the resolved (long) path.
        // We use 0 flags = expanded long path (NOT SLGP_RAWPATH which
        // keeps env vars). Ignore the result — we'll detect "no target"
        // by checking the buffer.
        let mut target_buf = [0u16; 260];
        unsafe {
            shell_link
                .GetPath(&mut target_buf, std::ptr::null_mut(), 0u32)
                .ok()?;
        }

        let target_path = String::from_utf16_lossy(
            &target_buf[..target_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(target_buf.len())],
        );

        if !target_path.is_empty() && std::path::Path::new(&target_path).exists() {
            Some(target_path)
        } else {
            None
        }
    })();

    if need_uninit {
        // SAFETY: Matched CoUninitialize for our CoInitializeEx call.
        unsafe { CoUninitialize() };
    }

    result
}

// ─── Extract → PNG (multi-strategy fallback per file type) ───────────

/// Extract a file's icon as PNG bytes.
///
/// Strategy:
/// 1. `.lnk` → resolve target via `IShellLinkW`, then for `.exe`
///    targets try `ExtractIconExW` first; fall back to
///    `SHGetFileInfoW` on the target; last-resort `SHGetFileInfoW` on
///    the `.lnk` itself.
/// 2. `.exe` → `ExtractIconExW` first, then `SHGetFileInfoW`.
/// 3. Anything else → `SHGetFileInfoW` only.
///
/// Each strategy's output is checked for all-transparent pixels (via
/// WIC alpha-channel scan) to detect bogus / invisible icons; those
/// are treated as failures.
pub fn extract_icon_png(path: &str) -> Result<Vec<u8>, IconError> {
    let lower = path.to_ascii_lowercase();
    let is_lnk = lower.ends_with(".lnk");

    if is_lnk {
        if let Some(target) = resolve_lnk_target(path) {
            tracing::info!("Resolved .lnk target: {} -> {}", path, target);

            if target.to_ascii_lowercase().ends_with(".exe") {
                match extract_icon_via_extract_icon_ex(&target) {
                    Ok(png) if !wic::decode_png_alpha_check(&png) => {
                        tracing::info!("ExtractIconExW succeeded for target: {}", target);
                        return Ok(png);
                    }
                    Ok(_) => {
                        tracing::debug!("ExtractIconExW returned transparent icon: {}", target);
                    }
                    Err(e) => {
                        tracing::debug!("ExtractIconExW failed for target {}: {}", target, e);
                    }
                }
            }

            match extract_icon_via_shgetfileinfo(&target) {
                Ok(png) if !wic::decode_png_alpha_check(&png) => {
                    tracing::info!("SHGetFileInfoW succeeded for target: {}", target);
                    return Ok(png);
                }
                Ok(_) => {
                    tracing::debug!("SHGetFileInfoW returned transparent icon: {}", target);
                }
                Err(e) => {
                    tracing::debug!("SHGetFileInfoW failed for target {}: {}", target, e);
                }
            }
        }

        match extract_icon_via_shgetfileinfo(path) {
            Ok(png) if !wic::decode_png_alpha_check(&png) => Ok(png),
            Ok(_) => {
                tracing::debug!(
                    "SHGetFileInfoW returned transparent icon for .lnk: {}",
                    path
                );
                Err(IconError::AllTransparent {
                    path: path.to_string(),
                })
            }
            Err(e) => {
                tracing::debug!("SHGetFileInfoW failed for .lnk {}: {}", path, e);
                Err(e)
            }
        }
    } else if lower.ends_with(".exe") {
        match extract_icon_via_extract_icon_ex(path) {
            Ok(png) if !wic::decode_png_alpha_check(&png) => return Ok(png),
            Ok(_) => {
                tracing::debug!("ExtractIconExW returned transparent icon: {}", path);
            }
            Err(e) => {
                tracing::debug!("ExtractIconExW failed for {}: {}", path, e);
            }
        }
        extract_icon_via_shgetfileinfo(path)
    } else {
        extract_icon_via_shgetfileinfo(path)
    }
}

/// Strategy 1 — `.exe`/PE files: read embedded icon resource directly
/// via `ExtractIconExW`. Bypasses Shell shortcut resolution.
fn extract_icon_via_extract_icon_ex(path: &str) -> Result<Vec<u8>, IconError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
    use windows::core::PCWSTR;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut large_icon = HICON::default();

    // SAFETY: ExtractIconExW with a valid null-terminated wide path,
    // index 0 (first icon), 1 large-icon slot, no small-icon slot.
    let count = unsafe {
        ExtractIconExW(
            PCWSTR(wide_path.as_ptr()),
            0,
            Some(&mut large_icon),
            None,
            1,
        )
    };

    if count == 0 || large_icon.is_invalid() {
        return Err(IconError::Extract {
            path: path.to_string(),
            win32_error: last_win32_error(),
        });
    }

    let result = hicon_to_png(large_icon, path);

    // SAFETY: We own the HICON returned by ExtractIconExW.
    unsafe {
        let _ = DestroyIcon(large_icon);
    }

    result
}

/// Strategy 2 — `SHGetFileInfoW` for any file type. The Shell resolves
/// the associated icon based on type / registered handlers.
fn extract_icon_via_shgetfileinfo(path: &str) -> Result<Vec<u8>, IconError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
    use windows::core::PCWSTR;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut shfi = SHFILEINFOW::default();

    // SAFETY: SHGetFileInfoW with a valid null-terminated wide-string
    // path and a properly-sized SHFILEINFOW out-buffer. The `0` flags
    // arg is `FILE_FLAGS_AND_ATTRIBUTES(0)` = "real filesystem path"
    // (NOT `SHGFI_USEFILEATTRIBUTES`).
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };

    if result == 0 {
        return Err(IconError::Extract {
            path: path.to_string(),
            win32_error: last_win32_error(),
        });
    }

    let hicon = shfi.hIcon;
    let png = hicon_to_png(hicon, path);

    // SAFETY: We own the HICON populated by SHGetFileInfoW.
    unsafe {
        let _ = DestroyIcon(hicon);
    }

    png
}

// ─── HICON → RGBA → PNG via WIC ──────────────────────────────────────

/// Convert an HICON to a PNG byte vector at the icon's native
/// resolution. Detects the actual HICON bitmap dimensions via
/// `GetObject(BITMAP)` so high-DPI icons (e.g. 48x48 on 150% scaling)
/// are captured in full rather than truncated to 32x32.
fn hicon_to_png(
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    _path: &str,
) -> Result<Vec<u8>, IconError> {
    use windows::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    // SAFETY: GetIconInfo is called with a valid HICON; returns BOOL
    // mapped to `windows::core::Result<()>`. On failure we surface the
    // wrapped HRESULT as a `Com` error (call-site context preserved).
    let mut icon_info = ICONINFO::default();
    unsafe {
        GetIconInfo(hicon, &mut icon_info).map_err(|e| IconError::Com {
            ctx: "hicon_to_png/GetIconInfo",
            message: e.to_string(),
        })?;
    }

    // Detect actual icon bitmap size (Windows may return 48x48 on
    // 150% DPI scaling).
    // SAFETY: GetObjectW reads BITMAP from a valid HBITMAP.
    let icon_size: i32 = unsafe {
        let mut bm = BITMAP::default();
        let bytes_written = GetObjectW(
            icon_info.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut std::ffi::c_void),
        );
        if bytes_written > 0 && bm.bmWidth > 0 && bm.bmHeight > 0 {
            tracing::info!("HICON actual bitmap size: {}x{}", bm.bmWidth, bm.bmHeight);
            bm.bmWidth.max(bm.bmHeight)
        } else {
            32
        }
    };

    // SAFETY: CreateCompatibleDC(None) = compatible with the screen DC.
    let hdc = unsafe { CreateCompatibleDC(None) };

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: icon_size,
            biHeight: -icon_size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; (icon_size * icon_size * 4) as usize];

    // SAFETY: GetDIBits reads `icon_size` rows of pixel data from the
    // icon's colour bitmap into our pre-sized buffer.
    unsafe {
        GetDIBits(
            hdc,
            icon_info.hbmColor,
            0,
            icon_size as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
    }

    // BGRA (Windows bitmap order) → RGBA (PNG/web order).
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    // SAFETY: We own the DC and bitmap handles from GetIconInfo. The
    // HICON itself is owned by the caller (NOT freed here).
    unsafe {
        let _ = DeleteDC(hdc);
        let _ = DeleteObject(icon_info.hbmColor);
        let _ = DeleteObject(icon_info.hbmMask);
    }

    wic::encode_png(&pixels, icon_size as u32, icon_size as u32)
}

/// Wrap `GetLastError()` in plain `u32`. Typed `windows::core::Error`
/// would force callers to depend on `windows-core`; the raw u32 is
/// sufficient for diagnostic logging.
fn last_win32_error() -> u32 {
    use windows::Win32::Foundation::GetLastError;
    // SAFETY: GetLastError is a thread-local accessor; safe to call.
    unsafe { GetLastError() }.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_icon_hash_deterministic() {
        let hash1 = compute_icon_hash("C:\\Users\\test\\Desktop\\file.txt");
        let hash2 = compute_icon_hash("C:\\Users\\test\\Desktop\\file.txt");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn compute_icon_hash_different_paths_differ() {
        let hash1 = compute_icon_hash("C:\\file_a.txt");
        let hash2 = compute_icon_hash("C:\\file_b.txt");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn compute_icon_hash_is_hex_string() {
        let hash = compute_icon_hash("test_path");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn resolve_lnk_target_returns_none_for_non_lnk() {
        // Passing a plain file path to a function that internally
        // initialises COM and asks for IPersistFile::Load on a
        // non-.lnk should fail-soft and return None.
        let r = resolve_lnk_target("C:/does-not-exist.txt");
        assert!(r.is_none());
    }
}
