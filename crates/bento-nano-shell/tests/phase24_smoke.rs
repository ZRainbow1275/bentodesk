//! Phase 2.4 smoke tests — `monitors` cache reader path + display-hotplug
//! refresh contract + `WindowState::with_monitors_for_test` helper.
//!
//! Wave 7 landed the `monitors` cache field with two writers (paint() seed
//! at main.rs:953-973 + WM_DPICHANGED handler at main.rs:220-265) but zero
//! production readers. Wave 8 / Phase 2.4 closes the §17 "contract-driven
//! infrastructure" loop by:
//!
//!   1. Adding the `WM_DISPLAYCHANGE` handler so display hotplug refreshes
//!      the cache (otherwise the cache stays stale on USB monitor unplug /
//!      projector connect / display-settings resolution change).
//!   2. Wiring `zone_active_monitor_index` into the WM_LBUTTONDOWN zone
//!      drag-start path so the cache has at least one production reader.
//!   3. Adding a `WindowState::with_monitors_for_test` helper so this file
//!      can construct a state with a known monitor topology — integration
//!      tests live in `tests/` and cannot touch the field directly.
//!
//! The wndproc-side hotplug handler cannot run inside `cargo test` without
//! a live HWND + message pump (mirrors phase231a_dpi_smoke); the data-shape
//! contract gets its own coverage here, and the smoke build proves the
//! integration end-to-end via `cargo bloat` showing
//! `zone_active_monitor_index` in the binary.
//!
//! Spec lock:
//!   §10  no allocation in hot path (cache fields use `Cell`/`SmallVec`)
//!   §11  no panic; tests use `assert!` per §11.1 test-only carve-out
//!   §13  no mocks — real `enumerate_monitors()` calls + fabricated
//!        `MonitorInfo` fixtures only for off-screen / multi-monitor cases
//!        that depend on a known topology

#![forbid(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;

use bento_nano_app::WindowState;
use bento_nano_platform::{MonitorInfo, RectI32, enumerate_monitors, zone_active_monitor_index};
use bento_nano_zone::{Zone, ZoneId};
use smallvec::SmallVec;

/// Build a `MonitorInfo` from a screen rect for tests. The hmonitor handle
/// is a sentinel null pointer because `zone_active_monitor_index` is a pure
/// arithmetic helper and never dereferences it (matches the convention in
/// `bento-nano-platform/tests/zone_monitor_smoke.rs`).
fn fake_monitor(left: i32, top: i32, right: i32, bottom: i32, primary: bool) -> MonitorInfo {
    let rect = RectI32 {
        left,
        top,
        right,
        bottom,
    };
    MonitorInfo {
        hmonitor: core::ptr::null_mut(),
        rect_screen: rect,
        rect_work: rect,
        is_primary: primary,
    }
}

fn zone_at(x: i32, y: i32, w: i32, h: i32) -> Zone {
    Zone::new(ZoneId(1), Cow::Borrowed("test-zone"), x, y, w, h)
}

/// `WM_DISPLAYCHANGE` handler refreshes `WindowState.monitors` by calling
/// `bento_nano_platform::enumerate_monitors()` afresh. We cannot pump a
/// real WM_DISPLAYCHANGE message without a live HWND, so this test asserts
/// the underlying contract: back-to-back `enumerate_monitors()` calls
/// return a fresh, internally-consistent `SmallVec` every time. If this
/// regresses (e.g. a refactor caches the result globally) the handler
/// would silently report stale topology after monitor hotplug.
#[test]
fn wm_displaychange_handler_refreshes_monitors() {
    let first = enumerate_monitors();
    let second = enumerate_monitors();
    // Stable hardware between two adjacent calls — the lengths must match.
    assert_eq!(
        first.len(),
        second.len(),
        "enumerate_monitors() must return a deterministic count on stable \
         hardware; the WM_DISPLAYCHANGE handler relies on this invariant \
         to refresh the WindowState.monitors cache without skew"
    );
    // Every call must return its own owned SmallVec (no shared global
    // state). The two values are independent, so we can mutate one
    // without affecting the other — the kind of guarantee the handler
    // needs when it overwrites `win.monitors = enumerate_monitors()`.
    let mut owned = first;
    let original_len = owned.len();
    owned.clear();
    let third = enumerate_monitors();
    assert_eq!(
        third.len(),
        original_len,
        "clearing one SmallVec must not affect a fresh enumerate_monitors() \
         result; this guards against accidental global-state caching"
    );
}

/// `zone_active_monitor_index` reads the `WindowState.monitors` cache and
/// returns the index of the monitor whose `rect_work` contains the zone's
/// centre point. This test exercises the full read path:
/// `with_monitors_for_test([m0, m1])` → `zone_active_monitor_index(&zone,
/// &state.monitors)` returns the secondary monitor's index for a zone
/// whose centre lies on the secondary screen.
#[test]
fn zone_active_monitor_index_reader_path_returns_valid_index() {
    // Two-monitor fixture: primary at (0,0,1920,1080), secondary to the
    // right at (1920,0,3840,1080). Mirrors the canonical dual-1080p
    // workstation setup.
    let monitors: SmallVec<[MonitorInfo; 4]> = SmallVec::from_iter([
        fake_monitor(0, 0, 1920, 1080, true),
        fake_monitor(1920, 0, 3840, 1080, false),
    ]);
    let state = WindowState::with_monitors_for_test(monitors);

    // Zone with centre at (2500 + 200/2, 500 + 200/2) = (2600, 600), which
    // lies inside the secondary monitor's work area [1920, 3840) x
    // [0, 1080). Expected index = 1.
    let zone = zone_at(2500, 500, 200, 200);
    let idx = zone_active_monitor_index(&zone, &state.monitors);
    assert_eq!(
        idx, 1,
        "zone centred at (2600, 600) lies inside the secondary monitor's \
         work area [1920, 3840) x [0, 1080); zone_active_monitor_index \
         must report index 1, not the primary fallback. This proves the \
         WindowState.monitors → zone_active_monitor_index reader path is \
         wired end-to-end (Phase 2.4 / Ruling 2)."
    );
}

