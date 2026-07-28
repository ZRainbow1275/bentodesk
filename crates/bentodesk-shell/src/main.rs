#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! BentoDesk native shell — assembles platform + app + widgets into a runnable
//! exe.
//!
//! Build & run:
//!   cargo build --release
//!   .\target\x86_64-pc-windows-msvc\release\BentoDesk.exe
//!     ESC               → exit
//!
//! Spec lock:
//!   §1  ≤ 30 MB Private Bytes
//!   §2  single process — every HWND lives on the UI thread
//!   §4.1 NoRedirectionBitmap window only
//!   §6  mimalloc default-features=false; env vars set BEFORE first allocation
//!   §9  GetMessageW pump; zero async runtime
//!   §10 zero heap allocation per frame in the wndproc
//!
//! ## Multi-window architecture (T-008/T-009/T-010 / Wave B)
//!
//! State is split into two tiers (mirrors the doc-comment in
//! `bentodesk-app::render`):
//!
//! | Tier        | Owner                                                | Cardinality |
//! |-------------|------------------------------------------------------|-------------|
//! | Process     | `AppRoot` (heap-leaked, reached via `app_root()`)    | 1           |
//! |             | — `AppState` (widget tree, zones, pinned/settings)   |             |
//! |             | — `WindowRegistry` (per-HWND `WindowSlot` boxed)     |             |
//! |             | — `EventDispatcher` (cross-thread Command bus)       |             |
//! |             | — `hovered`, frame counter, last-tick timestamp      |             |
//! | Per window  | `WindowSlot` (boxed inside `WindowRegistry`)         | N           |
//! |             | — `WindowState` (LayoutEngine + DPI/monitor cache)   |             |
//! |             | — `Renderer` (DComp visual tree + swap chain)        |             |
//! |             | — `is_visible`, hibernation timestamps, paint err    |             |
//!
//! The wndproc fetches the per-window `*mut WindowSlot` from `GWLP_USERDATA`
//! (set at slot registration); `AppRoot` is reached via the process-global
//! `APP_ROOT` `AtomicPtr`. The unsafe raw-pointer cast may make rust-analyzer
//! noisy; `cargo build` is the truth source.

#![forbid(unsafe_op_in_unsafe_fn)]
#![windows_subsystem = "windows"]

