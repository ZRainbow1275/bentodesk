//! D3D11 device factory (process-global singleton).
//!
//! Spec §1 RSS lever — the default runtime uses WARP for the tiny always-on
//! desktop surface. On the measured 0505 baseline this avoids loading both
//! hybrid-GPU user-mode driver stacks and drops Private Bytes from ~100 MB to
//! ~13 MB while preserving D2D + DComp composition. Hardware D3D remains
//! available through `BENTODESK_D3D_HARDWARE=1` for diagnostic runs.
//!
//! Spec §4: D3D device singleton — Mc-2b made it a rebuildable
//! `RwLock<Option<Arc<D3dDevice>>>` holder (was `OnceLock`) so a lost device
//! (TDR / GPU reset / driver upgrade) can be recreated in place without a
//! restart. `device()` clones the `Arc` out of the guard; `rebuild()` swaps a
//! fresh device in. The std `RwLock` + `Arc` keep this §8-clean (no new crate).
//! Spec §11: every fallible call returns `Result`; `unsafe` blocks have SAFETY notes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP,
    D3D_FEATURE_LEVEL_9_1, D3D_FEATURE_LEVEL_9_2, D3D_FEATURE_LEVEL_9_3, D3D_FEATURE_LEVEL_10_0,
    D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_GPU_PREFERENCE,
    DXGI_GPU_PREFERENCE_MINIMUM_POWER, DXGI_GPU_PREFERENCE_UNSPECIFIED, IDXGIAdapter,
    IDXGIAdapter1, IDXGIDevice3, IDXGIFactory2, IDXGIFactory6,
};
use windows::core::Interface;

use crate::errors::{PlatformError, ok};

const D3D_HARDWARE_ENV: &str = "BENTODESK_D3D_HARDWARE";
const D3D_WARP_ENV: &str = "BENTODESK_D3D_WARP";

/// Process-wide D3D11 device + immediate context pair.
pub struct D3dDevice {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
}

// SAFETY: ID3D11Device is documented thread-safe for resource creation. The
//         immediate ID3D11DeviceContext is NOT thread-safe; native accesses it
//         only from the single UI thread holding the message loop. Mc-2b: the
//         device is now handed out as `Arc<D3dDevice>`; because `D3dDevice: Sync`
//         (asserted above), `Arc<D3dDevice>` is itself `Send + Sync`, so the
//         holder change is sound. In practice every `Arc` clone flows on that
//         single UI thread only.
unsafe impl Send for D3dDevice {}
unsafe impl Sync for D3dDevice {}

/// Rebuildable process-wide D3D device holder. `None` until first `device()`;
/// `recover_device_chain` / `rebuild` swap a fresh `Arc` in after a device loss.
static D3D: RwLock<Option<Arc<D3dDevice>>> = RwLock::new(None);

/// Shared device-generation counter (d3d / d2d / dcomp all observe this one
/// value). Incremented exactly once per completed `recover_device_chain`, so its
/// value equals the number of full device-chain recoveries the process has done.
/// The renderer compares its cached generation against this to self-heal on the
/// next paint. Lives here (rather than a separate module) so the holder + the
/// counter sit together; re-exported from `lib.rs`.
static DEVICE_GEN: AtomicU64 = AtomicU64::new(0);

/// Lazy D3D11 device accessor. Creates on first call, then clones the cached
/// `Arc` out of the read guard so no lock is held across COM calls.
pub fn device() -> Result<Arc<D3dDevice>, PlatformError> {
    if let Some(d) = D3D.read().ok().and_then(|g| g.clone()) {
        return Ok(d);
    }
    let mut w = D3D
        .write()
        .map_err(|_| PlatformError::Init("D3D RwLock poisoned"))?;
    if let Some(d) = w.as_ref() {
        return Ok(d.clone());
    }
    let created = Arc::new(create()?);
    *w = Some(created.clone());
    Ok(created)
}

/// Tear down the cached D3D device and create a fresh one. Used by
/// `recover_device_chain` after a device-lost HRESULT. Does NOT bump
/// `DEVICE_GEN` — only the orchestrator does, once, after the whole chain is
/// rebuilt.
pub fn rebuild() -> Result<Arc<D3dDevice>, PlatformError> {
    let mut w = D3D
        .write()
        .map_err(|_| PlatformError::Init("D3D RwLock poisoned"))?;
    *w = None;
    let created = Arc::new(create()?);
    *w = Some(created.clone());
    Ok(created)
}

