//! Direct2D factory + per-window render target + brush helpers.
//!
//! Spec §4:
//!   - D2D factory single instance (D2D1_FACTORY_TYPE_SINGLE_THREADED).
//!   - Render target = `ID2D1DeviceContext` over DXGI swap chain backbuffer.
//!   - Antialias mode = D2D1_ANTIALIAS_MODE_PER_PRIMITIVE.
//!
//! Hot-path discipline (spec §10): no `String::new`, no `Vec::new`, no
//! `format!` per frame. All resources created once and reused.
//!
//! Mc-2b: the factory singleton is a rebuildable `RwLock<Option<Arc<D2dFactory>>>`
//! (was `OnceLock`) so `recover_device_chain` can swap in a factory built over
//! the recreated D3D device after a device loss. `factory()` clones the `Arc`
//! out of the guard — no lock held across COM calls.

use std::sync::{Arc, RwLock};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
#[cfg(feature = "shadow")]
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Shadow, D2D1_PROPERTY_TYPE_FLOAT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION,
    ID2D1Effect,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_NONE,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
    D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_INTERPOLATION_MODE_LINEAR,
    D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE, D2D1CreateFactory, ID2D1Bitmap,
    ID2D1Bitmap1, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
    ID2D1RoundedRectangleGeometry, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface, IDXGISwapChain1};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICFormatConverter,
    IWICImagingFactory, IWICStream, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
    WICDecodeMetadataCacheOnLoad,
};
use windows::core::Interface;

use crate::d3d::{D3dDevice, device as d3d_device};
use crate::errors::{PlatformError, ok};

const MAX_BITMAP_DIMENSION: u32 = 4_096;
const MAX_BITMAP_PIXELS: u64 = 3_840 * 2_160;

/// Process-wide D2D factory + device pair. Created once, never freed.
pub struct D2dFactory {
    pub factory: ID2D1Factory1,
    pub device: ID2D1Device,
}

// SAFETY: D2D factory + device are documented free-threaded for creation calls.
//         Mc-2b: handed out as `Arc<D2dFactory>`; `D2dFactory: Sync` (asserted
//         here) makes `Arc<D2dFactory>: Send + Sync`. Arc clones flow only on
//         the single UI thread.
unsafe impl Send for D2dFactory {}
unsafe impl Sync for D2dFactory {}

/// Rebuildable process-wide D2D factory holder. `recover_device_chain` rebuilds
/// the whole `D2dFactory` (factory + device) over the recreated D3D device.
static FACTORY: RwLock<Option<Arc<D2dFactory>>> = RwLock::new(None);

/// Lazy D2D factory accessor. Clones the cached `Arc` out of the read guard so
/// no lock is held across the COM calls in `create_factory`.
///
/// Mc-2b CRITICAL (design §A): the sibling `d3d::device()` is acquired (and its
/// lock RELEASED, holding only the `Arc`) BEFORE the D2D write lock is taken —
/// `create_factory` no longer reaches into `d3d::device()` while the D2D guard is
/// live. Holding a singleton's guard across a sibling `device()` call is a latent
/// re-entrancy / deadlock risk, so the d3d Arc is hoisted out first.
pub fn factory() -> Result<Arc<D2dFactory>, PlatformError> {
    if let Some(f) = FACTORY.read().ok().and_then(|g| g.clone()) {
        return Ok(f);
    }
    // Acquire the sibling D3D device FIRST — this takes and releases the D3D
    // lock, leaving us holding only the `Arc`. NO D2D lock is held here.
    let d3d = d3d_device()?;
    let mut w = FACTORY
        .write()
        .map_err(|_| PlatformError::Init("D2D factory RwLock poisoned"))?;
    if let Some(f) = w.as_ref() {
        return Ok(f.clone());
    }
    let created = Arc::new(create_factory(&d3d)?);
    *w = Some(created.clone());
    Ok(created)
}

