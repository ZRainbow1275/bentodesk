//! Win32 window creation wrapper.
//!
//! Spec §4.1: per-pixel-alpha transparent windows MUST use
//! `WS_EX_NOREDIRECTIONBITMAP` ALONE — combining with `WS_EX_LAYERED` silently
//! breaks DComp composition. Helper makes the correct choice non-bypassable.
//!
//! Phase 1 / T-011 — adds the multi-window factory `create_window(kind, parent)`
//! plus the `WindowKind` taxonomy that drives ex-style selection, default
//! sizing, focus / activation behaviour, and §11 R5 hibernation eligibility.
//! `create_transparent_window` remains for the legacy single-Main-window path;
//! MiniBar / Settings are now created through the multi-window factory.

use std::ptr;

use windows_sys::Win32::Foundation::{HMODULE, HWND};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CW_USEDEFAULT, CreateWindowExW, HCURSOR, HICON, IDC_ARROW, LoadCursorW, LoadIconW,
    RegisterClassExW, SW_SHOWNORMAL, ShowWindow, WNDCLASSEXW, WNDPROC, WS_CAPTION, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

use crate::errors::PlatformError;

const APP_ICON_RESOURCE_ID: u16 = 101;
const TRAY_ICON_RESOURCE_ID: u16 = 102;
const WINDOW_CLASS_STYLE: u32 = CS_DBLCLKS;

#[allow(clippy::manual_dangling_ptr)]
fn load_module_icon(hinst: HMODULE, resource_id: u16) -> HICON {
    // SAFETY: Win32 `MAKEINTRESOURCEW(id)` is the documented low-word pointer
    // sentinel accepted by LoadIconW; both IDs are compiled into app.res.
    unsafe { LoadIconW(hinst, resource_id as usize as *const u16) }
}

/// Load the BentoDesk tray icon embedded in the current executable.
///
/// The returned handle is a shared module resource and must not be destroyed.
pub fn load_tray_icon() -> HICON {
    // SAFETY: null requests the current executable module.
    let hinst = unsafe { GetModuleHandleW(ptr::null()) };
    load_module_icon(hinst, TRAY_ICON_RESOURCE_ID)
}

