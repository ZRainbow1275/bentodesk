#![allow(clippy::doc_lazy_continuation)]
//! T-080 — native icon extraction via the Windows Shell API.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/extractor.rs`. Three
//! mechanical changes from the 1.x source:
//!
//! 1. `image` crate (forbidden by spec §8) → `super::wic::encode_png`
//!    + `super::wic::decode_png_alpha_check`. BGRA pixels read from the
//!    HICON via `GetDIBits` are normalized to straight RGBA, using the
//!    legacy AND mask when the colour bitmap has no alpha, then encoded
//!    to PNG via WIC.
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
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;

use super::IconError;
use super::wic;

pub use super::shortcut::resolve_lnk_target;

const MAX_INTERNET_SHORTCUT_BYTES: usize = 64 * 1024;
const MAX_ICON_DIMENSION: u32 = 1_024;

struct HiconPixels {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    invert_mask: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternetShortcutIconLocation {
    path: String,
    index: i32,
}

/// Compute a deterministic 16-hex-char hash for a file path. Used as
/// the icon cache key. `DefaultHasher` is SipHash-1-3 in the current
/// stdlib, sufficient for non-adversarial cache keying.
pub fn compute_icon_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Read the icon resource selected by a Windows Internet Shortcut (`.url`).
///
/// Explorer stores this in the `[InternetShortcut]` section as `IconFile` and
/// an optional `IconIndex`. `SHGetFileInfoW` only returns the registered URL
/// file-type icon on some systems, so parsing the resource is required to
/// match the icon that the desktop actually paints.
fn read_internet_shortcut_icon(path: &str) -> Option<InternetShortcutIconLocation> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take((MAX_INTERNET_SHORTCUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_INTERNET_SHORTCUT_BYTES {
        return None;
    }
    let text = decode_internet_shortcut_text(&bytes)?;
    parse_internet_shortcut_icon(&text)
}

fn decode_internet_shortcut_text(bytes: &[u8]) -> Option<String> {
    if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        if body.len() % 2 != 0 {
            return None;
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units).ok();
    }
    if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        if body.len() % 2 != 0 {
            return None;
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units).ok();
    }

    let body = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    std::str::from_utf8(body).ok().map(str::to_owned)
}

fn parse_internet_shortcut_icon(text: &str) -> Option<InternetShortcutIconLocation> {
    let mut in_internet_shortcut = false;
    let mut icon_path = None;
    let mut icon_index = 0;

    for raw_line in text.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.starts_with('[') && line.ends_with(']') {
            in_internet_shortcut = line[1..line.len() - 1]
                .trim()
                .eq_ignore_ascii_case("InternetShortcut");
            continue;
        }
        if !in_internet_shortcut || line.is_empty() || line.starts_with(';') {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim();
        if key.eq_ignore_ascii_case("IconFile") {
            let unquoted = if value.len() >= 2
                && ((value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\'')))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };
            if !unquoted.is_empty() {
                icon_path = Some(unquoted.to_owned());
            }
        } else if key.eq_ignore_ascii_case("IconIndex") {
            icon_index = value.parse::<i32>().unwrap_or(0);
        }
    }

    icon_path.map(|path| InternetShortcutIconLocation {
        path,
        index: icon_index,
    })
}

// ─── Extract → PNG (multi-strategy fallback per file type) ───────────

