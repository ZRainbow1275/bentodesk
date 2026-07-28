//! Manages the desktop overlay layer.
//!
//! Architecture (inspired by Rainmeter and Stardock Fences):
//!
//! The overlay uses `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` to hide from
//! Alt-Tab and prevent focus stealing. Both the selected-stack DComp host and
//! legacy attachment path use Tauri's `HWND_BOTTOM` policy: above Explorer's
//! desktop, below ordinary application windows. Neither path is topmost.
//!
//! A WndProc subclass intercepts `WM_WINDOWPOSCHANGING` to prevent Windows
//! from pushing the overlay behind the desktop when the user clicks elsewhere.
//! The subclass uses a bypass flag (`BYPASS_SUBCLASS`) so that our own
//! show / hide / reposition calls are not blocked.
//!
//! Window decorations (title bar, borders) are explicitly removed via
//! `GWL_STYLE` manipulation; we suppress NC paint/calculation messages while
//! delegating `WM_NCHITTEST` to the selected-stack shell so blank desktop
//! space can return `HTTRANSPARENT`.
//!
//! Click-through is handled by the selected-stack shell's exact HWND region;
//! [`set_cursor_passthrough`] records the matching semantic state without
//! toggling a full-screen DComp extended style on every frame.

// HWND is `*mut c_void` — Clippy's `not_unsafe_ptr_arg_deref` flags every
// public function that forwards an HWND into a Win32 API call. In this
// module HWND is opaque (the OS owns the pointee, we never deref it
// ourselves), so the lint is a false positive. Marking each function
// `unsafe` would force the dispatcher / tray / resolution monitor to wrap
// every call in `unsafe {}` for no added safety. The actual `unsafe`
// blocks inside each body still carry SAFETY comments per spec §11.1.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, RemoveWindowSubclass, SUBCLASSPROC, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, HWND_BOTTOM, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, WINDOWPOS, WM_MOUSEACTIVATE, WM_WINDOWPOSCHANGING, WS_BORDER,
    WS_CAPTION, WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

use crate::system;

/// Subclass ID for our overlay WndProc subclass.
const OVERLAY_SUBCLASS_ID: usize = 0xBE470;

/// `SWP_SHOWWINDOW` flag (0x0040) — set by Windows in `WINDOWPOS.flags`
/// when the window is about to be shown. Not re-exported by `windows-sys`
/// in all feature sets so we define the raw value.
const SWP_SHOWWINDOW_RAW: u32 = 0x0040;
/// `SWP_HIDEWINDOW` flag (0x0080) — counterpart to `SWP_SHOWWINDOW`.
const SWP_HIDEWINDOW_RAW: u32 = 0x0080;

// Non-client area message constants (windows-sys does not export these).
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCPAINT: u32 = 0x0085;
const WM_NCACTIVATE: u32 = 0x0086;

// Power broadcast message constants.
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;

/// `DWMWA_WINDOW_CORNER_PREFERENCE` — Windows 11 11.0+ corner preference attr.
/// `windows-sys` 0.59 takes `dwattribute` as `u32` (the underlying ABI type),
/// not the `DWMWINDOWATTRIBUTE` enum alias 1.x used.
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;

/// Errors surfaced by the ghost-layer API.
#[derive(Debug)]
pub enum GhostLayerError {
    /// `attach` was called with a null HWND.
    NullHwnd,
    /// `attach` could not install the WndProc subclass.
    SubclassInstallFailed,
}

impl core::fmt::Display for GhostLayerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NullHwnd => f.write_str("ghost layer: null HWND"),
            Self::SubclassInstallFailed => f.write_str("ghost layer: SetWindowSubclass failed"),
        }
    }
}

impl core::error::Error for GhostLayerError {}

/// Events the ghost layer emits to the rest of the application.
///
/// 1.x called into `power::handle_resume` directly from the WndProc — that
/// crate-level coupling is broken here. Instead the WndProc forwards events
/// over a [`crossbeam_channel::Sender`] supplied via [`set_event_sender`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GhostLayerEvent {
    /// `WM_POWERBROADCAST` with `PBT_APMRESUMEAUTOMATIC`. Consumer should
    /// rebuild file watchers / re-attach overlays / etc.
    PowerResume,
}

// ─── Process-local state ────────────────────────────────────────────

/// Global visibility flag — single source of truth for whether the overlay
/// window is currently shown. Used by the tray toggle instead of querying
/// `IsWindowVisible` which can return stale state when our own bypass path
/// races a Windows-initiated show/hide.
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

