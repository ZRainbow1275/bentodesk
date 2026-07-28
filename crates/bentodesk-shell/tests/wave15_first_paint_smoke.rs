//! Wave 15 smoke tests — `WindowState.first_paint_done` one-shot guard.
//!
//! Tier 0 #29/#31 + #28 implementation guards the post-first-paint
//! `EmptyWorkingSet(GetCurrentProcess())` + `ID2D1Device::ClearResources(0)`
//! call site with this `Cell<bool>`. The wndproc-side wire-up requires a
//! live HWND + message pump and cannot run inside `cargo test`, so we
//! cover the data-shape contract here and let the smoke build prove the
//! integration. Production reader/writer pair lives in
//! `bentodesk-shell/src/main.rs::paint` (the same `if !...get()` site
//! that issues the trim and then `set(true)` flips the latch).
//!
//! Spec lock:
//!   §10  no allocation in hot path (Cell<bool> is interior mutability)
//!   §11  no panic; tests use `assert!` per §11.1 test-only carve-out
//!   §13  no mocks — exercises the real `WindowState::default` path
//!   §17  `first_paint_done` must have a closed reader/writer pair; the
//!        production site is the only reader+writer, this test asserts
//!        the data-shape so the latch can never silently regress to
//!        always-true (which would make the post-first-paint trim a no-op
//!        forever and lose the Tier 0 #29/#31 working-set savings).

#![forbid(unsafe_op_in_unsafe_fn)]

use bentodesk_app::WindowState;

/// Default state of the latch: `false` so the very first WM_PAINT triggers
/// the trim. Picked over `true` because a never-flipped `true` default
/// would silently disable the entire Tier 0 #29/#31 lever — code review
/// cannot easily catch that regression so we make the default itself the
/// load-bearing contract.
#[test]
fn first_paint_done_default_is_false_on_window_state_new() {
    let win = WindowState::new();
    assert!(
        !win.first_paint_done.get(),
        "Wave 15 contract: fresh WindowState must report first_paint_done == false so the very first successful render triggers EmptyWorkingSet + ClearResources exactly once",
    );
}

/// `Cell<bool>::set(true)` round-trip — guards against a regression where
/// somebody swaps the field type to a non-Copy aggregate and breaks the
/// production paint-site's interior-mutability path. The shell does
/// `win_ref.first_paint_done.set(true)` exactly like this test.
#[test]
fn first_paint_done_setter_round_trips_via_cell() {
    let win = WindowState::new();
    win.first_paint_done.set(true);
    assert!(
        win.first_paint_done.get(),
        "Cell<bool> set/get must round-trip; this is the contract the WM_PAINT one-shot trim relies on to never re-trigger after the first successful frame",
    );
}

/// One-shot semantics — once flipped to `true`, the latch must stay
/// `true`. Re-trimming working set on every paint would page-fault hot
/// resources back in on the next frame and defeat the whole point of the
/// post-first-paint hook. This test mirrors the production guard pattern
/// (`if !first_paint_done.get() { ...; first_paint_done.set(true); }`)
/// and verifies the second iteration short-circuits.
#[test]
fn first_paint_done_one_shot_guard_short_circuits_after_first_set() {
    let win = WindowState::new();
    let mut trim_count = 0u32;
    // Simulate the production guard pattern over three "paints".
    for _ in 0..3 {
        if !win.first_paint_done.get() {
            // Stand-in for the EmptyWorkingSet / ClearResources call pair.
            trim_count += 1;
            win.first_paint_done.set(true);
        }
    }
    assert_eq!(
        trim_count, 1,
        "Wave 15 contract: post-first-paint trim must fire exactly once across the lifetime of the window; re-trimming would page-fault hot resources back in",
    );
    assert!(
        win.first_paint_done.get(),
        "after the one-shot fires the latch must stay true forever",
    );
}
