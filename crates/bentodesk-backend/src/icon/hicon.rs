//! Deterministic HICON rasterization for colour/alpha extraction.

use super::IconError;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const INVERT_MASK_CHUNK: &[u8; 4] = b"bdIM";
const INVERT_MASK_VERSION: u8 = 1;
const MAX_ICON_DIMENSION: u32 = 1_024;

/// Return the destination-invert mask carried by a legacy monochrome icon.
///
/// The mask lives in a private ancillary PNG chunk, so WIC and ordinary PNG
/// readers still see a valid image while the native renderer can preserve the
/// HICON's otherwise-unrepresentable `AND=1/XOR=1` pixels.
pub fn legacy_invert_mask(png: &[u8]) -> Option<(u32, u32, &[u8])> {
    if !png.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let mut offset = PNG_SIGNATURE.len();
    while offset.checked_add(12)? <= png.len() {
        let length = u32::from_be_bytes(png[offset..offset + 4].try_into().ok()?) as usize;
        let data_start = offset.checked_add(8)?;
        let data_end = data_start.checked_add(length)?;
        let chunk_end = data_end.checked_add(4)?;
        if chunk_end > png.len() {
            return None;
        }
        let kind = &png[offset + 4..offset + 8];
        if kind == INVERT_MASK_CHUNK {
            let data = &png[data_start..data_end];
            if data.len() < 9 || data[0] != INVERT_MASK_VERSION {
                return None;
            }
            let width = u32::from_be_bytes(data[1..5].try_into().ok()?);
            let height = u32::from_be_bytes(data[5..9].try_into().ok()?);
            let bits = width.checked_mul(height)?.checked_add(7)? / 8;
            if width == 0
                || height == 0
                || width > MAX_ICON_DIMENSION
                || height > MAX_ICON_DIMENSION
                || data.len() != 9usize.checked_add(bits as usize)?
            {
                return None;
            }
            let stored_crc = u32::from_be_bytes(png[data_end..chunk_end].try_into().ok()?);
            if png_crc32(&png[offset + 4..data_end]) != stored_crc {
                return None;
            }
            return Some((width, height, &data[9..]));
        }
        if kind == b"IEND" {
            break;
        }
        offset = chunk_end;
    }
    None
}

pub(super) fn png_has_visible_content(png: &[u8]) -> bool {
    !super::wic::decode_png_alpha_check(png)
        || legacy_invert_mask(png).is_some_and(|(_, _, bits)| bits.iter().any(|&byte| byte != 0))
}

pub(super) fn attach_legacy_invert_mask(
    png: &mut Vec<u8>,
    width: u32,
    height: u32,
    bits: &[u8],
) -> Result<(), &'static str> {
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or("legacy invert-mask dimensions overflow")?;
    if width == 0
        || height == 0
        || width > MAX_ICON_DIMENSION
        || height > MAX_ICON_DIMENSION
        || bits.len() != expected as usize
    {
        return Err("legacy invert-mask dimensions do not match its bitset");
    }
    if png.len() < PNG_SIGNATURE.len() + 12 || !png.starts_with(PNG_SIGNATURE) {
        return Err("legacy invert-mask target is not PNG data");
    }

    let mut iend = None;
    let mut offset = PNG_SIGNATURE.len();
    while offset.checked_add(12).is_some_and(|end| end <= png.len()) {
        let length = u32::from_be_bytes(
            png[offset..offset + 4]
                .try_into()
                .map_err(|_| "invalid PNG chunk length")?,
        ) as usize;
        let chunk_end = offset
            .checked_add(12)
            .and_then(|base| base.checked_add(length))
            .ok_or("PNG chunk length overflow")?;
        if chunk_end > png.len() {
            return Err("PNG chunk exceeds its payload");
        }
        if &png[offset + 4..offset + 8] == b"IEND" {
            iend = Some(offset);
            break;
        }
        offset = chunk_end;
    }
    let iend = iend.ok_or("PNG is missing IEND")?;
    let data_len = 9usize
        .checked_add(bits.len())
        .ok_or("legacy invert-mask chunk is too large")?;
    let mut chunk = Vec::with_capacity(data_len + 12);
    chunk.extend_from_slice(&(data_len as u32).to_be_bytes());
    chunk.extend_from_slice(INVERT_MASK_CHUNK);
    chunk.push(INVERT_MASK_VERSION);
    chunk.extend_from_slice(&width.to_be_bytes());
    chunk.extend_from_slice(&height.to_be_bytes());
    chunk.extend_from_slice(bits);
    let crc = png_crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    png.splice(iend..iend, chunk);
    Ok(())
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let bit = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & bit);
        }
    }
    !crc
}