/// `true` when blank desktop regions should pass mouse input through to the
/// shell below the overlay.
static CURSOR_PASSTHROUGH: AtomicBool = AtomicBool::new(true);

/// Stored HWND so show / hide / reposition can be called from anywhere
/// without re-resolving. Stored as `usize` (raw pointers are not `Send`).
static MAIN_HWND: OnceLock<usize> = OnceLock::new();

/// Channel for forwarding ghost-layer events out of the WndProc subclass.
static EVENT_TX: OnceLock<Sender<GhostLayerEvent>> = OnceLock::new();

/// When `true`, the WndProc subclass allows z-order changes initiated by
/// our own code. Set to `true` immediately before our `SetWindowPos` /
/// `ShowWindow` calls and reset to `false` afterwards.
static BYPASS_SUBCLASS: AtomicBool = AtomicBool::new(false);

/// Keep the resident Main surface in the desktop layer: above Explorer's
/// desktop, below every ordinary application window. This is the exact
/// `HWND_BOTTOM` policy used by the Tauri benchmark.
fn park_at_desktop_layer(hwnd: HWND, extra_flags: u32) {
    // SAFETY: callers pass the live Main HWND. The bypass flag only matters for
    // the legacy subclass path; it is harmless for the lightweight binding.
    unsafe {
        BYPASS_SUBCLASS.store(true, Ordering::Release);
        SetWindowPos(
            hwnd,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | extra_flags,
        );
        BYPASS_SUBCLASS.store(false, Ordering::Release);
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Wire the WndProc subclass to a [`crossbeam_channel::Sender`] so events
/// like `PBT_APMRESUMEAUTOMATIC` can be observed by the rest of the app.
///
/// Call this once before [`attach`]. Subsequent calls are no-ops (the
/// channel is held in a `OnceLock`).
pub fn set_event_sender(tx: Sender<GhostLayerEvent>) {
    let _ = EVENT_TX.set(tx);
}

/// Query whether the overlay window is currently visible.
pub fn is_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Relaxed)
}

/// Query whether cursor input currently passes through blank desktop space.
pub fn cursor_passthrough() -> bool {
    CURSOR_PASSTHROUGH.load(Ordering::Relaxed)
}

/// Record cursor passthrough state for the selected-stack overlay.
///
/// This is the selected-stack equivalent of Tauri's
/// `setIgnoreCursorEvents(true/false)`. Runtime hit testing is now owned by the
/// Main HWND region (`Renderer::apply_main_click_through_region`): blank desktop
/// pixels fall outside the window, and painted chrome remains inside it. Keeping
/// this function state-only avoids toggling `WS_EX_TRANSPARENT` on the full
/// resident DComp window, which forces a large DWM private allocation.
pub fn set_cursor_passthrough(enabled: bool) {
    let previous = CURSOR_PASSTHROUGH.swap(enabled, Ordering::Relaxed);
    if previous != enabled {
        tracing::debug!(
            target: "bentodesk::ghost_layer",
            cursor_passthrough = enabled,
            "ghost_layer cursor passthrough changed"
        );
    }
}

/// Temporarily bypass the WndProc subclass z-order protection.
///
/// Used by other modules (resolution monitor, multi-monitor pivot) that need
/// to call `SetWindowPos` on the overlay without being blocked. Returns a
/// guard that restores the bypass flag on drop.
pub fn bypass_subclass_guard() -> BypassGuard {
    BYPASS_SUBCLASS.store(true, Ordering::Release);
    BypassGuard
}

/// RAII guard that resets `BYPASS_SUBCLASS` to `false` on drop.
pub struct BypassGuard;

impl Drop for BypassGuard {
    fn drop(&mut self) {
        BYPASS_SUBCLASS.store(false, Ordering::Release);
    }
}

/// Show the overlay window without activating it (no focus steal).
///
/// The selected-stack host remains in the normal non-topmost z-order; showing
/// it without activation must not promote it over the active application.
pub fn show_window() {
    let Some(&raw_hwnd) = MAIN_HWND.get() else {
        return;
    };
    let hwnd = raw_hwnd as HWND;

    // SAFETY: We stored `raw_hwnd` from a valid HWND in `attach`; the OS
    // guarantees an HWND remains usable until WM_DESTROY.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    park_at_desktop_layer(hwnd, 0);
    WINDOW_VISIBLE.store(true, Ordering::Relaxed);
    tracing::debug!("ghost_layer: shown (NOACTIVATE, HWND_BOTTOM)");
}

