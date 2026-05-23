//! D3D11 device factory (process-global singleton).
//!
//! Spec §1 RSS lever — the default runtime uses WARP for the tiny always-on
//! desktop surface. On the measured 0505 baseline this avoids loading both
//! hybrid-GPU user-mode driver stacks and drops Private Bytes from ~100 MB to
//! ~13 MB while preserving D2D + DComp composition. Hardware D3D remains
//! available through `BENTODESK_NANO_D3D_HARDWARE=1` for diagnostic runs.
//!
//! Spec §4: `OnceLock<ID3D11Device>` — one device per process.
//! Spec §11: every fallible call returns `Result`; `unsafe` blocks have SAFETY notes.

use std::sync::OnceLock;

use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP,
    D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_GPU_PREFERENCE,
    DXGI_GPU_PREFERENCE_MINIMUM_POWER, DXGI_GPU_PREFERENCE_UNSPECIFIED, IDXGIAdapter,
    IDXGIAdapter1, IDXGIFactory2, IDXGIFactory6,
};
use windows::core::Interface;

use crate::errors::{PlatformError, ok};

const D3D_HARDWARE_ENV: &str = "BENTODESK_NANO_D3D_HARDWARE";
const D3D_WARP_ENV: &str = "BENTODESK_NANO_D3D_WARP";

/// Process-wide D3D11 device + immediate context pair.
pub struct D3dDevice {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
}

// SAFETY: ID3D11Device is documented thread-safe for resource creation. The
//         immediate ID3D11DeviceContext is NOT thread-safe; nano accesses it
//         only from the single UI thread holding the message loop.
unsafe impl Send for D3dDevice {}
unsafe impl Sync for D3dDevice {}

static D3D: OnceLock<D3dDevice> = OnceLock::new();

/// Lazy D3D11 device accessor. Creates on first call.
pub fn device() -> Result<&'static D3dDevice, PlatformError> {
    if let Some(d) = D3D.get() {
        return Ok(d);
    }
    let created = create()?;
    let _ = D3D.set(created);
    D3D.get()
        .ok_or(PlatformError::Init("D3D OnceLock empty after set"))
}

/// Pick the minimum-power adapter (integrated GPU on dual-GPU laptops),
/// falling back to the unspecified-preference adapter. Returns the first
/// `IDXGIAdapter1` matching the preference, or an error if no adapter is
/// available (e.g. headless CI without WARP, or pre-Win10 1803 DXGI).
pub fn select_low_power_adapter() -> Result<IDXGIAdapter1, PlatformError> {
    // SAFETY: CreateDXGIFactory2 canonical entry; flags=0 = no debug.
    let factory: IDXGIFactory2 = ok("CreateDXGIFactory2", unsafe {
        CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
    })?;
    // Spec §15.1 — Interface::cast canonical for COM cross-cast.
    // IDXGIFactory6 is the version exposing EnumAdapterByGpuPreference. On
    // older systems (pre-Win10 1803) the cast fails and we surface the error
    // so the caller can fall back to the legacy code path.
    let factory6: IDXGIFactory6 = ok("IDXGIFactory2::cast<IDXGIFactory6>", factory.cast())?;

    if let Some(adapter) = enum_adapter(&factory6, DXGI_GPU_PREFERENCE_MINIMUM_POWER) {
        return Ok(adapter);
    }
    if let Some(adapter) = enum_adapter(&factory6, DXGI_GPU_PREFERENCE_UNSPECIFIED) {
        return Ok(adapter);
    }
    Err(PlatformError::Null {
        ctx: "EnumAdapterByGpuPreference no adapters",
    })
}

fn enum_adapter(factory6: &IDXGIFactory6, pref: DXGI_GPU_PREFERENCE) -> Option<IDXGIAdapter1> {
    // SAFETY: factory6 valid; adapter index 0 = first match per preference;
    //         the trait method picks IDXGIAdapter1 via T-monomorphisation.
    let res: windows::core::Result<IDXGIAdapter1> =
        unsafe { factory6.EnumAdapterByGpuPreference(0, pref) };
    res.ok()
}

fn create() -> Result<D3dDevice, PlatformError> {
    // RSS lever: WARP is the default for BentoDesk's small retained desktop
    // surface. Hardware remains an explicit diagnostic fallback.
    let (adapter_opt, driver_type): (Option<IDXGIAdapter1>, D3D_DRIVER_TYPE) = if use_warp_driver()
    {
        (None, D3D_DRIVER_TYPE_WARP)
    } else {
        match select_low_power_adapter() {
            // Per MSDN: when an explicit adapter is supplied, driver type
            // MUST be D3D_DRIVER_TYPE_UNKNOWN — otherwise CreateDevice fails
            // with E_INVALIDARG.
            Ok(a) => (Some(a), D3D_DRIVER_TYPE_UNKNOWN),
            Err(_) => (None, D3D_DRIVER_TYPE_HARDWARE),
        }
    };

    let levels = [D3D_FEATURE_LEVEL_11_0];
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let mut feature_level = D3D_FEATURE_LEVEL_11_0;

    // D3D11CreateDevice wants Param<IDXGIAdapter>; IDXGIAdapter1 derefs to
    // IDXGIAdapter so we project the option through Deref to satisfy the
    // bound. The temporary `&IDXGIAdapter` borrow lives for the call.
    let adapter_base: Option<&IDXGIAdapter> = adapter_opt.as_ref().map(|a| -> &IDXGIAdapter { a });

    // SAFETY: D3D11CreateDevice canonical; `levels` lives for the call;
    //         out-params are `Option<T>` per windows-rs convention.
    //         When `padapter` is Some, `driver_type` is UNKNOWN per MSDN.
    let result = unsafe {
        D3D11CreateDevice(
            adapter_base,
            driver_type,
            None,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut feature_level),
            Some(&mut context),
        )
    };
    ok("D3D11CreateDevice", result)?;

    let device = device.ok_or(PlatformError::Null {
        ctx: "D3D11CreateDevice device",
    })?;
    let context = context.ok_or(PlatformError::Null {
        ctx: "D3D11CreateDevice context",
    })?;
    Ok(D3dDevice { device, context })
}

fn use_warp_driver() -> bool {
    std::env::var_os(D3D_WARP_ENV).is_some() || std::env::var_os(D3D_HARDWARE_ENV).is_none()
}
