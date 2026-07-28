//! P0 desktop click-through (CLICKTHROUGH-FIX-VALIDATED.md) — pure-CPU
//! geometry tests for [`chrome_region_rects`]. The GDI `SetWindowRgn`
//! application is not headless-testable (needs a live HWND), same exemption
//! as the GPU/window draw paths; these tests pin the DIP rect set the region
//! is built from. No GPU / window / Argon2 → runs under the min-RSS suite.
use super::{
    CHROME_REGION_SHADOW_MARGIN_DIP, chrome_region_rects, full_client_device_region,
    main_region_precedes_present,
};
use crate::AppState;
use crate::business::{icons::IconKind, popover};
use crate::state::ZoneDisplayMode;
use bento_nano_platform::WindowKind;
use bento_nano_style::{Rect, Size};
use bento_nano_zone::{Zone, ZoneId};

fn covered(rects: &[Rect], x: f32, y: f32) -> bool {
    rects
        .iter()
        .any(|r| x >= r.x && x < r.right() && y >= r.y && y < r.bottom())
}

fn pill_zone(id: u64, x: i32, y: i32) -> Zone {
    Zone::new(ZoneId(id), "Docs", x, y, 160, 120)
}

fn app_with_viewport() -> AppState {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1920.0,
        height: 1080.0,
    };
    // Hover is the default mode — a fresh zone is therefore a collapsed
    // pill (no hover / selection set), exercising the pill case by default.
    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    app
}

#[test]
fn active_drag_region_is_one_stable_full_client_rect() {
    let viewport = Size {
        width: 800.0,
        height: 600.0,
    };
    assert_eq!(
        full_client_device_region(viewport, 1.5),
        Some((0, 0, 1200, 900))
    );
    assert_eq!(
        full_client_device_region(
            Size {
                width: 0.0,
                height: 600.0,
            },
            1.0,
        ),
        None
    );
}

#[test]
fn active_main_motion_installs_region_before_first_present() {
    assert!(main_region_precedes_present(WindowKind::Main, true, false));
    assert!(main_region_precedes_present(WindowKind::Main, false, true));
    assert!(!main_region_precedes_present(
        WindowKind::Main,
        false,
        false
    ));
    assert!(!main_region_precedes_present(
        WindowKind::Settings,
        true,
        true
    ));
}

#[test]
fn blank_state_with_no_zones_is_empty() {
    // No zones, nothing open, no drag → no painted surface → empty region
    // → the WHOLE desktop is click-through.
    let app = app_with_viewport();
    let rects = chrome_region_rects(&app);
    assert!(rects.is_empty(), "blank state must yield no chrome rects");
    // A representative blank coord is therefore not covered.
    assert!(!covered(&rects, 800.0, 500.0));
}

#[test]
fn main_surface_context_menu_adds_only_its_compact_input_bounds() {
    let app = app_with_viewport();
    let mut rows = popover::ContextMenuRows::new();
    rows.push(popover::ContextMenuRow::command(1, "Edit", IconKind::Edit));
    let mut session = popover::ContextMenuSession::new(rows, popover::ContextMenuRows::new());
    session.set_origin(420.0, 260.0);
    let expected = popover::context_menu_bounds(&session);
    app.active_context_menu.borrow_mut().replace(session);

    let rects = chrome_region_rects(&app);
    assert_eq!(rects.as_slice(), &[expected]);
    assert!(covered(&rects, expected.x + 20.0, expected.y + 20.0));
    assert!(!covered(&rects, 40.0, 40.0));
}

#[test]
fn one_collapsed_pill_yields_one_rect_about_pill_size_plus_margin() {
    let mut app = app_with_viewport();
    app.zones.add(pill_zone(1, 300, 200));
    let rects = chrome_region_rects(&app);
    assert_eq!(rects.len(), 1, "one collapsed pill → exactly one rect");

    // The rect is the pill geometry inflated by the shadow margin on each
    // side. Compare against the SSoT `pill_layout_for_zone`.
    let zone = app.zones.iter().next().expect("zone present");
    let pill = crate::zone_pill_geometry::pill_layout_for_zone(zone, zone.items.len()).rect;
    let m = CHROME_REGION_SHADOW_MARGIN_DIP;
    let got = rects[0];
    assert!((got.x - (pill.x - m)).abs() < 0.5, "x inflated by margin");
    assert!((got.y - (pill.y - m)).abs() < 0.5, "y inflated by margin");
    assert!(
        (got.width - (pill.width + m * 2.0)).abs() < 0.5,
        "width inflated by 2×margin"
    );
    assert!(
        (got.height - (pill.height + m * 2.0)).abs() < 0.5,
        "height inflated by 2×margin"
    );

    // A coord at the pill CENTRE is inside the region (interactive).
    let cx = pill.x + pill.width / 2.0;
    let cy = pill.y + pill.height / 2.0;
    assert!(covered(&rects, cx, cy), "pill centre must be in region");

    // A coord far from any chrome is NOT covered → reaches the desktop.
    assert!(
        !covered(&rects, 1700.0, 950.0),
        "blank far corner must be click-through"
    );
}

