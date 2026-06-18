//! Self-rendered "real acrylic" backdrop — desktop capture + D2D gaussian blur.
//!
//! Frosted-backdrop spec (`receipts/FROSTED-BACKDROP-SPEC.md`, 2026-06-01):
//! the DComp-v3 `CreateGaussianBlurEffect` path is hardware-dead on this rig
//! (Win11 26100 + iGPU → `E_NOINTERFACE`; forensics in `dcomp.rs`), and the
//! deprecated `SetWindowCompositionAttribute(ACCENT_ENABLE_ACRYLICBLURBEHIND)`
//! is forbidden by spec §3.2. So we render the frost ourselves: BitBlt the
//! primary work area into a downsampled GDI DIB, hand the pixels to D2D as a
//! bitmap, run `CLSID_D2D1GaussianBlur` then `CLSID_D2D1Saturation` (the same
//! `CreateEffect`/`SetValue` machinery already exercised for `CLSID_D2D1Shadow`
//! under the `shadow` feature) to match Tauri's `backdrop-filter: blur(24px)
//! saturate(1.7)`, and bake the effect output into an offscreen `ID2D1Bitmap1`
//! that Pass 2 wraps in a bitmap brush behind every Main-overlay zone surface.
//!
//! §11 discipline: every COM / GDI failure degrades to `Err(PlatformError::…)`
//! via `ok(...)` — no `unwrap` / `expect` / `panic`. The renderer (Pass 2)
//! treats `Err` as "no frost, fall back to a single flat tint", never murk.
//!
//! §10 discipline: capture + blur is on-demand (Pass 2 caches the `Backdrop`
//! and only rebuilds it on `backdrop_dirty`); this module does no per-frame
//! work. The GDI DIB + DCs are freed deterministically by a scope guard before
//! the function returns, so nothing GDI-side outlives the call.
//!
//! Capture is self-excluding: the Main overlay is momentarily flagged
//! `WDA_EXCLUDEFROMCAPTURE` so the frost samples the wallpaper + other apps
//! WITHOUT the overlay's own pills bleeding back into the blur (a feedback
//! loop). A scope guard ALWAYS restores `WDA_NONE` — even on an early `?`
//! return — so BentoDesk stays visible to the user's own screen recorders in
//! the steady state.