/// Hide the overlay window.
pub fn hide_window() {
    let Some(&raw_hwnd) = MAIN_HWND.get() else {
        return;
    };
    let hwnd = raw_hwnd as HWND;

    // SAFETY: see `show_window` — HWND validity guaranteed by the OS post-
    // attach.
    unsafe {
        BYPASS_SUBCLASS.store(true, Ordering::Release);
        ShowWindow(hwnd, SW_HIDE);
        BYPASS_SUBCLASS.store(false, Ordering::Release);
    }
    WINDOW_VISIBLE.store(false, Ordering::Relaxed);
    tracing::debug!("ghost_layer: hidden");
}

/// Refresh the overlay frame after a display change without touching its
/// z-order. Called by the resolution monitor. The Wave H1 z-order policy
/// keeps the overlay at `HWND_TOPMOST` via the `WS_EX_TOPMOST` ex-style
/// flag, which persists across `SWP_NOZORDER` calls; this routine only
/// requests `SWP_FRAMECHANGED` so the subclass recomputes the non-client
/// layout.
pub fn reposition_to_work_area() {
    let Some(&raw_hwnd) = MAIN_HWND.get() else {
        return;
    };
    let hwnd = raw_hwnd as HWND;

    park_at_desktop_layer(hwnd, SWP_FRAMECHANGED);

    tracing::info!("ghost_layer: refreshed frame layout and restored HWND_BOTTOM desktop layer");
}

// ─── WndProc subclass ───────────────────────────────────────────────

/// SAFETY: Called by Windows as a subclass procedure. The `lparam` for
/// `WM_WINDOWPOSCHANGING` points to a valid `WINDOWPOS` struct per MSDN.
unsafe extern "system" fn overlay_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    // ── Complete non-client area suppression ───────────────────────
    //
    // These four handlers work together to eliminate the window frame /
    // title bar even when DWM composition is active:
    //
    // WM_NCCALCSIZE: "entire window is client area" (no room for frame)
    // WM_NCPAINT:    "don't paint any non-client area" (no frame rendering)
    // WM_NCACTIVATE: "don't draw activation state" (no title bar highlight)
    // WM_NCHITTEST:  delegate to shell hit-test so empty desktop space can
    //                return HTTRANSPARENT while real surfaces stay clickable.

    if msg == WM_NCCALCSIZE {
        return 0;
    }
    if msg == WM_NCPAINT {
        return 0;
    }
    if msg == WM_NCACTIVATE {
        return 1;
    }
    if msg == WM_WINDOWPOSCHANGING {
        // SAFETY: per MSDN, `lparam` for `WM_WINDOWPOSCHANGING` is a valid
        // `WINDOWPOS *` until DefWindowProc returns; we mutate in place and
        // never store the pointer.
        let wp: &mut WINDOWPOS = unsafe { &mut *(lparam as *mut WINDOWPOS) };

        // Always prevent activation — the overlay must never steal focus.
        wp.flags |= SWP_NOACTIVATE;

        // Show / hide passes must reach DefSubclassProc untouched so
        // `ShowWindow(SW_HIDE / SW_SHOWNOACTIVATE)` works.
        let is_show_hide =
            (wp.flags & SWP_SHOWWINDOW_RAW) != 0 || (wp.flags & SWP_HIDEWINDOW_RAW) != 0;

        if !is_show_hide && !BYPASS_SUBCLASS.load(Ordering::Acquire) {
            // External z-order change (e.g. user clicked desktop) — block it.
            wp.flags |= SWP_NOZORDER;
        }
    } else if msg == WM_MOUSEACTIVATE {
        return 3; // MA_NOACTIVATE
    } else if msg == WM_POWERBROADCAST && wparam == PBT_APMRESUMEAUTOMATIC {
        if let Some(tx) = EVENT_TX.get() {
            let _ = tx.send(GhostLayerEvent::PowerResume);
        }
        return 1; // TRUE — we processed it.
    }

    // SAFETY: DefSubclassProc forwards to the next subclass / DefWindowProc;
    // we hand it the same parameters Windows handed to us.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

// ─── Attach / detach ────────────────────────────────────────────────

