use super::*;

/// Per-HWND state. Holds the layout engine (cache lives here per Ruling 5)
/// and any future per-window scratch buffers. Constructed by the shell when
/// a window is created, destroyed at WM_DESTROY.
#[derive(Debug)]
pub struct WindowState {
    pub layout: LayoutEngine,
    /// `false` until `load_zones_once` has checked `app.zones_path`. The shell
    /// calls it before Main's first draw; `Renderer::render` retains a fallback
    /// for other consumers. Failure still flips this so paints never retry.
    pub loaded: Cell<bool>,
    /// Phase 2.3.1a — current device DPI for this HWND (PER_MONITOR_AWARE_V2).
    /// Updated by the shell on `WM_DPICHANGED` and seeded once after window
    /// creation via `GetDpiForWindow`. Default `96` matches the Win32 100%
    /// scale baseline, so a never-updated cache cannot accidentally produce
    /// half-size output. `Cell` (not `RefCell`) because `u32` is `Copy`.
    pub dpi: Cell<u32>,
    /// Phase 2.3.1a — cached enumeration of all attached monitors. Refreshed
    /// after window creation and again on every `WM_DPICHANGED` (because a
    /// DPI change typically coincides with a display reconfiguration). The
    /// 4-element inline capacity matches `bentodesk_platform::monitor`'s
    /// `enumerate_monitors` to keep the typical workstation case heap-free.
    /// Phase 2.4 will route zones to monitors against this cache.
    pub monitors: SmallVec<[MonitorInfo; 4]>,
    /// Wave 15 — Tier 0 #29/#31 one-shot guard. `false` until the first
    /// successful `Renderer::render` returns; the shell's WM_PAINT handler
    /// then calls `EmptyWorkingSet(GetCurrentProcess())` exactly once and
    /// flips this to `true`. Subsequent paints short-circuit so we never
    /// pay the working-set trim cost twice (re-enabling it on every paint
    /// would page-fault the next frame's hot resources back in).
    ///
    /// Reader: WM_PAINT handler in `bentodesk-shell/src/main.rs` (the same
    /// `if !first_paint_done.get()` site that issues the trim). Writer:
    /// the same site flips `set(true)` immediately after the trim returns.
    /// `Cell` (not `RefCell`) because `bool` is `Copy` and the WM_PAINT
    /// handler is single-threaded by Win32 message-pump contract.
    pub first_paint_done: Cell<bool>,
    /// Work-area/DPI/display changes schedule one group-aware geometry repair
    /// after the Main renderer has measured its new logical viewport.
    pub pending_zone_geometry_normalize: Cell<bool>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            layout: LayoutEngine::default(),
            loaded: Cell::new(false),
            // 96 DPI = 100% scale (Win32 USER_DEFAULT_SCREEN_DPI). Picked over
            // 0 so any code path reading `dpi` before WM_DPICHANGED / the
            // post-create seed gets a usable scale factor instead of dividing
            // through zero in the eventual Phase 2.3.1b scaling math.
            dpi: Cell::new(96),
            // Empty until the shell calls `enumerate_monitors()` post-create.
            // Phase 2.3.1b / 2.4 callers must tolerate the empty-cache window
            // between WM_NCCREATE and the first paint.
            monitors: SmallVec::new(),
            // Wave 15 — Tier 0 #29/#31 one-shot trim guard, defaults to
            // `false` so the very first WM_PAINT triggers the EmptyWorkingSet
            // call. After the first successful paint the shell flips this
            // to `true` and never trims again (re-trimming would just
            // page-fault hot resources back in on the next frame).
            first_paint_done: Cell::new(false),
            pending_zone_geometry_normalize: Cell::new(false),
        }
    }
}

impl WindowState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule one geometry normalization on the next Main paint.
    pub fn schedule_zone_geometry_normalize(&self) -> bool {
        !self.pending_zone_geometry_normalize.replace(true)
    }

    /// Consume the pending normalization flag exactly once.
    pub fn take_zone_geometry_normalize(&self) -> bool {
        self.pending_zone_geometry_normalize.replace(false)
    }

    /// Load persisted Zones once before this HWND's first draw.
    pub fn load_zones_once(&self, app: &mut AppState) {
        if self.loaded.replace(true) || app.zones_path.as_os_str().is_empty() {
            return;
        }
        match bentodesk_platform::storage::read_zones(&app.zones_path) {
            Ok(mut loaded) => {
                if loaded.repair_duplicate_item_ids() > 0 {
                    app.mark_dirty();
                }
                app.zones = loaded;
            }
            Err(bentodesk_platform::PlatformError::Storage(_)) => {
                let _ = bentodesk_platform::storage::quarantine_corrupt(&app.zones_path);
            }
            Err(_) => {
                // IO / permission / other — leave the file in place.
            }
        }
    }

    /// Run a layout pass over `app.tree` at `app.viewport`. Cached — see
    /// `LayoutEngine::layout_with_epoch` for the invalidation key.
    pub fn run_layout(&mut self, app: &AppState) -> Result<(), LayoutError> {
        self.layout.layout(&app.tree, app.viewport).map(|_| ())
    }

    /// Test-helper: construct a `WindowState` with a known monitor list
    /// pre-seeded. Production code populates `monitors` via `paint()`'s
    /// lazy-init seed (Ruling 4 / Wave 7) or the `WM_DPICHANGED` /
    /// `WM_DISPLAYCHANGE` handlers (Phase 2.4 / Ruling 1). Integration tests
    /// living in `tests/` cannot touch private fields, so this helper is
    /// the only sanctioned construction path that bypasses the empty-cache
    /// default. `#[doc(hidden)]` keeps it out of the public rustdoc surface
    /// while still being callable from cross-crate test harnesses.
    #[doc(hidden)]
    pub fn with_monitors_for_test(monitors: SmallVec<[MonitorInfo; 4]>) -> Self {
        Self {
            monitors,
            ..Self::default()
        }
    }
}
