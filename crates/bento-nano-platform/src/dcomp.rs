//! DirectComposition visual tree + Acrylic effect chain.
//!
//! Spec §4: DComp device global single instance; per-window visual tree.
//! Spec §3.2: NO `SetWindowCompositionAttribute(ACCENT_ENABLE_ACRYLICBLURBEHIND)`
//! (deprecated). NO `window-vibrancy` crate.
//!
//! Acrylic chain (when feature `acrylic` enabled):
//!   IDXGISwapChain1 (composition flag)
//!     -> IDCompositionVisual2 (root) -> SetEffect(blur)
//!     -> SetContent(swap chain)

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{DXGI_STATUS_OCCLUDED, HWND};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionTarget, IDCompositionVisual,
};
// `IDCompositionDevice3` is only referenced behind `feature = "acrylic"`
// (the v3 device is the gaussian-blur capability slot); gate the import so
// `--no-default-features` clippy stays warning-clean per spec §16.1.
#[cfg(feature = "acrylic")]
use windows::Win32::Graphics::DirectComposition::IDCompositionDevice3;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_PRESENT_TEST, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2,
    IDXGISwapChain1, IDXGISwapChain2,
};
use windows::core::Interface;

use crate::d3d::device as d3d_device;
use crate::errors::{PlatformError, ok};
use crate::message_loop;

/// Process-wide DComp device.
///
/// Wave 16 — interface family is `IDCompositionDesktopDevice` (the v2-family
/// HWND-bound device), not `IDCompositionDevice` (v1):
///
///   * `IDCompositionDevice` (v1, IID c37ea93a-…) is a **distinct** COM
///     interface family from `IDCompositionDevice2`/`3`. A device created
///     via `DCompositionCreateDevice` cannot QueryInterface to v3 — even
///     when the underlying dcomp.dll fully supports v3 — because v1 was
///     never documented to expose the v2/v3 vtables. Empirically (Wave 16
///     stderr capture pre-fix on Win11 26100) this manifests as
///     `E_NOINTERFACE (0x80004002)` on `dcomp.cast::<v3>()`, which broke
///     the Wave 13 acrylic chain on every supported Windows build.
///   * `IDCompositionDesktopDevice` (IID 5f4633fe-…) inherits from
///     `IDCompositionDevice2`, owns `CreateTargetForHwnd` + `CreateVisual`
///     and is the canonical v2-family device for HWND-bound composition.
///
/// Created via `DCompositionCreateDevice2`, which accepts an `IUnknown`
/// rendering device (here our D3D11 device's `IDXGIDevice` view) and
/// returns the requested interface directly via the generic `T: Interface`
/// slot — no post-create QI dance. This matches the documented Microsoft
/// pattern for v2/v3 init.
///
/// `device3` is the v3-family interface used for `CreateGaussianBlurEffect`.
/// On Win11 26100 + iGPU we observed that **all** documented v3-acquisition
/// paths fail with `E_NOINTERFACE` against an `IDXGIDevice` source — see
/// the Wave 16 forensic notes in `device()` below. We therefore treat
/// `device3` as **optional**: when v3 is unreachable the swap chain +
/// visual tree are still wired up (the window paints normally), we just
/// skip the blur effect attachment. This is consistent with spec §11
/// init-path discipline (degrade rather than panic) and matches the Wave
/// 13 ablation finding that the acrylic effect is a visual polish, not a
/// memory lever (the previous "11 MB regression on disable" measurement
/// was confounded by the cast failure that left `paint()` returning Err
/// every frame — i.e. there was no working acrylic baseline to regress
/// from).
pub struct DCompState {
    /// HWND-bound device — owns `CreateTargetForHwnd` + `CreateVisual` (v2).
    pub device: IDCompositionDesktopDevice,
    /// Effects-capable device — owns `CreateGaussianBlurEffect` (v3).
    /// `None` when v3 acquisition fails (older driver / iGPU combo); the
    /// renderer skips the blur path in that case but the window still
    /// composes through the v2 visual tree.
    #[cfg(feature = "acrylic")]
    pub device3: Option<IDCompositionDevice3>,
}