/// Categorical role of a window. Drives ex-style selection (`ex_style_for`),
/// default geometry (`default_size`), focus/activation behaviour, intended
/// z-order, and the §11 R5 hibernation decision.
///
/// Variant cheat-sheet (intended behaviour, enforced by `ex_style_for`):
///
/// | Kind          | ex-style highlights                                  | Activation | Z-order      | Hibernates? |
/// |---------------|------------------------------------------------------|------------|--------------|-------------|
/// | `Main`        | NoRedirectionBitmap + ToolWindow + NoActivate             | NO steal | below apps | NEVER |
/// | `IconPicker`  | NoRedirectionBitmap                                  | accepts    | above main   | yes         |
/// | `CapsulePicker`| NoRedirectionBitmap                                 | accepts    | above main   | yes         |
/// | `ContextMenu` | NoRedirectionBitmap + ToolWindow                     | accepts    | above owner  | yes         |
/// | `Tooltip`     | ToolWindow + NoActivate + Transparent (click-thru)   | NO steal   | topmost      | yes         |
/// | `DragPreview` | NoRedirectionBitmap + Layered + Transparent + Topmost| NO steal   | topmost      | yes         |
/// | `Settings`    | NoRedirectionBitmap + borderless popup               | accepts    | normal-modal | yes         |
/// | `About`       | NoRedirectionBitmap + borderless popup               | accepts    | normal-modal | yes         |
/// | `MiniBar`     | NoRedirectionBitmap + NoActivate + Topmost           | NO steal   | topmost      | yes         |
/// | `Timeline`    | NoRedirectionBitmap                                  | accepts    | normal-modal | yes         |
/// | `SnapshotPicker`| NoRedirectionBitmap                                | accepts    | normal-modal | yes         |
/// | `Search`      | NoRedirectionBitmap                                  | accepts    | normal-modal | yes         |
///
/// `DragPreview` is the only Layered transparent-window mode because it follows
/// the cursor via the `UpdateLayeredWindow` pattern. MiniBar is rendered by the
/// same DComp swap-chain path as other selected-stack aux windows, so it must
/// keep NoRedirectionBitmap and never request Layered. See §4.1:
/// NoRedirectionBitmap + Layered are **mutually exclusive**.
///
/// ΔB ruling (master-decomposition §11): `serde::Serialize + Deserialize`
/// derives are forward-compat surface for the v2.x scripting / plugin
/// re-introduction. The Phase-1 single-process build never serializes
/// `WindowKind` at runtime; the `bentodesk-app::dispatcher::Command`
/// `ShowWindow / HideWindow` variants carry it through the in-process bus
/// only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WindowKind {
    /// Primary application window. Fully transparent fullscreen overlay
    /// — no DWM Mica/Acrylic, no full-screen scrim. Per-pill + per-modal
    /// surfaces paint their own translucent dark in `Renderer::draw_zones`
    /// and `Renderer::draw_settings_panel`. It stays in the normal desktop
    /// z-order so regular foreground applications cover it, matching the
    /// Tauri `alwaysOnTop: false` baseline. Never hibernates.
    Main,
    /// Icon picker — full-grid widget gallery for assigning icons to zones.
    /// Accepts focus, sized 1280×720, hibernates when dismissed.
    IconPicker,
    /// Capsule picker — compact app-launcher style picker. Accepts focus,
    /// 480×600, hibernates on dismiss.
    CapsulePicker,
    /// Right-click context menu — app-rendered, focusable owned popup. Its
    /// ToolWindow style keeps it out of Alt-Tab/taskbar; activation enables
    /// Escape/arrow navigation and reliable outside-click dismissal.
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
    /// About dialog. Borderless DComp card; the runtime content is drawn
    /// by `bentodesk-app::Renderer::draw_about_panel`. 640x520 default.
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
    /// Modal-style popup with caption. 480×460 default.
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
/// viewport sentinel for `bentodesk-app::startup_layout_viewport` when no
/// HWND has been seeded yet (cold-start layout migration).
pub fn default_size(kind: WindowKind) -> (i32, i32) {
    match kind {
        WindowKind::Main => (1920, 1080),
        WindowKind::IconPicker => (480, 640),
        WindowKind::CapsulePicker => (480, 600),
        WindowKind::ContextMenu => (240, 200),
        WindowKind::Tooltip => (200, 40),
        WindowKind::DragPreview => (64, 64),
        WindowKind::Settings => (800, 600),
        WindowKind::About => (640, 520),
        WindowKind::MiniBar => (280, 80),
        WindowKind::PalettePicker => (320, 240),
        WindowKind::RulesWizard => (640, 480),
        WindowKind::BulkManager => (720, 540),
        WindowKind::ZoneEditor => (480, 460),
        WindowKind::ItemFileRename => (520, 240),
        WindowKind::Suggestor => (522, 574),
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
            // `WM_NCHITTEST` (see `bentodesk-shell::ui::main_nchittest_kind`)
            // returns `HTTRANSPARENT` for empty pixels so Explorer keeps
            // receiving desktop-icon clicks.
            //
            // W13-A (2026-07-13) — Tauri's real baseline is
            // `alwaysOnTop: false`: Zone surfaces belong to the desktop layer
            // and regular application windows must cover them. TOPMOST here
            // made the full-screen DComp host overlay Word/browser windows.
            // ToolWindow keeps the host out of Alt-Tab/taskbar; NoActivate
            // prevents closing another app from foregrounding the fullscreen
            // transparent host.
            //
            // W13-B — SetWindowRgn is the selected stack's single
            // click-through authority. Keeping WS_EX_TRANSPARENT on Main made
            // its correctly-shaped region visible but unable to receive real
            // mouse input once the host moved below regular applications.
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                WS_POPUP | WS_VISIBLE,
            )
        }
        WindowKind::IconPicker
        | WindowKind::CapsulePicker
        | WindowKind::Settings
        | WindowKind::About
        | WindowKind::PalettePicker
        | WindowKind::RulesWizard
        | WindowKind::BulkManager
        | WindowKind::ZoneEditor
        | WindowKind::ItemFileRename
        | WindowKind::Suggestor
        | WindowKind::Timeline
        | WindowKind::SnapshotPicker
        | WindowKind::Search => {
            // Every focusable application-owned surface is fully painted by
            // the native D2D/DComp renderer.  A second OS caption produces the
            // unfinished "debug window" layer the desktop product must never
            // expose, and WS_VISIBLE can flash that unpositioned host at (0,0)
            // before its swap chain exists.  Use one hidden, borderless popup
            // contract; the shell centres and shows it only after registration.
            (WS_EX_NOREDIRECTIONBITMAP, WS_POPUP)
        }
        WindowKind::ContextMenu => {
            // App-rendered menu: ToolWindow keeps it out of Alt-Tab/taskbar,
            // while a focusable owned popup receives Escape/arrow navigation
            // and closes on deactivation. It starts hidden so its DComp slot is
            // fully seeded before the shell positions and shows the first row.
            (WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW, WS_POPUP)
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
        WindowKind::MiniBar => {
            // MiniBar is painted by the DComp renderer, not UpdateLayeredWindow.
            // Per spec 4.1 it must use NoRedirectionBitmap exclusively of
            // Layered; Topmost + NoActivate keeps it floating without focus
            // theft while still allowing direct clicks on the bar.
            (
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
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
    /// so existing callers (`bentodesk-shell::main`) keep their semantics.
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
                WindowKind::Tooltip | WindowKind::DragPreview | WindowKind::MiniBar
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
    let app_icon = load_module_icon(hinst, APP_ICON_RESOURCE_ID);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        // Tauri ItemCard opens on native double-click. CS_DBLCLKS makes
        // user32 emit WM_LBUTTONDBLCLK instead of a second WM_LBUTTONDOWN so
        // the shell can preserve single-click selection/drag semantics.
        style: WINDOW_CLASS_STYLE,
        lpfnWndProc: desc.wnd_proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: app_icon,
        // SAFETY: LoadCursorW with null hinst + system IDC_ARROW always valid.
        hCursor: unsafe { LoadCursorW(0 as HMODULE, IDC_ARROW) } as HCURSOR,
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: desc.class_name.as_ptr(),
        hIconSm: app_icon,
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

    // Fix #17 — legacy caption-bearing dialogs (PalettePicker / RulesWizard /
    // BulkManager / ItemFileRename / Suggestor / Timeline / SnapshotPicker /
    // Search; see `ex_style_for`) carry a native
    // OS title bar. BentoDesk is dark-themed, but on Windows 10 1809+
    // those captions render light/white by default — a visible mismatch
    // against the dark panels. Gate the immersive-dark-caption attribute on
    // the actual `WS_CAPTION` style bit (self-maintaining — any future
    // caption kind picks it up automatically) rather than a hard-coded kind
    // list. Borderless windows (Main / Settings / pickers / minibar) have no
    // caption to darken, so the call is skipped for them.
    if (style & WS_CAPTION) != 0 {
        apply_immersive_dark_caption(hwnd);
    }

    // Respect the kind matrix's initial-visibility contract. Hidden popups
    // (ContextMenu and ZoneEditor) must be positioned and have their DComp
    // renderer seeded before a caller explicitly shows them; unconditionally
    // calling ShowWindow here caused a visible top-left construction flash.
    if (style & WS_VISIBLE) != 0 {
        // SAFETY: hwnd valid; SW_SHOWNORMAL standard cmd.
        unsafe { ShowWindow(hwnd, SW_SHOWNORMAL) };
    }
    Ok(hwnd)
}

/// Set `DWMWA_USE_IMMERSIVE_DARK_MODE = TRUE` on a caption-bearing HWND so the
/// native OS title bar renders dark, matching BentoDesk's dark theme.
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
mod tests;