/// Extract a file's icon as PNG bytes.
///
/// Strategy:
/// 1. `.lnk` → read its stored icon/target locally, reject unsafe references,
///    then extract only from a guarded local resource or target.
/// 2. `.url` → parse its Explorer `IconFile` / `IconIndex`, then fall back to
///    `SHGetFileInfoW` when the resource is absent or invalid.
/// 3. `.exe` → `ExtractIconExW` first, then `SHGetFileInfoW`.
/// 4. Anything else → `SHGetFileInfoW` only.
///
/// Each strategy's output is checked for all-transparent pixels (via
/// WIC alpha-channel scan) to detect bogus / invisible icons; those
/// are treated as failures.
pub fn extract_icon_png(path: &str) -> Result<Vec<u8>, IconError> {
    if crate::path_may_access_network(std::path::Path::new(path)) {
        return Err(IconError::Io {
            path: std::path::PathBuf::from(path),
            message: "automatic icon extraction does not read network paths".to_owned(),
        });
    }
    let lower = path.to_ascii_lowercase();
    let is_lnk = lower.ends_with(".lnk");
    let is_url = lower.ends_with(".url");

    if is_lnk {
        let Some(metadata) = super::shortcut::read_link_metadata(path) else {
            return Err(IconError::Extract {
                path: path.to_owned(),
                win32_error: 0,
            });
        };
        if metadata.blocked_reference {
            tracing::warn!(%path, "shortcut network reference rejected before icon extraction");
        }
        let blocked_reference = metadata.blocked_reference;
        if let Some(location) = metadata.icon {
            match extract_icon_via_extract_icon_ex(&location.path, location.index) {
                Ok(png) if super::hicon::png_has_visible_content(&png) => return Ok(png),
                Ok(_) => tracing::debug!(
                    "ExtractIconExW returned transparent shortcut icon: {}",
                    location.path
                ),
                Err(error) => tracing::debug!(
                    "ExtractIconExW failed for shortcut icon {}: {}",
                    location.path,
                    error
                ),
            }
        }

        if let Some(target) = metadata.target {
            tracing::info!("Resolved .lnk target: {} -> {}", path, target);

            if target.to_ascii_lowercase().ends_with(".exe") {
                match extract_icon_via_extract_icon_ex(&target, 0) {
                    Ok(png) if super::hicon::png_has_visible_content(&png) => {
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
                Ok(png) if super::hicon::png_has_visible_content(&png) => {
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

        if blocked_reference {
            return Err(IconError::Io {
                path: std::path::PathBuf::from(path),
                message: "shortcut references a path rejected by automatic icon extraction"
                    .to_owned(),
            });
        }

        Err(IconError::AllTransparent {
            path: path.to_string(),
        })
    } else if is_url {
        if let Some(location) = read_internet_shortcut_icon(path)
            && !crate::path_may_access_network(std::path::Path::new(&location.path))
        {
            match extract_icon_via_extract_icon_ex(&location.path, location.index) {
                Ok(png) if super::hicon::png_has_visible_content(&png) => {
                    tracing::info!(
                        "ExtractIconExW matched Internet Shortcut icon: {} index={} -> {}",
                        location.path,
                        location.index,
                        path
                    );
                    return Ok(png);
                }
                Ok(_) => {
                    tracing::debug!(
                        "ExtractIconExW returned transparent .url resource icon: {} index={}",
                        location.path,
                        location.index
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "ExtractIconExW failed for .url resource {} index={}: {}",
                        location.path,
                        location.index,
                        e
                    );
                }
            }
        }
        extract_icon_via_shgetfileinfo(path)
    } else if lower.ends_with(".exe") {
        match extract_icon_via_extract_icon_ex(path, 0) {
            Ok(png) if super::hicon::png_has_visible_content(&png) => return Ok(png),
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
fn extract_icon_via_extract_icon_ex(path: &str, icon_index: i32) -> Result<Vec<u8>, IconError> {
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
    // caller-selected resource index, 1 large-icon slot, no small-icon slot.
    let count = unsafe {
        ExtractIconExW(
            PCWSTR(wide_path.as_ptr()),
            icon_index,
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
    if let Err(error) = unsafe { DestroyIcon(large_icon) } {
        tracing::warn!(
            %path,
            %error,
            "DestroyIcon failed after ExtractIconExW conversion"
        );
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
    if hicon.is_invalid() {
        return Err(IconError::Extract {
            path: path.to_string(),
            win32_error: last_win32_error(),
        });
    }
    let png = hicon_to_png(hicon, path);

    // SAFETY: We own the HICON populated by SHGetFileInfoW.
    if let Err(error) = unsafe { DestroyIcon(hicon) } {
        tracing::warn!(
            %path,
            %error,
            "DestroyIcon failed after SHGetFileInfoW conversion"
        );
    }

    match png {
        Ok(png) if !super::hicon::png_has_visible_content(&png) => Err(IconError::AllTransparent {
            path: path.to_string(),
        }),
        result => result,
    }
}

// ─── HICON → RGBA → PNG via WIC ──────────────────────────────────────

/// Convert an HICON to a PNG byte vector at the icon's native
/// resolution. Detects the actual HICON bitmap dimensions via
/// `GetObject(BITMAP)` so high-DPI icons (e.g. 48x48 on 150% scaling)
/// are captured in full rather than truncated to 32x32.
fn hicon_to_png(
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    path: &str,
) -> Result<Vec<u8>, IconError> {
    use windows::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP,
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

    // SAFETY: CreateCompatibleDC(None) = compatible with the screen DC.
    let hdc = unsafe { CreateCompatibleDC(None) };
    let pixel_result = (|| -> Result<HiconPixels, IconError> {
        if hdc.is_invalid() {
            return Err(IconError::Extract {
                path: path.to_owned(),
                win32_error: last_win32_error(),
            });
        }

        fn bitmap_dimensions(bitmap: HBITMAP, path: &str) -> Result<(u32, u32), IconError> {
            if bitmap.is_invalid() {
                return Err(IconError::Com {
                    ctx: "hicon_to_png/bitmap_dimensions",
                    message: format!("missing bitmap for {path}"),
                });
            }
            let mut value = BITMAP::default();
            // SAFETY: `bitmap` is a live HBITMAP returned by GetIconInfo and
            // `value` has exactly the size advertised to GetObjectW.
            let written = unsafe {
                GetObjectW(
                    bitmap,
                    std::mem::size_of::<BITMAP>() as i32,
                    Some((&mut value as *mut BITMAP).cast()),
                )
            };
            if written != std::mem::size_of::<BITMAP>() as i32 {
                return Err(IconError::Extract {
                    path: path.to_owned(),
                    win32_error: last_win32_error(),
                });
            }
            let width = u32::try_from(value.bmWidth).map_err(|_| IconError::Com {
                ctx: "hicon_to_png/bitmap_dimensions",
                message: "bitmap width is not positive".to_owned(),
            })?;
            let height = u32::try_from(value.bmHeight).map_err(|_| IconError::Com {
                ctx: "hicon_to_png/bitmap_dimensions",
                message: "bitmap height is not positive".to_owned(),
            })?;
            checked_icon_pixel_len(width, height).map_err(|message| IconError::Com {
                ctx: "hicon_to_png/bitmap_dimensions",
                message: message.to_owned(),
            })?;
            Ok((width, height))
        }

        fn read_bgra(
            hdc: windows::Win32::Graphics::Gdi::HDC,
            bitmap: HBITMAP,
            width: u32,
            height: u32,
            path: &str,
        ) -> Result<Vec<u8>, IconError> {
            let width_i32 = i32::try_from(width).map_err(|_| IconError::Com {
                ctx: "hicon_to_png/GetDIBits",
                message: "bitmap width exceeds i32".to_owned(),
            })?;
            let height_i32 = i32::try_from(height).map_err(|_| IconError::Com {
                ctx: "hicon_to_png/GetDIBits",
                message: "bitmap height exceeds i32".to_owned(),
            })?;
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width_i32,
                    biHeight: -height_i32,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels =
                vec![
                    0u8;
                    checked_icon_pixel_len(width, height).map_err(|message| IconError::Com {
                        ctx: "hicon_to_png/GetDIBits",
                        message: message.to_owned(),
                    },)?
                ];
            // SAFETY: `bitmap` and `hdc` are live GDI handles. `pixels` is
            // exactly width * height * 4 bytes and the requested DIB is 32bpp.
            let rows = unsafe {
                GetDIBits(
                    hdc,
                    bitmap,
                    0,
                    height,
                    Some(pixels.as_mut_ptr().cast()),
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            };
            if rows != height_i32 {
                return Err(IconError::Extract {
                    path: path.to_owned(),
                    win32_error: last_win32_error(),
                });
            }
            Ok(pixels)
        }

        if !icon_info.hbmColor.is_invalid() {
            let (width, height) = bitmap_dimensions(icon_info.hbmColor, path)?;
            tracing::info!("HICON actual bitmap size: {}x{}", width, height);
            let mut pixels = read_bgra(hdc, icon_info.hbmColor, width, height, path)?;
            let all_alpha_zero = pixels.chunks_exact(4).all(|pixel| pixel[3] == 0);
            let mask = if all_alpha_zero {
                let (mask_width, mask_height) = bitmap_dimensions(icon_info.hbmMask, path)?;
                if mask_width != width || mask_height < height {
                    return Err(IconError::Com {
                        ctx: "hicon_to_png/AND-mask dimensions",
                        message: "colour and AND-mask dimensions differ".to_owned(),
                    });
                }
                Some(read_bgra(hdc, icon_info.hbmMask, width, height, path)?)
            } else {
                pixels = super::hicon::render_bgra(hdc, hicon, width, height, path)?;
                None
            };
            normalize_color_bgra_to_rgba(&mut pixels, mask.as_deref(), !all_alpha_zero).map_err(
                |message| IconError::Com {
                    ctx: "hicon_to_png/normalise colour",
                    message: message.to_owned(),
                },
            )?;
            Ok(HiconPixels {
                rgba: pixels,
                width,
                height,
                invert_mask: None,
            })
        } else {
            let (width, mask_height) = bitmap_dimensions(icon_info.hbmMask, path)?;
            if mask_height % 2 != 0 {
                return Err(IconError::Com {
                    ctx: "hicon_to_png/monochrome dimensions",
                    message: "monochrome HICON mask height must contain AND and XOR planes"
                        .to_owned(),
                });
            }
            let height = mask_height / 2;
            let mask = read_bgra(hdc, icon_info.hbmMask, width, mask_height, path)?;
            let (pixels, invert_mask) =
                monochrome_mask_to_rgba(&mask, width, height).map_err(|message| {
                    IconError::Com {
                        ctx: "hicon_to_png/normalise monochrome",
                        message: message.to_owned(),
                    }
                })?;
            Ok(HiconPixels {
                rgba: pixels,
                width,
                height,
                invert_mask,
            })
        }
    })();

    // SAFETY: We own the DC and bitmap handles from GetIconInfo. The
    // HICON itself is owned by the caller (NOT freed here).
    unsafe {
        if !hdc.is_invalid() && !DeleteDC(hdc).as_bool() {
            tracing::warn!(
                %path,
                win32_error = last_win32_error(),
                "DeleteDC failed after HICON conversion"
            );
        }
        if !icon_info.hbmColor.is_invalid() && !DeleteObject(icon_info.hbmColor).as_bool() {
            tracing::warn!(
                %path,
                win32_error = last_win32_error(),
                "DeleteObject failed for HICON colour bitmap"
            );
        }
        if !icon_info.hbmMask.is_invalid() && !DeleteObject(icon_info.hbmMask).as_bool() {
            tracing::warn!(
                %path,
                win32_error = last_win32_error(),
                "DeleteObject failed for HICON mask bitmap"
            );
        }
    }

    let pixels = pixel_result?;
    let mut png = wic::encode_png(&pixels.rgba, pixels.width, pixels.height)?;
    if let Some(bits) = pixels.invert_mask {
        super::hicon::attach_legacy_invert_mask(&mut png, pixels.width, pixels.height, &bits)
            .map_err(|message| IconError::Com {
                ctx: "hicon_to_png/legacy invert mask",
                message: message.to_owned(),
            })?;
    }
    Ok(png)
}

pub(super) fn checked_icon_pixel_len(width: u32, height: u32) -> Result<usize, &'static str> {
    if width == 0 || height == 0 {
        return Err("icon dimensions must be positive");
    }
    if width > MAX_ICON_DIMENSION || height > MAX_ICON_DIMENSION {
        return Err("icon dimensions exceed the extraction budget");
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("icon pixel buffer length overflow")
}

fn mask_pixel_is_set(pixel: &[u8]) -> bool {
    pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0
}

fn unpremultiply(channel: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        0
    } else if alpha == u8::MAX {
        channel
    } else {
        ((u32::from(channel) * u32::from(u8::MAX) + u32::from(alpha) / 2) / u32::from(alpha))
            .min(u32::from(u8::MAX)) as u8
    }
}

fn normalize_color_bgra_to_rgba(
    pixels: &mut [u8],
    and_mask_bgra: Option<&[u8]>,
    premultiplied: bool,
) -> Result<(), &'static str> {
    if pixels.is_empty() || !pixels.len().is_multiple_of(4) {
        return Err("colour pixel buffer is not complete BGRA data");
    }
    let all_alpha_zero = pixels.chunks_exact(4).all(|pixel| pixel[3] == 0);
    if all_alpha_zero && and_mask_bgra.is_none_or(|mask| mask.len() != pixels.len()) {
        return Err("zero-alpha colour icon requires a matching AND mask");
    }
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let (blue, green, red) = if premultiplied {
            (
                unpremultiply(pixel[0], pixel[3]),
                unpremultiply(pixel[1], pixel[3]),
                unpremultiply(pixel[2], pixel[3]),
            )
        } else {
            (pixel[0], pixel[1], pixel[2])
        };
        pixel[0] = red;
        pixel[1] = green;
        pixel[2] = blue;
        if all_alpha_zero {
            let mask = and_mask_bgra.ok_or("missing AND mask")?;
            pixel[3] = if mask_pixel_is_set(&mask[index * 4..index * 4 + 4]) {
                0
            } else {
                u8::MAX
            };
        } else if pixel[3] == 0 {
            pixel[..3].fill(0);
        }
    }
    Ok(())
}

fn monochrome_mask_to_rgba(
    mask_bgra: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, Option<Vec<u8>>), &'static str> {
    let plane_len = checked_icon_pixel_len(width, height)?;
    if mask_bgra.len() != plane_len.checked_mul(2).ok_or("mask length overflow")? {
        return Err("monochrome mask does not contain equal AND and XOR planes");
    }
    let (and_plane, xor_plane) = mask_bgra.split_at(plane_len);
    let mut pixels = vec![0u8; plane_len];
    let mut invert_mask = vec![0u8; (width as usize * height as usize).div_ceil(8)];
    let mut has_invert = false;
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let and_set = mask_pixel_is_set(&and_plane[index * 4..index * 4 + 4]);
        let xor_set = mask_pixel_is_set(&xor_plane[index * 4..index * 4 + 4]);
        let value = if xor_set { u8::MAX } else { 0 };
        pixel[..3].fill(value);
        pixel[3] = if and_set { 0 } else { u8::MAX };
        if and_set && xor_set {
            invert_mask[index / 8] |= 1 << (index % 8);
            has_invert = true;
        }
    }
    Ok((pixels, has_invert.then_some(invert_mask)))
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
mod tests;
