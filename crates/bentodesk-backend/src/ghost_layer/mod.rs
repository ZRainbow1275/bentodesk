//! T-084 — Desktop ghost-layer overlay (lift-verbatim from 1.x
//! `src-tauri/src/ghost_layer/`).
//!
//! Promotes a top-level HWND to a desktop overlay: stripped of decorations,
//! parked at `HWND_TOPMOST` (Wave H1, 2026-05-20 — raised from the Wave G1
//! default-z policy after foreground apps were found to occlude the zone
//! pills; the prior `HWND_BOTTOM` policy sank the D2D paint beneath
//! Explorer's `WorkerW`), invisible to Alt-Tab, and protected against
//! external z-order changes via a WndProc subclass.
//!
//! ## Differences vs 1.x
//!
//! | 1.x                                              | native                                                                         |
//! |--------------------------------------------------|------------------------------------------------------------------------------|
//! | `attach(handle: &AppHandle)` — looks up "main" webview HWND via Tauri | [`attach`] takes a raw HWND directly                                          |
//! | `set_decorations(false)` Tauri shim before WS_* edits | Direct `GetWindowLongPtrW` / `SetWindowLongPtrW` (Tauri shim was redundant) |
//! | `setIgnoreCursorEvents` from WebView/Tauri frontend | shell-owned geometry polling toggles [`set_cursor_passthrough`]             |
//! | `power::handle_resume(handle.clone())` on `WM_POWERBROADCAST` | [`set_event_sender`] wires a `Sender<GhostLayerEvent>` once at startup       |
//! | `windows` crate                                  | `windows-sys` (spec §3.1.1: hot-path Win32 must use `windows-sys`)           |
//!
//! ## Modules
//!
//! - [`manager`]           — overlay lifecycle (attach / show / hide / detach).
//! - [`highlight_overlay`] — desktop file highlight emission (rule-preview).

pub mod highlight_overlay;
pub mod manager;

pub use highlight_overlay::{
    DEFAULT_HIGHLIGHT_DURATION_MS, HighlightPayload, HighlightTarget, emit_clear, emit_highlight,
};
pub use manager::{
    BypassGuard, GhostLayerError, GhostLayerEvent, attach, attach_selected_stack,
    bypass_subclass_guard, cursor_passthrough, detach, hide_window, is_visible,
    reposition_to_work_area, set_cursor_passthrough, set_event_sender, show_window,
};