// SAFETY: DComp device is documented thread-safe; we only mutate the visual
//         tree from the UI thread.
unsafe impl Send for DCompState {}
unsafe impl Sync for DCompState {}

static DCOMP: OnceLock<DCompState> = OnceLock::new();

pub fn acrylic_feature_enabled() -> bool {
    cfg!(feature = "acrylic")
}

pub fn acrylic_runtime_available() -> Option<bool> {
    #[cfg(feature = "acrylic")]
    {
        DCOMP.get().map(|state| state.device3.is_some())
    }
    #[cfg(not(feature = "acrylic"))]
    {
        Some(false)
    }
}

pub fn device() -> Result<&'static DCompState, PlatformError> {
    if let Some(d) = DCOMP.get() {
        return Ok(d);
    }
    let d3d = d3d_device()?;
    // Spec §15.1 — Interface::cast canonical for COM cross-cast. The v2/v3
    // create-path takes an `IUnknown`, so we hand it the IDXGIDevice view
    // of our D3D11 device (matches what the v1 path used to do — same
    // underlying object, just reached through a different abi slot).
    let dxgi_dev: IDXGIDevice = ok("D3D::cast<IDXGIDevice>", d3d.device.cast())?;
    // SAFETY: DCompositionCreateDevice2 is the documented v2/v3 entry —
    //         it accepts an IUnknown rendering device and returns the
    //         requested interface directly. Failure (HRESULT) is reflected
    //         through `ok(...)`.
    let dcomp: IDCompositionDesktopDevice =
        ok("DCompositionCreateDevice2<DesktopDevice>", unsafe {
            DCompositionCreateDevice2(&dxgi_dev)
        })?;
    // Wave 16 forensic record (Win11 26100, dcomp.dll 10.0.26100.7859,
    // integrated GPU via DXGI_GPU_PREFERENCE_MINIMUM_POWER):
    //
    //   * `dcomp.cast::<IDCompositionDevice3>()` on the v1 or v2 singleton
    //     returns `E_NOINTERFACE (0x80004002)` — windows-rs `Interface::cast`
    //     routes through `QueryInterface`, so the failure is genuinely on
    //     dcomp.dll's side: the typed-view QI table does not bridge sibling
    //     interfaces on this build.
    //   * `DCompositionCreateDevice2(&dxgi_dev)` requesting v3 IID returns
    //     `E_NOINTERFACE` — dcomp.dll won't synthesise a v3 vtable from an
    //     IDXGIDevice source.
    //   * `DCompositionCreateDevice3(&dxgi_dev)` (the explicit v3 entry
    //     point) likewise returns `E_NOINTERFACE`.
    //   * QI from the `IUnknown` root of the DesktopDevice to v3 also
    //     returns `E_NOINTERFACE`.
    //
    // Best-effort acquisition: try `DCompositionCreateDevice2<v3>` first.
    // On failure the window still composes — we just lose the gaussian
    // blur. This matches spec §11 init-path discipline (degrade, don't
    // panic) and turns the silent paint-loop failure into a visible
    // window. The probe is silent on failure to keep stderr empty in
    // the steady state; future tooling can read `device3.is_some()` to
    // surface acrylic availability in diagnostics.
    #[cfg(feature = "acrylic")]
    let device3: Option<IDCompositionDevice3> = {
        let res: windows::core::Result<IDCompositionDevice3> =
            unsafe { DCompositionCreateDevice2(&dxgi_dev) };
        res.ok()
    };
    #[cfg(feature = "acrylic")]
    let _ = DCOMP.set(DCompState {
        device: dcomp,
        device3,
    });
    #[cfg(not(feature = "acrylic"))]
    let _ = DCOMP.set(DCompState { device: dcomp });
    DCOMP
        .get()
        .ok_or(PlatformError::Init("DComp OnceLock empty"))
}