/// Mc-2b — tear down the cached D2D factory and rebuild it over the (already
/// recreated) D3D device. Called by `recover_device_chain` AFTER `d3d::rebuild`.
/// Does not bump the shared device generation — the orchestrator does that once.
///
/// Mc-2b CRITICAL (design §A): same hoist as `factory()` — the d3d Arc is taken
/// before the D2D write lock so no D2D guard is held across the sibling
/// `d3d::device()` call.
pub fn rebuild() -> Result<Arc<D2dFactory>, PlatformError> {
    // Acquire the (already recreated) sibling D3D device FIRST; releases the D3D
    // lock before we take the D2D write lock below.
    let d3d = d3d_device()?;
    let mut w = FACTORY
        .write()
        .map_err(|_| PlatformError::Init("D2D factory RwLock poisoned"))?;
    *w = None;
    let created = Arc::new(create_factory(&d3d)?);
    *w = Some(created.clone());
    Ok(created)
}

/// Build a fresh `D2dFactory` over the supplied (already-acquired) D3D device.
/// Takes the `D3dDevice` by reference rather than calling `d3d::device()` itself
/// so the caller can hoist the sibling acquisition outside the D2D write lock
/// (Mc-2b re-entrancy rule, design §A).
fn create_factory(d3d: &D3dDevice) -> Result<D2dFactory, PlatformError> {
    let opts = D2D1_FACTORY_OPTIONS {
        debugLevel: Default::default(),
    };
    // SAFETY: D2D1CreateFactory is the canonical entry; T monomorphises GUID matching.
    let factory: ID2D1Factory1 = ok("D2D1CreateFactory", unsafe {
        D2D1CreateFactory::<ID2D1Factory1>(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&opts))
    })?;

    // Spec §15.1 — Interface::cast is the canonical COM cross-cast.
    let dxgi_dev: IDXGIDevice = ok("D3D::cast<IDXGIDevice>", d3d.device.cast())?;
    // SAFETY: factory + dxgi_dev valid; CreateDevice expects DXGI device.
    let d2d_device = ok("ID2D1Factory1::CreateDevice", unsafe {
        factory.CreateDevice(&dxgi_dev)
    })?;

    Ok(D2dFactory {
        factory,
        device: d2d_device,
    })
}

/// Per-window D2D context bound to a DXGI swap chain backbuffer.
pub struct WindowSurface {
    pub ctx: ID2D1DeviceContext,
    pub target: Option<ID2D1Bitmap1>,
}

// SAFETY: wrappers above an `Rc`-equivalent COM ref; access pinned to UI thread.
unsafe impl Send for WindowSurface {}
unsafe impl Sync for WindowSurface {}

impl WindowSurface {
    /// Create a fresh device-context + bitmap target around `swap` backbuffer 0.
    pub fn create(swap: &IDXGISwapChain1) -> Result<Self, PlatformError> {
        let f = factory()?;
        // SAFETY: factory.device valid.
        let ctx = ok("ID2D1Device::CreateDeviceContext", unsafe {
            f.device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
        })?;

        // SAFETY: backbuffer index 0 always valid for a created swap chain.
        let surface: IDXGISurface = ok("GetBuffer<IDXGISurface>(0)", unsafe { swap.GetBuffer(0) })?;

        let bmp_props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        // SAFETY: ctx + surface valid; bmp_props lives until call returns.
        let target: ID2D1Bitmap1 = ok("CreateBitmapFromDxgiSurface", unsafe {
            ctx.CreateBitmapFromDxgiSurface(&surface, Some(&bmp_props))
        })?;

        // SAFETY: ctx + target valid.
        unsafe {
            ctx.SetTarget(&target);
            ctx.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            // 2026-06-29 V21-T: even on the premultiplied-alpha DComp target,
            // current Tauri-reference crops keep closer title ink with explicit
            // ClearType than with Direct2D's alpha-target grayscale fallback.
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
        }

        Ok(WindowSurface {
            ctx,
            target: Some(target),
        })
    }

    /// Drop bitmap target so caller can recreate after swap-chain resize.
    pub fn release_target(&mut self) {
        // SAFETY: SetTarget(None) clears reference; target drop releases COM ref.
        unsafe {
            self.ctx.SetTarget(None);
        }
        self.target = None;
    }
}

