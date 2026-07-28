//! Phase 2.3 / Ruling 1 — smoke tests for `bentodesk-platform::monitor`.
//!
//! These touch the real Win32 enumeration path (the principle in §13 says
//! no mocks). Windows guarantees ≥ 1 monitor on any interactive session,
//! so the assertions below are stable across CI hardware.

use bentodesk_platform::{
    MonitorInfo, RectI32, enumerate_monitors, monitor_from_point, primary_monitor,
};

#[test]
fn monitor_enumerate_returns_at_least_one() {
    let monitors = enumerate_monitors();
    assert!(
        !monitors.is_empty(),
        "Windows guarantees at least one display monitor; enumeration returned none"
    );
    let primary_count = monitors.iter().filter(|m| m.is_primary).count();
    assert!(
        primary_count <= 1,
        "GetMonitorInfoW must flag at most one monitor as primary; got {primary_count}"
    );
}

#[test]
fn primary_monitor_is_marked_primary() {
    let p = primary_monitor();
    assert!(
        p.is_primary,
        "primary_monitor() must return a monitor flagged is_primary=true"
    );
    assert!(
        p.rect_screen.width() > 0,
        "primary screen rect must have positive width"
    );
    assert!(
        p.rect_screen.height() > 0,
        "primary screen rect must have positive height"
    );
    assert!(
        p.rect_work.width() <= p.rect_screen.width(),
        "work area cannot exceed screen area horizontally"
    );
    assert!(
        p.rect_work.height() <= p.rect_screen.height(),
        "work area cannot exceed screen area vertically"
    );
}

#[test]
fn rect_i32_width_height_correct() {
    let r = RectI32 {
        left: 100,
        top: 50,
        right: 400,
        bottom: 300,
    };
    assert_eq!(r.width(), 300);
    assert_eq!(r.height(), 250);

    // Negative-origin secondary monitor (Windows places these to the
    // left/above the primary).
    let neg = RectI32 {
        left: -1920,
        top: -1080,
        right: 0,
        bottom: 0,
    };
    assert_eq!(neg.width(), 1920);
    assert_eq!(neg.height(), 1080);
}

#[test]
fn rect_i32_contains_point_inclusive_top_left_exclusive_bottom_right() {
    let r = RectI32 {
        left: 0,
        top: 0,
        right: 100,
        bottom: 100,
    };
    // Inclusive top/left.
    assert!(r.contains_point(0, 0));
    assert!(r.contains_point(50, 50));
    // Exclusive bottom/right — matches Win32 PtInRect semantics.
    assert!(!r.contains_point(100, 50));
    assert!(!r.contains_point(50, 100));
    assert!(!r.contains_point(100, 100));
    // Outside.
    assert!(!r.contains_point(-1, 50));
    assert!(!r.contains_point(50, -1));
    assert!(!r.contains_point(101, 101));
}

#[test]
fn monitor_from_point_at_origin_returns_a_monitor() {
    // (0,0) is on the primary monitor's top-left under default Windows
    // setups, but even if the user has the primary somewhere else
    // MonitorFromPoint(MONITOR_DEFAULTTOPRIMARY) falls back to primary.
    // Either way we get a valid MonitorInfo with a non-degenerate rect.
    let m: MonitorInfo = monitor_from_point(0, 0);
    assert!(
        m.rect_screen.width() > 0 && m.rect_screen.height() > 0,
        "monitor_from_point fallback must yield a non-degenerate screen rect"
    );
}