/// Current device generation — the number of completed device-chain recoveries.
/// `Acquire` pairs with the `Release` bump in `recover_device_chain` so a reader
/// that sees the new generation also sees the rebuilt devices.
pub fn trim() -> Result<(), PlatformError> {
    let d3d = device()?;
    if let Ok(dxgi) = d3d.device.cast::<IDXGIDevice3>() {
        // SAFETY: IDXGIDevice3 comes from this process's live D3D device. Trim
        // is an advisory idle-memory notification and has no return value.
        unsafe { dxgi.Trim() };
    }
    Ok(())
}

#[inline]
pub fn device_generation() -> u64 {
    DEVICE_GEN.load(Ordering::Acquire)
}

/// Recreate the entire GPU device chain after a device-lost event, in dependency
/// order: D3D first (d2d + dcomp both consume its `IDXGIDevice`), then D2D, then
/// DComp. Each `rebuild()` takes and releases only its own lock, so there is no
/// cross-module lock held during the sibling `d3d::device()` calls the d2d/dcomp
/// create paths make. The shared generation is bumped exactly once, at the end,
/// with `Release` ordering so any thread that observes the new generation also
/// observes the fully-rebuilt chain.
pub fn recover_device_chain() -> Result<(), PlatformError> {
    crate::d3d::rebuild()?;
    crate::d2d::rebuild()?;
    crate::dcomp::rebuild()?;
    DEVICE_GEN.fetch_add(1, Ordering::Release);
    Ok(())
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

/// Full descending feature-level negotiation set. D3D11CreateDevice selects
/// the highest level the device supports, so FL<11 GPUs/WARP and very old
/// systems can still create instead of being rejected by a single 11_0 entry.
const FEATURE_LEVELS_ALL: [windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL; 7] = [
    D3D_FEATURE_LEVEL_11_1,
    D3D_FEATURE_LEVEL_11_0,
    D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_10_0,
    D3D_FEATURE_LEVEL_9_3,
    D3D_FEATURE_LEVEL_9_2,
    D3D_FEATURE_LEVEL_9_1,
];

/// Same list minus the leading 11_1 entry. Used as the Win7-RTM/SP1-without-
/// Platform-Update fallback: those systems lack the D3D11.1 runtime, and
/// passing `D3D_FEATURE_LEVEL_11_1` makes D3D11CreateDevice fail the ENTIRE
/// call with E_INVALIDARG rather than skipping that level gracefully.
const FEATURE_LEVELS_NO_11_1: [windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL; 6] = [
    D3D_FEATURE_LEVEL_11_0,
    D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_10_0,
    D3D_FEATURE_LEVEL_9_3,
    D3D_FEATURE_LEVEL_9_2,
    D3D_FEATURE_LEVEL_9_1,
];

/// Single attempt at `D3D11CreateDevice` for one (adapter, driver_type) pair,
/// with the descending feature-level negotiation and the Win7 E_INVALIDARG
/// retry-without-11_1 fallback. Does NOT change driver type — driver-type
/// fallback is the caller's (`create`) responsibility.
///
/// MSDN contract preserved by the caller: an explicit `adapter` REQUIRES
/// `D3D_DRIVER_TYPE_UNKNOWN`; a `None` adapter REQUIRES a concrete driver type
/// (HARDWARE or WARP), never UNKNOWN.
fn try_create_device(
    adapter: Option<&IDXGIAdapter>,
    driver_type: D3D_DRIVER_TYPE,
) -> Result<D3dDevice, PlatformError> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // Sane default; D3D11CreateDevice overwrites with the level it selects.
    let mut feature_level = D3D_FEATURE_LEVEL_11_0;

    // SAFETY: D3D11CreateDevice canonical; `FEATURE_LEVELS_ALL` is 'static and
    //         lives for the call; out-params are `Option<T>` per windows-rs
    //         convention. When `adapter` is Some, `driver_type` is UNKNOWN per
    //         MSDN (guaranteed by the caller).
    let result = unsafe {
        D3D11CreateDevice(
            adapter,
            driver_type,
            None,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&FEATURE_LEVELS_ALL),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut feature_level),
            Some(&mut context),
        )
    };

    // Win7 gotcha: E_INVALIDARG for the whole call means the 11_1 runtime is
    // absent. Retry once with the array MINUS the leading 11_1 entry.
    let result = match result {
        Err(e) if e.code() == E_INVALIDARG => {
            device = None;
            context = None;
            feature_level = D3D_FEATURE_LEVEL_11_0;
            // SAFETY: identical contract to the first call; the only change is
            //         the (still 'static) feature-level list with 11_1 dropped.
            unsafe {
                D3D11CreateDevice(
                    adapter,
                    driver_type,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&FEATURE_LEVELS_NO_11_1),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    Some(&mut context),
                )
            }
        }
        other => other,
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