/// Centred rounded-rectangle geometry — used by widget skeletons until layout
/// drives this from the tree.
pub fn card_geometry(
    factory: &ID2D1Factory1,
    win_w: f32,
    win_h: f32,
    card_w: f32,
    card_h: f32,
    radius: f32,
) -> Result<ID2D1RoundedRectangleGeometry, PlatformError> {
    let left = (win_w - card_w) * 0.5;
    let top = (win_h - card_h) * 0.5;
    let rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left,
            top,
            right: left + card_w,
            bottom: top + card_h,
        },
        radiusX: radius,
        radiusY: radius,
    };
    // SAFETY: factory valid; rr stack value lives for the call.
    ok("CreateRoundedRectangleGeometry", unsafe {
        factory.CreateRoundedRectangleGeometry(&rr)
    })
}

#[inline]
fn straight_alpha_color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    // `D2D1_COLOR_F` is always straight alpha, even when the render target is
    // premultiplied. Direct2D performs the destination conversion itself.
    // Premultiplying here would therefore darken every translucent brush a
    // second time (white@4% became white@0.16% in the final surface).
    D2D1_COLOR_F { r, g, b, a }
}

/// Solid colour brush using Direct2D's straight-alpha colour contract.
/// `CreateSolidColorBrush` lives on the base `ID2D1RenderTarget`; cast the
/// device context to call it.
pub fn solid_brush(
    ctx: &ID2D1DeviceContext,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> Result<ID2D1SolidColorBrush, PlatformError> {
    let color = straight_alpha_color(r, g, b, a);
    let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
    // SAFETY: rt valid; color lives for the call.
    ok("CreateSolidColorBrush", unsafe {
        rt.CreateSolidColorBrush(&color, None)
    })
}

/// Decode WIC-supported image bytes and create a Direct2D bitmap that can be
/// drawn on the supplied device context.
///
/// This is the selected-stack runtime bridge from real icon/image bytes to the
/// D2D compositor without adding the forbidden `image` crate. WIC auto-detects
/// the container from the stream and converts every accepted frame to 32bpp
/// premultiplied BGRA, matching the swap-chain target pixel format.
pub fn bitmap_from_image_bytes(
    ctx: &ID2D1DeviceContext,
    bytes: &[u8],
) -> Result<ID2D1Bitmap1, PlatformError> {
    if bytes.is_empty() {
        return Err(PlatformError::Storage(
            "bitmap_from_image_bytes: empty image payload",
        ));
    }

    // SAFETY: COM object creation through the registered WIC in-proc server.
    let factory: IWICImagingFactory = ok("WIC/CoCreateInstance", unsafe {
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
    })?;
    // SAFETY: CreateStream allocates an IWICStream owned by WIC.
    let stream: IWICStream = ok("WIC/CreateStream", unsafe { factory.CreateStream() })?;
    // SAFETY: InitializeFromMemory borrows `bytes`, which remains live until
    // every WIC object created from the stream has been consumed below.
    ok("WIC/InitializeFromMemory", unsafe {
        stream.InitializeFromMemory(bytes)
    })?;
    // SAFETY: Decoder creation auto-detects the container from the stream.
    let decoder = ok("WIC/CreateDecoderFromStream", unsafe {
        factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
    })?;
    // SAFETY: Use the first frame for static image widgets and icon payloads.
    let frame = ok("WIC/GetFrame(0)", unsafe { decoder.GetFrame(0) })?;
    let frame_src: IWICBitmapSource = ok("WIC/frame.cast<IWICBitmapSource>", frame.cast())?;
    let (mut width, mut height) = (0u32, 0u32);
    ok("WIC/GetSize", unsafe {
        frame_src.GetSize(&mut width, &mut height)
    })?;
    validate_bitmap_dimensions(width, height)?;
    // SAFETY: Converter is a standard WIC object. Target format is D2D's
    // preferred premultiplied BGRA layout for alpha-correct DrawBitmap.
    let converter: IWICFormatConverter = ok("WIC/CreateFormatConverter", unsafe {
        factory.CreateFormatConverter()
    })?;
    ok("WIC/FormatConverter.Initialize(PBGRA)", unsafe {
        converter.Initialize(
            &frame_src,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )
    })?;

    let bitmap_props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    // SAFETY: ctx is a live D2D device context and converter implements
    // IWICBitmapSource. D2D copies the bitmap data into the device resource.
    ok("D2D/CreateBitmapFromWicBitmap(image)", unsafe {
        ctx.CreateBitmapFromWicBitmap(&converter, Some(&bitmap_props))
    })
}

fn validate_bitmap_dimensions(width: u32, height: u32) -> Result<(), PlatformError> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_BITMAP_DIMENSION
        || height > MAX_BITMAP_DIMENSION
        || pixels > MAX_BITMAP_PIXELS
    {
        return Err(PlatformError::Storage(
            "bitmap dimensions exceed the decode budget",
        ));
    }
    Ok(())
}

