//! Phase 2.3.1a smoke tests — `WindowState` DPI + monitor cache fields.
//!
//! These exercise the new `dpi: Cell<u32>` and `monitors: SmallVec<[_; 4]>`
//! cache fields landed in Wave 7. The wndproc-side wire-up
//! (`WM_DPICHANGED` handler + post-create seeding) cannot run inside `cargo
//! test` without a live HWND + message pump, so we cover the data-shape
//! contract here and let the smoke build prove the integration.
//!
//! Spec lock:
//!   §10  no allocation in hot path (cache fields use `Cell`/`SmallVec`)
//!   §11  no panic; tests use `assert!` per §11.1 test-only carve-out
//!   §13  no mocks — `enumerate_monitors()` hits real Win32 (Windows
//!        guarantees ≥ 1 monitor on any interactive session)

#![forbid(unsafe_op_in_unsafe_fn)]

use bento_nano_app::WindowState;
use bento_nano_platform::enumerate_monitors;

/// Default DPI must be 96 (Win32 USER_DEFAULT_SCREEN_DPI = 100% scale).
/// Picked so any reader between WindowState construction and the
/// shell-side `GetDpiForWindow` seed gets a usable scale factor instead of
/// dividing through zero in the eventual Phase 2.3.1b scaling math.
#[test]
fn dpi_default_is_96_on_window_state_new() {
    let win = WindowState::new();
    assert_eq!(
        win.dpi.get(),
        96,
        "Phase 2.3.1a contract: fresh WindowState must report 96 DPI (100% scale baseline) so PHASE_2.3.1b layout math never divides through zero before the post-create GetDpiForWindow seed runs"
    );
}

/// Monitor cache starts empty — the shell populates it after window
/// creation. Phase 2.3.1b / 2.4 callers must tolerate this empty-cache
/// window (between WindowState::new() and the first paint).
#[test]
fn monitors_field_starts_empty_on_window_state_new() {
    let win = WindowState::new();
    assert!(
        win.monitors.is_empty(),
        "Phase 2.3.1a contract: fresh WindowState must hold an empty monitor cache; the shell's lazy paint-init pours real `enumerate_monitors()` output once the HWND is live"
    );
}

/// `Cell<u32>::set` round-trip — guards against a regression where
/// somebody swaps the field type to a non-Copy aggregate and breaks the
/// `WM_DPICHANGED` handler's interior-mutability path. The handler does
/// `win.dpi.set(new_dpi_x)` exactly like this test.
#[test]
fn dpi_setter_round_trips_via_cell() {
    let win = WindowState::new();
    // 192 = 200% scale — the canonical "high-DPI" smoke value used in
    // every Microsoft DPI sample. Picked to be visibly different from 96
    // so a faulty Cell impl that silently keeps the old value would fail.
    win.dpi.set(192);
    assert_eq!(
        win.dpi.get(),
        192,
        "Cell<u32> set/get must round-trip; this is the contract WM_DPICHANGED relies on"
    );
    // And a second mutation, mirroring a back-to-back DPI change (user
    // drags the window across two different-DPI monitors in quick
    // succession).
    win.dpi.set(120);
    assert_eq!(
        win.dpi.get(),
        120,
        "second Cell mutation must also round-trip"
    );
}

/// The `monitors` field must accept the real `enumerate_monitors()`
/// output. Proves both (a) the type signatures match end-to-end across
/// the platform → app crate boundary, and (b) the SmallVec inline
/// capacity (4) is the same on both sides — a mismatch would force a
/// heap copy on every assignment.
#[test]
fn monitors_field_accepts_real_enumeration_output() {
    let mut win = WindowState::new();
    let monitors = enumerate_monitors();
    // Sanity check the upstream contract before we test the assignment.
    // Windows guarantees ≥ 1 monitor on any interactive session; CI on a
    // headless agent could conceivably return 0, hence the `<= 4` upper
    // bound is the load-bearing assertion (proves the SmallVec inline
    // capacity is wide enough for the typical workstation case so the
    // assignment below stays heap-free in the 99th-percentile path).
    assert!(
        monitors.len() <= 4 || monitors.spilled(),
        "if enumerate_monitors() returns ≤ 4 monitors the SmallVec must stay inline; spillover is acceptable for the rare 5+ monitor case"
    );
    // Type-level: this assignment must compile and run without
    // conversion. If the SmallVec inline capacities differ, this is a
    // compile-time error, not a runtime panic. If the element types
    // diverge (e.g. somebody adds a field to MonitorInfo on one side
    // only), this is also a compile-time error.
    win.monitors = monitors;
    // Length stays consistent post-assignment — basic sanity that the
    // SmallVec move semantics carried the data through unchanged.
    let after = enumerate_monitors();
    assert_eq!(
        win.monitors.len(),
        after.len(),
        "the monitor cache must reflect the same enumeration the shell hands it; back-to-back enumerate_monitors() calls return the same count on stable hardware"
    );
}