#[path = "shell/prelude.rs"]
mod shell_prelude;
use shell_prelude::*;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// Tray icon callback message — `WM_USER + 1` per Ruling B.
const WM_TRAY_ICON: u32 = 0x0500;
/// Deferred OLE drag-out starter. Posted after the threshold-crossing
/// `WM_MOUSEMOVE` returns so `DoDragDrop` enters from a normal UI-pump
/// message rather than from the middle of the mouse-move handler.
const WM_ITEM_DRAG_OUT: u32 = WM_APP + 0x0505;
/// Mc-1b — posted by a second launch attempt to the already-running instance
/// so the primary instance surfaces its Main window. 0x0505 is taken by
/// `WM_ITEM_DRAG_OUT` above, so this uses 0x0506.
const WM_WAKE_INSTANCE: u32 = WM_APP + 0x0506;
/// Posted once startup icon-cache repair has queued its UI-thread results.
const WM_ICON_CACHE_READY: u32 = WM_APP + 0x0507;
/// Mc-1b — process-wide single-instance mutex HANDLE (stored as `isize`).
/// Held for the whole process lifetime; we deliberately never `CloseHandle`
/// it (closing would release the named mutex and defeat the guard — the OS
/// reclaims the handle on process exit).
static MUTEX_HANDLE: OnceLock<isize> = OnceLock::new();
/// Mc-1b — cached `RegisterWindowMessageW("TaskbarCreated")` id. Explorer
/// broadcasts this when its tray is (re)created (e.g. after an explorer.exe
/// restart); we re-add the notify icon on receipt.
static TASKBAR_CREATED_MSG: OnceLock<u32> = OnceLock::new();
/// Mc-1b(c) — consecutive `paint()` failures. A permanently-dead renderer
/// (D3D/D2D unavailable) keeps failing; a transient device-loss recovers.
/// Reset to 0 on the first successful paint. Mc-2 device-recovery will also
/// reset this once the device is rebuilt, so a recoverable loss never trips
/// the fatal box.
static PAINT_FAIL_STREAK: AtomicU32 = AtomicU32::new(0);
/// Mc-1b(c) — guards the renderer-unavailable fatal box to a single showing.
static PAINT_FATAL_SHOWN: AtomicBool = AtomicBool::new(false);
/// Mc-1b(c) — consecutive-failure threshold before declaring the renderer
/// permanently unavailable. Deliberately high so a transient device-loss
/// (TDR/RDP/sleep) never trips the fatal box — only a renderer that cannot
/// initialise at all does.
const PAINT_FATAL_STREAK_THRESHOLD: u32 = 120;
const TRAY_ICON_ID: u32 = 1;
const TRAY_ICON_RETRY_TIMER_ID: usize = 0xB470_0506;
const TRAY_ICON_RETRY_MS: u32 = 1_500;
const TRAY_ICON_MAX_RETRIES: u8 = 4;
const TRAY_ICON_VERSION: u32 = NOTIFYICON_VERSION_4;
const TRAY_ICON_GUID: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x2D1E_B405,
    data2: 0x0505,
    data3: 0x4B70,
    data4: [0xA1, 0x10, 0xB3, 0xA7, 0x0D, 0x35, 0xB5, 0x05],
};
const LIVE_FOLDER_PICKER_HOST_ARG: &str = "--bentodesk-live-folder-picker-host";
const LIVE_FOLDER_PICKER_HOST_SELECTED_EXIT: i32 = 0;
const LIVE_FOLDER_PICKER_HOST_ERROR_EXIT: i32 = 1;
const LIVE_FOLDER_PICKER_HOST_CANCEL_EXIT: i32 = 2;
const LIVE_FOLDER_PICKER_HOST_TIMEOUT: Duration = Duration::from_secs(300);
const LIVE_FOLDER_PICKER_HOST_POLL: Duration = Duration::from_millis(100);
const GMEM_INVALID_HANDLE_FLAG: u32 = 0x8000;
const DROPFILES_HEADER_LEN: usize = 20;
const MAX_RAW_DROPFILES_BYTES: usize = 64 * 1024;
const MAX_RAW_DROPFILES_PATH_CHARS: usize = 32 * 1024;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const RESTART_ATTEMPT_ARG: &str = "--bentodesk-restart-attempt=";
const RESTART_WINDOW_START_ARG: &str = "--bentodesk-restart-window-start=";

/// Legacy ghost passthrough timer id. The selected-stack Main HWND now relies
/// on the renderer-owned window region for idle click-through, so startup no
/// longer arms this timer; the id remains reserved for cleanup/test stability.
const GHOST_PASSTHROUGH_TIMER_ID: usize = 0xB470_0505;

/// Backend/native event bridge. File watchers, live-folder refreshes, updater
/// events, and scheduler events arrive on crossbeam channels; this timer wakes
/// the UI thread even when no animation or input is producing paint messages.
const BACKEND_EVENT_POLL_TIMER_ID: usize = 0xB470_0507;
const BACKEND_EVENT_POLL_MS: u32 = 250;

/// On-demand frame backstop for hover-intent, stack bloom, capsule morph, and
/// theme cross-fade animations.
/// `InvalidateRect` is still the primary immediate-mode pump; this timer only
/// runs while a hover scheduler deadline, stack bloom, pill animation, or
/// Settings theme transition is active, so a cursor move into transparent
/// desktop space cannot strand a pending collapse, petal reveal, or color fade.
const HOVER_FRAME_TIMER_ID: usize = 0xB470_050A;
const SETTINGS_OUTSIDE_CLICK_TIMER_ID: usize = 0xB470_0511;
const SETTINGS_OUTSIDE_CLICK_POLL_MS: u32 = 32;
const CONTEXT_MENU_INPUT_TIMER_ID: usize = 0xB470_0512;
const CONTEXT_MENU_INPUT_POLL_MS: u32 = 24;
// Win32's default timer quantum is commonly ~15.6 ms. Requesting 16 ms can
// quantize to two ticks (~31 ms); USER_TIMER_MINIMUM (10 ms) reliably lands on
// one quantum without a process-wide timeBeginPeriod side effect.
const HOVER_FRAME_POLL_MS: u32 = 10;