/// Compatibility wrapper for the icon-cache path. The implementation is the
/// same WIC stream decoder used by file-backed image widgets.
pub fn bitmap_from_png_bytes(
    ctx: &ID2D1DeviceContext,
    bytes: &[u8],
) -> Result<ID2D1Bitmap1, PlatformError> {
    bitmap_from_image_bytes(ctx, bytes)
}

/// Draw a decoded icon bitmap into a D2D rectangle using linear filtering.
pub fn draw_bitmap(
    ctx: &ID2D1DeviceContext,
    bitmap: &ID2D1Bitmap1,
    rect: D2D_RECT_F,
    opacity: f32,
) -> Result<(), PlatformError> {
    let base: ID2D1Bitmap = ok("D2D/icon bitmap cast", bitmap.cast())?;
    // SAFETY: bitmap + destination rect are valid for the call duration.
    unsafe {
        ctx.DrawBitmap(
            &base,
            Some(&rect),
            opacity,
            D2D1_INTERPOLATION_MODE_LINEAR,
            None,
            None,
        );
    }
    Ok(())
}

/// Optional D2D shadow effect (gated by `shadow` feature).
#[cfg(feature = "shadow")]
pub fn shadow_effect(ctx: &ID2D1DeviceContext, blur: f32) -> Result<ID2D1Effect, PlatformError> {
    // SAFETY: ctx valid; CLSID known D2D1Shadow.
    let effect: ID2D1Effect = ok("CreateEffect(D2D1Shadow)", unsafe {
        ctx.CreateEffect(&CLSID_D2D1Shadow)
    })?;
    let value: f32 = blur;
    // SAFETY: effect valid; SetValue with f32 (4 bytes).
    ok("ID2D1Effect::SetValue(blur)", unsafe {
        effect.SetValue(
            D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            std::slice::from_raw_parts(
                (&value as *const f32) as *const u8,
                std::mem::size_of::<f32>(),
            ),
        )
    })?;
    Ok(effect)
}

#[cfg(test)]
mod tests {
    use super::{straight_alpha_color, validate_bitmap_dimensions};

    #[test]
    fn d2d_brush_color_keeps_rgb_unpremultiplied() {
        let color = straight_alpha_color(0.25, 0.5, 0.75, 0.2);

        assert!((color.r - 0.25).abs() < f32::EPSILON);
        assert!((color.g - 0.5).abs() < f32::EPSILON);
        assert!((color.b - 0.75).abs() < f32::EPSILON);
        assert!((color.a - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn bitmap_dimensions_are_bounded_before_wic_decode() {
        assert!(validate_bitmap_dimensions(3_840, 2_160).is_ok());
        assert!(validate_bitmap_dimensions(0, 1).is_err());
        assert!(validate_bitmap_dimensions(4_097, 1).is_err());
        assert!(validate_bitmap_dimensions(4_096, 4_096).is_err());
        assert!(validate_bitmap_dimensions(4_096, 4_097).is_err());
    }
}
