//! Phase 2.3 / Ruling 1 — smoke tests for `zone_active_monitor_index`.
//!
//! Pure-function tests using fabricated `MonitorInfo` fixtures so they are
//! independent of CI hardware. The Win32 enumeration path is exercised in
//! `monitor_smoke.rs`; this file only verifies the zone-to-monitor mapping
//! arithmetic.

use std::borrow::Cow;

use bento_nano_platform::{MonitorInfo, RectI32, zone_active_monitor_index};
use bento_nano_zone::{Zone, ZoneId};

fn fake(left: i32, top: i32, right: i32, bottom: i32, primary: bool) -> MonitorInfo {
    let rect = RectI32 {
        left,
        top,
        right,
        bottom,
    };
    MonitorInfo {
        // Sentinel handle — `zone_active_monitor_index` never dereferences it.
        hmonitor: core::ptr::null_mut(),
        rect_screen: rect,
        rect_work: rect,
        is_primary: primary,
    }
}

fn zone_at(x: i32, y: i32, w: i32, h: i32) -> Zone {
    Zone::new(ZoneId(1), Cow::Borrowed("z"), x, y, w, h)
}

#[test]
fn zone_active_monitor_index_handles_offscreen_zone() {
    // Two-monitor fixture: primary at (0,0), secondary to the right.
    let monitors = [
        fake(0, 0, 1920, 1080, true),
        fake(1920, 0, 3840, 1080, false),
    ];
    // Zone centre at (-10_000, -10_000) is fully off both monitors;
    // contract says fall back to index 0 (primary).
    let z = zone_at(-10_100, -10_100, 200, 200);
    assert_eq!(zone_active_monitor_index(&z, &monitors), 0);

    // Empty monitor list also returns 0 (degenerate caller-state guard).
    let z2 = zone_at(100, 100, 100, 100);
    assert_eq!(zone_active_monitor_index(&z2, &[]), 0);

    // Centre inside the secondary monitor returns its index.
    let z3 = zone_at(2900, 500, 200, 200); // centre = (3000, 600)
    assert_eq!(zone_active_monitor_index(&z3, &monitors), 1);
}