/// Per-window DComp / DXGI artefacts.
///
/// `swap_chain` is `Option` so T-099 hibernation can drop the backbuffer
/// (largest per-window allocation, ~1.2 MB at 480×320×4×2) for hidden
/// non-Main windows without tearing down the visual tree. `hwnd` is cached
/// so the chain can be rebuilt by `ensure_chain` on the next show.
pub struct WindowComp {
    pub swap_chain: Option<IDXGISwapChain1>,
    pub target: IDCompositionTarget,
    pub root_visual: IDCompositionVisual,
    /// Cached HWND for `ensure_chain` rebuilds. Never null for a constructed
    /// `WindowComp`; only re-read when the chain has been released by T-099
    /// hibernation and a subsequent show / paint requires resurrection.
    hwnd: HWND,
    /// Mc-2 #11 — occluded-present state. Set when `Present` reports
    /// `DXGI_STATUS_OCCLUDED` (window fully covered, session locked, or RDP
    /// client minimised); while set, `present()` polls cheaply with
    /// `Present(0, DXGI_PRESENT_TEST)` instead of doing a real vsync present
    /// so we stop burning GPU/CPU compositing frames nobody sees. `AtomicBool`
    /// (not `Cell`) so the field is `Sync`-safe regardless of how `WindowComp`
    /// is shared; a relaxed load on the hot path is as cheap as a `Cell` read.
    occluded: AtomicBool,
}

// SAFETY: COM ref-counted handles; access pinned to UI thread.
unsafe impl Send for WindowComp {}
unsafe impl Sync for WindowComp {}

impl WindowComp {
    pub fn create(hwnd: HWND, width: u32, height: u32) -> Result<Self, PlatformError> {
        let swap = create_swap_chain(hwnd, width, height)?;
        let (target, root_visual) = create_visual_tree(hwnd, &swap)?;
        Ok(WindowComp {
            swap_chain: Some(swap),
            target,
            root_visual,
            hwnd,
            occluded: AtomicBool::new(false),
        })
    }

    /// T-099 — drop the swap chain backbuffer. Visual tree + DComp target
    /// stay alive (rebuilding them on every show would re-trigger DComp
    /// commits and waste effort); only the DXGI backbuffer goes. After this,
    /// `present()` and `resize()` are no-ops until `ensure_chain` rebuilds.
    pub fn release_chain(&mut self) {
        if self.swap_chain.is_some() {
            // Detach from the visual tree so DComp doesn't hold a stale ref.
            // SAFETY: root_visual valid; SetContent(None) clears the binding.
            let _ = unsafe {
                self.root_visual
                    .SetContent(None as Option<&windows::core::IUnknown>)
            };
            // Commit the detach so DComp actually releases its reference
            // before we drop the COM handle below. Without this the chain
            // can stay resident until the next paint.
            // SAFETY: DComp device global; Commit is the documented flush.
            if let Ok(dev) = device() {
                let _ = unsafe { dev.device.Commit() };
            }
            self.swap_chain = None;
        }
    }

    /// T-099 — recreate the swap chain at `w × h` after a previous
    /// `release_chain`. Idempotent: if a chain is already resident the call
    /// is a no-op (cheap branch). Re-attaches the backbuffer to the visual
    /// tree + commits so the next paint composites correctly.
    pub fn ensure_chain(&mut self, w: u32, h: u32) -> Result<(), PlatformError> {
        if self.swap_chain.is_some() {
            return Ok(());
        }
        let swap = create_swap_chain(self.hwnd, w, h)?;
        // SAFETY: root_visual + swap valid; rebind backbuffer to the tree.
        ok("Visual::SetContent(rebound)", unsafe {
            self.root_visual.SetContent(&swap)
        })?;
        // SAFETY: DComp device global; Commit publishes the rebound visual.
        let dev = device()?;
        ok("DComp::Commit(rebound)", unsafe { dev.device.Commit() })?;
        self.swap_chain = Some(swap);
        Ok(())
    }

