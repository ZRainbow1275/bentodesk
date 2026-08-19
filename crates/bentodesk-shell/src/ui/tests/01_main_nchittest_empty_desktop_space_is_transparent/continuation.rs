#[test]
fn hit_test_zone_item_uses_zone_grid_columns() {
    let mut zone = Zone::new(ZoneId(3), Cow::Borrowed("z"), 10, 10, 240, 180);
    zone.set_grid_columns(2);
    let _first = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Left.lnk".to_owned()),
            Cow::Borrowed("left"),
        )
        .expect("first item");
    let second = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Right.lnk".to_owned()),
            Cow::Borrowed("right"),
        )
        .expect("second item");
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);

    // P3.8: row 0 starts at zone_top(10) + 56-DIP header/content offset =
    // 66; y=70 hits it. With 2 columns and 16-DIP side insets, x=150 lands
    // in the right column.
    let hit = hit_test_zone_item(&app, 150.0, 70.0).expect("right-column item hit");
    assert_eq!(hit.0, ZoneId(3));
    assert_eq!(hit.1, second);
    assert_eq!(hit.2, "C:/Users/BentoDeskTest/Desktop/Right.lnk");
}

#[test]
fn hit_test_zone_item_tracks_expanded_content_scroll() {
    let mut zone = Zone::new(ZoneId(3), Cow::Borrowed("z"), 10, 10, 240, 180);
    zone.set_grid_columns(2);
    for name in ["One", "Two"] {
        zone.add_item(
            Cow::Owned(format!("C:/Users/BentoDeskTest/Desktop/{name}.lnk")),
            Cow::Owned(name.to_owned()),
        )
        .expect("first row");
    }
    let third = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Three.lnk".to_owned()),
            Cow::Borrowed("Three"),
        )
        .expect("third item");
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);

    assert_ne!(
        hit_test_zone_item(&app, 28.0, 100.0).map(|hit| hit.1),
        Some(third)
    );
    app.set_zone_content_scroll(ZoneId(3), 86.0);
    assert_eq!(
        hit_test_zone_item(&app, 28.0, 100.0).map(|hit| hit.1),
        Some(third)
    );
    let zone = app.zones.get(ZoneId(3)).expect("zone");
    let panel = app.zone_expanded_placement(zone).panel;
    let expected = app
        .resolve_zone_item_flow_layout(zone, panel, 0.0, zone.items.iter())
        .max_scroll;
    assert!(
        (app.zone_content_scroll_offset(ZoneId(3)) - expected).abs() < 0.01,
        "scroll must clamp to the responsive variable-flow maximum"
    );
}

#[test]
fn hit_test_zone_item_uses_auto_placed_item_rects_when_wide_cards_shift_following_items() {
    let mut zone = Zone::new(ZoneId(4), Cow::Borrowed("z"), 64, 332, 320, 220);
    zone.set_grid_columns(5);
    let first = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/item-01.txt".to_owned()),
            Cow::Borrowed("wide"),
        )
        .expect("first item");
    assert!(zone.toggle_item_wide(first));
    let second = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/item-02.txt".to_owned()),
            Cow::Borrowed("second"),
        )
        .expect("second item");
    let third = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/item-03.txt".to_owned()),
            Cow::Borrowed("third"),
        )
        .expect("third item");
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let zone = app.zones.get(ZoneId(4)).expect("zone");
    let panel = app.zone_expanded_placement(zone).panel;

    // The renderer auto-places the wide first card across slots 0-1. The
    // second item is therefore painted in effective slot 2, not at its raw
    // persisted grid_x=1. The shell hit-test must use that same item-aware
    // helper or a click on the visible second card starts no drag.
    let second_rect = bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
        zone,
        zone.item(second).expect("second"),
        panel,
    );
    let second_hit = hit_test_zone_item(
        &app,
        second_rect.x + second_rect.width * 0.5,
        second_rect.y + second_rect.height * 0.5,
    )
    .expect("second item hit");
    assert_eq!(second_hit.0, ZoneId(4));
    assert_eq!(second_hit.1, second);
    assert_eq!(second_hit.2, "C:/Users/BentoDeskTest/Desktop/item-02.txt");

    let third_rect = bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
        zone,
        zone.item(third).expect("third"),
        panel,
    );
    let third_hit = hit_test_zone_item(
        &app,
        third_rect.x + third_rect.width * 0.5,
        third_rect.y + third_rect.height * 0.5,
    )
    .expect("third item hit");
    assert_eq!(third_hit.1, third);
}