/// One-shot post-startup memory trim. First-paint trim runs before late
/// startup work (tray, minibar restore, watcher warm-up) has fully settled, so
/// this timer gives the selected-stack runtime one more bounded chance to
/// release retained allocator/D2D resources before the WS-7 t10 sample.
const STARTUP_MEMORY_TRIM_TIMER_ID: usize = 0xB470_0508;
const STARTUP_MEMORY_TRIM_MS: u32 = 5_000;
/// Low-frequency resident trim for the always-on desktop surface. The first
/// two startup trims cover cold boot; this keeps long-idle D2D/DXGI allocator
/// rebound from crossing the strict WS-7 t30/t60 Private Bytes gate.
const RESIDENT_MEMORY_TRIM_TIMER_ID: usize = 0xB470_0509;
const RESIDENT_MEMORY_TRIM_MS: u32 = 25_000;
/// One-shot trim after StackTray opens. The first paint presents the tray, then
/// this drops rebuildable renderer caches before the strict t10/t30 samples.
const STACK_TRAY_MEMORY_TRIM_TIMER_ID: usize = 0xB470_050B;
const STACK_TRAY_MEMORY_TRIM_MS: u32 = 900;

/// 500 ms hibernation gate (T-099). A non-Main window hidden for less than
/// this many milliseconds keeps its swap chain backbuffer resident, so a
/// fast hide-then-show cycle (menu re-shown immediately on dismissal)
/// doesn't thrash the GPU surface allocation.
const HIBERNATE_GATE_MS: u32 = 500;
/// DIP distance before an item mouse-down becomes an OLE drag-out operation.
const ITEM_DRAG_THRESHOLD_DIP: i32 = 5;
const CF_UNICODETEXT_ID: u32 = 13;
const VK_BACKSPACE: u32 = 0x08;
const VK_ENTER: u32 = 0x0D;
const VK_SPACE_KEY: u32 = 0x20;
const VK_ESCAPE_KEY: u32 = 0x1B;
const VK_F2_KEY: u32 = 0x71;
const VK_F3_KEY: u32 = 0x72;
const VK_F4_KEY: u32 = 0x73;
const VK_F5_KEY: u32 = 0x74;
const VK_DELETE_KEY: u32 = 0x2E;
const VK_C_KEY: u32 = 0x43;
const VK_D_KEY: u32 = 0x44;
const VK_E_KEY: u32 = 0x45;
const VK_F_KEY: u32 = 0x46;
const VK_G_KEY: u32 = 0x47;
const VK_H_KEY: u32 = 0x48;
const VK_I_KEY: u32 = 0x49;
const VK_L_KEY: u32 = 0x4C;
const VK_M_KEY: u32 = 0x4D;
const VK_O_KEY: u32 = 0x4F;
const VK_P_KEY: u32 = 0x50;
const VK_R_KEY: u32 = 0x52;
const VK_S_KEY: u32 = 0x53;
const VK_T_KEY: u32 = 0x54;
const VK_U_KEY: u32 = 0x55;
const VK_A_KEY: u32 = 0x41;
const VK_N_KEY: u32 = 0x4E;
const VK_LEFT_KEY: u32 = 0x25;
const VK_UP_KEY: u32 = 0x26;
const VK_RIGHT_KEY: u32 = 0x27;
const VK_DOWN_KEY: u32 = 0x28;

fn startup_diag_skip(flag: &str) -> bool {
    startup_diag_skip_value(std::env::var("BENTODESK_DIAG_SKIP").ok().as_deref(), flag)
}

fn startup_diag_skip_value(value: Option<&str>, flag: &str) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.split(',').any(|part| {
        let token = part.trim();
        token.eq_ignore_ascii_case("all") || token.eq_ignore_ascii_case(flag)
    })
}