    /// Whether the swap chain is currently resident. T-099 diagnostics
    /// and the renderer paint guard read this to skip frames on hibernated
    /// windows without invoking COM.
    #[inline]
    pub fn is_chain_resident(&self) -> bool {
        self.swap_chain.is_some()
    }
}

/// Construct the DXGI swap chain (extracted so `create` and `ensure_chain`
/// share one path — Wave 12 frame-latency-waitable flag, 2-buffer flip-discard,
/// premultiplied alpha — without duplicating the spec §1 RSS lever
/// configuration).
fn create_swap_chain(
    hwnd: HWND,
    width: u32,
    height: u32,
) -> Result<IDXGISwapChain1, PlatformError> {
    let _ = hwnd; // composition swap chain is HWND-independent (DComp owns binding).
    let d3d = &d3d_device()?.device;

    // SAFETY: standard CreateDXGIFactory2 entry.
    let factory: IDXGIFactory2 = ok("CreateDXGIFactory2", unsafe {
        CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
    })?;

    // Spec §1 RSS lever — minimal swap chain footprint:
    //   - BufferCount = 2 (not 3): saves one full-res backbuffer. DComp
    //     requires ≥2 for the flip-model surface; 1 is rejected.
    //   - Width/Height follow caller-supplied client area; no 800x600 hard-code.
    //     For a 480x320 window (BentoDesk default) this is ~480*320*4*2 ≈ 1.2 MB
    //     of GPU-backed memory vs ~3.7 MB at 800x600 with 3 buffers.
    //   - Format B8G8R8A8_UNORM: D2D-compatible + minimal pixel alignment.
    //   - AlphaMode PREMULTIPLIED: required for DComp transparent windows.
    //   - SwapEffect FLIP_DISCARD: smaller commit than FLIP_SEQUENTIAL
    //     because the OS can free the previous frame immediately on Present.
    //   - Flags = FRAME_LATENCY_WAITABLE_OBJECT (Wave 12 / Tier-0 #16):
    //     unlocks IDXGISwapChain2::SetMaximumFrameLatency below so DXGI
    //     caps queued frames at 1 instead of the default 3 — directly
    //     reduces GPU surface commit by ~1-3 MB. We do NOT gate Present
    //     on the waitable handle in this wave (would touch message_loop
    //     and is an input-latency concern, not memory).
    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: width.max(1),
        Height: height.max(1),
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        // DXGI_SWAP_CHAIN_FLAG is i32-newtype; DESC1.Flags is u32 — cast at the boundary.
        Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
    };
    // SAFETY: factory + d3d valid.
    let swap = ok("CreateSwapChainForComposition", unsafe {
        factory.CreateSwapChainForComposition(d3d, &desc, None)
    })?;

    // Wave 12 — cap DXGI's queued-frame depth at 1 (default 3). Requires
    // the FRAME_LATENCY_WAITABLE_OBJECT flag set above; without it the
    // call returns DXGI_ERROR_INVALID_CALL. windows-rs 0.58 (spec §3.1.1):
    // IDXGISwapChain2 lives in `windows::Win32::Graphics::Dxgi`.
    // Spec §15.1 — Interface::cast canonical for COM cross-cast.
    let swap2: IDXGISwapChain2 = ok("Swap::cast<IDXGISwapChain2>", swap.cast())?;
    // SAFETY: swap2 is a valid IDXGISwapChain2 returned from QueryInterface;
    //         max latency = 1 is in the documented [1, 16] range.
    ok("IDXGISwapChain2::SetMaximumFrameLatency", unsafe {
        swap2.SetMaximumFrameLatency(1)
    })?;

    // T-007 / Wave A — publish the frame-latency waitable handle to the
    // message loop so `MsgWaitForMultipleObjectsEx` can pace UI wakeups
    // at ~60 FPS even when no input is arriving (animation tick at idle).
    // Returns the handle whose state is signalled when DXGI is ready
    // for the next frame; ownership stays with the swap chain — we
    // never CloseHandle it ourselves. Returns NULL on failure (e.g.
    // the FRAME_LATENCY_WAITABLE_OBJECT flag was somehow stripped),
    // which the message loop treats as "stay on cold-start fallback".
    // SAFETY: swap2 valid (owned by this call). The returned HANDLE is
    //         a borrowed view tied to the swap chain; spec discipline
    //         (`message_loop::register_frame_handle` doc-comment) is
    //         that we never CloseHandle this value.
    let frame_handle = unsafe { swap2.GetFrameLatencyWaitableObject() };
    if !frame_handle.is_invalid() {
        // windows-rs 0.58 `HANDLE` is a newtype wrapping `*mut c_void`;
        // windows-sys `HANDLE` is the same `*mut c_void` type alias.
        // Convert at the boundary so the public API stays on the
        // windows-sys ABI (spec §3.1.1 — message_loop is a hot-path
        // Win32 module, so it tracks windows-sys types).
        message_loop::register_frame_handle(frame_handle.0 as _);
    }

    Ok(swap)
}