use windows::Win32::Foundation::{HWND, TRUE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_BORDER_MODE_HARD, D2D1_COMPOSITE_MODE_SOURCE_OVER,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1GaussianBlur, CLSID_D2D1Saturation, D2D1_BITMAP_OPTIONS_NONE,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
    D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_INTERPOLATION_MODE_LINEAR,
    D2D1_PROPERTY_TYPE_ENUM, D2D1_PROPERTY_TYPE_FLOAT, D2D1_SATURATION_PROP_SATURATION,
    ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Image,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HALFTONE, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SRCCOPY,
    SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

use crate::errors::{PlatformError, ok};
use crate::monitor::{RectI32, primary_monitor};

/// A baked, blurred snapshot of the primary monitor work area. The `bitmap` is
/// an offscreen D2D bitmap (NOT an effect output) so Pass 2 can build an
/// `ID2D1BitmapBrush` from it directly. `src_w` / `src_h` are the DOWNSAMPLED
/// bitmap dimensions (device px / `downsample`); `region` is the screen rect
/// that was captured (so the brush transform can map captured-px → DIP).
pub struct Backdrop {
    /// Baked, blurred offscreen bitmap. Downsampled size = `src_w` × `src_h`.
    pub bitmap: ID2D1Bitmap1,
    /// Downsampled bitmap width in px (`region.width() / downsample`, ≥ 1).
    pub src_w: u32,
    /// Downsampled bitmap height in px (`region.height() / downsample`, ≥ 1).
    pub src_h: u32,
    /// Screen rect (work area) that was captured, in device px.
    pub region: RectI32,
}

// SAFETY: wraps a single COM ref (`ID2D1Bitmap1`), the same ownership shape as
//         `WindowSurface` / `WindowComp` in this crate. The bitmap is created
//         on the UI thread and only ever touched there; the `Send`/`Sync`
//         assertion exists solely so `Backdrop` can live inside the renderer's
//         (UI-thread-pinned) state alongside the other COM handles.
unsafe impl Send for Backdrop {}
unsafe impl Sync for Backdrop {}

/// Capture the primary-monitor work area, blur it, and bake the result into an
/// offscreen D2D bitmap.
///
/// `downsample` (≥ 1, forced) trades resolution for memory: at `2` the source
/// DIB and the baked bitmap are each `~(workarea / 4)` px. `stddev` is the
/// gaussian standard deviation in DOWNSAMPLED px and `saturation` is the
/// post-blur colour-matrix saturation factor.
///
/// Tauri spec: `backdrop-filter: blur(24px) saturate(1.7)` under
/// `surface-expanded rgba(12,12,18,0.82)`. At `downsample = 2` the half-res
/// stddev is `24 / 2 = 12.0` and saturation is `1.7` (a `D2D1Saturation`
/// effect chained after `D2D1GaussianBlur`).
///
/// Steps (frosted-backdrop spec "New platform module"):
///  1. `region = primary_monitor().rect_work`.
///  2. Momentary `WDA_EXCLUDEFROMCAPTURE` on `main_hwnd` + `DwmFlush()` so the
///     exclusion lands before the blit; an `AffinityGuard` restores `WDA_NONE`
///     on every exit path (including an early `?`).
///  3. `StretchBlt(HALFTONE)` the screen DC into a top-down 32bpp BGRA DIB,
///     downsampled — bounds memory (no full-res DIB ever exists).
///  4. `ctx.CreateBitmap` over the DIB pixels (`alphaMode = IGNORE`: BitBlt'd
///     desktop has no real alpha, so treating it as opaque avoids premultiply
///     darkening).
///  5. `CreateEffect(CLSID_D2D1GaussianBlur)` + `SetInput` +
///     `SetValue(STANDARD_DEVIATION, stddev)` + `SetValue(BORDER_MODE, HARD)`
///     (HARD clamps beyond-bitmap samples, killing the dark edge halo `SOFT`
///     produces), then chain `CreateEffect(CLSID_D2D1Saturation)` +
///     `SetValue(SATURATION, saturation)` to re-saturate the blurred output
///     (Tauri's `saturate(1.7)`).
///  6. Bake the effect output to an offscreen `ID2D1Bitmap1` TARGET via
///     save-target → `SetTarget` → `BeginDraw`/`Clear`/`DrawImage`/`EndDraw` →
///     restore the original target. (A brush needs an `ID2D1Bitmap`, not a raw
///     effect output, hence the bake.)
///
/// Degrades (returns `Err`) on any COM/GDI failure; never panics.
///
/// # Safety contract
///
/// `main_hwnd` must be a live window handle (it is passed straight to
/// `SetWindowDisplayAffinity`). Pass 2 only ever calls this with the Main
/// overlay's own HWND, which is alive for the whole render.
pub fn capture_primary_workarea_blurred(
    ctx: &ID2D1DeviceContext,
    main_hwnd: HWND,
    downsample: u32,
    stddev: f32,
    saturation: f32,
) -> Result<Backdrop, PlatformError> {
    // 1. Primary work-area rect (screen px). Degenerate work areas (the
    //    `FALLBACK_NO_MONITOR` sentinel) collapse to a 1×1 capture via
    //    `dib_dims`, which still produces a valid — if tiny — backdrop rather
    //    than a divide-by-zero.
    let region = primary_monitor().rect_work;
    let (dst_w, dst_h) = dib_dims(region, downsample);
    let src_w = region.width().max(1);
    let src_h = region.height().max(1);

    // 2. Momentary self-exclude. The guard restores WDA_NONE on Drop so an
    //    early `?` below still clears the affinity (keeps BentoDesk visible to
    //    the user's own screen recorders in steady state).
    let _affinity = AffinityGuard::engage(main_hwnd)?;
    // One compositor pass so the exclusion is applied before the blit. A flush
    // failure is non-fatal: worst case the overlay leaks one frame into the
    // frost, which the next refresh corrects — so log nothing, just blit.
    // SAFETY: DwmFlush takes no args and is always callable.
    let _ = unsafe { DwmFlush() };

    // 3. BitBlt the screen into a downsampled top-down 32bpp BGRA DIB. The
    //    `DibCapture` guard frees the DCs + DIB deterministically on Drop.
    let capture = DibCapture::take(region.left, region.top, src_w, src_h, dst_w, dst_h)?;

    // 4. Wrap the DIB pixels into a D2D bitmap. Alpha IGNORE: the BitBlt'd
    //    desktop carries no meaningful alpha channel, so treat it as opaque to
    //    avoid premultiply-darkening the frost.
    let bmp_props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_IGNORE,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    let size = D2D_SIZE_U {
        width: dst_w,
        height: dst_h,
    };
    // 4-byte BGRA, top-down DIB: stride is exactly width * 4.
    let pitch = dst_w.saturating_mul(4);
    // SAFETY: `capture.bits` is a valid pointer to `dst_w*dst_h*4` bytes owned
    //         by the live DIB section (kept alive by `capture` until after this
    //         call). D2D *copies* the pixels into a device resource, so the
    //         pointer need not outlive the call. `bmp_props` lives on the stack
    //         for the call duration.
    let source: ID2D1Bitmap1 = ok("D2D/CreateBitmap(captured DIB)", unsafe {
        ctx.CreateBitmap(size, Some(capture.bits), pitch, &bmp_props)
    })?;
    // The DIB + DCs are no longer needed once D2D has copied the pixels.
    drop(capture);

    // 5. Gaussian blur effect over the captured bitmap.
    // SAFETY: ctx valid; CLSID is the documented gaussian-blur effect (mirrors
    //         the `CLSID_D2D1Shadow` path in `d2d.rs::shadow_effect`).
    let effect = ok("D2D/CreateEffect(GaussianBlur)", unsafe {
        ctx.CreateEffect(&CLSID_D2D1GaussianBlur)
    })?;
    // `ID2D1Bitmap1` is in the `ID2D1Image` interface hierarchy, so it is a
    // valid `SetInput` source. `TRUE` = invalidate (re-evaluate the graph).
    // SAFETY: effect + source valid; SetInput borrows neither past the call.
    unsafe {
        effect.SetInput(0, &source, TRUE);
    }
    let stddev_bytes = stddev.to_ne_bytes();
    // SAFETY: effect valid; `SetValue` lives on the `ID2D1Properties` base
    //         (effect derefs to it). The prop index is the 0-based gaussian
    //         standard-deviation slot; the data slice is exactly one f32.
    ok("D2D/Effect.SetValue(STANDARD_DEVIATION)", unsafe {
        effect.SetValue(
            D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            &stddev_bytes,
        )
    })?;
    // P1.4 — clamp beyond-bitmap samples to the bitmap edge (HARD), preventing
    // the dark halo the default SOFT mode draws within ~`stddev` device-px of
    // the capture edge (counterintuitively HARD *prevents* the halo). Mirrors
    // CSS `backdrop-filter`'s effectively edge-clamped sampling.
    let border_mode_bytes = (D2D1_BORDER_MODE_HARD.0 as u32).to_ne_bytes();
    // SAFETY: effect valid; BORDER_MODE is an enum-typed gaussian prop; the
    //         data slice is exactly one u32 (the enum discriminant).
    ok("D2D/Effect.SetValue(BORDER_MODE)", unsafe {
        effect.SetValue(
            D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,
            D2D1_PROPERTY_TYPE_ENUM,
            &border_mode_bytes,
        )
    })?;

    // P1.1 — chain a saturation colour-matrix effect after the gaussian so the
    // frost reads with Tauri's `saturate(1.7)` vibrancy (a plain blur of the
    // desktop is noticeably greyer than the CSS reference). The saturation
    // effect takes the gaussian's output as its input; `blurred` is then
    // rebound from the saturation output for the bake below.
    // SAFETY: ctx valid; CLSID is the documented saturation effect (same
    //         CreateEffect machinery as the gaussian above).
    let sat = ok("D2D/CreateEffect(Saturation)", unsafe {
        ctx.CreateEffect(&CLSID_D2D1Saturation)
    })?;
    // SAFETY: effect valid; GetOutput returns the gaussian's output image,
    //         which is in the ID2D1Image hierarchy (a valid SetInput source).
    let blurred_in: ID2D1Image = ok("D2D/GaussianBlur.GetOutput", unsafe { effect.GetOutput() })?;
    // SAFETY: sat + blurred_in valid; SetInput borrows neither past the call.
    //         `TRUE` = invalidate (re-evaluate the graph).
    unsafe {
        sat.SetInput(0, &blurred_in, TRUE);
    }
    let sat_bytes = saturation.to_ne_bytes();
    // SAFETY: sat valid; SATURATION is a float-typed prop on the saturation
    //         effect's ID2D1Properties base; the data slice is exactly one f32.
    ok("D2D/Effect.SetValue(SATURATION)", unsafe {
        sat.SetValue(
            D2D1_SATURATION_PROP_SATURATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            &sat_bytes,
        )
    })?;
    // SAFETY: sat valid; GetOutput returns the saturated (final) output image.
    let blurred: ID2D1Image = ok("D2D/Saturation.GetOutput", unsafe { sat.GetOutput() })?;

    // 6. Bake the effect output into an offscreen TARGET bitmap. A brush needs
    //    a concrete `ID2D1Bitmap`, not a live effect graph, so we render once
    //    here and hand Pass 2 the static result.
    let target_props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            // Premultiplied so the baked bitmap composites correctly as a brush
            // (matches the swap-chain target format in `d2d.rs`).
            alphaMode: windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    // SAFETY: ctx valid; `None` source data + TARGET option allocates a blank
    //         renderable bitmap. `target_props` lives on the stack for the call.
    let offscreen: ID2D1Bitmap1 = ok("D2D/CreateBitmap(offscreen target)", unsafe {
        ctx.CreateBitmap(size, None, 0, &target_props)
    })?;

    // Save the current target so we can restore it after baking. `GetTarget`
    // returns `Ok` for a context with a bound target (the swap-chain backbuffer
    // in steady state). A `TargetGuard` restores it on every exit path.
    // SAFETY: ctx valid; GetTarget reads the currently bound target image.
    let prev_target: ID2D1Image = ok("D2D/GetTarget", unsafe { ctx.GetTarget() })?;
    let target_guard = TargetGuard {
        ctx,
        prev: prev_target,
    };

    // SAFETY: ctx + offscreen valid; offscreen is in the ID2D1Image hierarchy.
    unsafe {
        ctx.SetTarget(&offscreen);
    }
    // SAFETY: ctx valid; the target is the offscreen bitmap set above.
    //         `BeginDraw` on the RenderTarget base returns `()` (errors surface
    //         from the paired `EndDraw` below).
    unsafe {
        ctx.BeginDraw();
    }
    // SAFETY: ctx valid; Clear(None) clears to transparent.
    unsafe {
        ctx.Clear(None);
    }
    // SAFETY: ctx + blurred image valid; draw the blurred output at the origin.
    //         `DrawImage` returns `()` — D2D defers draw-error reporting to the
    //         paired `EndDraw` below, so we balance Begin/End and check there.
    unsafe {
        ctx.DrawImage(
            &blurred,
            None,
            None,
            D2D1_INTERPOLATION_MODE_LINEAR,
            D2D1_COMPOSITE_MODE_SOURCE_OVER,
        );
    }
    // SAFETY: ctx valid; EndDraw flushes the bake and surfaces any deferred
    //         draw error (incl. `D2DERR_RECREATE_TARGET`, which `ok()` maps to
    //         `DeviceLost`). Must balance the `BeginDraw` above on every path.
    let end = unsafe { ctx.EndDraw(None, None) };
    // Restore the original target before surfacing any bake error so the
    // renderer's next frame draws into the right target regardless.
    drop(target_guard);
    ok("D2D/EndDraw(bake)", end)?;

    Ok(Backdrop {
        bitmap: offscreen,
        src_w: dst_w,
        src_h: dst_h,
        region,
    })
}

/// Downsampled DIB dimensions for a capture `region` at `downsample`. Never
/// returns 0 on either axis (a zero-area DIB / D2D bitmap is an instant COM
/// failure), and `downsample` is floored at 1. A tiny or odd region rounds
/// DOWN to at least 1px per axis: e.g. a 1×1 region at any downsample → (1, 1).
///
/// Pure helper — the unit-test surface for the otherwise GPU-only capture path.
pub fn dib_dims(region: RectI32, downsample: u32) -> (u32, u32) {
    let ds = downsample.max(1);
    let w = (region.width().max(1) as u32 / ds).max(1);
    let h = (region.height().max(1) as u32 / ds).max(1);
    (w, h)
}

/// Brush scale (captured-bitmap px → pre-world DIP) for Pass 2's
/// `ID2D1BitmapBrush` transform.
///
/// Derivation (corrected): the renderer issues every draw call in logical DIP,
/// and the per-frame world transform `W = base_scale` maps DIP → device px. The
/// bitmap-brush transform `B` maps bitmap-px → DIP, so the composed bitmap →
/// device mapping is `base_scale · B`. To land one downsampled bitmap pixel on
/// exactly `downsample` device px (its true on-screen footprint), we need
/// `base_scale · B = downsample`, i.e. **`B = downsample / base_scale`**.
///
/// The earlier form returned a bare `downsample` on the false assumption that
/// `base_scale` cancels — it does NOT at non-100% DPI (the brush transform sits
/// in PRE-world DIP space, which is divided by `base_scale` relative to device
/// px), so the frost drifted out of alignment with the wallpaper on HiDPI rigs.
///
/// `downsample` is floored at 1 (matches `dib_dims`); `base_scale` is floored at
/// a tiny epsilon so a degenerate `0.0` can never divide-by-zero.
///
/// Pure helper.
pub fn backdrop_brush_scale(downsample: u32, base_scale: f32) -> f32 {
    downsample.max(1) as f32 / base_scale.max(1e-3)
}

// -----------------------------------------------------------------------------
// internals — scope guards (deterministic GDI/affinity/target cleanup)
// -----------------------------------------------------------------------------

/// Restores `WDA_NONE` on the Main overlay when dropped, so the momentary
/// self-exclusion is cleared on EVERY exit path (including an early `?`).
struct AffinityGuard {
    hwnd: HWND,
}

impl AffinityGuard {
    /// Engage `WDA_EXCLUDEFROMCAPTURE` and return a guard that clears it on Drop.
    fn engage(hwnd: HWND) -> Result<Self, PlatformError> {
        // SAFETY: hwnd is a live window per this fn's safety contract.
        ok("SetWindowDisplayAffinity(EXCLUDE)", unsafe {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
        })?;
        Ok(AffinityGuard { hwnd })
    }
}

impl Drop for AffinityGuard {
    fn drop(&mut self) {
        // SAFETY: hwnd was live when the guard was created; clearing affinity
        //         on a since-destroyed window is a harmless no-op error which
        //         we deliberately swallow (degrade-not-panic, and Drop cannot
        //         return). Restores BentoDesk's visibility to user captures.
        let _ = unsafe { SetWindowDisplayAffinity(self.hwnd, WDA_NONE) };
    }
}

/// A downsampled top-down 32bpp BGRA capture of a screen rect, backed by a GDI
/// DIB section. Owns the screen DC, the memory DC, the DIB, and the previously
/// selected GDI object; `Drop` frees them all deterministically (no leaks).
struct DibCapture {
    /// Pointer to the DIB's pixel bytes (`dst_w * dst_h * 4`). Valid for the
    /// lifetime of `self` (the DIB section stays selected until Drop).
    bits: *const core::ffi::c_void,
    screen_dc: HDC,
    mem_dc: HDC,
    dib: HBITMAP,
    /// The object `SelectObject` displaced when the DIB was selected into the
    /// memory DC; reselected before the DIB is deleted (GDI hygiene).
    prev_obj: HGDIOBJ,
}

impl DibCapture {
    /// BitBlt `(src_x, src_y, src_w, src_h)` from the desktop into a downsampled
    /// `dst_w × dst_h` top-down BGRA DIB. Frees all GDI handles on any error
    /// path before returning (no partial leak).
    fn take(
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        dst_w: u32,
        dst_h: u32,
    ) -> Result<Self, PlatformError> {
        // Screen DC. `None` HWND == the whole virtual desktop.
        // SAFETY: GetDC(None) returns the screen DC or a null HDC on failure.
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.0.is_null() {
            return Err(PlatformError::Win32 {
                ctx: "GetDC(screen)",
                code: 0,
            });
        }
        // Memory DC compatible with the screen DC.
        // SAFETY: screen_dc valid; CreateCompatibleDC returns null on failure.
        let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if mem_dc.0.is_null() {
            // SAFETY: release the screen DC we just acquired before bailing.
            unsafe {
                ReleaseDC(None, screen_dc);
            }
            return Err(PlatformError::Win32 {
                ctx: "CreateCompatibleDC",
                code: 0,
            });
        }

        // Top-down 32bpp BGRA DIB: negative biHeight => origin at top-left, so
        // row 0 is the top scanline (matches D2D's top-left-origin bitmaps).
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: dst_w as i32,
                biHeight: -(dst_h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default(); 1],
        };
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        // SAFETY: mem_dc valid; `bmi` lives on the stack for the call; `bits`
        //         receives the DIB pixel pointer (owned by the DIB section).
        let dib =
            match unsafe { CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) } {
                Ok(h) if !h.0.is_null() && !bits.is_null() => h,
                _ => {
                    // SAFETY: free the DCs acquired above before bailing.
                    unsafe {
                        let _ = DeleteDC(mem_dc);
                        ReleaseDC(None, screen_dc);
                    }
                    return Err(PlatformError::Win32 {
                        ctx: "CreateDIBSection",
                        code: 0,
                    });
                }
            };

        // Select the DIB into the memory DC so StretchBlt targets it.
        // SAFETY: mem_dc + dib valid; SelectObject returns the displaced object.
        let prev_obj = unsafe { SelectObject(mem_dc, dib) };

        // HALFTONE gives a quality downscale (Tauri's blur reads cleaner over a
        // smoothly-shrunk source than over a nearest-neighbour one).
        // SAFETY: mem_dc valid.
        unsafe {
            SetStretchBltMode(mem_dc, HALFTONE);
        }
        // SAFETY: dest = mem_dc (DIB), src = screen_dc; all rects in-range.
        let blit = unsafe {
            StretchBlt(
                mem_dc,
                0,
                0,
                dst_w as i32,
                dst_h as i32,
                screen_dc,
                src_x,
                src_y,
                src_w,
                src_h,
                SRCCOPY,
            )
        };
        if !blit.as_bool() {
            // SAFETY: reselect the displaced object, then free DIB + DCs.
            unsafe {
                SelectObject(mem_dc, prev_obj);
                let _ = DeleteObject(dib);
                let _ = DeleteDC(mem_dc);
                ReleaseDC(None, screen_dc);
            }
            return Err(PlatformError::Win32 {
                ctx: "StretchBlt",
                code: 0,
            });
        }

        Ok(DibCapture {
            bits: bits as *const core::ffi::c_void,
            screen_dc,
            mem_dc,
            dib,
            prev_obj,
        })
    }
}

