//! Multi-window registry — keyed by `HWND`, holds per-window `WindowState +
//! Renderer + WindowComp + visibility tracking`.
//!
//! Phase 1 / T-008. Spec lock:
//!   §2  single process — every HWND lives on the UI thread that owns the
//!       message pump; no secondary threads.
//!   §10 hot-path no alloc — slot lookup is `iter().find` over a 1-8 element
//!       inline `SmallVec`; no `HashMap`, no `Box`, no allocation per frame.
//!   §11 R7 (master-decomposition) — at most 8 pinned `MiniBar` windows; the
//!       9th `register` of `WindowKind::MiniBar` is refused (debug-stream
//!       trace via `OutputDebugStringA`, no panic).
//!   §17 every field documented; no `todo!`, no stubs.
//!
//! ### Why `SmallVec<[WindowSlot; 8]>` (not `HashMap<HWND, WindowSlot>`)
//!
//! BentoDesk launches with one Main window and adds at most a handful of
//! popup / tooltip / minibar HWNDs at runtime. The §11 R7 ruling caps total
//! population at ≈ 8 visible windows (1 Main + ≤8 MiniBar + a couple of
//! transient popups during interaction). At those sizes a linear scan over
//! a stack-inlined buffer is faster than `HashMap` (no hash compute, no
//! heap), satisfies §10 (no allocation in steady state), and keeps the
//! whole registry on the cache line `WinState` already pays for.

use std::cell::Cell;

use bentodesk_platform::WindowKind;
use smallvec::SmallVec;
use windows_sys::Win32::Foundation::HWND;

use crate::render::{RenderError, Renderer};
use crate::state::{AppState, WindowState};

/// Per-HWND slot — owns every per-window resource (renderer + DComp visual
/// tree + layout cache + visibility tracking). Constructed by the shell at
/// `create_window`-time, dropped at `WM_DESTROY` via `WindowRegistry::unregister`.
///
/// Intentionally not `Debug` — the embedded `Renderer` carries COM interface
/// pointers that cannot be safely formatted; `WindowRegistry` exposes the
/// inspection surface (`len` / `count_kind` / `paint_err_total`) that
/// diagnostics actually need.
pub struct WindowSlot {
    /// The Win32 handle. Process-private; never null for a slot in the
    /// registry (registration validates non-null).
    pub hwnd: HWND,
    /// Categorical role — drives ex-style selection (T-011), z-order, focus
    /// behaviour, and the §11 R5 hibernation decision (Main never
    /// hibernates; everyone else may).
    pub kind: WindowKind,
    /// Per-window layout / DPI / monitor cache. Per Ruling 5 the
    /// `LayoutEngine` lives here, never on `AppState`.
    pub state: WindowState,
    /// Per-window D2D render target + DComp visual tree + per-frame scratch
    /// buffer. Promoted to per-window in T-009 — global singletons (D2D
    /// factory, D3D device, DWrite factory, DComp device) stay in
    /// `bentodesk-platform` `OnceLock`s.
    pub renderer: Renderer,
    /// `true` when the OS reports the window as visible (driven by
    /// `WM_SHOWWINDOW`). Used by T-099 hibernation to decide whether the
    /// swap chain backbuffer should be resident.
    pub is_visible: Cell<bool>,
    /// Wall-clock millisecond timestamp (`GetTickCount`) of the last
    /// `is_visible` flip. T-099 uses this to gate hibernation by 500 ms so
    /// fast hide/show cycles (e.g. menu re-shown immediately after dismiss)
    /// don't thrash the swap chain.
    pub last_visible_change_ms: Cell<u32>,
    /// Pending hibernation request — set by `WM_SHOWWINDOW(FALSE)` for
    /// non-Main windows; honoured during the next paint cycle once
    /// `last_visible_change_ms` is ≥ 500 ms in the past. `false` once the
    /// swap chain has actually been released, until the next hide.
    pub pending_hibernate: Cell<bool>,
    /// Per-window paint-error count over the lifetime of this slot.
    /// Read by tests + soak runs to verify the Phase 0 closure mandate
    /// (paint err must remain 0 for 60 s soak per window).
    pub paint_err: Cell<u32>,
}

