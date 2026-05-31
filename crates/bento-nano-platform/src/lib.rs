#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-platform` — Win32 + D2D + DComp + DWrite low-level layer.
//!
//! Spec lock:
//!   §2  single process, no IPC
//!   §3.1.1 windows-sys (plain Win32) + windows 0.58 (COM-typed graphics)
//!   §4  D2D / DComp / DWrite singletons via `OnceLock`
//!   §4.1 WS_EX_NOREDIRECTIONBITMAP only — never LAYERED with it
//!   §11 zero panic-shaped operations; every fallible call returns `Result`
//!   §15 every module ≤ 800 LOC; rustdoc on every public item

#![forbid(unsafe_op_in_unsafe_fn)]
#![allow(clippy::missing_safety_doc)] // SAFETY notes are inline at every unsafe block.

// Wave 19 — `allocator` is link-time-only: registers a `.CRT$XCU`
// function pointer that runs before Rust `main`. Nothing in the rest of
// the crate references it; it must still be `pub mod` so the linker
// retains its `#[used]` static.
pub mod allocator;
pub mod d2d;
pub mod d3d;
pub mod dcomp;
pub mod dpi;
pub mod dwrite;
pub mod errors;
pub mod message_loop;
pub mod monitor;
pub mod storage;
pub mod svg;
pub mod svg_cache;
pub mod window;

pub use d3d::{
    RecoveryAction, RecoveryState, decide_recovery, device_generation, recover_device_chain,
};
pub use errors::{PlatformError, ok};
pub use monitor::{
    MonitorInfo, RectI32, clamp_window_to_monitors, clamp_zone_to_monitors, enumerate_monitors,
    monitor_from_point, monitor_from_window, primary_monitor, zone_active_monitor_index,
};
pub use window::{
    WindowDesc, WindowKind, create_transparent_window, create_window, default_size, ex_style_for,
    main_window_rect, to_windows_hwnd,
};