/// Create the process-global device. Tries the primary driver, then falls back
/// to the OTHER driver type once: WARP-primary → hardware-default fallback;
/// hardware/explicit-adapter primary → WARP fallback. On dual failure the
/// primary's (more informative) error is returned — never a panic.
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

    // D3D11CreateDevice wants Param<IDXGIAdapter>; IDXGIAdapter1 derefs to
    // IDXGIAdapter so we project the option through Deref to satisfy the
    // bound. The temporary `&IDXGIAdapter` borrow lives for the call.
    let adapter_base: Option<&IDXGIAdapter> = adapter_opt.as_ref().map(|a| -> &IDXGIAdapter { a });

    let primary = try_create_device(adapter_base, driver_type);
    if primary.is_ok() {
        return primary;
    }

    // Mutual driver fallback. The fallback ALWAYS uses a `None` adapter, so it
    // MUST pair with a concrete driver type (HARDWARE or WARP) — never
    // UNKNOWN-with-null-adapter, which D3D11CreateDevice rejects with
    // E_INVALIDARG (MSDN). The explicit-adapter→UNKNOWN rule above is therefore
    // only ever exercised on the primary attempt that carries the adapter.
    let fallback_driver = if driver_type == D3D_DRIVER_TYPE_WARP {
        // WARP failed (rare: missing/locked-down warp dll) → try hardware on
        // the default adapter.
        D3D_DRIVER_TYPE_HARDWARE
    } else {
        // Hardware or explicit-adapter primary failed → try software WARP, the
        // always-available reference rasterizer.
        D3D_DRIVER_TYPE_WARP
    };
    if let Ok(dev) = try_create_device(None, fallback_driver) {
        return Ok(dev);
    }

    // Both paths failed: surface the PRIMARY error (most informative).
    primary
}

fn use_warp_driver() -> bool {
    std::env::var_os(D3D_WARP_ENV).is_some() || std::env::var_os(D3D_HARDWARE_ENV).is_none()
}

/// Mc-2b — recovery policy state, owned by the shell/app driver. Pure data; the
/// `Instant`-based 60-second retry window lives in the shell (later dispatch),
/// which passes the `within_window` flag in. Kept GPU-free so the whole policy
/// is unit-testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    /// No device loss outstanding — normal painting.
    Healthy,
    /// A device-lost event is being handled; `attempts` recreate tries so far
    /// inside the current retry window (1 after the first BeginRecreate).
    Recovering { attempts: u32 },
    /// Exceeded the retry budget inside the window — recovery abandoned (the
    /// shell shows a fatal box + quits). Further losses are ignored.
    GaveUp,
}

/// Mc-2b — the action the caller should take for a device-lost event given the
/// current `RecoveryState`. Returned by [`decide_recovery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Do nothing (already given up — avoid recreate storms).
    Ignore,
    /// Recreate the device chain. The caller then transitions to
    /// `Recovering { attempts: n }` where `n` is the new attempt count.
    BeginRecreate,
    /// Recreation budget exhausted inside the window — give up.
    GiveUp,
}