impl WindowSlot {
    /// Construct a slot. `state.dpi` and `state.monitors` should be seeded
    /// by the caller (shell does this immediately after `create_window`)
    /// so the first paint has a usable DPI cache.
    pub fn new(hwnd: HWND, kind: WindowKind, state: WindowState, renderer: Renderer) -> Self {
        Self {
            hwnd,
            kind,
            state,
            renderer,
            is_visible: Cell::new(true),
            last_visible_change_ms: Cell::new(0),
            pending_hibernate: Cell::new(false),
            paint_err: Cell::new(0),
        }
    }

    /// Bump the paint-error counter. Called from the wndproc on a paint
    /// failure; the global aggregate is computed by `WindowRegistry::paint_err_total`.
    #[inline]
    pub fn note_paint_err(&self) {
        self.paint_err.set(self.paint_err.get().saturating_add(1));
    }

    /// Render one frame on this window. Mirrors the legacy
    /// `Renderer::render(&mut app, &mut win, kind)` call but folds the
    /// per-window state lookup so the wndproc can paint without
    /// hand-untangling field borrows. Increments `paint_err` on failure
    /// (T-010 reads the per-window count for diagnostics).
    pub fn paint(&mut self, app: &mut AppState) -> Result<(), RenderError> {
        let r = self.renderer.render(app, &mut self.state, self.kind);
        if r.is_err() {
            self.note_paint_err();
        }
        r
    }

    /// Mark the slot as shown / hidden + record the timestamp. T-099 calls
    /// this from the `WM_SHOWWINDOW` arm; the registry's hibernation pass
    /// reads `pending_hibernate` + `last_visible_change_ms`.
    pub fn set_visible(&self, visible: bool, now_ms: u32) {
        let prev = self.is_visible.replace(visible);
        if prev == visible {
            return;
        }
        self.last_visible_change_ms.set(now_ms);
        if !visible && self.kind != WindowKind::Main {
            self.pending_hibernate.set(true);
        } else {
            // Becoming visible cancels any pending hibernation.
            self.pending_hibernate.set(false);
        }
    }
}

/// Process-wide HWND → slot mapping. Owns every `WindowSlot` via `Box` so
/// each slot has a stable heap address (the wndproc stashes `*mut WindowSlot`
/// in `GWLP_USERDATA`, and that pointer must survive subsequent
/// `register` / `unregister` calls — a bare `SmallVec<[WindowSlot; N]>`
/// would invalidate prior pointers on `push` past the inline cap).
///
/// The outer `SmallVec<[Box<WindowSlot>; 12]>` keeps the steady-state lookup
/// path heap-free (the box-pointer array stays inline up to 12 entries =
/// §11 R7 cap of 8 MiniBars + 1 Main + ≤3 transient popups, exhausting the
/// realistic runtime population).
pub struct WindowRegistry {
    windows: SmallVec<[Box<WindowSlot>; 12]>,
}

impl core::fmt::Debug for WindowRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Custom Debug avoids the COM-pointer formatting issue in `Renderer`
        // while still surfacing the bookkeeping reviewers care about.
        f.debug_struct("WindowRegistry")
            .field("len", &self.windows.len())
            .field("paint_err_total", &self.paint_err_total())
            .finish()
    }
}

/// Soft cap on `WindowKind::MiniBar` registrations. Any 9th MiniBar `register`
/// is refused (debug trace via `OutputDebugStringA`); existing minibars
/// continue to function. Per master-decomposition §11 R7.
pub const MAX_MINIBARS: usize = 8;