fn drag_proof_log_enabled() -> bool {
    std::env::var_os("BENTODESK_DRAG_PROOF_LOG").is_some()
}

fn animation_proof_log_enabled() -> bool {
    std::env::var_os("BENTODESK_ANIM_PROOF_LOG").is_some()
}

const ZONE_EDITOR_CAPSULE_PRESETS: &[(CapsuleSizeChoice, CapsuleShapeChoice)] = &[
    (CapsuleSizeChoice::Small, CapsuleShapeChoice::Pill),
    (CapsuleSizeChoice::Medium, CapsuleShapeChoice::Pill),
    (CapsuleSizeChoice::Medium, CapsuleShapeChoice::Rounded),
    (CapsuleSizeChoice::Large, CapsuleShapeChoice::Circle),
    (CapsuleSizeChoice::Large, CapsuleShapeChoice::Minimal),
    (CapsuleSizeChoice::Large, CapsuleShapeChoice::Square),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuxiliaryEscapeAction {
    CloseAbout,
    HideAuxWindow,
}

// -----------------------------------------------------------------------------
// AppRoot — process-wide state singleton. Reached via `app_root()`.
// -----------------------------------------------------------------------------

/// Process singleton. Owns every cross-window resource. The wndproc reaches
/// this via [`app_root`]; per-HWND slots are stashed in `GWLP_USERDATA`
/// directly (raw `*mut WindowSlot`).
///
/// `RefCell` / `Cell` interior mutability is sound because the Win32 message
/// pump is strictly single-threaded — every wndproc call comes from the UI
/// thread that owns the pump (Win32 contract, not our enforcement).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingItemDragOut {
    zone_id: ZoneId,
    item_id: ZoneItemId,
    path: SmolStr,
    /// Explorer convention: Ctrl-drag copies and keeps the source Zone item;
    /// an ordinary drag moves the item out of BentoDesk after OLE confirms a
    /// successful drop.
    copy_only: bool,
}

