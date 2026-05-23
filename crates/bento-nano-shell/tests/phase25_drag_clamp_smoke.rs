//! Phase 2.5 smoke tests — `clamp_zone_to_monitors` cross-monitor drag
//! clamping helper + production wire-up at the WM_MOUSEMOVE drag branch.
//!
//! Wave 8 / Phase 2.4 closed the `monitors` field §17 contract loop with a
//! single read site (`zone_active_monitor_index` at WM_LBUTTONDOWN). Wave 9
//! / Phase 2.5 adds the second read site that turns the cache from a
//! routing oracle into an active drag-time invariant: every drag-induced
//! position update is followed by a clamp so the user can never strand a
//! zone fully off-screen by dragging past every monitor's work area.
//!
//! Production caller:
//!   `bento-nano-shell/src/main.rs` — `handle_mouse_move`, inside the
//!   `app.zone_drag` branch, immediately after the `z.x / z.y` write and
//!   under the same outer `app.borrow_mut()` scope (§13' R7 single
//!   borrow_mut at write site preserved; the read-only `win` borrow lives
//!   in a sibling scope that cannot alias the `app` mutable borrow because
//!   they're independent RefCells).
//!
//! These tests cover the helper itself (pure arithmetic, no Win32, runs in
//! `cargo test` without a live HWND). The drag handler integration is
//! exercised end-to-end by the existing phase24_smoke + phase21_smoke
//! drag-RefCell-discipline tests; here we lock the contract by direct
//! invocation against fabricated `MonitorInfo` topologies (§13 no-mocks
//! holds — we use `MonitorInfo` literals, never a mock trait).
//!
//! Spec lock:
//!   §10  helper is allocation-free (asserted indirectly via `cargo bloat`)
//!   §11  no panic; tests use `assert!` per §11.1 test-only carve-out
//!   §13  no mocks — fabricated `MonitorInfo` literals matching
//!        `enumerate_monitors()` shape; real call also exercised
//!   §17  helper writer + `main.rs` reader landed same wave (this file is
//!        the contract proof; reader file:line listed in the report)

#![forbid(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;

use bento_nano_platform::{MonitorInfo, RectI32, clamp_zone_to_monitors, enumerate_monitors};
use bento_nano_zone::{Zone, ZoneId};

/// Build a `MonitorInfo` from a screen rect for tests. `hmonitor` is a
/// sentinel null pointer because `clamp_zone_to_monitors` is a pure
/// arithmetic helper and never dereferences it. `rect_screen` and
/// `rect_work` are kept identical so the test's "work area" geometry is
/// the same as the screen geometry, which is the worst case (no taskbar
/// shrinkage to forgive minor off-by-one errors).
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

/// `[zone.x, zone.x+w) × [zone.y, zone.y+h)` ∩ `m.rect_work` non-empty?
/// Mirrors the helper's internal predicate so tests can assert the
/// post-clamp invariant without depending on the helper's private fn.
fn zone_overlaps_work(zone: &Zone, m: &MonitorInfo) -> bool {
    let zr = zone.x + zone.w;
    let zb = zone.y + zone.h;
    zone.x < m.rect_work.right
        && zr > m.rect_work.left
        && zone.y < m.rect_work.bottom
        && zb > m.rect_work.top
}

/// Baseline: a zone fully inside `monitors[0].rect_work` is a no-op. The
/// helper must not nudge a zone that already satisfies the invariant.
#[test]
fn zone_overlapping_monitor_is_unchanged() {
    let monitors = [fake_monitor(0, 0, 1920, 1080, true)];
    let mut zone = zone_at(400, 300, 200, 150);
    let (bx, by, bw, bh) = (zone.x, zone.y, zone.w, zone.h);
    clamp_zone_to_monitors(&mut zone, &monitors);
    assert_eq!(zone.x, bx, "x must not change for an in-bounds zone");
    assert_eq!(zone.y, by, "y must not change for an in-bounds zone");
    assert_eq!(zone.w, bw, "width is never modified");
    assert_eq!(zone.h, bh, "height is never modified");
}

/// Two monitors arranged left-right. Zone dragged to (5000, 5000) — far
/// past both. Clamp must land inside `monitors[1].rect_work` (the closest
/// to the off-screen position) and leave width / height untouched.
#[test]
fn zone_dragged_offscreen_clamps_to_nearest_monitor() {
    let monitors = [
        fake_monitor(0, 0, 1920, 1080, true),
        fake_monitor(1920, 0, 3840, 1080, false),
    ];
    let mut zone = zone_at(5000, 5000, 200, 150);
    clamp_zone_to_monitors(&mut zone, &monitors);
    assert!(
        zone_overlaps_work(&zone, &monitors[1]),
        "clamped zone {:?} must overlap monitors[1].rect_work {:?}",
        zone,
        monitors[1].rect_work
    );
    assert!(
        !zone_overlaps_work(&zone, &monitors[0]),
        "clamped zone should be on the closer monitor, not back on monitors[0]"
    );
    assert_eq!(zone.w, 200, "width is never modified");
    assert_eq!(zone.h, 150, "height is never modified");
}

