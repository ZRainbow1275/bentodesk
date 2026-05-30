//! Win32 window creation wrapper.
//!
//! Spec §4.1: per-pixel-alpha transparent windows MUST use
//! `WS_EX_NOREDIRECTIONBITMAP` ALONE — combining with `WS_EX_LAYERED` silently
//! breaks DComp composition. Helper makes the correct choice non-bypassable.
//!
//! Phase 1 / T-011 — adds the multi-window factory `create_window(kind, parent)`
//! plus the `WindowKind` taxonomy that drives ex-style selection, default
//! sizing, focus / activation behaviour, and §11 R5 hibernation eligibility.
//! `create_transparent_window` remains for the legacy single-Main-window path
//! (the BentoDesk minibar uses it) until T-077 / T-078 pull MiniBar / Settings
//! through the factory.

use std::ptr;

use windows_sys::Win32::Foundation::{HMODULE, HWND};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, HCURSOR, HICON, IDC_ARROW, LoadCursorW, RegisterClassExW,
    SW_SHOWNORMAL, ShowWindow, WNDCLASSEXW, WNDPROC, WS_CAPTION, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    WS_POPUPWINDOW, WS_VISIBLE,
};

use crate::errors::PlatformError;

/// Categorical role of a window. Drives ex-style selection (`ex_style_for`),
/// default geometry (`default_size`), focus/activation behaviour, intended
/// z-order, and the §11 R5 hibernation decision.
///
/// Variant cheat-sheet (intended behaviour, enforced by `ex_style_for`):
///
/// | Kind          | ex-style highlights                                  | Activation | Z-order      | Hibernates? |
/// |---------------|------------------------------------------------------|------------|--------------|-------------|
/// | `Main`        | NoRedirectionBitmap + Topmost (fully transparent overlay) | normal | topmost      | NEVER       |
/// | `IconPicker`  | NoRedirectionBitmap                                  | accepts    | above main   | yes         |
/// | `CapsulePicker`| NoRedirectionBitmap                                 | accepts    | above main   | yes         |
/// | `ContextMenu` | NoRedirectionBitmap + Topmost + NoActivate           | NO steal   | topmost      | yes         |
/// | `Tooltip`     | ToolWindow + NoActivate + Transparent (click-thru)   | NO steal   | topmost      | yes         |
/// | `DragPreview` | NoRedirectionBitmap + Layered + Transparent + Topmost| NO steal   | topmost      | yes         |
/// | `Settings`    | NoRedirectionBitmap (popup-window + caption)         | accepts    | normal-modal | yes         |
/// | `About`       | NoRedirectionBitmap (popup-window + caption)         | accepts    | normal-modal | yes         |
/// | `MiniBar`     | NoActivate + Topmost + Layered                       | NO steal   | topmost      | yes         |
/// | `Timeline`    | NoRedirectionBitmap                                  | accepts    | normal-modal | yes         |
/// | `SnapshotPicker`| NoRedirectionBitmap                                | accepts    | normal-modal | yes         |
/// | `Search`      | NoRedirectionBitmap                                  | accepts    | normal-modal | yes         |
///
/// `DragPreview` and `MiniBar` use `WS_EX_LAYERED` instead of NoRedirectionBitmap
/// because they need cursor-following alpha animation (the `WS_EX_LAYERED` path
/// is the only Win32 transparent-window mode that also follows the cursor
/// without DComp visual tree work). See §4.1: NoRedirectionBitmap + Layered are
/// **mutually exclusive** — picking one or the other per kind avoids that trap.
///
/// ΔB ruling (master-decomposition §11): `serde::Serialize + Deserialize`
/// derives are forward-compat surface for the v2.x scripting / plugin
/// re-introduction. The Phase-1 single-process build never serializes
/// `WindowKind` at runtime; the `bento-nano-app::dispatcher::Command`
/// `ShowWindow / HideWindow` variants carry it through the in-process bus
/// only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WindowKind {
    /// Primary application window. Fully transparent fullscreen overlay
    /// — no DWM Mica/Acrylic, no full-screen scrim. Per-pill + per-modal
    /// surfaces paint their own translucent dark in `Renderer::draw_zones`
    /// and `Renderer::draw_settings_panel`. Accepts focus, `HWND_TOPMOST`
    /// z-order (Wave H1 — keeps zone pills above foreground apps the way
    /// the Tauri 1.2.4 baseline did). Never hibernates (always-resident
    /// swap chain).
    Main,
    /// Icon picker — full-grid widget gallery for assigning icons to zones.
    /// Accepts focus, sized 1280×720, hibernates when dismissed.
    IconPicker,
    /// Capsule picker — compact app-launcher style picker. Accepts focus,
    /// 480×600, hibernates on dismiss.
    CapsulePicker,
    /// Right-click context menu — small floating panel. Topmost,
    /// NoActivate so the menu doesn't steal focus from the owner window
    /// when it appears. 240×200 default.
    ContextMenu,
    /// Hover tooltip — tiny click-through label. Topmost, NoActivate,
    /// Transparent (mouse passes straight through). 200×40 default.
    Tooltip,
    /// Drag-and-drop visual feedback — semi-transparent thumbnail that
    /// follows the cursor during a drag. Topmost, transparent, layered
    /// (alpha animation needs the layered surface). 64×64 default.
    DragPreview,
    /// Settings panel as a standalone HWND (vs the in-place overlay used
    /// today). Popup-window with caption; modal-style click-outside
    /// dismissal handled by the shell. 800×600 default.
    Settings,
    /// About dialog. Popup-window with caption; the runtime content is drawn
    /// by `bento-nano-app::Renderer::draw_about_panel`. 360×280 default.
    About,
    /// Pinned-zone mini bar — one HWND per pinned zone. Topmost,
    /// NoActivate. §11 R7 caps the count at 8 (registry refuses the 9th).
    /// 280×80 default per 1.x parity.
    MiniBar,
    /// Colour-palette picker — small grid of swatches for theming a zone.
    /// Modal-style popup with caption. 320×240 default.
    PalettePicker,
    /// Multi-step rules wizard — drives the auto-grouping rules editor.
    /// Modal-style popup with caption. 640×480 default.
    RulesWizard,
    /// Bulk-action manager — table panel for batch zone / item operations.
    /// Modal-style popup with caption. 720×540 default.
    BulkManager,
    /// Single-zone form editor — name / icon / size / position fields.
    /// Modal-style popup with caption. 480×360 default.
    ZoneEditor,
    /// Single-item filesystem rename dialog.
    ItemFileRename,
    /// Smart-group suggestor — list of suggested groupings the user can
    /// accept or dismiss. Modal-style popup with caption. 480×320 default.
    Suggestor,
    /// Timeline panel — checkpoint list, preview, restore, undo, redo, and
    /// manual save controls. Modal-style popup with caption. 820×600 default.
    Timeline,
    /// Snapshot picker — saved layout snapshot list with save/load/delete.
    /// Modal-style popup with caption. 520×520 default.
    SnapshotPicker,
    /// Global SearchBar. Modal-style popup with caption. 620x540 default.
    Search,
}