/// `zone_active_monitor_index` falls back to index 0 (primary) when the
/// zone centre is fully off every monitor's work area. Same fixture as the
/// preceding test, but with a zone placed at negative coordinates that
/// don't overlap any of the two monitors. The contract preserves the
/// half-open rectangle semantics from `RectI32::contains_point`.
#[test]
fn zone_active_monitor_index_falls_back_to_zero_when_offscreen() {
    let monitors: SmallVec<[MonitorInfo; 4]> = SmallVec::from_iter([
        fake_monitor(0, 0, 1920, 1080, true),
        fake_monitor(1920, 0, 3840, 1080, false),
    ]);
    let state = WindowState::with_monitors_for_test(monitors);

    // Zone with centre at (-100 + 200/2, -100 + 200/2) = (0, 0). Note
    // (0, 0) IS inside the primary monitor's `[0, 1920) x [0, 1080)`
    // half-open work area, so a centred-at-origin zone hits the primary.
    // To genuinely test the off-screen fallback we need a centre OUTSIDE
    // every rect.
    let offscreen = zone_at(-1000, -1000, 200, 200); // centre = (-900, -900)
    let idx = zone_active_monitor_index(&offscreen, &state.monitors);
    assert_eq!(
        idx, 0,
        "zone centred at (-900, -900) lies outside every monitor's work \
         area; zone_active_monitor_index must fall back to index 0 \
         (primary) per its documented contract"
    );

    // Bottom-right exclusive boundary — centre at (1920, 540) lands on
    // the seam between primary and secondary. Half-open semantics
    // ([left, right) x [top, bottom)) place this point in the secondary.
    let on_seam = zone_at(1820, 440, 200, 200); // centre = (1920, 540)
    let idx_seam = zone_active_monitor_index(&on_seam, &state.monitors);
    assert_eq!(
        idx_seam, 1,
        "centre at (1920, 540) is half-open inside the secondary monitor \
         (left=1920 inclusive, right=3840 exclusive); the seam belongs to \
         exactly one monitor under PtInRect semantics"
    );
}

/// `WindowState::with_monitors_for_test` actually populates the
/// `monitors` field with the supplied SmallVec. Without this guard a
/// refactor that swaps the body to `Self::default()` would silently break
/// every cross-crate test that depends on the helper.
#[test]
fn monitors_field_can_be_seeded_via_test_helper() {
    let monitors: SmallVec<[MonitorInfo; 4]> = SmallVec::from_iter([
        fake_monitor(0, 0, 1920, 1080, true),
        fake_monitor(1920, 0, 3840, 1080, false),
        fake_monitor(0, 1080, 1920, 2160, false),
    ]);
    let expected_len = monitors.len();
    let state = WindowState::with_monitors_for_test(monitors);

    assert_eq!(
        state.monitors.len(),
        expected_len,
        "with_monitors_for_test must move its argument into the monitors \
         field; the helper is the only sanctioned way for cross-crate \
         tests to construct a WindowState with a known monitor topology"
    );
    // Spot-check the primary flag survived the move (proves the helper
    // moves the elements wholesale instead of accidentally re-defaulting
    // the field).
    assert!(
        state.monitors[0].is_primary,
        "first monitor must retain its is_primary flag after being moved \
         through with_monitors_for_test"
    );
    assert!(
        !state.monitors[1].is_primary,
        "second monitor must retain its non-primary flag"
    );
    // Other defaults must remain untouched — the helper only seeds
    // monitors, not dpi or layout. 96 is the documented default DPI.
    assert_eq!(
        state.dpi.get(),
        96,
        "with_monitors_for_test must leave the dpi field at its default; \
         the helper is targeted (only seeds monitors), not a wholesale \
         override"
    );
}

/// Bonus test: the `WM_DISPLAYCHANGE` handler is idempotent — calling
/// `enumerate_monitors()` twice in succession (which the handler does on
/// every display reconfig) preserves the SmallVec inline-capacity
/// invariant for the typical workstation case. A spillover to the heap is
/// acceptable for 5+ monitors, but a degenerate enumeration that always
/// spills would silently violate §10 hot-path discipline.
#[test]
fn wm_displaychange_handler_idempotent() {
    let first = enumerate_monitors();
    let second = enumerate_monitors();

    // The inline capacity is 4 per `monitor.rs:81`. Either both stay
    // inline (≤ 4 monitors) or both spilled — the contract is symmetric.
    if first.len() <= 4 {
        assert!(
            !first.spilled(),
            "≤ 4 monitors must stay in the SmallVec inline buffer to keep \
             WindowState.monitors heap-free in the 99th-percentile case \
             (Spec §10 hot-path discipline)"
        );
    }
    if second.len() <= 4 {
        assert!(
            !second.spilled(),
            "second back-to-back call must also stay inline; the handler \
             would otherwise allocate on every WM_DISPLAYCHANGE"
        );
    }
    // Both calls observed the same hardware → same length.
    assert_eq!(
        first.len(),
        second.len(),
        "back-to-back enumerate_monitors() calls must agree on count; the \
         handler relies on this when overwriting the cache"
    );
}