/// Defensive: an empty monitor slice (the brief window between
/// WM_NCCREATE and the first paint() seed) must be a no-op. Production
/// callers reach this branch when the cache has not been populated yet.
#[test]
fn empty_monitor_slice_is_no_op() {
    let mut zone = zone_at(-100, -100, 200, 150);
    let (bx, by, bw, bh) = (zone.x, zone.y, zone.w, zone.h);
    clamp_zone_to_monitors(&mut zone, &[]);
    assert_eq!(zone.x, bx);
    assert_eq!(zone.y, by);
    assert_eq!(zone.w, bw);
    assert_eq!(zone.h, bh);
}

/// Zone partially off the left edge of monitors[0] (still overlapping by
/// ≥ 1 px). Half-open semantics say this is "visible" so the helper must
/// be a no-op — moving it would needlessly snap a zone the user is mid-
/// drag of and cause visible jitter.
#[test]
fn zone_partially_offscreen_left_is_unchanged() {
    let monitors = [fake_monitor(0, 0, 1920, 1080, true)];
    // zone.x = -100 → rect [-100, 100). Overlaps [0, 1920) at columns
    // 0..100 (100 px visible). Visible per the half-open rule.
    let mut zone = zone_at(-100, 100, 200, 150);
    let (bx, by) = (zone.x, zone.y);
    clamp_zone_to_monitors(&mut zone, &monitors);
    assert_eq!(
        zone.x, bx,
        "still-overlapping zone must not be nudged (would cause drag jitter)"
    );
    assert_eq!(zone.y, by);
}

/// Zone fully off the left edge (no overlap with any monitor). Helper
/// must clamp to overlap monitors[0].rect_work by exactly 1 px on the
/// crossed axis; the other axis is left at its valid value.
#[test]
fn zone_fully_offscreen_left_snaps_inside_left_monitor() {
    let monitors = [
        fake_monitor(0, 0, 1920, 1080, true),
        fake_monitor(1920, 0, 3840, 1080, false),
    ];
    // zone.x + w = -201 + 200 = -1 → fully left of [0, 1920). Centre
    // x = -101, far closer to monitors[0] than monitors[1] (centre dist
    // 101 vs 2021 to monitors[1].left = 1920).
    let mut zone = zone_at(-201, 100, 200, 150);
    clamp_zone_to_monitors(&mut zone, &monitors);
    assert!(
        zone_overlaps_work(&zone, &monitors[0]),
        "clamped zone {:?} must overlap monitors[0].rect_work {:?}",
        zone,
        monitors[0].rect_work
    );
    // Half-open semantics + nearest-edge clamp: zone.x must equal
    // `monitors[0].rect_work.left - w + 1 = 0 - 200 + 1 = -199` so the
    // overlap is exactly the 1-px column at x=0.
    assert_eq!(
        zone.x, -199,
        "should snap to the minimal valid x for left edge"
    );
    assert_eq!(zone.y, 100, "y was already valid; must not be touched");
    assert_eq!(zone.w, 200);
    assert_eq!(zone.h, 150);
}

/// Defensive: zero-width or zero-height zones are a no-op. The "overlap"
/// predicate is undefined for empty rects under half-open semantics, so
/// the helper bails rather than synthesising a position.
#[test]
fn zero_area_zone_is_no_op() {
    let monitors = [fake_monitor(0, 0, 1920, 1080, true)];

    let mut zw0 = zone_at(5000, 5000, 0, 150);
    let (bw_x, bw_y) = (zw0.x, zw0.y);
    clamp_zone_to_monitors(&mut zw0, &monitors);
    assert_eq!((zw0.x, zw0.y), (bw_x, bw_y));

    let mut zh0 = zone_at(5000, 5000, 200, 0);
    let (bh_x, bh_y) = (zh0.x, zh0.y);
    clamp_zone_to_monitors(&mut zh0, &monitors);
    assert_eq!((zh0.x, zh0.y), (bh_x, bh_y));
}

/// Real `enumerate_monitors()` round-trip — proves the helper's input
/// shape matches the production cache populated in `paint()` /
/// WM_DPICHANGED / WM_DISPLAYCHANGE handlers (§13 no-mocks proof). At
/// least one monitor must be reported on any reachable test environment;
/// post-clamp the zone must overlap one of them.
#[test]
fn enumerate_monitors_round_trip_into_clamp() {
    let monitors = enumerate_monitors();
    assert!(
        !monitors.is_empty(),
        "Win32 must report at least one attached monitor"
    );
    // Far off-screen relative to any reasonable real display.
    let mut zone = zone_at(50_000, 50_000, 200, 150);
    clamp_zone_to_monitors(&mut zone, &monitors);
    let any_overlap = monitors.iter().any(|m| zone_overlaps_work(&zone, m));
    assert!(
        any_overlap,
        "post-clamp zone {:?} must overlap at least one of {:?}",
        zone, monitors
    );
}