impl WindowKind {
    /// `true` for windows whose swap chain may be released when hidden
    /// (§11 R5 hibernation). The `Main` window is the sole exception —
    /// dropping its backbuffer would force a 16 ms re-allocation on every
    /// Show / Hide of the tray, defeating the cold-start budget.
    #[inline]
    pub fn hibernation_eligible(self) -> bool {
        !matches!(self, Self::Main)
    }
}

/// Default device-pixel size (width, height) for a freshly created window of
/// `kind`. All values are at 96 DPI baseline; the shell scales further if the
/// monitor reports a higher DPI. Numbers come from the master-decomposition
/// T-011 spec.
///
/// Wave C (05-20 visual parity) — `WindowKind::Main` now reports a fallback
/// 1920×1080 desktop baseline only. Real Main HWND creation calls
/// [`main_window_rect`] which queries `MonitorFromPoint` for the primary
/// monitor work area at runtime. This constant survives as a synthetic
/// viewport sentinel for `bento-nano-app::startup_layout_viewport` when no
/// HWND has been seeded yet (cold-start layout migration).
pub fn default_size(kind: WindowKind) -> (i32, i32) {
    match kind {
        WindowKind::Main => (1920, 1080),
        WindowKind::IconPicker => (1280, 720),
        WindowKind::CapsulePicker => (480, 600),
        WindowKind::ContextMenu => (240, 200),
        WindowKind::Tooltip => (200, 40),
        WindowKind::DragPreview => (64, 64),
        WindowKind::Settings => (800, 600),
        WindowKind::About => (360, 280),
        WindowKind::MiniBar => (280, 80),
        WindowKind::PalettePicker => (320, 240),
        WindowKind::RulesWizard => (640, 480),
        WindowKind::BulkManager => (720, 540),
        WindowKind::ZoneEditor => (480, 360),
        WindowKind::ItemFileRename => (520, 240),
        WindowKind::Suggestor => (640, 560),
        WindowKind::Timeline => (820, 600),
        WindowKind::SnapshotPicker => (520, 520),
        WindowKind::Search => (620, 540),
    }
}