#[test]
fn hit_test_zone_resize_corner_only_in_bottom_right_box() {
    let app = app_with_zones(vec![Zone::new(
        ZoneId(7),
        Cow::Borrowed("z"),
        100,
        100,
        200,
        100,
    )]);
    // Wave C — resize corner only exists on expanded zones.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    // Inside body but outside corner.
    assert_eq!(hit_test_zone_resize_corner(&app, 150.0, 150.0), None);
    // 23 DIP from each edge is inside the new 24×24 corner box.
    assert_eq!(
        hit_test_zone_resize_corner(&app, 277.0, 177.0),
        Some(ZoneId(7))
    );
    // 25 DIP from each edge remains outside the corner box.
    assert_eq!(hit_test_zone_resize_corner(&app, 275.0, 175.0), None);
    // Edge boundary excluded (`<` not `<=`).
    assert_eq!(hit_test_zone_resize_corner(&app, 300.0, 200.0), None);
}

#[test]
fn resize_corner_never_targets_zone_buried_under_topmost_surface() {
    let app = app_with_zones(vec![
        Zone::new(ZoneId(75), Cow::Borrowed("lower"), 80, 80, 240, 180),
        Zone::new(ZoneId(76), Cow::Borrowed("upper"), 200, 120, 240, 180),
    ]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let lower_panel = app
        .zone_expanded_placement(app.zones.get(ZoneId(75)).expect("lower zone"))
        .panel;
    let buried_corner = (lower_panel.right() - 2.0, lower_panel.bottom() - 2.0);

    assert_eq!(
        hit_test_zone(&app, buried_corner.0, buried_corner.1),
        Some(ZoneId(76))
    );
    assert_eq!(
        zone_resize_session_for_point(&app, buried_corner.0, buried_corner.1),
        None
    );
}

#[test]
fn topmost_resize_corner_is_not_blocked_by_buried_item_card() {
    let mut lower = Zone::new(ZoneId(77), Cow::Borrowed("lower"), 80, 80, 240, 180);
    lower.set_grid_columns(1);
    for name in ["One", "Two"] {
        lower
            .add_item(
                Cow::Owned(format!("C:/Users/BentoDeskTest/Desktop/{name}.lnk")),
                Cow::Owned(name.to_owned()),
            )
            .expect("item");
    }
    let app = app_with_zones(vec![
        lower,
        Zone::new(ZoneId(78), Cow::Borrowed("upper"), 80, 80, 240, 180),
    ]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let panel = app
        .zone_expanded_placement(app.zones.get(ZoneId(78)).expect("upper zone"))
        .panel;
    let point = (panel.right() - 20.0, panel.bottom() - 8.0);

    assert_eq!(hit_test_zone(&app, point.0, point.1), Some(ZoneId(78)));
    assert_eq!(
        zone_resize_session_for_point(&app, point.0, point.1).map(|session| session.id),
        Some(ZoneId(78))
    );
    assert_eq!(hit_test_zone_item(&app, point.0, point.1), None);
    assert_eq!(
        actionable_zone_resize_session_for_point(&app, point.0, point.1).map(|session| session.id),
        Some(ZoneId(78))
    );
}

#[test]
fn topmost_zone_blocks_buried_header_and_inline_search() {
    let app = app_with_zones(vec![
        Zone::new(ZoneId(79), Cow::Borrowed("lower"), 80, 80, 240, 180),
        Zone::new(ZoneId(80), Cow::Borrowed("upper"), 200, 80, 240, 180),
    ]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let lower = app.zones.get(ZoneId(79)).expect("lower zone");
    let panel = app.zone_expanded_placement(lower).panel;
    let layout = expanded_zone_grid::expanded_zone_layout_for_rect(panel, 0);
    let header_point = (
        layout.header_close_btn.x + 2.0,
        layout.header_close_btn.y + 2.0,
    );

    assert_eq!(
        hit_test_zone(&app, header_point.0, header_point.1),
        Some(ZoneId(80))
    );
    assert_eq!(
        hit_test_zone_header_button(&app, header_point.0, header_point.1),
        None
    );

    app.zone_search_target.set(Some(ZoneId(79)));
    let search = search_bar::zone_inline_rect(panel);
    let search_point = (search.right() - 2.0, search.y + search.height * 0.5);
    assert_eq!(
        hit_test_zone(&app, search_point.0, search_point.1),
        Some(ZoneId(80))
    );
    assert_eq!(
        hit_test_inline_zone_search(&app, search_point.0, search_point.1),
        None
    );
}

#[test]
fn actionable_resize_corner_defers_to_painted_item_and_header_controls() {
    let mut item_zone = Zone::new(ZoneId(71), Cow::Borrowed("items"), 80, 80, 240, 180);
    item_zone.set_grid_columns(1);
    for name in ["One", "Two"] {
        item_zone
            .add_item(
                Cow::Owned(format!("C:/Users/BentoDeskTest/Desktop/{name}.lnk")),
                Cow::Owned(name.to_owned()),
            )
            .expect("item");
    }
    let item_app = app_with_zones(vec![item_zone]);
    item_app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let item_panel = item_app
        .zone_expanded_placement(item_app.zones.get(ZoneId(71)).expect("item zone"))
        .panel;
    // Item content remains higher priority in the 24-DIP corner, while the
    // innermost 8-DIP bottom strip is reserved for continuous edge resize.
    let item_point = (item_panel.right() - 20.0, item_panel.bottom() - 9.0);
    assert!(zone_resize_session_for_point(&item_app, item_point.0, item_point.1).is_some());
    assert!(hit_test_zone_item(&item_app, item_point.0, item_point.1).is_some());
    assert_eq!(
        actionable_zone_resize_session_for_point(&item_app, item_point.0, item_point.1),
        None
    );

    let header_app = app_with_zones(vec![Zone::new(
        ZoneId(72),
        Cow::Borrowed("header"),
        80,
        500,
        400,
        180,
    )]);
    header_app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let header_zone = header_app.zones.get(ZoneId(72)).expect("header zone");
    let placement = header_app.zone_expanded_placement(header_zone);
    assert_eq!(
        (placement.anchor_right, placement.anchor_bottom),
        (false, true)
    );
    let layout = expanded_zone_grid::expanded_zone_layout_for_rect(placement.panel, 0);
    let header_point = (
        layout.header_close_btn.right() - 2.0,
        layout.header_close_btn.y + 2.0,
    );
    assert!(zone_resize_session_for_point(&header_app, header_point.0, header_point.1).is_some());
    assert_eq!(
        hit_test_zone_header_button(&header_app, header_point.0, header_point.1),
        Some((ZoneId(72), HeaderButton::Close))
    );
    assert_eq!(
        actionable_zone_resize_session_for_point(&header_app, header_point.0, header_point.1),
        None
    );
}

#[test]
fn bottom_resize_edge_remains_actionable_when_variable_item_rows_overflow() {
    let mut zone = Zone::new(ZoneId(75), Cow::Borrowed("overflow"), 80, 80, 520, 420);
    zone.set_grid_columns(4);
    for index in 0..8 {
        zone.add_item(
            Cow::Owned(format!("C:/Users/BentoDeskTest/Desktop/{index}.lnk")),
            Cow::Owned(format!("WWWWWWWWWWWWWWWWWWWW item {index}")),
        )
        .expect("item");
    }
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let panel = app
        .zone_expanded_placement(app.zones.get(ZoneId(75)).expect("zone"))
        .panel;
    let point = (panel.x + panel.width * 0.25, panel.bottom() - 2.0);

    assert!(hit_test_zone_item(&app, point.0, point.1).is_none());
    assert_eq!(
        actionable_zone_resize_session_for_point(&app, point.0, point.1)
            .map(|session| session.handle),
        Some(bentodesk_app::ZoneResizeHandle::Bottom)
    );
}

#[test]
fn actionable_resize_corner_rejects_locked_zone_and_active_context_menu() {
    let mut locked = Zone::new(ZoneId(73), Cow::Borrowed("locked"), 80, 80, 240, 180);
    assert!(locked.set_locked(true));
    let locked_app = app_with_zones(vec![locked]);
    locked_app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let locked_panel = locked_app
        .zone_expanded_placement(locked_app.zones.get(ZoneId(73)).expect("locked zone"))
        .panel;
    let locked_point = (locked_panel.right() - 2.0, locked_panel.bottom() - 2.0);
    assert!(zone_resize_session_for_point(&locked_app, locked_point.0, locked_point.1).is_some());
    assert_eq!(
        actionable_zone_resize_session_for_point(&locked_app, locked_point.0, locked_point.1),
        None
    );

    let menu_app = app_with_zones(vec![Zone::new(
        ZoneId(74),
        Cow::Borrowed("menu"),
        80,
        80,
        240,
        180,
    )]);
    menu_app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let menu_panel = menu_app
        .zone_expanded_placement(menu_app.zones.get(ZoneId(74)).expect("menu zone"))
        .panel;
    let menu_point = (menu_panel.right() - 2.0, menu_panel.bottom() - 2.0);
    menu_app
        .active_context_menu
        .borrow_mut()
        .replace(popover::ContextMenuSession::new(
            popover::ContextMenuRows::new(),
            popover::ContextMenuRows::new(),
        ));
    assert_eq!(
        actionable_zone_resize_session_for_point(&menu_app, menu_point.0, menu_point.1),
        None
    );
}

#[test]
fn directional_resize_handle_mirrors_across_all_four_quadrants() {
    let homes = [(80, 80), (580, 80), (80, 500), (580, 500)];
    let anchors = [(false, false), (true, false), (false, true), (true, true)];
    for ((x, y), (anchor_right, anchor_bottom)) in homes.into_iter().zip(anchors) {
        let app = app_with_zones(vec![Zone::new(
            ZoneId(70),
            Cow::Borrowed("Directional"),
            x,
            y,
            240,
            180,
        )]);
        app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
        let zone = app.zones.get(ZoneId(70)).expect("zone");
        let placement = app.zone_expanded_placement(zone);
        assert_eq!(
            (placement.anchor_right, placement.anchor_bottom),
            (anchor_right, anchor_bottom)
        );
        let handle_x = if anchor_right {
            placement.panel.x + 2.0
        } else {
            placement.panel.right() - 2.0
        };
        let handle_y = if anchor_bottom {
            placement.panel.y + 2.0
        } else {
            placement.panel.bottom() - 2.0
        };
        let session = zone_resize_session_for_point(&app, handle_x, handle_y)
            .expect("mirrored resize handle");
        assert_eq!(session.id, ZoneId(70));
        assert_eq!(session.anchor_right, anchor_right);
        assert_eq!(session.anchor_bottom, anchor_bottom);
        assert_eq!(session.start_pointer_x, handle_x);
        assert_eq!(session.start_pointer_y, handle_y);
        assert_eq!(session.start_panel, placement.panel);
        assert_eq!(session.start_persisted_width, zone.w);
        assert_eq!(session.start_persisted_height, zone.h);
    }
}

#[test]
fn expanded_panel_exposes_four_edges_and_four_corner_handles() {
    use bentodesk_app::ZoneResizeHandle::*;
    let app = app_with_zones(vec![Zone::new(
        ZoneId(71),
        Cow::Borrowed("Eight handles"),
        100,
        100,
        240,
        180,
    )]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let panel = app
        .zone_expanded_placement(app.zones.get(ZoneId(71)).expect("zone"))
        .panel;
    let mid_x = panel.x + panel.width * 0.5;
    let mid_y = panel.y + panel.height * 0.5;
    for (x, y, expected) in [
        (panel.x + 2.0, mid_y, Left),
        (panel.right() - 2.0, mid_y, Right),
        (mid_x, panel.y + 2.0, Top),
        (mid_x, panel.bottom() - 2.0, Bottom),
        (panel.x + 2.0, panel.y + 2.0, TopLeft),
        (panel.right() - 2.0, panel.y + 2.0, TopRight),
        (panel.x + 2.0, panel.bottom() - 2.0, BottomLeft),
        (panel.right() - 2.0, panel.bottom() - 2.0, BottomRight),
    ] {
        let session = zone_resize_session_for_point(&app, x, y).expect("resize handle");
        assert_eq!(session.handle, expected);
    }
    assert!(zone_resize_session_for_point(&app, mid_x, mid_y).is_none());
}

#[test]
fn resize_edges_are_continuous_between_corner_targets() {
    use crate::ui::zone_hit::ZONE_RESIZE_CORNER;
    use bentodesk_app::ZoneResizeHandle::*;

    let app = app_with_zones(vec![Zone::new(
        ZoneId(72),
        Cow::Borrowed("Continuous edges"),
        100,
        100,
        240,
        180,
    )]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let panel = app
        .zone_expanded_placement(app.zones.get(ZoneId(72)).expect("zone"))
        .panel;
    let handle = |x, y| {
        zone_resize_session_for_point(&app, x, y)
            .map(|session| session.handle)
            .expect("continuous resize target")
    };

    assert_eq!(
        handle(panel.x + ZONE_RESIZE_CORNER - 1.0, panel.y + 2.0),
        TopLeft
    );
    assert_eq!(handle(panel.x + ZONE_RESIZE_CORNER, panel.y + 2.0), Top);
    assert_eq!(
        handle(panel.right() - ZONE_RESIZE_CORNER, panel.y + 2.0),
        TopRight
    );
    assert_eq!(
        handle(panel.x + 2.0, panel.y + ZONE_RESIZE_CORNER - 1.0),
        TopLeft
    );
    assert_eq!(handle(panel.x + 2.0, panel.y + ZONE_RESIZE_CORNER), Left);
    assert_eq!(
        handle(panel.x + 2.0, panel.bottom() - ZONE_RESIZE_CORNER),
        BottomLeft
    );

    for fraction in [0.25, 0.5, 0.75] {
        let x = panel.x + panel.width * fraction;
        let y = panel.y + panel.height * fraction;
        assert_eq!(handle(x, panel.y + 2.0), Top);
        assert_eq!(handle(x, panel.bottom() - 2.0), Bottom);
        assert_eq!(handle(panel.x + 2.0, y), Left);
        assert_eq!(handle(panel.right() - 2.0, y), Right);
    }
}

#[test]
fn hit_test_zone_header_button_search_and_close_when_expanded() {
    // GROUP-4 (2026-06-01) — the expanded PanelHeader search + close
    // buttons. Geometry is the paint==hit SSoT, so click the centre of
    // each layout rect and assert the right button.
    let zone = Zone::new(ZoneId(9), Cow::Borrowed("z"), 100, 100, 400, 200);
    let app = app_with_zones(vec![zone]);
    // Header buttons only exist on an expanded (body-visible) zone.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);

    let layout = expanded_zone_grid::expanded_zone_layout(app.zones.get(ZoneId(9)).expect("zone"));
    let centre = |r: Rect| (r.x + r.width * 0.5, r.y + r.height * 0.5);

    let (cx, cy) = centre(layout.header_close_btn);
    assert_eq!(
        hit_test_zone_header_button(&app, cx, cy),
        Some((ZoneId(9), HeaderButton::Close))
    );

    let (sx, sy) = centre(layout.header_search_btn);
    assert_eq!(
        hit_test_zone_header_button(&app, sx, sy),
        Some((ZoneId(9), HeaderButton::Search))
    );

    // The title region (left of the badge) is not a button.
    assert_eq!(hit_test_zone_header_button(&app, 130.0, 124.0), None);
    // Below the 48-DIP header band is not a button.
    assert_eq!(hit_test_zone_header_button(&app, cx, 180.0), None);
}

#[test]
fn hit_test_zone_header_button_absent_when_collapsed() {
    // A collapsed pill (default Hover mode, no hover) paints no
    // PanelHeader, so the action buttons must not be hit-testable.
    let zone = Zone::new(ZoneId(11), Cow::Borrowed("z"), 100, 100, 400, 200);
    let app = app_with_zones(vec![zone]);
    let layout = expanded_zone_grid::expanded_zone_layout(app.zones.get(ZoneId(11)).expect("zone"));
    let cx = layout.header_close_btn.x + layout.header_close_btn.width * 0.5;
    let cy = layout.header_close_btn.y + layout.header_close_btn.height * 0.5;
    assert_eq!(hit_test_zone_header_button(&app, cx, cy), None);
}
