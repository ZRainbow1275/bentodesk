use super::{
    ZONE_DRAG_VISUAL_OPACITY, moved_zone_drag_source, zone_drag_visual_opacity, zone_draw_layer,
};
use crate::AppState;
use bentodesk_style::Size;
use bentodesk_zone::{Zone, ZoneId};

fn app_with_source_and_target(source_x: i32, source_y: i32) -> AppState {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    app.zones
        .add(Zone::new(ZoneId(1), "Source", source_x, source_y, 160, 120));
    app.zones
        .add(Zone::new(ZoneId(2), "Target", 240, 80, 160, 120));
    app
}

#[test]
fn zone_drag_visual_stays_opaque_without_drag() {
    let app = app_with_source_and_target(240, 80);
    let source = app.zones.get(ZoneId(1)).expect("source fixture");

    assert!(!moved_zone_drag_source(&app, ZoneId(1)));
    assert_eq!(zone_drag_visual_opacity(&app, ZoneId(1)), 1.0);
    assert_eq!(zone_draw_layer(&app, source), 0);
}

#[test]
fn zone_drag_visual_stays_idle_before_drag_threshold_latches() {
    let app = app_with_source_and_target(240, 80);
    app.zone_drag.set(Some((ZoneId(1), 0, 0)));
    app.zone_drag_origin.set(Some((10, 10, false)));
    let source = app.zones.get(ZoneId(1)).expect("source fixture");

    assert!(!moved_zone_drag_source(&app, ZoneId(1)));
    assert_eq!(zone_drag_visual_opacity(&app, ZoneId(1)), 1.0);
    assert_eq!(zone_draw_layer(&app, source), 0);
}

#[test]
fn moved_zone_uses_tauri_drag_opacity_even_without_merge_target() {
    let app = app_with_source_and_target(20, 20);
    app.zone_drag.set(Some((ZoneId(1), 0, 0)));
    app.zone_drag_origin.set(Some((10, 10, true)));
    let source = app.zones.get(ZoneId(1)).expect("source fixture");

    assert!(moved_zone_drag_source(&app, ZoneId(1)));
    assert_eq!(
        zone_drag_visual_opacity(&app, ZoneId(1)),
        ZONE_DRAG_VISUAL_OPACITY
    );
    assert_eq!(zone_draw_layer(&app, source), 2);
}

#[test]
fn moved_source_stays_above_target_until_mouse_up_scores_the_merge() {
    let app = app_with_source_and_target(250, 90);
    app.zone_drag.set(Some((ZoneId(1), 0, 0)));
    app.zone_drag_origin.set(Some((10, 10, true)));
    let source = app.zones.get(ZoneId(1)).expect("source fixture");
    let target = app.zones.get(ZoneId(2)).expect("target fixture");

    assert_eq!(zone_draw_layer(&app, source), 2);
    assert_eq!(zone_draw_layer(&app, target), 0);
    assert_eq!(
        zone_drag_visual_opacity(&app, ZoneId(1)),
        ZONE_DRAG_VISUAL_OPACITY
    );
    assert_eq!(zone_drag_visual_opacity(&app, ZoneId(2)), 1.0);
}