struct AppRoot {
    /// Domain widget tree + zones + global UI flags. Shared across windows.
    app: RefCell<AppState>,
    /// HWND → boxed `WindowSlot` registry. Borrowed for `register` /
    /// `unregister` / hibernation pass; the steady-state per-frame paint
    /// path bypasses this borrow by going through `get_slot_ptr(hwnd)`.
    registry: RefCell<WindowRegistry>,
    /// Cross-thread Command bus. Background workers push commands; the UI
    /// thread drains once per paint.
    dispatcher: EventDispatcher,
    /// Last-known hovered node — drives hover anim retargeting.
    hovered: RefCell<Option<NodeId>>,
    /// Tick of the previous frame, in `GetTickCount` units (ms).
    last_tick_ms: Cell<u32>,
    /// Frame counter — fed to `Tree::flush_dirty` for the once-per-frame
    /// debug guard (Phase 1.1 / commitment C2).
    frame_id: Cell<u64>,
    /// Mc-2b — device-recovery policy state. The pure decision machine
    /// (`decide_recovery`) lives in the platform crate; the shell owns the
    /// state + the `Instant`-based retry window. `Healthy` until the first
    /// `RenderError::DeviceLost`; a clean frame resets it back to `Healthy`.
    recovery_state: Cell<bentodesk_platform::RecoveryState>,
    /// Mc-2b — when the most recent recovery attempt was made. Drives the 60 s
    /// retry window: attempts inside the window accumulate toward the cap,
    /// attempts outside it restart the streak. `None` while `Healthy`.
    last_recovery_at: Cell<Option<Instant>>,
    /// F2-01 — pinned-zone MiniBar cap enforcement, sourced from
    /// `bentodesk_app::business::minibar::MiniBarRoster`. The roster's
    /// `pin` / `unpin` are the §11 R7 user-space pre-check that prevents
    /// the registry's silent 9th-MiniBar refusal from surfacing to the user.
    minibar_roster: RefCell<MiniBarRoster>,
    /// F2-01 — per-pinned-zone `MiniBar` widget descriptors. Carried in
    /// `AppRoot` so the future per-window render path (which selects a tree
    /// per `WindowKind::MiniBar` HWND) can pick the right descriptor by
    /// zone id without re-walking `app.zones`. Inline cap matches §11 R7.
    minibars: RefCell<smallvec::SmallVec<[(ZoneId, MiniBar); BUSINESS_MAX_MINIBARS]>>,
    /// Selection context for the app-rendered Zone menu. The D2D menu owns
    /// presentation while this record keeps command IDs mapped to real zones.
    zone_context_menu: RefCell<Option<PendingZoneContextMenu>>,
    /// Selection context for the app-rendered item menu.
    item_context_menu: RefCell<Option<PendingItemContextMenu>>,
    /// Item identity captured when a drag crosses the external-drag threshold.
    /// The shell posts `WM_ITEM_DRAG_OUT` and starts OLE from that later pump
    /// turn so `DoDragDrop` is not entered from inside the threshold
    /// `WM_MOUSEMOVE` handler.
    pending_item_drag_out: RefCell<Option<PendingItemDragOut>>,
    /// Stack anchor produced by the current pointer-drop command. The domain
    /// relation is created by the dispatcher first; only then may hover reveal
    /// resolve the new anchor's members and start the Bloom under the unchanged
    /// release point. Context-menu `StackZone` commands never set this flag.
    pending_stack_drop_bloom: Cell<Option<ZoneId>>,
    /// Source-only OLE guard. While BentoDesk is dragging one of its own
    /// items out to another application, the main HWND must not also accept
    /// the same CF_HDROP payload through its registered OLE drop target.
    item_drag_out_active: Cell<bool>,
    /// Native tray popup state used while `TrackPopupMenu` owns its modal
    /// loop. Mirrors the zone menu contract so automation, keyboard, and OS
    /// menu delivery cannot drift from the selected-stack command mapper.
    tray_context_menu: RefCell<Option<PendingTrayContextMenu>>,
    tray_context_menu_consumed: Cell<bool>,
    /// Runtime hotkey table: defaults plus validated `keybinding.*` vault
    /// overrides. Kept as a tiny `SmallVec`; no hash table on keydown.
    hotkey_bindings: RefCell<smallvec::SmallVec<[hotkey::HotkeyBinding; 20]>>,
    /// OS-global hotkeys currently registered against the Main HWND.
    /// Registration can fail when another app owns a chord; only successful
    /// registrations are stored here and later unregistered.
    global_hotkeys: RefCell<smallvec::SmallVec<[GlobalHotkeyRegistration; 20]>>,
    /// System tray registration state for the Main HWND. `Shell_NotifyIconW`
    /// can fail while Explorer is still rebuilding its notification area; the
    /// bounded retry counter keeps that transient from becoming a silent loss.
    tray_registered: Cell<bool>,
    tray_retry_attempts: Cell<u8>,
    /// Mc-3 #15 — sticky uID-only fallback. On Win10 1607+ a `NIF_GUID` tray
    /// identity is bound to the registering EXE's full path; a relocated
    /// portable install can never re-`NIM_ADD` under the GUID, so retrying is
    /// futile. Once the GUID retry budget is exhausted we flip this `true`,
    /// reset the budget, and re-register with the (hWnd, uID) identity (no
    /// GUID), which is not path-bound. Sticky for the session — kept across
    /// `TaskbarCreated` so we never relapse to the known-bad GUID path.
    tray_uid_only: Cell<bool>,
    /// Desktop watcher handle. Keeping it in AppRoot preserves the underlying
    /// notify watcher for the whole process lifetime.
    desktop_watcher: RefCell<Option<bentodesk_backend::watcher::DesktopWatcher>>,
    /// Retained sender used to rebuild the desktop watcher after Settings path
    /// changes or power resume without replacing the receiver wired into the UI
    /// pump.
    desktop_event_tx: crossbeam_channel::Sender<bentodesk_backend::watcher::FileChangedEvent>,
    /// Live desktop-change events routed from the backend watcher into the UI
    /// pump. Drained once per paint by `drain_desktop_events`.
    desktop_events: crossbeam_channel::Receiver<bentodesk_backend::watcher::FileChangedEvent>,
    /// Live-folder refresh events routed from the backend watcher into the UI
    /// pump. Drained once per paint by `drain_live_folder_events`.
    live_folder_events: crossbeam_channel::Receiver<bentodesk_backend::watcher::ZoneRefreshEvent>,
    /// Set after startup has walked persisted zones and rebound every
    /// `live_folder_path` once.
    live_folder_rehydrated: Cell<bool>,
    /// Ghost-layer events emitted by the backend overlay subclass. Power
    /// resume events are drained in the paint pump and reassert work-area
    /// placement.
    ghost_events: crossbeam_channel::Receiver<bentodesk_backend::ghost_layer::GhostLayerEvent>,
    /// Delayed power-resume producer/consumer. `GhostLayerEvent::PowerResume`
    /// schedules the user-configured delay through the backend; only the
    /// resulting `PowerEvent::Resumed` performs watcher/overlay recovery.
    power_event_tx: crossbeam_channel::Sender<bentodesk_backend::power::PowerEvent>,
    power_events: crossbeam_channel::Receiver<bentodesk_backend::power::PowerEvent>,
    /// Selected-stack updater driver. Commands call this directly; lifecycle
    /// events are drained into `AppState::settings_updater_status`.
    updater: Updater,
    /// Typed updater events replacing the 1.x Tauri event bus names.
    updater_events: crossbeam_channel::Receiver<UpdateEvent>,
    /// Periodic rules scheduler events. The backend thread only decides
    /// due-ness; the UI pump applies rule effects against live shell state.
    rules_scheduler_events: crossbeam_channel::Receiver<SchedulerEvent>,
    /// In-memory timeline cursor/ring index over the disk-backed checkpoint
    /// store. The store itself lives under app-data `timeline/`.
    timeline_buffer: RefCell<TimelineBuffer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlobalHotkeyRegistration {
    id: i32,
    command: hotkey::HotkeyCommand,
}

/// Process-global pointer to the heap-leaked `AppRoot`. Stored as
/// `AtomicPtr<AppRoot>` rather than `OnceLock<&'static AppRoot>` because
/// `AppRoot` is `!Sync` (`RefCell` interior); only the UI thread ever
/// dereferences this pointer.
static APP_ROOT: AtomicPtr<AppRoot> = AtomicPtr::new(ptr::null_mut());

/// Install the process-global `AppRoot`. Called exactly once during `main`
/// before any `CreateWindowExW` so every wndproc dispatch can rely on
/// `app_root()` returning a live reference.
fn install_app_root(root: Box<AppRoot>) {
    let raw = Box::into_raw(root);
    let prev = APP_ROOT.swap(raw, Ordering::AcqRel);
    debug_assert!(prev.is_null(), "AppRoot installed twice");
    let _ = prev;
}

/// Borrow the process `AppRoot`. Returns `None` only during early startup
/// before `install_app_root` ran, which the wndproc tolerates (early
/// WM_NCCREATE has no app state to act on yet).
#[inline]
fn app_root<'a>() -> Option<&'a AppRoot> {
    let p = APP_ROOT.load(Ordering::Acquire);
    if p.is_null() {
        None
    } else {
        // SAFETY: `install_app_root` did `Box::into_raw`; the returned
        //         pointer is valid for the lifetime of the process (we
        //         never `Box::from_raw` it back). UI thread is the sole
        //         reader, so producing a shared reference is sound.
        Some(unsafe { &*p })
    }
}

const CLASS_NAME: &[u16] = &[
    b'B' as u16,
    b'e' as u16,
    b'n' as u16,
    b't' as u16,
    b'o' as u16,
    b'D' as u16,
    b'e' as u16,
    b's' as u16,
    b'k' as u16,
    b'S' as u16,
    b'h' as u16,
    b'e' as u16,
    b'l' as u16,
    b'l' as u16,
    0,
];
const WIN_TITLE: &[u16] = &[
    b'B' as u16,
    b'e' as u16,
    b'n' as u16,
    b't' as u16,
    b'o' as u16,
    b'D' as u16,
    b'e' as u16,
    b's' as u16,
    b'k' as u16,
    0,
];

/// #19-B (2026-05-31) — pick the startup UI language from the OS user locale.
///
/// ZRainbow Option B: a Chinese-OS user gets zh-CN; everyone else defaults to
/// English (EN_US is verified complete — 231 entries, 42 empty slots in the
/// SAME positions as ZH_CN). A runtime Settings toggle (`set_locale`) still
/// lets the user flip languages afterwards.
///
/// `GetUserDefaultLocaleName` (kernel32) is Vista+; we target Win7+ per the
/// Mc-1a manifest, so the STATIC import is safe — no `GetProcAddress`
/// soft-load needed. It writes a NUL-terminated BCP-47 tag (e.g. "zh-CN",
/// "en-US") into the caller's buffer and returns the character count incl. the
/// NUL (0 on failure). A "zh" prefix (case-insensitive) → Simplified Chinese;
/// anything else → English. On the (impossible-on-Win7+) failure path we
/// preserve the historical behaviour and return zh-CN.
fn detected_default_locale() -> &'static bentodesk_style::LookupTable {
    // `LOCALE_NAME_MAX_LENGTH` is 85 wide chars (windows-sys 0.59 doesn't
    // re-export the constant; the value is fixed by the Win32 contract).
    let mut buf = [0u16; 85];
    // SAFETY: `buf` is a valid writable [u16; 85]; we pass its length so the
    // API never writes past the end. The returned count bounds the slice we
    // read back.
    let written = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if written <= 0 {
        // Vista+ guarantees success; defend the impossible path by keeping the
        // current zh-CN default.
        return &bentodesk_style::ZH_CN;
    }
    // `written` includes the trailing NUL — trim it before decoding.
    let len = (written as usize).saturating_sub(1).min(buf.len());
    let tag: String = String::from_utf16_lossy(&buf[..len]).to_ascii_lowercase();
    if tag.starts_with("zh") {
        &bentodesk_style::ZH_CN
    } else {
        &bentodesk_style::EN_US
    }
}