/// Bind the selected-stack Main HWND to the ghost-layer passthrough controller
/// without re-running legacy Tauri window surgery.
///
/// The selected-stack window factory already creates Main as a borderless DComp
/// overlay (`WS_POPUP`, `WS_EX_NOREDIRECTIONBITMAP`, normal non-topmost z-order).
/// The selected stack uses the benchmark's `HWND_BOTTOM` desktop-layer policy;
/// normal creation z-order alone can leave a newly launched no-activate window
/// above an already-open browser or editor. Startup still installs the
/// benchmark's lightweight WndProc subclass so Explorer cannot push Main
/// behind `WorkerW` during Show Desktop. The expensive legacy DWM/frame-
/// extension path remains skipped.
pub fn attach_selected_stack(hwnd: HWND) -> Result<(), GhostLayerError> {
    if hwnd.is_null() {
        return Err(GhostLayerError::NullHwnd);
    }

    let _ = MAIN_HWND.set(hwnd as usize);
    set_cursor_passthrough(true);
    CURSOR_PASSTHROUGH.store(true, Ordering::Relaxed);

    // SAFETY: `hwnd` is the live Main top-level window and the callback/id pair
    // is removed by `detach`. This is the same z-order guard used by the Tauri
    // benchmark, without legacy DWM non-client mutations.
    unsafe {
        let installed =
            SetWindowSubclass(hwnd, Some(overlay_subclass_proc), OVERLAY_SUBCLASS_ID, 0);
        if installed == 0 {
            tracing::warn!("ghost_layer: failed to install selected-stack z-order subclass");
        }
    }

    park_at_desktop_layer(hwnd, 0);
    WINDOW_VISIBLE.store(true, Ordering::Relaxed);
    tracing::info!(
        "ghost_layer: attached lightweight selected-stack binding at HWND_BOTTOM with z-order guard"
    );
    Ok(())
}