/// Primary-monitor work-area rectangle (x, y, width, height) in virtual-screen
/// device pixels. Wave C (05-20 visual parity) — Main HWND covers the primary
/// monitor's work area so zone pills can sit on the desktop with click-through
/// on transparent regions (parent PRD D1/D6). Falls back to
/// `default_size(WindowKind::Main)` at origin (0,0) when monitor enumeration
/// fails (headless test harness, unusual VM configuration).
///
/// Spec §11 — total: never panics, always returns a usable rectangle. The
/// returned position is in virtual-screen coordinates (negative origins are
/// possible on secondary-monitor primaries; we still target the primary so
/// usually `(0, 0)` on a single-display workstation).
pub fn main_window_rect() -> (i32, i32, i32, i32) {
    let primary = crate::monitor::primary_monitor();
    let work = primary.rect_work;
    let width = work.width();
    let height = work.height();
    if width > 0 && height > 0 {
        return (work.left, work.top, width, height);
    }
    let (fallback_w, fallback_h) = default_size(WindowKind::Main);
    (0, 0, fallback_w, fallback_h)
}

/// Compute the (`ex_style`, `style`) bitmask pair for `kind`. Centralised so
/// the §4.1 NoRedirectionBitmap + Layered mutual-exclusion rule has exactly
/// one decision site (verifiable by review).
pub fn ex_style_for(kind: WindowKind) -> (u32, u32) {
    match kind {
        WindowKind::Main => {
            // Spec §4.1 — DComp composition path. NoRedirectionBitmap is the
            // single correct flag for per-pixel-alpha transparent windows;
            // never combine with Layered.
            //
            // Wave C (05-20 visual parity) — Main HWND is a borderless
            // fullscreen overlay sitting at the primary-monitor work area.
            // `WS_POPUP` strips the caption / sysmenu / resize border so the
            // entire client area is a hit-testable transparent canvas, and
            // `WM_NCHITTEST` (see `bento-nano-shell::ui::main_nchittest_kind`)
            // returns `HTTRANSPARENT` for empty pixels so Explorer keeps
            // receiving desktop-icon clicks.
            //
            // Wave H1 (2026-05-20) — `WS_EX_TOPMOST` lifts the overlay above
            // all normal windows so the user always sees the zone pills, the
            // way the Tauri 1.2.4 baseline did. Blank-pixel click-through is
            // preserved by the `WS_EX_TRANSPARENT` toggle managed by
            // `apply_ghost_cursor_passthrough_for_point` (the wndproc subclass
            // adds the flag on attach and the per-frame timer toggles it off
            // when the cursor is over a real surface).
            //
            // V-10 (2026-05-21) — `WS_EX_TRANSPARENT` is now part of the
            // initial ex_style so the overlay starts click-through. Without
            // this, the gap between `CreateWindowExW` and the first
            // `apply_ghost_cursor_passthrough_for_point` tick (driven by the
            // ghost-layer WM_TIMER ~16ms cadence) leaves the Main HWND opaque
            // to input — Explorer cannot receive desktop-icon clicks during
            // launch. The per-frame timer still *clears* the flag whenever the
            // cursor sits over a real surface (pill / expanded zone), so the
            // semantic is inverted: default = transparent, real-surface =
            // opaque. See `set_cursor_passthrough` in
            // `bento-nano-backend::ghost_layer::manager`.
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::IconPicker | WindowKind::CapsulePicker => {
            // Picker windows accept focus (user types / clicks selection).
            // WS_POPUP + NoRedirectionBitmap = focusable transparent surface.
            (WS_EX_NOREDIRECTIONBITMAP, WS_POPUP | WS_VISIBLE)
        }
        WindowKind::ContextMenu => {
            // Menus must NOT steal focus from the owner: NoActivate keeps the
            // owner's caret intact. Topmost so the menu paints above the
            // BentoCard. NoRedirectionBitmap is still fine here (DComp paint).
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::Tooltip => {
            // ToolWindow keeps the tooltip out of the alt-tab list and
            // task-bar. NoActivate + Transparent = mouse events pass to the
            // window underneath (canonical tooltip behaviour). We deliberately
            // OMIT NoRedirectionBitmap here — Transparent + ToolWindow expects
            // the GDI/redirection-bitmap path; combining with
            // NoRedirectionBitmap silently breaks click-through (§4.1).
            (
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOPMOST,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::DragPreview => {
            // Drag preview wants alpha-animatable cursor follower. Layered
            // is the only Win32 transparent-window mode that supports the
            // `UpdateLayeredWindow` cursor-snap pattern, so we use it
            // EXCLUSIVELY of NoRedirectionBitmap (§4.1 mutex).
            (
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::Settings => {
            // V-1 (TL ruling 2026-05-21) — Settings is a Tauri-style borderless
            // modal: panel paints its own header (设置 + ×) per frame_060, so
            // the OS caption + border + sysmenu must be off. NoRedirectionBitmap
            // keeps DComp composition active for the floating dark card chrome
            // + drop-shadow ring; WS_EX_TOPMOST keeps it above foreground apps
            // (Wave H4 ruling carry-over). Bare WS_POPUP means no caption, no
            // sysmenu, no resize border — exactly what frame_060 shows.
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::About
        | WindowKind::PalettePicker
        | WindowKind::RulesWizard
        | WindowKind::BulkManager
        | WindowKind::ZoneEditor
        | WindowKind::ItemFileRename
        | WindowKind::Suggestor
        | WindowKind::Timeline
        | WindowKind::SnapshotPicker
        | WindowKind::Search => {
            // Modal-style wizard/overlay dialogs that still want a native OS
            // caption + sysmenu (legacy paths not yet migrated to a Tauri-style
            // borderless shell). WS_POPUPWINDOW = WS_POPUP | WS_BORDER |
            // WS_SYSMENU; add WS_CAPTION so the title bar shows.
            // NoRedirectionBitmap keeps DComp composition active for the panel
            // chrome (Mica-style fill once theme picks it up).
            //
            // Wave H4 (2026-05-20) — `WS_EX_TOPMOST` so they stay above active
            // foreground apps (modal-dialog UX).
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
                WS_POPUPWINDOW | WS_CAPTION | WS_VISIBLE,
            )
        }
        WindowKind::MiniBar => {
            // Pinned MiniBar follows the same alpha-animatable pattern as
            // DragPreview but without TRANSPARENT (the user must be able
            // to click the bar to interact with the zone). Layered + Topmost
            // + NoActivate keeps it floating without focus theft.
            (
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                WS_POPUP | WS_VISIBLE,
            )
        }
    }
}

/// Window-creation parameters. `class_name` and `title` MUST be NUL-terminated
/// UTF-16 slices (use the `utf16_z!` helper or hand-rolled `&[u16]`).
#[derive(Clone, Copy)]
pub struct WindowDesc<'a> {
    pub class_name: &'a [u16],
    pub title: &'a [u16],
    pub width: i32,
    pub height: i32,
    /// Initial position (`CW_USEDEFAULT` = let OS decide).
    pub x: i32,
    pub y: i32,
    /// Caller-supplied wndproc. Must use `extern "system"` calling convention.
    pub wnd_proc: WNDPROC,
    /// Topmost flag — sets `WS_EX_TOPMOST` in addition to NoRedirectionBitmap.
    /// Only consulted by `create_transparent_window` (legacy path); the
    /// kind-driven `create_window` reads topmost from `ex_style_for`.
    pub topmost: bool,
    /// Categorical role. T-011 / multi-window pivot. `Main` is the default
    /// so existing callers (`bento-nano-shell::main`) keep their semantics.
    pub kind: WindowKind,
}

impl<'a> WindowDesc<'a> {
    /// New desc with default 480x320 size (BentoDesk minibar footprint),
    /// OS-chosen position, non-topmost. Spec §1 RSS lever — the smaller
    /// default keeps the DXGI swap chain backbuffer commit minimal until the
    /// caller picks an explicit size.
    pub fn new(class_name: &'a [u16], title: &'a [u16], wnd_proc: WNDPROC) -> Self {
        Self {
            class_name,
            title,
            width: 480,
            height: 320,
            x: CW_USEDEFAULT,
            y: CW_USEDEFAULT,
            wnd_proc,
            topmost: false,
            kind: WindowKind::Main,
        }
    }

    /// Builder — descriptor pre-populated with the per-kind default size and
    /// kind tag. Caller may still override width/height/position via the
    /// public fields before passing to `create_window`.
    pub fn for_kind(
        class_name: &'a [u16],
        title: &'a [u16],
        wnd_proc: WNDPROC,
        kind: WindowKind,
    ) -> Self {
        let (w, h) = default_size(kind);
        Self {
            class_name,
            title,
            width: w,
            height: h,
            x: CW_USEDEFAULT,
            y: CW_USEDEFAULT,
            wnd_proc,
            topmost: matches!(
                kind,
                WindowKind::ContextMenu
                    | WindowKind::Tooltip
                    | WindowKind::DragPreview
                    | WindowKind::MiniBar
            ),
            kind,
        }
    }

    /// Fluent override for the explicit width × height (device pixels @ 96 DPI).
    pub fn with_size(mut self, w: i32, h: i32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
}

/// Register the window class and create a per-pixel-alpha transparent window.
/// Returns the raw `windows-sys` HWND (caller responsible for storing state
/// via `GWLP_USERDATA` if needed).
///
/// Legacy single-window path. The BentoDesk Main window still arrives via
/// here (via the `kind = WindowKind::Main` default in `WindowDesc::new`).
/// New popups / pickers / minibars should use `create_window` instead, which
/// dispatches on `desc.kind`.
pub fn create_transparent_window(desc: &WindowDesc<'_>) -> Result<HWND, PlatformError> {
    create_window(desc, ptr::null_mut())
}

/// Register the window class (idempotent — classes are process-wide and
/// `RegisterClassExW` returns ERROR_CLASS_ALREADY_EXISTS on the second call,
/// which we silently absorb) and create the window with the kind-driven
/// ex-style + style pair. `parent` is the owner HWND (popups / tooltips /
/// menus); `null_mut()` for top-level windows.
///
/// SAFETY discipline: every Win32 call here is documented + has its failure
/// path translated to `PlatformError` (no `unwrap`, no `panic`).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // HWND is the Win32 ABI's opaque
// owner-handle slot; the OS resolves
// it on `CreateWindowExW`'s side. We
// never dereference `parent` here, and
// `null_mut()` is the documented
// top-level sentinel — marking the
// factory `unsafe` would force every
// caller (shell + future popup code)
// into an unsafe block for no extra
// safety invariant.
pub fn create_window(desc: &WindowDesc<'_>, parent: HWND) -> Result<HWND, PlatformError> {
    // SAFETY: GetModuleHandleW(null) returns the .exe module; always succeeds.
    let hinst: HMODULE = unsafe { GetModuleHandleW(ptr::null()) };
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: desc.wnd_proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: 0 as HICON,
        // SAFETY: LoadCursorW with null hinst + system IDC_ARROW always valid.
        hCursor: unsafe { LoadCursorW(0 as HMODULE, IDC_ARROW) } as HCURSOR,
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: desc.class_name.as_ptr(),
        hIconSm: 0 as HICON,
    };
    // SAFETY: `class` fully initialised; pointer lives for the call.
    let atom = unsafe { RegisterClassExW(&class) };
    // ERROR_CLASS_ALREADY_EXISTS (1410) is benign — the class was registered
    // by a previous call (typical when the same kind launches multiple
    // windows). Any other zero-return is a real failure.
    if atom == 0 {
        let code = last_err();
        const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
        if code != ERROR_CLASS_ALREADY_EXISTS {
            return Err(PlatformError::Win32 {
                ctx: "RegisterClassExW",
                code,
            });
        }
    }

    let (ex_style, style) = ex_style_for(desc.kind);
    // Spec §4.1 — NoRedirectionBitmap is the *only* per-pixel-alpha path.
    // Combining with WS_EX_LAYERED silently breaks DComp. The matrix in
    // `ex_style_for` enforces this: every kind picks ONE of NoRedirectionBitmap
    // OR Layered, never both. Reviewers verify the matrix, not each call site.
    debug_assert!(
        !((ex_style & WS_EX_NOREDIRECTIONBITMAP) != 0 && (ex_style & WS_EX_LAYERED) != 0),
        "WindowKind::{:?} requested NoRedirectionBitmap + Layered (mutex per §4.1)",
        desc.kind
    );

    let create_dpi = creation_dpi(parent);
    let create_width = scale_96_dpi_dimension(desc.width, create_dpi);
    let create_height = scale_96_dpi_dimension(desc.height, create_dpi);

    // SAFETY: CreateWindowExW with documented flags; class registered above.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            desc.class_name.as_ptr(),
            desc.title.as_ptr(),
            style,
            desc.x,
            desc.y,
            create_width,
            create_height,
            parent,
            ptr::null_mut(),
            hinst,
            ptr::null_mut(),
        )
    };
    if hwnd.is_null() {
        return Err(PlatformError::Win32 {
            ctx: "CreateWindowExW",
            code: last_err(),
        });
    }

    // Wave G1 visual-parity fix — Tauri baseline (video review of
    // `屏幕录制 2026-05-20 161936.mp4`) shows the desktop wallpaper through
    // a fully transparent overlay; only individual zone pills + the
    // Settings modal paint their own translucent dark surfaces. DWM Mica
    // blends a system-tinted acrylic underneath the DComp swap chain
    // (light-theme hosts get a cream/white tint that leaks through alpha=0
    // D2D paint and produces a whiteboard look), so the attribute is no
    // longer set for the Main HWND.

    // Fix #17 — legacy caption-bearing dialogs (About / PalettePicker /
    // RulesWizard / BulkManager / ZoneEditor / ItemFileRename / Suggestor /
    // Timeline / SnapshotPicker / Search; see `ex_style_for`) carry a native
    // OS title bar. BentoDesk Nano is dark-themed, but on Windows 10 1809+
    // those captions render light/white by default — a visible mismatch
    // against the dark panels. Gate the immersive-dark-caption attribute on
    // the actual `WS_CAPTION` style bit (self-maintaining — any future
    // caption kind picks it up automatically) rather than a hard-coded kind
    // list. Borderless windows (Main / Settings / pickers / minibar) have no
    // caption to darken, so the call is skipped for them.
    if (style & WS_CAPTION) != 0 {
        apply_immersive_dark_caption(hwnd);
    }

    // SAFETY: hwnd valid; SW_SHOWNORMAL standard cmd.
    unsafe { ShowWindow(hwnd, SW_SHOWNORMAL) };
    Ok(hwnd)
}

/// Set `DWMWA_USE_IMMERSIVE_DARK_MODE = TRUE` on a caption-bearing HWND so the
/// native OS title bar renders dark, matching BentoDesk Nano's dark theme.
///
/// The attribute id changed across Windows 10 builds: 20H1 (build 19041+) and
/// Windows 11 use `20` (the documented `DWMWA_USE_IMMERSIVE_DARK_MODE`), while
/// builds 1809–1903 (17763–18362) accepted the *undocumented* pre-20H1 id `19`.
/// We try `20` first and, on an `HRESULT` failure (`hr < 0`, since `S_OK == 0`),
/// retry with the raw `19`. Both failures are swallowed — pre-1809 / unsupported
/// hosts simply keep the default caption, which is harmless.
///
/// No build-number probe is needed: the `HRESULT` self-reports support. We also
/// deliberately do NOT consult the theme crate (platform must not depend on
/// theme/app — layering). The app is dark-themed unconditionally today; a future
/// light theme would revisit this gate.
#[cfg(windows)]
fn apply_immersive_dark_caption(hwnd: HWND) {
    // BOOL TRUE — DWM reads `cbAttribute` bytes from `pvAttribute`.
    let enabled: u32 = 1;
    // Pre-20H1 undocumented id; no clean const exists in windows-sys 0.59.
    const DWMWA_USE_IMMERSIVE_DARK_MODE_PRE_20H1: u32 = 19;

    // SAFETY: `hwnd` is the freshly created, non-null HWND from `CreateWindowExW`
    // above. `&enabled` points to a live `u32` for the duration of the call and
    // `cbAttribute` matches its size. `DwmSetWindowAttribute` is a no-op + error
    // return on builds that lack the attribute; we never deref the result.
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &enabled as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    if hr < 0 {
        // Windows 10 1809–1903 (17763–18362) used the undocumented `19` id.
        // SAFETY: identical contract to the call above; same live `&enabled`.
        let _ = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE_PRE_20H1,
                &enabled as *const u32 as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
    }
}

/// Non-Windows stub — no DWM, so a dark caption is a no-op. Keeps the
/// `create_window` call site compiling on non-Windows targets.
#[cfg(not(windows))]
fn apply_immersive_dark_caption(_hwnd: HWND) {}

/// Bridge a `windows-sys` HWND (raw `*mut c_void`) into a `windows` HWND
/// newtype. Both wrap the same pointer; this keeps callers honest about the
/// crate split (spec §3.1.1).
#[inline]
pub fn to_windows_hwnd(h: HWND) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(h)
}

#[inline]
fn last_err() -> u32 {
    // SAFETY: GetLastError is always callable.
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

fn creation_dpi(parent: HWND) -> u32 {
    // Mc-1a — DPI APIs are soft-loaded via `crate::dpi` (GetProcAddress) so the
    // shell EXE no longer carries a static import of `GetDpiForSystem` /
    // `GetDpiForWindow` (absent on Win10 <1607 / Win8.1 / 7 → load failure).
    let dpi = if parent.is_null() {
        crate::dpi::get_dpi_for_system()
    } else {
        crate::dpi::get_dpi_for_window(parent)
    };
    if dpi == 0 { 96 } else { dpi }
}

fn scale_96_dpi_dimension(value: i32, dpi: u32) -> i32 {
    if value <= 0 {
        return value;
    }
    let dpi = i64::from(if dpi == 0 { 96 } else { dpi });
    let scaled = (i64::from(value) * dpi + 95) / 96;
    if scaled > i64::from(i32::MAX) {
        i32::MAX
    } else {
        scaled as i32
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CW_USEDEFAULT, WindowKind, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        WS_POPUP, default_size, ex_style_for, main_window_rect, scale_96_dpi_dimension,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::WS_OVERLAPPEDWINDOW;

    #[test]
    fn scale_96_dpi_dimension_keeps_96_dpi_identity() {
        assert_eq!(scale_96_dpi_dimension(480, 96), 480);
        assert_eq!(scale_96_dpi_dimension(360, 96), 360);
    }

    #[test]
    fn scale_96_dpi_dimension_expands_scaled_desktops() {
        assert_eq!(scale_96_dpi_dimension(480, 144), 720);
        assert_eq!(scale_96_dpi_dimension(360, 144), 540);
    }

    #[test]
    fn scale_96_dpi_dimension_preserves_sentinels() {
        assert_eq!(scale_96_dpi_dimension(CW_USEDEFAULT, 144), CW_USEDEFAULT);
        assert_eq!(scale_96_dpi_dimension(0, 144), 0);
    }

    /// Wave C — Main default size is the fullscreen 1920×1080 fallback for
    /// `bento-nano-app::startup_layout_viewport` sentinel use; the real
    /// runtime rect comes from `main_window_rect()`.
    #[test]
    fn main_default_size_is_fullscreen_fallback() {
        assert_eq!(default_size(WindowKind::Main), (1920, 1080));
    }

    /// Wave C — Main HWND must be a borderless `WS_POPUP` transparent overlay.
    /// Stripping `WS_OVERLAPPEDWINDOW` eliminates the caption / sysmenu /
    /// resize border so the entire client area is a hit-testable canvas.
    #[test]
    fn main_ex_style_is_popup_overlay_no_caption() {
        let (ex, style) = ex_style_for(WindowKind::Main);
        assert!(
            ex & WS_EX_NOREDIRECTIONBITMAP != 0,
            "Main must keep NoRedirectionBitmap per spec §4.1"
        );
        assert!(
            style & WS_POPUP != 0,
            "Main must be a popup window (no caption / no sysmenu)"
        );
        // The legacy `WS_OVERLAPPEDWINDOW` is `WS_OVERLAPPED | WS_CAPTION |
        // WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX`.
        // Wave C requires those bits cleared so no chrome paints on top of
        // the transparent canvas.
        const NON_OVERLAY_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
        assert_eq!(
            style & NON_OVERLAY_BITS,
            0,
            "Main must not request caption/sysmenu/resize-border bits",
        );
    }

    /// Wave H1 (2026-05-20) — Main HWND must carry `WS_EX_TOPMOST` so the
    /// fully transparent overlay (zone pills + toolbar) floats above all
    /// foreground apps, matching the Tauri 1.2.4 baseline. Click-through
    /// for blank pixels is preserved by `WS_EX_TRANSPARENT` toggled by
    /// the ghost-layer cursor-passthrough timer.
    #[test]
    fn main_ex_style_is_topmost() {
        let (ex, _) = ex_style_for(WindowKind::Main);
        assert!(
            ex & WS_EX_TOPMOST != 0,
            "Main must request WS_EX_TOPMOST per Wave H1 (pills above foreground apps)"
        );
    }

    /// V-10 (2026-05-21) — Main HWND must carry `WS_EX_TRANSPARENT` at creation
    /// so launch-time clicks fall through to Explorer / desktop icons. The
    /// ghost-layer cursor-passthrough timer *clears* the flag when the cursor
    /// is over a real surface (pill / expanded zone), inverting Wave H1's
    /// "toggled on at first tick" semantics into "default on, cleared when
    /// over a real surface". Closes the startup interaction-blocked window.
    #[test]
    fn main_ex_style_is_transparent_at_creation() {
        let (ex, _) = ex_style_for(WindowKind::Main);
        assert!(
            ex & WS_EX_TRANSPARENT != 0,
            "Main must request WS_EX_TRANSPARENT at creation per V-10 (startup click-through)"
        );
    }

    /// V-1 (TL ruling 2026-05-21) — Settings is a Tauri-style borderless modal:
    /// panel paints its own header per frame_060, so the OS caption / border /
    /// sysmenu must not be on the HWND. Bare `WS_POPUP` keeps NoRedirectionBitmap
    /// + TOPMOST while stripping every non-popup chrome bit.
    #[test]
    fn settings_ex_style_is_borderless_topmost() {
        let (ex, style) = ex_style_for(WindowKind::Settings);
        assert!(
            ex & WS_EX_NOREDIRECTIONBITMAP != 0,
            "Settings must keep NoRedirectionBitmap (DComp panel chrome)"
        );
        assert!(
            ex & WS_EX_TOPMOST != 0,
            "Settings must be TOPMOST so it stays above foreground apps"
        );
        assert!(
            style & WS_POPUP != 0,
            "Settings must be WS_POPUP (no native chrome)"
        );
        // V-1 — explicit borderless: no caption, no sysmenu, no border, no resize
        const NON_POPUP_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
        assert_eq!(
            style & NON_POPUP_BITS,
            0,
            "Settings must not request caption / sysmenu / border / resize bits",
        );
    }

    /// Wave C — `main_window_rect()` always reports a usable rectangle. In a
    /// CI/headless test harness `MonitorFromPoint` may report a zero work
    /// area; the function falls back to `default_size(Main)` so the shell
    /// can still call `CreateWindowExW` with sane dimensions.
    #[test]
    fn main_window_rect_is_non_empty() {
        let (_x, _y, w, h) = main_window_rect();
        assert!(w > 0, "main window rect width must be positive");
        assert!(h > 0, "main window rect height must be positive");
    }
}