impl Drop for DibCapture {
    fn drop(&mut self) {
        // Reselect the displaced object so the DIB is no longer bound, then
        // free the DIB, the memory DC, and release the screen DC. Order matters:
        // a selected DIB cannot be deleted.
        // SAFETY: all handles were valid when the guard was constructed and are
        //         freed exactly once here. Errors are swallowed — Drop cannot
        //         return and a double-free is impossible (we own each handle).
        unsafe {
            SelectObject(self.mem_dc, self.prev_obj);
            let _ = DeleteObject(self.dib);
            let _ = DeleteDC(self.mem_dc);
            ReleaseDC(None, self.screen_dc);
        }
    }
}

/// Restores a saved D2D render target on Drop, so the offscreen bake never
/// leaves the context pointed at the wrong target (even on an early error).
struct TargetGuard<'a> {
    ctx: &'a ID2D1DeviceContext,
    prev: ID2D1Image,
}

impl Drop for TargetGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: ctx valid for the borrow; `prev` was the bound target when the
        //         guard was created (a live ID2D1Image). Restoring it is the
        //         documented inverse of the SetTarget(offscreen) above.
        unsafe {
            self.ctx.SetTarget(&self.prev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RectI32 {
        RectI32 {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn dib_dims_typical_downsample_halves_each_axis() {
        // 2560×1400 work area at downsample 2 → 1280×700.
        assert_eq!(dib_dims(rect(0, 0, 2560, 1400), 2), (1280, 700));
        // downsample 3 → floor division (853×466).
        assert_eq!(dib_dims(rect(0, 0, 2560, 1400), 3), (853, 466));
        // Negative-origin work area (secondary-left monitor): only the EXTENT
        // matters, not the origin.
        assert_eq!(dib_dims(rect(-1920, -200, 0, 880), 2), (960, 540));
    }

    #[test]
    fn dib_dims_never_zero_on_tiny_or_odd_regions() {
        // 1×1 region at any downsample must stay (1, 1) — a zero-area D2D
        // bitmap is an instant CreateBitmap failure.
        assert_eq!(dib_dims(rect(0, 0, 1, 1), 1), (1, 1));
        assert_eq!(dib_dims(rect(0, 0, 1, 1), 8), (1, 1));
        // Odd extents floor toward 1 but never to 0.
        assert_eq!(dib_dims(rect(0, 0, 3, 5), 4), (1, 1));
        assert_eq!(dib_dims(rect(0, 0, 7, 9), 4), (1, 2));
        // A degenerate (zero/negative-extent) region — e.g. the
        // FALLBACK_NO_MONITOR sentinel's empty rect_work — clamps to 1×1
        // rather than dividing by zero or producing a 0-size bitmap.
        assert_eq!(dib_dims(rect(0, 0, 0, 0), 2), (1, 1));
        assert_eq!(dib_dims(rect(10, 10, 5, 5), 2), (1, 1));
    }

    #[test]
    fn dib_dims_downsample_zero_is_treated_as_one() {
        // downsample 0 would divide by zero; it is floored to 1.
        assert_eq!(dib_dims(rect(0, 0, 800, 600), 0), (800, 600));
    }

    #[test]
    fn backdrop_brush_scale_divides_by_base_scale() {
        // bitmap → device is `base_scale · B`; to land a downsampled px on
        // `downsample` device px the brush scale is `downsample / base_scale`.
        // At 100% DPI base_scale cancels to the bare downsample…
        assert_eq!(backdrop_brush_scale(2, 1.0), 2.0);
        // …but at 200% DPI the brush must shrink so the frost stays aligned.
        assert_eq!(backdrop_brush_scale(2, 2.0), 1.0);
        // 150% DPI, downsample 3 → 3 / 1.5 == 2.0.
        assert_eq!(backdrop_brush_scale(3, 1.5), 2.0);
        // downsample 0 floors to 1 (matches dib_dims).
        assert_eq!(backdrop_brush_scale(0, 1.0), 1.0);
        // base_scale 0.0 is floored at epsilon → no divide-by-zero, finite.
        assert!(backdrop_brush_scale(2, 0.0).is_finite());
    }
}
