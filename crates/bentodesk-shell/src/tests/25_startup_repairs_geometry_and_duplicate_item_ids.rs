#[test]
fn startup_repairs_and_persists_legacy_duplicate_zone_item_ids() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-repairs-duplicate-item-ids");
    let state_dir = zones_path.parent().expect("scratch parent");
    let _ = std::fs::remove_dir_all(state_dir);
    std::fs::create_dir_all(state_dir).expect("scratch");
    let mut selected_zones = ZoneList::new();
    let mut zone = Zone::new(ZoneId(153), "Legacy duplicate items", 40, 50, 260, 180);
    zone.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/first.txt"),
        Cow::Borrowed("first"),
        0,
        0,
    ));
    zone.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/duplicate.txt"),
        Cow::Borrowed("duplicate"),
        1,
        0,
    ));
    selected_zones.add(zone);
    storage::write_zones_atomic(&zones_path, &selected_zones).expect("persist duplicate state");

    load_startup_zones_or_migrate_legacy(&root, &zones_path).expect("startup load");
    assert!(root.app.borrow().dirty.get());
    flush_dirty_zones(&root);
    let persisted = storage::read_zones(&zones_path).expect("read repaired state");
    let zone = persisted.get(ZoneId(153)).expect("zone");
    assert_eq!(zone.items[0].id, ZoneItemId(1));
    assert_eq!(zone.items[1].id, ZoneItemId(2));

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn first_paint_load_repairs_zones_before_draw_and_does_not_reload_them() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-clamps-selected-zones");
    let state_dir = zones_path.parent().expect("scratch parent");
    let _ = std::fs::remove_dir_all(state_dir);
    std::fs::create_dir_all(state_dir).expect("scratch");
    let mut selected_zones = ZoneList::new();
    selected_zones.add(Zone::new(
        ZoneId(3),
        "Offscreen tray Zone",
        2_011,
        1_418,
        200,
        120,
    ));
    selected_zones.add(Zone::new(
        ZoneId(4),
        "Legal Zone",
        40,
        50,
        260,
        180,
    ));
    storage::write_zones_atomic(&zones_path, &selected_zones).expect("persist zones");
    let window = WindowState::new();
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 1_707.0,
            height: 912.0,
        };
        window.load_zones_once(&mut app);
        assert_eq!(normalize_startup_zone_geometry(&mut app), 1);
        assert!(app.dirty.get());
        let repaired = app.zones.get(ZoneId(3)).expect("repaired zone");
        let (_, _, capsule_w, capsule_h) =
            bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, repaired);
        assert_eq!(
            (repaired.x, repaired.y, repaired.w, repaired.h),
            (1_707 - capsule_w, 912 - capsule_h, 200, 120)
        );
        let legal = app.zones.get(ZoneId(4)).expect("legal zone");
        assert_eq!((legal.x, legal.y, legal.w, legal.h), (40, 50, 260, 180));

        // Renderer::render calls the same seam. It must not reload the still
        // off-screen file after the shell repairs the in-memory first frame.
        window.load_zones_once(&mut app);
        let repaired = app.zones.get(ZoneId(3)).expect("one-shot repaired zone");
        assert_eq!(
            (repaired.x, repaired.y),
            (1_707 - capsule_w, 912 - capsule_h)
        );
        assert_eq!(normalize_startup_zone_geometry(&mut app), 0);
    }

    flush_dirty_zones(&root);
    let persisted = storage::read_zones(&zones_path).expect("read repaired zones");
    let repaired = persisted.get(ZoneId(3)).expect("persisted repaired zone");
    let (_, _, capsule_w, capsule_h) =
        bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&persisted, repaired);
    assert_eq!(
        (repaired.x, repaired.y, repaired.w, repaired.h),
        (1_707 - capsule_w, 912 - capsule_h, 200, 120)
    );
    assert!(!root.app.borrow().dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn normalizer_keeps_legal_edge_capsule_home_instead_of_fitting_expanded_panel() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1_707.0,
        height: 912.0,
    };
    let mut zone = Zone::new(ZoneId(31), "Edge", 0, 0, 800, 600);
    let pill = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(&zone, 0).rect;
    zone.x = 1_707 - pill.width.round() as i32;
    zone.y = 912 - pill.height.round() as i32;
    let expected_home = (zone.x, zone.y);
    app.zones.add(zone);

    assert_eq!(normalize_startup_zone_geometry(&mut app), 0);
    let zone = app.zones.get(ZoneId(31)).expect("edge zone");
    assert_eq!((zone.x, zone.y), expected_home);
    assert_eq!((zone.w, zone.h), (800, 600));
    assert!(!app.dirty.get());
}

