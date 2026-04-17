//! Native icon extraction via the Windows Shell API.
//!
//! Uses `SHGetFileInfoW` to retrieve the system icon for any file or folder,
//! then converts the `HICON` to a 32x32 PNG for WebView2 display.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::error::BentoDeskError;

/// Compute a deterministic hash for a file path (used as icon cache key).
pub fn compute_icon_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Resolve a `.lnk` shortcut to its target path using COM `IShellLinkW`.
///
/// Returns `None` if the target cannot be resolved (e.g. broken shortcut).
pub fn resolve_lnk_target(lnk_path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    // SAFETY: COM initialization — COINIT_APARTMENTTHREADED is safe for single-threaded use.
    // S_FALSE (already initialized) and RPC_E_CHANGED_MODE are non-fatal.
    let com_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let need_uninit = com_init.is_ok();

    let result = (|| -> Option<String> {
        // SAFETY: CoCreateInstance creates a well-known COM object (ShellLink).
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;

        // SAFETY: QueryInterface for IPersistFile — standard COM pattern.
        let persist_file: windows::Win32::System::Com::IPersistFile =
            windows::core::Interface::cast(&shell_link).ok()?;

        let wide_path: Vec<u16> = OsStr::new(lnk_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: IPersistFile::Load with a valid null-terminated wide path.
        // Load the .lnk file in read-only mode (STGM_READ = 0).
        unsafe {
            persist_file.Load(
                PCWSTR(wide_path.as_ptr()),
                windows::Win32::System::Com::STGM(0),
            )
        }
        .ok()?;

        // Get the target path (use default flags = 0 for expanded long path).
        let mut target_buf = [0u16; 260];
        unsafe {
            shell_link
                .GetPath(
                    &mut target_buf,
                    std::ptr::null_mut(),
                    0u32, // Default: expanded long path (NOT SLGP_RAWPATH which keeps env vars)
                )
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

/// Extract a file's icon as PNG bytes.
///
/// For `.lnk` shortcut files, the target is resolved first via `IShellLinkW`
/// and the icon is extracted from the TARGET executable. This uses a multi-strategy
/// approach:
///
/// 1. For `.lnk` files: resolve the target, then try `ExtractIconExW` on the
///    target `.exe` (bypasses Shell shortcut resolution issues), then fall back
///    to `SHGetFileInfoW` on the target, then on the `.lnk` itself.
/// 2. For `.exe` files: try `ExtractIconExW` first (direct resource extraction),
///    then fall back to `SHGetFileInfoW`.
/// 3. For all other files: use `SHGetFileInfoW`.
///
/// Each extracted icon is checked for all-transparent pixels (which indicates
/// a bogus/invisible icon) and treated as a failure if detected.
///
/// The caller does NOT need to manage any GDI handles -- they are cleaned up
/// before this function returns.
pub fn extract_icon_png(path: &str) -> Result<Vec<u8>, BentoDeskError> {
    let is_lnk = path.to_lowercase().ends_with(".lnk");

    if is_lnk {
        // For .lnk files: resolve target FIRST, then try ExtractIconExW on
        // the target .exe. SHGetFileInfoW on .lnk paths returns the generic
        // shortcut overlay icon on some Windows configurations.
        if let Some(target) = resolve_lnk_target(path) {
            tracing::info!("Resolved .lnk target: {} -> {}", path, target);

            // Strategy 1: ExtractIconExW on the resolved target (most reliable)
            if target.to_lowercase().ends_with(".exe") {
                match extract_icon_via_extract_icon_ex(&target) {
                    Ok(png) if !is_icon_all_transparent(&png) => {
                        tracing::info!("ExtractIconExW succeeded for target: {}", target);
                        return Ok(png);
                    }
                    Ok(_) => {
                        tracing::debug!("ExtractIconExW returned transparent icon for: {}", target);
                    }
                    Err(e) => {
                        tracing::debug!("ExtractIconExW failed for target {}: {}", target, e);
                    }
                }
            }

            // Strategy 2: SHGetFileInfoW on the resolved target
            match extract_icon_via_shgetfileinfo(&target) {
                Ok(png) if !is_icon_all_transparent(&png) => {
                    tracing::info!("SHGetFileInfoW succeeded for target: {}", target);
                    return Ok(png);
                }
                Ok(_) => {
                    tracing::debug!("SHGetFileInfoW returned transparent icon for: {}", target);
                }
                Err(e) => {
                    tracing::debug!("SHGetFileInfoW failed for target {}: {}", target, e);
                }
            }
        }

        // Strategy 3: SHGetFileInfoW on the .lnk itself (last resort)
        match extract_icon_via_shgetfileinfo(path) {
            Ok(png) if !is_icon_all_transparent(&png) => return Ok(png),
            Ok(_) => {
                tracing::debug!(
                    "SHGetFileInfoW returned transparent icon for .lnk: {}",
                    path
                );
            }
            Err(e) => {
                tracing::debug!("SHGetFileInfoW failed for .lnk {}: {}", path, e);
            }
        }

        Err(BentoDeskError::IconError {
            path: path.to_string(),
            source: windows::core::Error::from_win32(),
        })
    } else if path.to_lowercase().ends_with(".exe") {
        // For .exe files: try ExtractIconExW first, fall back to SHGetFileInfoW
        match extract_icon_via_extract_icon_ex(path) {
            Ok(png) if !is_icon_all_transparent(&png) => return Ok(png),
            Ok(_) => {
                tracing::debug!("ExtractIconExW returned transparent icon for: {}", path);
            }
            Err(e) => {
                tracing::debug!("ExtractIconExW failed for {}: {}", path, e);
            }
        }
        extract_icon_via_shgetfileinfo(path)
    } else {
        // For all other files: SHGetFileInfoW only
        extract_icon_via_shgetfileinfo(path)
    }
}

/// Check whether a PNG icon is effectively all-transparent (invisible).
///
/// Decodes the PNG and checks if all alpha channel values are zero. This catches
/// cases where icon extraction "succeeds" but produces an invisible image.
fn is_icon_all_transparent(png_bytes: &[u8]) -> bool {
    if let Ok(img) = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
        let rgba = img.to_rgba8();
        // Check if every pixel has alpha == 0
        rgba.pixels().all(|p| p.0[3] == 0)
    } else {
        // Can't decode — treat as non-transparent (don't reject valid icons)
        false
    }
}

/// Extract an icon from a `.exe` (or other PE file) using `ExtractIconExW`.
///
/// This directly reads the embedded icon resource from the executable, bypassing
/// all Shell shortcut resolution. More reliable than `SHGetFileInfoW` for getting
/// the actual application icon from an `.exe` file.
fn extract_icon_via_extract_icon_ex(path: &str) -> Result<Vec<u8>, BentoDeskError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut large_icon = HICON::default();

    // SAFETY: ExtractIconExW is a well-documented Shell API. We pass a valid
    // null-terminated wide path, request the first icon (index 0), and provide
    // a valid HICON buffer. We request 1 icon.
    let count = unsafe {
        ExtractIconExW(
            PCWSTR(wide_path.as_ptr()),
            0, // First icon resource
            Some(&mut large_icon),
            None, // Don't need small icon
            1,    // Extract 1 icon
        )
    };

    if count == 0 || large_icon.is_invalid() {
        return Err(BentoDeskError::IconError {
            path: path.to_string(),
            source: windows::core::Error::from_win32(),
        });
    }

    let result = hicon_to_png(large_icon, path);

    // SAFETY: We own the HICON from ExtractIconExW and must free it.
    unsafe {
        let _ = DestroyIcon(large_icon);
    }

    result
}

/// Extract an icon using `SHGetFileInfoW` (the Shell API).
///
/// Works for any file type — the Shell resolves the associated icon based on
/// file type, registered handlers, etc. For `.lnk` files, the Shell *may*
/// auto-resolve to the target icon, but this is unreliable on some configs.
fn extract_icon_via_shgetfileinfo(path: &str) -> Result<Vec<u8>, BentoDeskError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut shfi = SHFILEINFOW::default();

    // SAFETY: SHGetFileInfoW is a well-documented Shell API. We pass a valid
    // null-terminated wide-string path and a properly sized SHFILEINFOW buffer.
    // The second parameter is 0 because we are querying a real filesystem path
    // (not using SHGFI_USEFILEATTRIBUTES).
    let result = unsafe {
        SHGetFileInfoW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };

    if result == 0 {
        return Err(BentoDeskError::IconError {
            path: path.to_string(),
            source: windows::core::Error::from_win32(),
        });
    }

    let hicon = shfi.hIcon;
    let png = hicon_to_png(hicon, path);

    // SAFETY: We own the HICON from SHGetFileInfoW and must free it.
    unsafe {
        let _ = DestroyIcon(hicon);
    }

    png
}

/// Convert an HICON to a PNG byte vector at the icon's native resolution.
///
/// Detects the actual HICON bitmap dimensions via `GetObject(BITMAP)` so that
/// high-DPI icons (e.g. 48x48 on 150% scaling) are captured in full rather than
/// being truncated to 32x32. Reads the colour bitmap via `GetDIBits`, converts
/// from BGRA to RGBA, and encodes as PNG.
fn hicon_to_png(
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    _path: &str,
) -> Result<Vec<u8>, BentoDeskError> {
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    // SAFETY: GetIconInfo is called with a valid HICON.
    let mut icon_info = ICONINFO::default();
    unsafe {
        GetIconInfo(hicon, &mut icon_info).map_err(|e| BentoDeskError::ComError(e))?;
    }

    // Detect the actual icon bitmap size instead of assuming 32x32.
    // On high-DPI systems (e.g. 150%), Windows may return 48x48 or larger icons.
    // SAFETY: GetObjectW reads BITMAP struct from a valid HBITMAP.
    let icon_size: i32 = unsafe {
        let mut bm = BITMAP::default();
        let bytes_written = GetObjectW(
            icon_info.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut std::ffi::c_void),
        );
        if bytes_written > 0 && bm.bmWidth > 0 && bm.bmHeight > 0 {
            let actual = bm.bmWidth.max(bm.bmHeight);
            tracing::info!("HICON actual bitmap size: {}x{}", bm.bmWidth, bm.bmHeight);
            actual
        } else {
            32 // Fallback to standard size
        }
    };

    // Create a memory device context for reading pixel data.
    // SAFETY: CreateCompatibleDC(None) creates a DC compatible with the screen.
    let hdc = unsafe { CreateCompatibleDC(None) };

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: icon_size,
            biHeight: -icon_size, // negative = top-down bitmap
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB = 0
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; (icon_size * icon_size * 4) as usize];

    // SAFETY: GetDIBits reads pixel data from the icon's colour bitmap into our buffer.
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

    // Convert BGRA (Windows bitmap order) to RGBA (PNG/web order)
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2); // B <-> R
    }

    // Clean up GDI resources (NOT the HICON — caller owns that).
    // SAFETY: We own the DC and bitmap handles from GetIconInfo.
    unsafe {
        let _ = DeleteDC(hdc);
        let _ = DeleteObject(icon_info.hbmColor);
        let _ = DeleteObject(icon_info.hbmMask);
    }

    // Encode the raw RGBA pixels to PNG
    let img = image::RgbaImage::from_raw(icon_size as u32, icon_size as u32, pixels)
        .ok_or_else(|| BentoDeskError::ImageError("Failed to create image buffer".into()))?;

    let mut png_bytes: Vec<u8> = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        icon_size as u32,
        icon_size as u32,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| BentoDeskError::ImageError(format!("PNG encoding failed: {e}")))?;

    Ok(png_bytes)
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
        assert_eq!(hash.len(), 16); // 16 hex chars = 64-bit hash
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
