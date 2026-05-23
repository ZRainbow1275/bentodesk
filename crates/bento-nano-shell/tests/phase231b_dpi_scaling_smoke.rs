//! Phase 2.3.1b smoke tests — DPI scaling math (logical ↔ device).
//!
//! Anchors the Phase 2.3.1b contract:
//!   * `bento_nano_style::dpi::scale_factor` returns `dpi / 96` (with a
//!     0-DPI guard).
//!   * `device_to_logical_f32` is a true inverse of `dpi/96` scale at the
//!     canonical Win32 DPI tiers (96 / 120 / 144 / 192).
//!   * Hit-testing a logical-space widget against device-space click input
//!     produces the same hit decision regardless of DPI — proving the
//!     wndproc-side `device_to_logical_f32` conversion is sufficient.
//!
//! Spec lock:
//!   §10  no allocation in hot path (helpers are pure float math)
//!   §11  no panic; tests use `assert!` per §11.1 test-only carve-out
//!   §13  no mocks — exercises the real public API end-to-end
//!   §17  every public symbol added in Phase 2.3.1b is reached from a
//!        real call site (renderer reads `scale_factor`, wndproc reads
//!        `device_to_logical_f32`, renderer reads `device_size_to_logical`)

#![forbid(unsafe_op_in_unsafe_fn)]

use bento_nano_style::Size;
use bento_nano_style::dpi::{
    BASE_DPI, device_size_to_logical, device_to_logical_f32, scale_factor,
};

/// Shared epsilon for f32 round-trip comparisons. `1e-4` covers the worst
/// case (192 DPI ÷ 96 = exactly 2.0, but intermediate fractional DPI tiers
/// like 120 / 96 = 1.25 introduce only mantissa-noise rounding).
const EPS: f32 = 1e-4;

/// `device_to_logical_f32` must invert the `dpi/96` scale across every
/// Windows shipping DPI tier. `120` (125%) is the canonical "high-DPI
/// laptop" value, `144` (150%) ships on most Microsoft Surface devices,
/// `192` (200%) is the documented MS DPI smoke value used in every Win32
/// sample.
#[test]
fn device_to_logical_inverts_scale_at_96_120_144_192_dpi() {
    // BASE_DPI is the documented constant — re-asserted here so a typo on
    // either side of the test/source pair fails loudly.
    assert_eq!(
        BASE_DPI, 96,
        "BASE_DPI must equal Win32 USER_DEFAULT_SCREEN_DPI"
    );

    for dpi in [96u32, 120, 144, 192] {
        let logical_expected = 100.0_f32;
        // Synthesise the device-pixel value the renderer would project
        // onto via `SetTransform(scale_factor(dpi))`. Then run the
        // wndproc-side conversion and confirm we land back on the
        // logical input.
        let s = scale_factor(dpi);
        let device = logical_expected * s;
        let logical_back = device_to_logical_f32(device, dpi);
        assert!(
            (logical_back - logical_expected).abs() < EPS,
            "round-trip failed at {dpi} DPI: logical {logical_expected} × {s} = device {device} → logical {logical_back} (delta {})",
            (logical_back - logical_expected).abs(),
        );
    }
}