#[path = "shell/bootstrap.rs"]
mod bootstrap;

#[path = "shell/main_window_proc.rs"]
mod main_window_proc;

#[path = "shell/main_window_timer.rs"]
mod main_window_timer;

use main_window_timer::*;

use main_window_proc::*;

#[path = "shell/input_core.rs"]
mod input_core;

use input_core::*;

#[path = "shell/stack_interaction.rs"]
mod stack_interaction;

use stack_interaction::*;

#[path = "shell/zone_motion.rs"]
mod zone_motion;

use zone_motion::*;

#[path = "shell/stack_surfaces.rs"]
mod stack_surfaces;

use stack_surfaces::*;

#[path = "shell/suggestor_highlight.rs"]
mod suggestor_highlight;

use suggestor_highlight::*;

#[path = "shell/search_input.rs"]
mod search_input;

use search_input::*;

#[path = "shell/bulk_input.rs"]
mod bulk_input;

use bulk_input::*;

#[path = "shell/rules_input.rs"]
mod rules_input;

use rules_input::*;

#[path = "shell/settings_input.rs"]
mod settings_input;

use settings_input::*;

#[path = "shell/editor_input.rs"]
mod editor_input;

use editor_input::*;

#[path = "shell/settings_keys_restore.rs"]
mod settings_keys_restore;