/// Configure an existing top-level HWND as a non-intrusive desktop overlay.
///
/// Steps (lifted from 1.x):
/// 1. Strip window decorations (`WS_CAPTION`, `WS_THICKFRAME`, ...).
/// 2. Set extended styles:
///    `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`.
///    The target HWND is created by the selected-stack window factory with
///    `WS_EX_NOREDIRECTIONBITMAP`; `attach` must never add `WS_EX_LAYERED`
///    because DirectComposition transparent windows require the two flags to
///    stay mutually exclusive.
/// 3. Install WndProc subclass for z-order protection + NC suppression.
/// 4. Disable DWM non-client rendering + extend frame fully into client area.
/// 5. Force frame refresh and park at `HWND_BOTTOM` in normal z-order.
/// 6. Show without activating.
/// 7. Re-apply WS_POPUP after a short delay (defends against external code
///    that re-touches the style — Tauri did this in 1.x; in native this still
///    catches anyone else who pokes the style).
pub fn attach(hwnd: HWND) -> Result<(), GhostLayerError> {
    if hwnd.is_null() {
        return Err(GhostLayerError::NullHwnd);
    }

    // Store the HWND for later show / hide / reposition calls.
    let _ = MAIN_HWND.set(hwnd as usize);

    // ── Step 1: Install WndProc subclass FIRST ────────────────────
    //
    // Must be installed BEFORE any style changes so that our `WM_NCCALCSIZE`
    // handler is active when Windows re-evaluates the window frame. The
    // `WM_NCCALCSIZE → return 0` trick tells Windows "this window has NO
    // non-client area" — it cannot be overridden by a later style restore.
    //
    // SAFETY: SetWindowSubclass with valid HWND and a `'static` extern fn.
    let install_ok = unsafe {
        let proc: SUBCLASSPROC = Some(overlay_subclass_proc);
        SetWindowSubclass(hwnd, proc, OVERLAY_SUBCLASS_ID, 0)
    };
    if install_ok == 0 {
        tracing::warn!("ghost_layer: SetWindowSubclass returned FALSE");
        return Err(GhostLayerError::SubclassInstallFailed);
    }

    // ── Step 2: Strip ALL window decorations ──────────────────────
    //
    // Set WS_POPUP and clear all frame-related flags. With the subclass
    // intercepting WM_NCCALCSIZE, the non-client area (title bar + borders)
    // is forcibly zeroed out.
    //
    // SAFETY: GetWindowLongPtrW/SetWindowLongPtrW with valid HWND.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let new_style = (style | WS_POPUP as isize)
            & !(WS_CAPTION as isize)
            & !(WS_THICKFRAME as isize)
            & !(WS_SYSMENU as isize)
            & !(WS_MINIMIZEBOX as isize)
            & !(WS_MAXIMIZEBOX as isize)
            & !(WS_BORDER as isize)
            & !(WS_DLGFRAME as isize);
        SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);

        // Force immediate frame recalculation — triggers WM_NCCALCSIZE which
        // our subclass intercepts to return 0 (no non-client area).
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }

    // ── Step 3: Set extended styles ───────────────────────────────
    //
    // SAFETY: GetWindowLongPtrW/SetWindowLongPtrW with valid HWND.
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_ex_style = ex_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);
    }
    CURSOR_PASSTHROUGH.store(true, Ordering::Relaxed);

    // ── Step 4: Disable DWM border / shadow / rounded corners ─────
    //
    // SAFETY: DwmSetWindowAttribute with valid HWND and pointers backed by
    // named locals.
    unsafe {
        // DWMNCRP_DISABLED = 1: no window chrome rendering. DWMWA_NCRENDERING_POLICY
        // exists on Vista+, so this stays UNGUARDED (must run on Win10).
        let disabled: u32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            &disabled as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // Fix #18 — DWMWA_WINDOW_CORNER_PREFERENCE (33) only exists on Windows 11
        // (build >= 22000 = Win11 RTM); on Win10 it returns E_INVALIDARG. Guard
        // the call so we don't invoke a build-specific DWM attribute on a build
        // that lacks it (Wave-G discipline).
        if system::windows_build() >= 22000 {
            // Disable Windows 11 rounded corners on the overlay window.
            let do_not_round: u32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &do_not_round as *const u32 as *const _,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    // ── Step 5: Force a non-moving frame refresh + HWND_BOTTOM ────
    //
    // W13-A restores the source benchmark's desktop-layer contract: Main is
    // above the shell at rest but below ordinary application windows.
    //
    park_at_desktop_layer(hwnd, SWP_FRAMECHANGED);

    // ── Step 6: Show without activating ───────────────────────────
    // SAFETY: ShowWindow with valid HWND.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    WINDOW_VISIBLE.store(true, Ordering::Relaxed);

    tracing::info!(
        "ghost_layer: attached preserving selected-stack HWND bounds — NOREDIRECTIONBITMAP | TOOLWINDOW | NOACTIVATE | TRANSPARENT_BY_DEFAULT | NO_DECORATIONS | HWND_BOTTOM | SUBCLASSED"
    );

    // ── Step 7: Deferred style re-apply ──────────────────────────
    //
    // External code (Tauri in 1.x; the multi-window pivot in native) may
    // re-apply window styles after our setup hook returns. Spawn a worker
    // that waits 500 ms and re-applies the WS_POPUP style + frame change.
    // The subclass's `WM_NCCALCSIZE` handler keeps the title bar gone
    // even between, but this second pass is a defensive net.
    let hwnd_raw = hwnd as usize;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let hwnd = hwnd_raw as HWND;
        // SAFETY: We stored the HWND at attach; the OS keeps it valid.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let new_style = (style | WS_POPUP as isize)
                & !(WS_CAPTION as isize)
                & !(WS_THICKFRAME as isize)
                & !(WS_SYSMENU as isize)
                & !(WS_MINIMIZEBOX as isize)
                & !(WS_MAXIMIZEBOX as isize)
                & !(WS_BORDER as isize)
                & !(WS_DLGFRAME as isize);
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);
            BYPASS_SUBCLASS.store(true, Ordering::Release);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            BYPASS_SUBCLASS.store(false, Ordering::Release);
            let final_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            tracing::info!(
                "ghost_layer: deferred style re-apply: 0x{:08X}",
                final_style
            );
        }
    });

    Ok(())
}

/// Detach the overlay by removing the WndProc subclass.
///
/// Idempotent — subsequent calls are no-ops.
pub fn detach(hwnd: HWND) -> Result<(), GhostLayerError> {
    if hwnd.is_null() {
        return Err(GhostLayerError::NullHwnd);
    }
    // SAFETY: RemoveWindowSubclass with valid HWND, the same SUBCLASSPROC
    // and id we used in `attach`.
    unsafe {
        let proc: SUBCLASSPROC = Some(overlay_subclass_proc);
        let _ = RemoveWindowSubclass(hwnd, proc, OVERLAY_SUBCLASS_ID);
    }
    tracing::info!("ghost_layer: detached (subclass removed)");
    Ok(())
}