/// Pure device-recovery decision. The caller owns the `RecoveryState` and the
/// `Instant`-based window; this function just maps (state, window, budget) to an
/// action. Intended transitions (caller applies them):
///
/// * `Healthy` + device-lost → `BeginRecreate`; caller → `Recovering { attempts: 1 }`.
/// * `Recovering { n }` **within** the window:
///     - `n >= max_attempts` → `GiveUp`; caller → `GaveUp`.
///     - otherwise → `BeginRecreate`; caller → `Recovering { attempts: n + 1 }`.
/// * `Recovering { .. }` **outside** the window (the 60 s budget elapsed since
///   the first failure) → the streak resets: `BeginRecreate`; caller →
///   `Recovering { attempts: 1 }`.
/// * `GaveUp` → `Ignore` (stays `GaveUp`).
///
/// A successful frame is the caller's cue to drop back to `Healthy`; that
/// success transition is not modelled here because no device-lost event drives it.
pub fn decide_recovery(
    state: RecoveryState,
    within_window: bool,
    max_attempts: u32,
) -> RecoveryAction {
    match state {
        RecoveryState::Healthy => RecoveryAction::BeginRecreate,
        RecoveryState::Recovering { attempts } => {
            if !within_window {
                // The retry window elapsed — the streak is stale; start fresh.
                RecoveryAction::BeginRecreate
            } else if attempts >= max_attempts {
                RecoveryAction::GiveUp
            } else {
                RecoveryAction::BeginRecreate
            }
        }
        RecoveryState::GaveUp => RecoveryAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The descending list must lead with 11_1 and the no-11_1 list must be
    // exactly the tail of it. Pure data assertions — no device creation, so
    // this runs on any host including headless CI.
    #[test]
    fn feature_level_lists_are_consistent() {
        assert_eq!(FEATURE_LEVELS_ALL[0], D3D_FEATURE_LEVEL_11_1);
        assert_eq!(FEATURE_LEVELS_NO_11_1, FEATURE_LEVELS_ALL[1..]);
        // Strictly descending by ordinal value (windows-rs FL = i32 magnitude).
        for pair in FEATURE_LEVELS_ALL.windows(2) {
            assert!(pair[0].0 > pair[1].0);
        }
    }

    // §11-safe device-creation smoke. WARP is the test default (no env vars)
    // and is universally available on supported Windows, so on THIS machine
    // creation must succeed. Uses `device()` (idempotent OnceLock) rather than
    // re-creating, so it is harmless if another test populated the singleton.
    // Skips silently if the env-gated hardware path is active to avoid the
    // documented diagnostic-only crash surface.
    #[test]
    fn warp_default_device_creates_on_this_machine() {
        if std::env::var_os(D3D_HARDWARE_ENV).is_some() {
            eprintln!("skipped: BENTODESK_D3D_HARDWARE set, hardware path not asserted");
            return;
        }
        let dev = device().expect("WARP-default D3D11 device should create on this host");
        // Touch both members so the binding is observably used. `dev` is an
        // `Arc<D3dDevice>`; field access derefs through the Arc.
        let _ = (&dev.device, &dev.context);
    }

    // Mc-2b — pure recovery-policy coverage. Zero-GPU, runs everywhere.
    #[test]
    fn decide_recovery_first_loss_begins_recreate() {
        // Healthy + device-lost → BeginRecreate (caller → Recovering{1}).
        assert_eq!(
            decide_recovery(RecoveryState::Healthy, true, 3),
            RecoveryAction::BeginRecreate
        );
    }

    #[test]
    fn decide_recovery_within_budget_retries() {
        // Recovering below the cap, inside the window → keep recreating.
        assert_eq!(
            decide_recovery(RecoveryState::Recovering { attempts: 1 }, true, 3),
            RecoveryAction::BeginRecreate
        );
        assert_eq!(
            decide_recovery(RecoveryState::Recovering { attempts: 2 }, true, 3),
            RecoveryAction::BeginRecreate
        );
    }

    #[test]
    fn decide_recovery_exhausted_gives_up() {
        // Recovering{max} inside the window → GiveUp.
        assert_eq!(
            decide_recovery(RecoveryState::Recovering { attempts: 3 }, true, 3),
            RecoveryAction::GiveUp
        );
        assert_eq!(
            decide_recovery(RecoveryState::Recovering { attempts: 4 }, true, 3),
            RecoveryAction::GiveUp
        );
    }

    #[test]
    fn decide_recovery_gave_up_ignores() {
        assert_eq!(
            decide_recovery(RecoveryState::GaveUp, true, 3),
            RecoveryAction::Ignore
        );
        // Even out of window, GaveUp stays terminal.
        assert_eq!(
            decide_recovery(RecoveryState::GaveUp, false, 3),
            RecoveryAction::Ignore
        );
    }

    #[test]
    fn decide_recovery_out_of_window_resets_streak() {
        // The window elapsed: even a maxed-out streak restarts recreation
        // (caller will reset attempts to 1) rather than giving up.
        assert_eq!(
            decide_recovery(RecoveryState::Recovering { attempts: 9 }, false, 3),
            RecoveryAction::BeginRecreate
        );
    }

    // Mc-2b — real-WARP device-recreate smoke. Exercises the actual
    // `rebuild()` recreate path three times in a row; each must yield a fresh,
    // valid device. Ignored by default (needs a working WARP/GPU stack); run
    // via `cargo test -p bentodesk-platform --lib -- --ignored device_lost_smoke`.
    // Skips silently under the env-gated hardware path, mirroring the WARP
    // smoke test above.
    #[test]
    #[ignore = "real-device recreate smoke; run with -- --ignored"]
    fn device_lost_smoke() {
        if std::env::var_os(D3D_HARDWARE_ENV).is_some() {
            eprintln!("skipped: BENTODESK_D3D_HARDWARE set, hardware path not asserted");
            return;
        }
        for attempt in 0..3 {
            let dev = rebuild().unwrap_or_else(|e| {
                panic!("d3d::rebuild() attempt {attempt} should recreate a WARP device: {e}")
            });
            // Touch the members so the recreated device is observably valid.
            let _ = (&dev.device, &dev.context);
        }
    }
}