use settings_keys_restore::*;

#[path = "shell/settings_runtime.rs"]
mod settings_runtime;

use settings_runtime::*;

#[path = "shell/settings_save.rs"]
mod settings_save;

use settings_save::*;

#[path = "shell/hotkeys_updates.rs"]
mod hotkeys_updates;

use hotkeys_updates::*;

#[path = "shell/themes_plugins.rs"]
mod themes_plugins;

use themes_plugins::*;

#[path = "shell/persisted_settings.rs"]
mod persisted_settings;

use persisted_settings::*;

#[path = "shell/context_capsules.rs"]
mod context_capsules;

use context_capsules::*;

#[path = "shell/timeline_ui.rs"]
mod timeline_ui;

use timeline_ui::*;

#[path = "shell/timeline_storage.rs"]
mod timeline_storage;

use timeline_storage::*;

#[path = "shell/live_folders.rs"]
mod live_folders;

use live_folders::*;

#[path = "shell/rules_wizard_runtime.rs"]
mod rules_wizard_runtime;

use rules_wizard_runtime::*;

#[path = "shell/rules_execution.rs"]
mod rules_execution;

use rules_execution::*;

#[path = "shell/startup_backup_types.rs"]
mod startup_backup_types;

use startup_backup_types::*;

#[path = "shell/settings_backups.rs"]
mod settings_backups;

