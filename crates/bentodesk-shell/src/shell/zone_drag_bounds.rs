use super::*;

/// Clamp a live Zone drag to the Main HWND's logical viewport.
///
/// Device-monitor bounds are intentionally accepted only as regression-proof
/// context: Main-client logical DIPs are the persistence coordinate space and
/// must not roam into a larger physical/virtual-screen union.
pub(super) fn clamp_zone_drag_to_logical_viewport(
    zones: &ZoneList,
    id: ZoneId,
    x: i32,
    y: i32,
    viewport: bentodesk_style::Size,
    _device_monitors: &[bentodesk_platform::MonitorInfo],
) -> (i32, i32) {
    let Some(zone) = zones.get(id) else {
        return (x, y);
    };
    let (_, _, width, height) =
        bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(zones, zone);
    bentodesk_app::zone_pill_geometry::clamp_capsule_origin_to_viewport(
        x, y, width, height, viewport,
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod viewport_drag_tests {
    use super::*;

    #[test]
    fn logical_viewport_clamp_covers_dpi_taskbar_edges_and_capsule_footprints() {
        use bentodesk_platform::RectI32;
        use bentodesk_style::Size;

        let viewport_cases = [
            (
                "100%-bottom",
                96,
                Size {
                    width: 1_920.75,
                    height: 1_040.75,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 3_840,
                    bottom: 2_160,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 3_840,
                    bottom: 2_080,
                },
            ),
            (
                "125%-top",
                120,
                Size {
                    width: 1_920.75,
                    height: 1_040.75,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 2_400,
                    bottom: 1_350,
                },
                RectI32 {
                    left: 0,
                    top: 50,
                    right: 2_400,
                    bottom: 1_350,
                },
            ),
            (
                "150%-left",
                144,
                Size {
                    width: 1_872.75,
                    height: 1_080.75,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 2_880,
                    bottom: 1_620,
                },
                RectI32 {
                    left: 72,
                    top: 0,
                    right: 2_880,
                    bottom: 1_620,
                },
            ),
            (
                "200%-right",
                192,
                Size {
                    width: 1_872.75,
                    height: 1_080.75,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 3_840,
                    bottom: 2_160,
                },
                RectI32 {
                    left: 0,
                    top: 0,
                    right: 3_744,
                    bottom: 2_160,
                },
            ),
        ];
        let footprints = [
            ("small-pill", "small", "pill", false),
            ("medium-pill", "medium", "pill", false),
            ("large-pill", "large", "pill", false),
            ("small-circle", "small", "circle", false),
            ("stack", "medium", "pill", true),
        ];

        for (viewport_name, dpi, viewport, rect_screen, rect_work) in viewport_cases {
            let device_monitor = bentodesk_platform::MonitorInfo {
                hmonitor: core::ptr::null_mut(),
                rect_screen,
                rect_work,
                is_primary: true,
            };
            assert!(
                rect_work.right - rect_work.left > viewport.width.floor() as i32
                    || rect_work.bottom - rect_work.top > viewport.height.floor() as i32,
                "fixture must expose inflated device bounds: {viewport_name} dpi={dpi}"
            );

            for (footprint_name, size, shape, stack) in footprints {
                let mut zones = ZoneList::new();
                let mut anchor = Zone::new(ZoneId(1), "zone", 10, 10, 480, 432);
                anchor.set_capsule(size, shape);
                zones.add(anchor);
                if stack {
                    zones.add(Zone::new(ZoneId(2), "child", 20, 20, 320, 240));
                    assert!(zones.stack(ZoneId(1), ZoneId(2)));
                }
                let zone = zones.get(ZoneId(1)).expect("zone");
                let (_, _, capsule_width, capsule_height) =
                    bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&zones, zone);
                let expected = (
                    (viewport.width.floor() as i32 - capsule_width).max(0),
                    (viewport.height.floor() as i32 - capsule_height).max(0),
                );

                assert_eq!(
                    clamp_zone_drag_to_logical_viewport(
                        &zones,
                        ZoneId(1),
                        i32::MAX / 4,
                        i32::MAX / 4,
                        viewport,
                        &[device_monitor],
                    ),
                    expected,
                    "{viewport_name} dpi={dpi} footprint={footprint_name}"
                );
                assert_eq!(
                    clamp_zone_drag_to_logical_viewport(
                        &zones,
                        ZoneId(1),
                        -500,
                        -500,
                        viewport,
                        &[device_monitor],
                    ),
                    (0, 0),
                    "negative clamp: {viewport_name} footprint={footprint_name}"
                );
            }
        }
    }
}
