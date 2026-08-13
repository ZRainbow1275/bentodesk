#[test]
fn workarea_refresh_cancels_every_main_client_pointer_session() {
    let app = AppState::new();
    app.zone_drag.set(Some((ZoneId(1), 20, 24)));
    app.zone_drag_origin.set(Some((30, 40, true)));
    app.zone_drag_body_visible_at_start
        .set(Some((ZoneId(1), true)));
    app.zone_drag_selected_before_start.set(Some(ZoneId(2)));
    app.zone_resize.set(Some(ZoneResizeSession {
        id: ZoneId(2),
        start_pointer_x: 100.0,
        start_pointer_y: 120.0,
        start_visible_width: 240.0,
        start_visible_height: 180.0,
        anchor_right: true,
        anchor_bottom: false,
    }));
    app.item_drag.borrow_mut().replace(ItemDragCandidate {
        zone_id: ZoneId(3),
        item_id: ZoneItemId(4),
        path: SmolStr::new_static(r"C:\Desktop\proof.txt"),
        start_x: 12,
        start_y: 18,
        last_x: 12,
        last_y: 18,
        is_internal_dragging: false,
    });
    app.stack_tray_drag.set(Some(StackTrayDragState::new(
        ZoneId(5),
        ZoneId(6),
        0,
    )));

    assert!(app.cancel_viewport_gestures());
    assert_eq!(app.zone_drag.get(), None);
    assert_eq!(app.zone_resize.get(), None);
    assert!(app.item_drag.borrow().is_none());
    assert_eq!(app.stack_tray_drag.get(), None);
    assert_eq!(app.zone_drag_origin.get(), None);
    assert_eq!(app.zone_drag_body_visible_at_start.get(), None);
    assert_eq!(app.zone_drag_selected_before_start.get(), None);
    assert!(!app.cancel_viewport_gestures());
}

#[test]
fn workarea_refresh_pending_normalize_is_coalesced_and_consumed_once() {
    let window = WindowState::new();
    assert!(window.schedule_zone_geometry_normalize());
    assert!(!window.schedule_zone_geometry_normalize());
    assert!(window.take_zone_geometry_normalize());
    assert!(!window.take_zone_geometry_normalize());
}