use settings_backups::*;

#[path = "shell/recovery_vault.rs"]
mod recovery_vault;

use recovery_vault::*;

#[path = "shell/recovery_commands.rs"]
mod recovery_commands;

use recovery_commands::*;

#[path = "shell/tray_minibars.rs"]
mod tray_minibars;

use tray_minibars::*;

#[path = "shell/tooltips_main.rs"]
mod tooltips_main;

use tooltips_main::*;

#[path = "shell/tooltips_aux.rs"]
mod tooltips_aux;

use tooltips_aux::*;

#[path = "shell/mouse_hover.rs"]
mod mouse_hover;

use mouse_hover::*;

#[path = "shell/pointer_down.rs"]
mod pointer_down;

#[path = "shell/settings_pointer.rs"]
mod settings_pointer;

use settings_pointer::*;

use pointer_down::*;

#[path = "shell/pointer_up_drag.rs"]
mod pointer_up_drag;

use pointer_up_drag::*;

#[path = "shell/drop_files.rs"]
mod drop_files;

use drop_files::*;

#[path = "shell/context_menu_model.rs"]
mod context_menu_model;

use context_menu_model::*;

#[path = "shell/context_menu_surface.rs"]
mod context_menu_surface;

use context_menu_surface::*;

#[path = "shell/aux_metadata.rs"]
mod aux_metadata;

use aux_metadata::*;

#[path = "shell/aux_positioning.rs"]
mod aux_positioning;

use aux_positioning::*;

#[path = "shell/aux_runtime.rs"]
mod aux_runtime;

use aux_runtime::*;

#[path = "shell/dispatcher.rs"]
mod dispatcher;

#[path = "shell/dispatch/window.rs"]
mod dispatch_window;

#[path = "shell/dispatch/zones.rs"]
mod dispatch_zones;

#[path = "shell/dispatch/stacks.rs"]
mod dispatch_stacks;

#[path = "shell/dispatch/items_settings.rs"]
mod dispatch_items_settings;

#[path = "shell/dispatch/recovery_updates.rs"]
mod dispatch_recovery_updates;

#[path = "shell/dispatch/workflows.rs"]
mod dispatch_workflows;

#[path = "shell/dispatch/bulk_surfaces.rs"]
mod dispatch_bulk_surfaces;

use dispatcher::*;

#[path = "shell/item_persistence.rs"]
mod item_persistence;

use item_persistence::*;

#[path = "shell/item_file_ops.rs"]
mod item_file_ops;

use item_file_ops::*;

#[path = "shell/smart_groups_icons.rs"]
mod smart_groups_icons;

use smart_groups_icons::*;

#[path = "shell/tray_menu.rs"]
mod tray_menu;

use tray_menu::*;

#[path = "shell/bulk_layout.rs"]
mod bulk_layout;

use bulk_layout::*;

#[path = "shell/aux_openers.rs"]
mod aux_openers;

use aux_openers::*;

#[path = "shell/search.rs"]
mod search;

use search::*;

#[path = "shell/suggestor.rs"]
mod suggestor;

use suggestor::*;

#[path = "shell/runtime_utils.rs"]
mod runtime_utils;

use runtime_utils::*;

#[path = "shell/paint_events.rs"]
mod paint_events;

use paint_events::*;

fn main() {
    bootstrap::run();
}

#[cfg(test)]
mod tests;