pub(super) fn render_bgra(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    width: u32,
    height: u32,
    path: &str,
) -> Result<Vec<u8>, IconError> {
    use windows::Win32::Graphics::Gdi::{
        BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, GetDIBits,
        SelectObject, SetDIBits,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DI_NOMIRROR, DI_NORMAL, DrawIconEx};

    let width_i32 = i32::try_from(width).map_err(|_| IconError::Com {
        ctx: "hicon_to_png/DrawIconEx",
        message: "bitmap width exceeds i32".to_owned(),
    })?;
    let height_i32 = i32::try_from(height).map_err(|_| IconError::Com {
        ctx: "hicon_to_png/DrawIconEx",
        message: "bitmap height exceeds i32".to_owned(),
    })?;
    let pixel_len = super::extractor::checked_icon_pixel_len(width, height).map_err(|message| {
        IconError::Com {
            ctx: "hicon_to_png/DrawIconEx",
            message: message.to_owned(),
        }
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
    let mut bits = std::ptr::null_mut();
    // SAFETY: `bmi` describes a bounded top-down 32-bpp DIB and no shared
    // section is supplied. The bitmap is selected only into the private DC.
    let bitmap = unsafe { CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|error| IconError::Com {
            ctx: "hicon_to_png/CreateDIBSection",
            message: error.to_string(),
        })?;
    let delete_bitmap = || -> Result<(), IconError> {
        // SAFETY: `bitmap` is owned by this scope. If DC restoration failed and
        // it is still selected, DeleteObject reports failure without freeing it.
        if unsafe { DeleteObject(bitmap) }.as_bool() {
            Ok(())
        } else {
            Err(IconError::Extract {
                path: path.to_owned(),
                win32_error: last_win32_error(),
            })
        }
    };
    if bits.is_null() {
        if let Err(error) = delete_bitmap() {
            tracing::warn!(%error, "DeleteObject failed for null-buffer DIB section");
        }
        return Err(IconError::Com {
            ctx: "hicon_to_png/CreateDIBSection",
            message: "DIB section returned a null pixel buffer".to_owned(),
        });
    }

    let mut pixels = vec![0u8; pixel_len];
    // SAFETY: `bitmap` is not selected yet, `pixels` is exactly the bounded
    // 32-bpp DIB size, and `bmi` describes the same top-down dimensions.
    let initialized_rows = unsafe {
        SetDIBits(
            hdc,
            bitmap,
            0,
            height,
            pixels.as_ptr().cast(),
            &bmi,
            DIB_RGB_COLORS,
        )
    };
    if initialized_rows != height_i32 {
        let win32_error = last_win32_error();
        if let Err(error) = delete_bitmap() {
            tracing::warn!(%error, "DeleteObject failed after SetDIBits failure");
        }
        return Err(IconError::Extract {
            path: path.to_owned(),
            win32_error,
        });
    }
    // SAFETY: both handles are live and owned by this scope.
    let previous = unsafe { SelectObject(hdc, bitmap) };
    if previous.is_invalid() {
        let win32_error = last_win32_error();
        if let Err(error) = delete_bitmap() {
            tracing::warn!(%error, "DeleteObject failed after SelectObject failure");
        }
        return Err(IconError::Extract {
            path: path.to_owned(),
            win32_error,
        });
    }

    // DI_NORMAL applies the HICON's colour and mask semantics. For a 32-bpp
    // alpha icon GDI renders premultiplied BGRA, avoiding colour heuristics.
    let draw = unsafe {
        DrawIconEx(
            hdc,
            0,
            0,
            hicon,
            width_i32,
            height_i32,
            0,
            None,
            DI_NORMAL | DI_NOMIRROR,
        )
    };
    // SAFETY: restore the DC before deleting the selected bitmap.
    let restored = unsafe { SelectObject(hdc, previous) };
    let read_result = if draw.is_ok() && !restored.is_invalid() {
        // SAFETY: `bitmap` is no longer selected, and `pixels` is exactly the
        // bounded 32-bpp buffer requested by `bmi`.
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
        if rows == height_i32 {
            Ok(())
        } else {
            Err(IconError::Extract {
                path: path.to_owned(),
                win32_error: last_win32_error(),
            })
        }
    } else if draw.is_ok() {
        Err(IconError::Extract {
            path: path.to_owned(),
            win32_error: last_win32_error(),
        })
    } else {
        Ok(())
    };
    let delete_result = delete_bitmap();
    if let Err(error) = &delete_result {
        tracing::warn!(%error, "DeleteObject failed after HICON rendering");
    }
    draw.map_err(|error| IconError::Com {
        ctx: "hicon_to_png/DrawIconEx",
        message: error.to_string(),
    })?;
    read_result?;
    delete_result?;
    Ok(pixels)
}

fn last_win32_error() -> u32 {
    use windows::Win32::Foundation::GetLastError;
    // SAFETY: GetLastError is a thread-local accessor.
    unsafe { GetLastError() }.0
}