/// Construct the DComp visual tree (target + root visual) and bind the
/// swap chain backbuffer + commit. Extracted alongside `create_swap_chain`
/// so `WindowComp::create` and the hibernation rebuild path share one source
/// of truth for the visual tree topology.
fn create_visual_tree(
    hwnd: HWND,
    swap: &IDXGISwapChain1,
) -> Result<(IDCompositionTarget, IDCompositionVisual), PlatformError> {
    let dcomp = &device()?.device;
    // SAFETY: dcomp valid; topmost=true places visual above other DComp
    //         content. CreateTargetForHwnd lives on
    //         IDCompositionDesktopDevice — same call signature as the
    //         v1 path, just reached through the v2-family vtable.
    let target = ok("CreateTargetForHwnd", unsafe {
        dcomp.CreateTargetForHwnd(hwnd, true)
    })?;
    // SAFETY: dcomp valid. v2 `CreateVisual` returns `IDCompositionVisual2`
    //         which derefs to `IDCompositionVisual` (Wave 16 — interface
    //         hierarchy at windows-rs 0.58 line 2424). We immediately
    //         cast down to v1 because the rest of WindowComp + the
    //         downstream renderer signature is keyed on v1 (SetContent /
    //         SetEffect both live there) and the cast is a no-op QI on
    //         the same underlying object.
    let root2 = ok("CreateVisual", unsafe { dcomp.CreateVisual() })?;
    let root: IDCompositionVisual = ok("Visual2::cast<Visual>", root2.cast())?;

    // Wave 16 — v3 may be unavailable (see `device()` doc-comment).
    // When it is, skip acrylic silently rather than failing the whole
    // window. Spec §11 init-path discipline: blur is visual polish,
    // not a correctness requirement.
    #[cfg(feature = "acrylic")]
    if let Some(dcomp3) = device()?.device3.as_ref() {
        attach_acrylic(dcomp3, &root)?;
    }

    // SAFETY: root + swap valid.
    ok("Visual::SetContent", unsafe { root.SetContent(swap) })?;
    // SAFETY: target valid.
    ok("Target::SetRoot", unsafe { target.SetRoot(&root) })?;
    // SAFETY: dcomp valid. `Commit` lives on `IDCompositionDevice2`,
    //         which `IDCompositionDesktopDevice` derefs to.
    ok("DComp::Commit", unsafe { dcomp.Commit() })?;

    Ok((target, root))
}