impl Default for WindowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self {
            windows: SmallVec::new(),
        }
    }

    /// Register a new window. Returns `Some(*mut WindowSlot)` on success
    /// (stable pointer to the heap-boxed slot — caller stashes it in the
    /// HWND's `GWLP_USERDATA` so the wndproc can find it back). Returns
    /// `None` when the registration was refused (currently: only the §11 R7
    /// MiniBar cap).
    ///
    /// Refusal is a non-fatal soft limit: the existing minibars keep
    /// running, the new window simply isn't added. Spec §11 init-path
    /// discipline — degrade rather than panic.
    pub fn register(&mut self, slot: WindowSlot) -> Option<*mut WindowSlot> {
        if slot.kind == WindowKind::MiniBar && self.count_kind(WindowKind::MiniBar) >= MAX_MINIBARS
        {
            // §11 R7 cap. Trace once to the debug stream; production
            // launches under `/SUBSYSTEM:WINDOWS` so stderr is unavailable
            // and `OutputDebugStringA` is the documented diagnostic path.
            log_minibar_cap_refused();
            return None;
        }
        let mut boxed = Box::new(slot);
        let raw: *mut WindowSlot = boxed.as_mut();
        self.windows.push(boxed);
        Some(raw)
    }

    /// Drop the slot for `hwnd`. Called from `WM_DESTROY`. Returns `true` if
    /// a slot was removed; `false` if the HWND wasn't tracked (idempotent).
    /// Caller MUST clear the HWND's `GWLP_USERDATA` before this call —
    /// otherwise a delayed wndproc dispatch on the destroyed HWND would
    /// follow a dangling pointer.
    pub fn unregister(&mut self, hwnd: HWND) -> bool {
        let pos = self.windows.iter().position(|w| w.hwnd == hwnd);
        match pos {
            Some(i) => {
                let _ = self.windows.swap_remove(i);
                true
            }
            None => false,
        }
    }

    /// Look up a slot by HWND. Linear scan over a 12-element inline buffer
    /// (typical runtime population is 1-4) — measurably faster than
    /// `HashMap::get` at this size and §10-clean (no allocation).
    pub fn get(&self, hwnd: HWND) -> Option<&WindowSlot> {
        self.windows
            .iter()
            .find(|w| w.hwnd == hwnd)
            .map(|b| b.as_ref())
    }

    pub fn get_mut(&mut self, hwnd: HWND) -> Option<&mut WindowSlot> {
        self.windows
            .iter_mut()
            .find(|w| w.hwnd == hwnd)
            .map(|b| b.as_mut())
    }

    /// Iterate all live slots. Used by:
    ///   * the hibernation pass (T-099) to find slots whose
    ///     `pending_hibernate` flag has aged past the 500 ms gate, and
    ///   * the soak harness to read `paint_err` totals.
    pub fn iter(&self) -> impl Iterator<Item = &WindowSlot> {
        self.windows.iter().map(|b| b.as_ref())
    }

    /// Mutable iteration — used by the hibernation pass (T-099) which needs
    /// `&mut Renderer` to drop the swap chain.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut WindowSlot> {
        self.windows.iter_mut().map(|b| b.as_mut())
    }

    /// Number of slots currently registered for `kind`. The §11 R7 cap is
    /// enforced via `count_kind(MiniBar) < MAX_MINIBARS` at register time.
    pub fn count_kind(&self, kind: WindowKind) -> usize {
        self.windows.iter().filter(|w| w.kind == kind).count()
    }

    /// Total registered window count. Convenience for diagnostics + soak.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Aggregate paint-error count across every slot. Used by the
    /// 4-metric report (paint err is the §17 success-or-fail signal).
    pub fn paint_err_total(&self) -> u64 {
        self.windows.iter().map(|w| w.paint_err.get() as u64).sum()
    }
}

/// Single-line debug trace — fires once when the §11 R7 MiniBar cap refuses
/// a registration. Stack-only (`SmallVec<[u8; 96]>`), §10 hot-path safe.
/// Compiled into release builds: tooling occasionally needs to confirm the
/// cap fired in the field, and the cost is one branch + one syscall on a
/// rare event.
fn log_minibar_cap_refused() {
    use core::fmt::Write as _;
    use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;

    let mut buf: SmallVec<[u8; 96]> = SmallVec::new();
    struct SvWriter<'a>(&'a mut SmallVec<[u8; 96]>);
    impl core::fmt::Write for SvWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            self.0.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }
    let _ = write!(
        SvWriter(&mut buf),
        "BentoDesk: ignoring pin_zone — already at {MAX_MINIBARS} minibar limit\0"
    );
    // SAFETY: buf ends in an explicit NUL byte (the `\0` in the format
    //         string above), satisfying OutputDebugStringA's C-string
    //         contract. Pointer is valid for the duration of the call.
    unsafe { OutputDebugStringA(buf.as_ptr()) };
}

// Unit tests are deferred to integration_smoke (real HWND + Renderer required).
// `WindowSlot` owns COM resources whose Drop dereferences live D2D pointers,
// so synthesising a fake slot via `mem::zeroed` would invoke UB at scope exit.
// The behaviour exercised here (count_kind / minibar cap / set_visible) is
// covered end-to-end by the multi-window smoke harness once T-010 lands.