#[test]
fn click_mode_selected_zone_yields_its_full_body_rect() {
    let mut app = app_with_viewport();
    app.zones.add(pill_zone(7, 400, 300));
    // Selection is the structural expansion producer only in Click mode.
    // The expanded (full x/y/w/h) body rect is then the painted surface.
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(7)));
    let rects = chrome_region_rects(&app);
    assert_eq!(rects.len(), 1, "one expanded zone → one rect");

    let m = CHROME_REGION_SHADOW_MARGIN_DIP;
    let got = rects[0];
    // Body rect is (400, 300, 160, 120) inflated by the margin.
    assert!((got.x - (400.0 - m)).abs() < 0.5);
    assert!((got.y - (300.0 - m)).abs() < 0.5);
    assert!((got.width - (160.0 + m * 2.0)).abs() < 0.5);
    assert!((got.height - (120.0 + m * 2.0)).abs() < 0.5);

    // A point inside the expanded body is interactive; a point outside the
    // (inflated) body is click-through.
    assert!(covered(&rects, 480.0, 360.0), "body interior in region");
    assert!(
        !covered(&rects, 800.0, 800.0),
        "point well outside the body is click-through"
    );
}

#[test]
fn settings_aux_window_does_not_expand_main_region() {
    let app = app_with_viewport();
    app.settings_open.set(true);
    let rects = chrome_region_rects(&app);
    assert!(rects.is_empty());
    assert!(!covered(&rects, 5.0, 5.0));
    assert!(!covered(&rects, 1900.0, 1070.0));
}

#[test]
fn blank_coord_between_two_pills_is_click_through() {
    let mut app = app_with_viewport();
    // Two well-separated collapsed pills with a wide blank gap between.
    app.zones.add(pill_zone(1, 100, 100));
    app.zones.add(pill_zone(2, 1000, 800));
    let rects = chrome_region_rects(&app);
    assert_eq!(rects.len(), 2, "two collapsed pills → two rects");

    // Each pill centre is interactive…
    let z1 = app.zones.get(ZoneId(1)).expect("z1");
    let p1 = crate::zone_pill_geometry::pill_layout_for_zone(z1, z1.items.len()).rect;
    assert!(covered(
        &rects,
        p1.x + p1.width / 2.0,
        p1.y + p1.height / 2.0
    ));
    // …and the empty space between the two pills is click-through.
    assert!(
        !covered(&rects, 600.0, 450.0),
        "gap between pills must reach the desktop"
    );
}

#[test]
fn oversized_zone_chrome_is_clamped_to_viewport() {
    // ROOT-CAUSE-corrupt-zone-geometry.md belt-and-suspenders: even if a
    // zone is sized FAR beyond the viewport (the legacy
    // `w=170667 h=91200` corruption), every returned region rect must stay
    // within the viewport + shadow margin so the whole window can never
    // catch every click.
    let mut app = app_with_viewport();
    // Expanded body sized many times the 1920×1080 viewport.
    app.zones
        .add(Zone::new(ZoneId(9), "Huge", 0, 0, 170_667, 91_200));
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(9)));

    let rects = chrome_region_rects(&app);
    assert!(!rects.is_empty(), "expanded zone must paint a body rect");

    let vp = app.viewport;
    let m = CHROME_REGION_SHADOW_MARGIN_DIP;
    for r in rects.iter() {
        // After clamping-then-inflating, no rect may extend past the
        // viewport by more than a single shadow margin on any edge.
        assert!(r.x >= -m - 0.5, "left within margin: {r:?}");
        assert!(r.y >= -m - 0.5, "top within margin: {r:?}");
        assert!(
            r.right() <= vp.width + m + 0.5,
            "right within viewport+margin: {r:?}"
        );
        assert!(
            r.bottom() <= vp.height + m + 0.5,
            "bottom within viewport+margin: {r:?}"
        );
    }

    // The viewport interior (the body) is still interactive…
    assert!(covered(&rects, 960.0, 540.0), "body interior in region");
    // …but a point BEYOND the real screen (where the corrupt body would
    // otherwise have stretched) is NOT covered — the desktop is alive.
    assert!(
        !covered(&rects, 5000.0, 5000.0),
        "far-offscreen point must be click-through"
    );
}

#[test]
fn zone_fully_offscreen_yields_no_region_rect() {
    // A zone whose body lies entirely past the viewport contributes nothing
    // to the region (its clamp-intersection is empty), so the area stays
    // click-through.
    let mut app = app_with_viewport();
    app.zones
        .add(Zone::new(ZoneId(3), "Gone", 5000, 5000, 160, 120));
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(3)));

    let rects = chrome_region_rects(&app);
    assert!(
        rects.is_empty(),
        "fully-offscreen zone must add no region rect, got {rects:?}"
    );
    assert!(!covered(&rects, 960.0, 540.0));
}
