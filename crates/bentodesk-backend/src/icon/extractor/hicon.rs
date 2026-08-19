use super::*;

/// Convert an HICON to a PNG byte vector at the icon's native
/// resolution. Detects the actual HICON bitmap dimensions via
/// `GetObject(BITMAP)` so high-DPI icons (e.g. 48x48 on 150% scaling)
/// are captured in full rather than truncated to 32x32.
pub(super) fn hicon_to_png(
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
                pixels = super::super::hicon::render_bgra(hdc, hicon, width, height, path)?;
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
        super::super::hicon::attach_legacy_invert_mask(
            &mut png,
            pixels.width,
            pixels.height,
            &bits,
        )
        .map_err(|message| IconError::Com {
            ctx: "hicon_to_png/legacy invert mask",
            message: message.to_owned(),
        })?;
    }
    Ok(png)
}
