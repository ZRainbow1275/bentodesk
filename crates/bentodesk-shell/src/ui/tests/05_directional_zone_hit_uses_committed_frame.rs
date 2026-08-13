#[test]
fn hit_test_uses_last_committed_main_frame_sample() {
    let zone = Zone::new(ZoneId(450), Cow::Borrowed("Frame"), 580, 500, 320, 240);
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let now_ms = seed_live_pill_morph(&app, ZoneId(450), 0.25);
    let zone = app.zones.get(ZoneId(450)).expect("zone");
    let painted = app.zone_effective_rect_at(zone, now_ms);
    assert_eq!(
        hit_test_zone(
            &app,
            painted.x + painted.width * 0.5,
            painted.y + painted.height * 0.5,
        ),
        Some(ZoneId(450))
    );
    assert_eq!(app.geometry_frame_now_ms.get(), now_ms);
}

#[test]
fn item_hit_during_directional_morph_uses_live_rect_not_final_panel() {
    let mut zone = Zone::new(ZoneId(451), Cow::Borrowed("Docs"), 580, 500, 400, 300);
    let item_id = zone
        .add_item(r"C:\Desktop\contract.txt", "hash")
        .expect("item");
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    let now_ms = seed_live_pill_morph(&app, ZoneId(451), 0.05);
    let zone = app.zones.get(ZoneId(451)).expect("zone");
    let final_panel = app.zone_expanded_placement(zone).panel;
    let final_card = bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
        zone,
        zone.item(item_id).expect("item"),
        final_panel,
    );
    let final_point = (
        final_card.x + final_card.width * 0.5,
        final_card.y + final_card.height * 0.5,
    );
    let live = app.zone_effective_rect_at(zone, now_ms);
    assert!(
        final_point.0 < live.x
            || final_point.0 >= live.right()
            || final_point.1 < live.y
            || final_point.1 >= live.bottom(),
        "fixture point must be outside early morph rect: point={final_point:?} live={live:?}"
    );
    assert_eq!(hit_test_zone_item(&app, final_point.0, final_point.1), None);

    let now_ms = seed_live_pill_morph(&app, ZoneId(451), 0.75);
    let live = app.zone_effective_rect_at(zone, now_ms);
    let live_card = bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
        zone,
        zone.item(item_id).expect("item"),
        live,
    );
    let live_point = (
        live_card.x + live_card.width * 0.5,
        live_card.y + live_card.height * 0.5,
    );
    assert_eq!(
        hit_test_zone_item(&app, live_point.0, live_point.1).map(|(_, id, _)| id),
        Some(item_id)
    );
}

#[test]
fn hit_test_zone_morph_complete_uses_full_rect() {
    let zone = Zone::new(ZoneId(46), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Morph finished at progress=1.0 with the zone in a settled expanded
    // state — renderer paints the full chrome, so hit-rect is full rect.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    // Far corner of full expanded rect → hit.
    assert_eq!(
        hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0),
        Some(ZoneId(46))
    );
}

#[test]
fn four_quadrant_live_morph_hit_boundary_tracks_effective_rect_within_one_dip() {
    let quadrants = [
        ("left-top", 20, 20, false, false),
        ("right-top", 720, 20, true, false),
        ("left-bottom", 20, 520, false, true),
        ("right-bottom", 720, 520, true, true),
    ];

    for (name, x, y, anchor_right, anchor_bottom) in quadrants {
        for progress in [0.0_f32, 0.05, 0.25, 0.5, 0.75, 0.95, 1.0] {
            let zone = Zone::new(ZoneId(460), Cow::Borrowed("Joint"), x, y, 320, 240);
            let app = app_with_zones(vec![zone]);
            app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
            let now_ms = seed_live_pill_morph(&app, ZoneId(460), progress);
            let zone = app.zones.get(ZoneId(460)).expect("zone");
            let placement = app.zone_expanded_placement(zone);
            assert_eq!(placement.anchor_right, anchor_right, "{name}");
            assert_eq!(placement.anchor_bottom, anchor_bottom, "{name}");
            let rect = app.zone_effective_rect_at(zone, now_ms);
            let cx = rect.x + rect.width * 0.5;
            let cy = rect.y + rect.height * 0.5;
            assert_eq!(
                hit_test_zone(&app, cx, cy),
                Some(ZoneId(460)),
                "{name} progress={progress} center"
            );

            let (inside_x, outside_x) = if anchor_right {
                (rect.right() - 0.5, rect.right() + 1.1)
            } else {
                (rect.x + 0.5, rect.x - 1.1)
            };
            assert_eq!(hit_test_zone(&app, inside_x, cy), Some(ZoneId(460)));
            assert_eq!(hit_test_zone(&app, outside_x, cy), None);
            let (inside_y, outside_y) = if anchor_bottom {
                (rect.bottom() - 0.5, rect.bottom() + 1.1)
            } else {
                (rect.y + 0.5, rect.y - 1.1)
            };
            assert_eq!(hit_test_zone(&app, cx, inside_y), Some(ZoneId(460)));
            assert_eq!(hit_test_zone(&app, cx, outside_y), None);
        }
    }
}

#[test]
fn four_quadrant_settled_header_item_and_inline_search_use_resolved_panel() {
    for (name, x, y, anchor_right, anchor_bottom) in [
        ("left-top", 20, 20, false, false),
        ("right-top", 720, 20, true, false),
        ("left-bottom", 20, 520, false, true),
        ("right-bottom", 720, 520, true, true),
    ] {
        let mut zone = Zone::new(ZoneId(461), Cow::Borrowed("Consumers"), x, y, 320, 240);
        let item_id = zone
            .add_item(r"C:\Desktop\consumer.txt", "hash")
            .expect("item");
        let app = app_with_zones(vec![zone]);
        app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
        let zone = app.zones.get(ZoneId(461)).expect("zone");
        let placement = app.zone_expanded_placement(zone);
        assert_eq!(placement.anchor_right, anchor_right, "{name}");
        assert_eq!(placement.anchor_bottom, anchor_bottom, "{name}");
        let panel = app.zone_effective_rect_at(zone, 0);

        let layout = bentodesk_app::expanded_zone_grid::expanded_zone_layout_for_rect(
            panel,
            zone.items.len(),
        );
        for (button, expected) in [
            (layout.header_search_btn, HeaderButton::Search),
            (layout.header_close_btn, HeaderButton::Close),
        ] {
            assert_eq!(
                hit_test_zone_header_button(
                    &app,
                    button.x + button.width * 0.5,
                    button.y + button.height * 0.5,
                ),
                Some((ZoneId(461), expected)),
                "{name} {expected:?} header"
            );
        }

        let item = zone.item(item_id).expect("item");
        let card = bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
            zone, item, panel,
        );
        assert_eq!(
            hit_test_zone_item(
                &app,
                card.x + card.width * 0.5,
                card.y + card.height * 0.5,
            )
            .map(|(_, id, _)| id),
            Some(item_id),
            "{name} item"
        );

        app.zone_search_target.set(Some(ZoneId(461)));
        let search = bentodesk_app::business::search_bar::zone_inline_rect(panel);
        assert_eq!(
            hit_test_inline_zone_search(
                &app,
                search.x + search.width * 0.5,
                search.y + search.height * 0.5,
            ),
            Some(InlineZoneSearchHit::Body),
            "{name} inline search"
        );
    }
}