/// Mc-2 #11 — classification of a raw `IDXGISwapChain1::Present` HRESULT for
/// the occlusion state machine. Pure (no COM); factored out so the decision
/// is unit-testable without a live swap chain.
#[derive(Debug, PartialEq, Eq)]
enum PresentOutcome {
    /// `DXGI_STATUS_OCCLUDED` — window fully covered / session locked / RDP
    /// minimised. SUCCESS HRESULT (high bit clear) so `.ok()` would mask it;
    /// we must branch on it explicitly.
    Occluded,
    /// `S_OK` — presented (or, in TEST mode, the window is visible again).
    Ok,
    /// Any other HRESULT — route through the existing `ok()` helper so genuine
    /// failures still map to `PlatformError` exactly as before.
    Other,
}

/// Classify a raw `Present` HRESULT integer (`HRESULT.0`). `S_OK` is `0`;
/// `DXGI_STATUS_OCCLUDED` is `0x087A_0001`.
#[inline]
fn classify_present(hr: i32) -> PresentOutcome {
    if hr == DXGI_STATUS_OCCLUDED.0 {
        PresentOutcome::Occluded
    } else if hr == 0 {
        PresentOutcome::Ok
    } else {
        PresentOutcome::Other
    }
}

impl WindowComp {
    /// Present the current backbuffer. Returns `Ok(())` as a no-op when the
    /// swap chain has been hibernated by T-099 — the renderer guards paint
    /// against released chains, so this only fires if a stale `present` was
    /// queued between `release_chain` and the next paint.
    ///
    /// Mc-2 #11 — occlusion state machine: a fully-covered / locked-session /
    /// minimised-RDP window must NOT keep doing full vsync `Present(1, …)`
    /// every frame (compositing frames nobody sees burns GPU/CPU). When DXGI
    /// reports `DXGI_STATUS_OCCLUDED` we latch `self.occluded` and, on
    /// subsequent calls, poll cheaply with `Present(0, DXGI_PRESENT_TEST)`
    /// (TEST renders nothing — it only reports whether the window is still
    /// occluded) until it returns `S_OK`, then resume real presenting.
    ///
    /// Reachability: this is correct without any timer/thread. When the
    /// covering window moves away / the session unlocks, Windows invalidates
    /// the newly-exposed region → a `WM_PAINT` → the normal paint→present path
    /// re-enters here, the TEST poll returns `S_OK`, and we resume. So we only
    /// need `present()` to be correct *when called*; we never spin.
    pub fn present(&self) -> Result<(), PlatformError> {
        let Some(swap) = self.swap_chain.as_ref() else {
            return Ok(());
        };

        // §10 hot path: an AtomicBool load + an HRESULT integer compare + a
        // branch — no allocation, no logging in the occluded case (it would
        // spam every frame; the occluded transition is silent by design).
        if self.occluded.load(Ordering::Relaxed) {
            // Cheap occlusion poll — PRESENT_TEST renders nothing.
            // SAFETY: swap valid; sync interval 0 + TEST flag = no render.
            let hr = unsafe { swap.Present(0, DXGI_PRESENT_TEST) };
            match classify_present(hr.0) {
                // Still occluded — cheap no-op, no render this frame.
                PresentOutcome::Occluded => return Ok(()),
                // Visible again — clear the latch and fall through to do a
                // real present so the newly-exposed region isn't stale.
                PresentOutcome::Ok => self.occluded.store(false, Ordering::Relaxed),
                // Genuine failure during the poll — surface it via `ok()`.
                PresentOutcome::Other => return ok("IDXGISwapChain1::Present(TEST)", hr.ok()),
            }
        }

        // Normal real present (vsync). SAFETY: swap valid; sync interval 1.
        let hr = unsafe { swap.Present(1, DXGI_PRESENT(0)) };
        match classify_present(hr.0) {
            // Just learned we're occluded — latch so the next frames poll
            // cheaply instead of presenting into the void.
            PresentOutcome::Occluded => {
                self.occluded.store(true, Ordering::Relaxed);
                Ok(())
            }
            // S_OK or any other HRESULT routes through `ok()` exactly as
            // before — preserving both success and error-mapping behaviour.
            PresentOutcome::Ok | PresentOutcome::Other => ok("IDXGISwapChain1::Present", hr.ok()),
        }
    }