/// Hit-test contract — at 96 DPI a logical (100, 100) widget has device
/// extent (100, 100); at 192 DPI the same logical widget has device extent
/// (200, 200). A click at device (150, 150) at 192 DPI converts to logical
/// (75, 75) which falls inside the widget rect (logical 0..100 × 0..100).
/// This is the load-bearing scenario the wndproc relies on.
#[test]
fn device_click_at_high_dpi_resolves_to_logical_hit() {
    // Widget lives at logical (0, 0) with logical size (100, 100). The
    // renderer's per-frame `SetTransform(scale)` projects this to device
    // size (100*scale, 100*scale) automatically.
    let widget_logical_x = 0.0_f32;
    let widget_logical_y = 0.0_f32;
    let widget_logical_w = 100.0_f32;
    let widget_logical_h = 100.0_f32;

    // 96 DPI → device click (100, 100) is just outside the widget (rect is
    // [0, 100) × [0, 100) per `ui::hit_test`'s `<` upper bounds).
    let dpi_lo = 96u32;
    let click_device = (50.0_f32, 50.0_f32);
    let click_logical = (
        device_to_logical_f32(click_device.0, dpi_lo),
        device_to_logical_f32(click_device.1, dpi_lo),
    );
    assert!(
        click_logical.0 >= widget_logical_x
            && click_logical.0 < widget_logical_x + widget_logical_w
            && click_logical.1 >= widget_logical_y
            && click_logical.1 < widget_logical_y + widget_logical_h,
        "device click {click_device:?} at 96 DPI must hit widget at logical (0,0)-(100,100); converted to logical {click_logical:?}",
    );

    // 192 DPI → the same widget covers device (0, 0)..(200, 200). A click
    // at device (150, 150) converts to logical (75, 75), which is INSIDE
    // the widget. Without the conversion the click would land at logical
    // (150, 150) and miss.
    let dpi_hi = 192u32;
    let click_device_hi = (150.0_f32, 150.0_f32);
    let click_logical_hi = (
        device_to_logical_f32(click_device_hi.0, dpi_hi),
        device_to_logical_f32(click_device_hi.1, dpi_hi),
    );
    assert!(
        (click_logical_hi.0 - 75.0).abs() < EPS,
        "expected logical x = 75.0, got {}",
        click_logical_hi.0,
    );
    assert!(
        (click_logical_hi.1 - 75.0).abs() < EPS,
        "expected logical y = 75.0, got {}",
        click_logical_hi.1,
    );
    assert!(
        click_logical_hi.0 >= widget_logical_x
            && click_logical_hi.0 < widget_logical_x + widget_logical_w
            && click_logical_hi.1 >= widget_logical_y
            && click_logical_hi.1 < widget_logical_y + widget_logical_h,
        "device click {click_device_hi:?} at 192 DPI must hit widget at logical (0,0)-(100,100); converted to logical {click_logical_hi:?}",
    );

    // Sanity — at 192 DPI a click at device (250, 250) converts to logical
    // (125, 125), which is OUTSIDE the widget. Proves the conversion isn't
    // a no-op that always reports hit.
    let click_miss = (250.0_f32, 250.0_f32);
    let click_miss_logical = (
        device_to_logical_f32(click_miss.0, dpi_hi),
        device_to_logical_f32(click_miss.1, dpi_hi),
    );
    assert!(
        click_miss_logical.0 >= widget_logical_x + widget_logical_w
            || click_miss_logical.1 >= widget_logical_y + widget_logical_h,
        "device click {click_miss:?} at 192 DPI must MISS widget; converted to logical {click_miss_logical:?}",
    );
}

/// Viewport sizing contract — `Renderer::render` calls
/// `device_size_to_logical(self.width × self.height, win.dpi)` to compute
/// `app.viewport`. At 96 DPI the conversion is identity (regression-safe
/// with the pre-Phase-2.3.1b code path); at 192 DPI a 960×640 backbuffer
/// becomes a 480×320 logical viewport so the same layout source produces
/// the same logical rects regardless of physical resolution.
#[test]
fn device_size_to_logical_matches_renderer_contract() {
    // 96 DPI identity — pre-Phase-2.3.1b code paths must keep working.
    let device = Size {
        width: 480.0,
        height: 320.0,
    };
    let logical_96 = device_size_to_logical(device, 96);
    assert!(
        (logical_96.width - 480.0).abs() < EPS && (logical_96.height - 320.0).abs() < EPS,
        "96 DPI must be identity; got {logical_96:?}",
    );

    // 192 DPI — physical 960×640 backbuffer → logical 480×320 viewport.
    let device_hi = Size {
        width: 960.0,
        height: 640.0,
    };
    let logical_192 = device_size_to_logical(device_hi, 192);
    assert!(
        (logical_192.width - 480.0).abs() < EPS && (logical_192.height - 320.0).abs() < EPS,
        "192 DPI must halve dimensions; got {logical_192:?}",
    );

    // 144 DPI (150%) — 720×480 device → 480×320 logical. Spot-check a
    // non-power-of-two scale to catch regressions where someone swaps the
    // implementation for an integer divide.
    let device_mid = Size {
        width: 720.0,
        height: 480.0,
    };
    let logical_144 = device_size_to_logical(device_mid, 144);
    assert!(
        (logical_144.width - 480.0).abs() < EPS && (logical_144.height - 320.0).abs() < EPS,
        "144 DPI must scale by 96/144 = 2/3; got {logical_144:?}",
    );
}

/// Defensive contract — `scale_factor(0)` MUST NOT divide through zero.
/// `GetDpiForWindow` is documented to return 0 only on Win10 1607- (rare),
/// but the wndproc fallback turns that into 96 before write. This test
/// guards against a future refactor that drops the fallback: the math
/// helpers themselves treat 0 as the 96-DPI baseline.
#[test]
fn scale_factor_zero_dpi_does_not_divide_through_zero() {
    let s = scale_factor(0);
    assert!(s.is_finite(), "scale_factor(0) must be finite, got {s}");
    assert!(
        (s - 1.0).abs() < EPS,
        "scale_factor(0) must equal scale_factor(96) = 1.0"
    );

    // And `device_to_logical_f32` stays safe — never NaN / inf at 0 DPI.
    let v = device_to_logical_f32(123.0, 0);
    assert!(
        v.is_finite(),
        "device_to_logical_f32(*, 0) must be finite, got {v}"
    );
    assert!(
        (v - 123.0).abs() < EPS,
        "0 DPI must collapse to 96 DPI identity, got {v}",
    );
}