#[test]
fn normalizer_lets_degenerate_viewport_containment_override_normal_minimum() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 31.75,
        height: 23.75,
    };
    app.zones
        .add(Zone::new(ZoneId(310), "Tiny", 500, 500, 800, 600));

    assert_eq!(normalize_startup_zone_geometry(&mut app), 1);
    let zone = app.zones.get(ZoneId(310)).expect("tiny viewport zone");
    assert_eq!((zone.x, zone.y, zone.w, zone.h), (0, 0, 31, 23));
}

#[test]
fn normalizer_persists_stack_offsets_and_anchor_promotion_across_restarts() {
    let zones_path = scratch_zones_path("stack-normalize-detach-restart");
    let state_dir = zones_path.parent().expect("scratch parent");
    let _ = std::fs::remove_dir_all(state_dir);
    std::fs::create_dir_all(state_dir).expect("scratch");

    let mut app = AppState::new();
    app.viewport = Size {
        width: 1_707.0,
        height: 912.0,
    };
    app.zones
        .add(Zone::new(ZoneId(32), "Anchor", 2_011, 1_418, 480, 360));
    app.zones
        .add(Zone::new(ZoneId(33), "Child", 2_051, 1_468, 420, 320));
    app.zones
        .add(Zone::new(ZoneId(34), "Child 2", 1_961, 1_378, 360, 280));
    assert!(app.zones.stack(ZoneId(32), ZoneId(33)));
    assert!(app.zones.stack(ZoneId(32), ZoneId(34)));
    let before_offsets = [ZoneId(33), ZoneId(34)].map(|id| {
        let anchor = app.zones.get(ZoneId(32)).expect("anchor");
        let child = app.zones.get(id).expect("child");
        (child.x - anchor.x, child.y - anchor.y)
    });
    storage::write_zones_atomic(&zones_path, &app.zones).expect("persist seeded stack");
    app.zones = storage::read_zones(&zones_path).expect("first startup read");

    assert!(normalize_startup_zone_geometry(&mut app) > 0);
    let anchor = app.zones.get(ZoneId(32)).expect("anchor");
    for (id, expected) in [ZoneId(33), ZoneId(34)].into_iter().zip(before_offsets) {
        let child = app.zones.get(id).expect("child");
        assert_eq!((child.x - anchor.x, child.y - anchor.y), expected);
    }
    let (_, _, stack_w, stack_h) =
        bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, anchor);
    assert_eq!((anchor.x, anchor.y), (1_707 - stack_w, 912 - stack_h));
    storage::write_zones_atomic(&zones_path, &app.zones).expect("persist normalized stack");
    app.zones = storage::read_zones(&zones_path).expect("normalized restart read");
    assert_eq!(normalize_startup_zone_geometry(&mut app), 0);

    let promoted_offset = {
        let promoted = app.zones.get(ZoneId(33)).expect("future anchor");
        let child = app.zones.get(ZoneId(34)).expect("remaining child");
        (child.x - promoted.x, child.y - promoted.y)
    };
    let outcome = app
        .zones
        .detach_from_stack(ZoneId(32))
        .expect("anchor detach");
    assert_eq!(outcome.new_anchor, Some(ZoneId(33)));
    assert_eq!(outcome.remaining_count, 2);
    let _ = normalize_startup_zone_geometry(&mut app);
    assert_eq!(app.zones.stack_anchor_for(ZoneId(34)), Some(ZoneId(33)));
    let promoted = app.zones.get(ZoneId(33)).expect("promoted anchor");
    let child = app.zones.get(ZoneId(34)).expect("promoted child");
    assert_eq!(
        (child.x - promoted.x, child.y - promoted.y),
        promoted_offset
    );
    for id in [ZoneId(32), ZoneId(33)] {
        let zone = app.zones.get(id).expect("visible detached surface");
        let (_, _, width, height) =
            bentodesk_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, zone);
        assert!(zone.x >= 0 && zone.x + width <= 1_707);
        assert!(zone.y >= 0 && zone.y + height <= 912);
    }
    storage::write_zones_atomic(&zones_path, &app.zones).expect("persist promoted stack");
    app.zones = storage::read_zones(&zones_path).expect("promoted restart read");
    assert_eq!(normalize_startup_zone_geometry(&mut app), 0);
    assert_eq!(app.zones.stack_anchor_for(ZoneId(34)), Some(ZoneId(33)));
    let promoted = app.zones.get(ZoneId(33)).expect("restarted anchor");
    let child = app.zones.get(ZoneId(34)).expect("restarted child");
    assert_eq!(
        (child.x - promoted.x, child.y - promoted.y),
        promoted_offset
    );

    let _ = std::fs::remove_dir_all(state_dir);
}