    /// Resize the backbuffer. No-op when hibernated — the next `ensure_chain`
    /// will allocate at the requested size directly. Wave 12: must re-pass
    /// the FRAME_LATENCY_WAITABLE_OBJECT flag so DXGI doesn't silently
    /// demote back to the 3-frame default queue.
    pub fn resize(&self, w: u32, h: u32) -> Result<(), PlatformError> {
        let Some(swap) = self.swap_chain.as_ref() else {
            return Ok(());
        };
        // SAFETY: swap valid; ResizeBuffers preserves format.
        ok("ResizeBuffers", unsafe {
            swap.ResizeBuffers(
                0,
                w.max(1),
                h.max(1),
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0),
            )
        })
    }
}

#[cfg(feature = "acrylic")]
fn attach_acrylic(
    dcomp3: &IDCompositionDevice3,
    visual: &IDCompositionVisual,
) -> Result<(), PlatformError> {
    use windows::Win32::Graphics::DirectComposition::IDCompositionGaussianBlurEffect;
    // Wave 16 — `CreateGaussianBlurEffect` lives on `IDCompositionDevice3`.
    // The pre-Wave-16 path tried `dcomp.cast::<Device3>()` on the v1 singleton
    // (and a transient Wave-16 attempt did the same against the v2-family
    // DesktopDevice singleton); both returned `E_NOINTERFACE (0x80004002)`
    // on Win11 26220 because dcomp.dll's QI table does not bridge sibling
    // interfaces in either direction. The robust pattern (and what we
    // exercise here) is to obtain `IDCompositionDevice3` via its own
    // `DCompositionCreateDevice2<Device3>` call against the same
    // `IDXGIDevice` — see `device()`. By the time `attach_acrylic` runs,
    // `dcomp3` is already that pre-materialised v3 vtable.
    // SAFETY: dcomp3 valid.
    let blur: IDCompositionGaussianBlurEffect = ok("CreateGaussianBlurEffect", unsafe {
        dcomp3.CreateGaussianBlurEffect()
    })?;
    // SAFETY: blur valid.
    ok("Blur::SetStandardDeviation", unsafe {
        blur.SetStandardDeviation2(30.0)
    })?;
    // SAFETY: visual valid; blur is an effect (IUnknown-compatible).
    ok("Visual::SetEffect(blur)", unsafe {
        visual.SetEffect(&blur)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mc-2 #11 — pure-logic coverage of the present-HRESULT classifier that
    // drives the occlusion state machine. `present()` itself needs a live
    // DXGI swap chain (GPU) and can't be unit-tested, but the decision it
    // makes is fully captured here.
    #[test]
    fn classify_present_maps_occluded_ok_and_other() {
        // DXGI_STATUS_OCCLUDED (0x087A0001) is a *success* HRESULT — the bug
        // this fix addresses is that `.ok()` masks it; classify must catch it.
        assert_eq!(classify_present(0x087A_0001), PresentOutcome::Occluded);
        assert_eq!(classify_present(DXGI_STATUS_OCCLUDED.0), PresentOutcome::Occluded);
        // S_OK.
        assert_eq!(classify_present(0), PresentOutcome::Ok);
        // A genuine failure (DXGI_ERROR_DEVICE_REMOVED) and an unrelated
        // success both route through the `ok()` helper as `Other`.
        assert_eq!(classify_present(0x887A_0005u32 as i32), PresentOutcome::Other);
        assert_eq!(classify_present(1), PresentOutcome::Other);
    }
}
