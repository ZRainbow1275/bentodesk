// V-6 Round-2 (2026-05-21) — `toolbar_point_for_event` helper retired
// alongside `mount_main_tree`'s toolbar removal. No Main-HWND
// IconButton exists to look up anymore.

#[test]
fn main_nchittest_empty_desktop_space_is_transparent() {
    let (app, win) = app_and_window_with_minibar(Vec::new());

    assert_eq!(
        main_nchittest_kind(&app, &win, 420.0, 280.0),
        HitKind::Transparent
    );
}

// V-6 Round-2 (2026-05-21) — `main_nchittest_keeps_toolbar_buttons_clickable`
// retired. `mount_main_tree` no longer attaches IconButtons to the
// Main HWND tree (the legacy toolbar painted at top-left of the
// transparent desktop overlay — pre-parity scaffolding removed). The
// remaining hit-test surface that needs to stay clickable on the Main
// HWND is zones / settings modal / about modal — those continue to be
// covered by `main_nchittest_keeps_real_zone_surfaces_clickable` +
// `main_nchittest_keeps_modal_overlay_dismissal_clickable` below.
#[test]
fn _retired_main_nchittest_keeps_toolbar_buttons_clickable_v6_r2() {}

#[test]
fn main_nchittest_keeps_real_zone_surfaces_clickable() {
    let zone = Zone::new(ZoneId(21), Cow::Borrowed("zone"), 120, 120, 180, 120);
    let (app, win) = app_and_window_with_minibar(vec![zone]);

    assert_eq!(
        main_nchittest_kind(&app, &win, 140.0, 150.0),
        HitKind::Client
    );
    assert_eq!(
        main_nchittest_kind(&app, &win, 420.0, 280.0),
        HitKind::Transparent
    );
}

#[test]
fn main_nchittest_keeps_app_rendered_context_menu_clickable() {
    let (app, win) = app_and_window_with_minibar(Vec::new());
    let mut rows = popover::ContextMenuRows::new();
    rows.push(popover::ContextMenuRow::command(
        1,
        "Edit zone",
        bento_nano_app::business::icons::IconKind::Edit,
    ));
    let mut session = popover::ContextMenuSession::new(rows, popover::ContextMenuRows::new());
    session.set_origin(320.0, 240.0);
    app.active_context_menu.borrow_mut().replace(session);

    assert_eq!(
        main_nchittest_kind(&app, &win, 340.0, 260.0),
        HitKind::Client
    );
    assert_eq!(
        main_nchittest_kind(&app, &win, 40.0, 40.0),
        HitKind::Transparent
    );
}

#[test]
fn main_nchittest_keeps_bloom_petals_and_floating_preview_clickable() {
    let anchor = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);
    let member = Zone::new(ZoneId(2), Cow::Borrowed("Member"), 420, 100, 320, 220);
    let (mut app, win) = app_and_window_with_minibar(vec![anchor, member]);
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    app.stack_bloom_anchor.set(Some(ZoneId(1)));
    app.stack_bloom_progress.set(1.0);
    app.stack_tray
        .borrow_mut()
        .replace(stack_tray::StackTrayState::bloom_preview(
            ZoneId(1),
            ZoneId(2),
        ));

    let anchor = app.zones.get(ZoneId(1)).expect("anchor");
    let member = app.zones.get(ZoneId(2)).expect("member");
    let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2);
    let selected_petal = petals[1];
    let preview =
        stack_tray::focused_bloom_preview_rect(app.viewport, selected_petal, &petals, member);

    assert_eq!(
        main_nchittest_kind(
            &app,
            &win,
            selected_petal.x + selected_petal.width * 0.5,
            selected_petal.y + selected_petal.height * 0.5
        ),
        HitKind::Client
    );
    assert_eq!(
        main_nchittest_kind(
            &app,
            &win,
            preview.x + preview.width * 0.5,
            preview.y + preview.height * 0.5
        ),
        HitKind::Client
    );
}

#[test]
fn main_nchittest_aux_modal_does_not_block_blank_desktop() {
    let (app, win) = app_and_window_with_minibar(Vec::new());
    app.settings_open.set(true);

    assert_eq!(
        main_nchittest_kind(&app, &win, 420.0, 280.0),
        HitKind::Transparent
    );
}

#[test]
fn settings_nchittest_drags_from_painted_header_but_not_close_button() {
    let viewport = Size {
        width: 480.0,
        height: 853.0,
    };
    let header = bento_nano_app::settings_panel::settings_header_rect(viewport);
    let close = bento_nano_app::settings_panel::settings_close_button_rect_m1(viewport);

    assert_eq!(
        settings_nchittest_kind(viewport, header.x + 120.0, header.y + header.height * 0.5),
        HitKind::Caption
    );
    assert_eq!(
        settings_nchittest_kind(
            viewport,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        HitKind::Client
    );
}

#[test]
fn search_nchittest_keeps_margin_transparent_and_header_draggable() {
    let viewport = Size {
        width: 620.0,
        height: 540.0,
    };
    let panel = bento_nano_app::business::search_bar::search_panel_rect(viewport);
    let close = bento_nano_app::business::search_bar::search_close_rect(viewport);
    assert_eq!(
        search_nchittest_kind(viewport, 2.0, 2.0),
        HitKind::Transparent
    );
    assert_eq!(
        search_nchittest_kind(viewport, panel.x + 120.0, panel.y + 24.0),
        HitKind::Caption
    );
    assert_eq!(
        search_nchittest_kind(
            viewport,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5,
        ),
        HitKind::Client
    );
}

#[test]
fn about_nchittest_drags_from_identity_header_but_not_close_button() {
    let viewport = Size {
        width: 640.0,
        height: 520.0,
    };
    let close = bento_nano_app::business::about::close_button_rect(viewport);

    assert_eq!(
        about_nchittest_kind(viewport, 240.0, 72.0),
        HitKind::Caption
    );
    assert_eq!(
        about_nchittest_kind(
            viewport,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        HitKind::Client
    );
    assert_eq!(
        about_nchittest_kind(viewport, 240.0, 300.0),
        HitKind::Client
    );
}

#[test]
fn zone_editor_nchittest_drags_only_header_and_preserves_close_input() {
    let viewport = Size {
        width: 480.0,
        height: 460.0,
    };
    let header = bento_nano_app::zone_editor_geometry::zone_editor_header_rect(viewport);
    let close = bento_nano_app::zone_editor_geometry::zone_editor_close_rect(viewport);
    let input = bento_nano_app::zone_editor_geometry::zone_editor_name_input_rect(viewport);

    assert_eq!(
        zone_editor_nchittest_kind(viewport, header.x + 96.0, header.y + header.height * 0.5),
        HitKind::Caption
    );
    assert_eq!(
        zone_editor_nchittest_kind(
            viewport,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        HitKind::Client
    );
    assert_eq!(
        zone_editor_nchittest_kind(
            viewport,
            input.x + input.width * 0.5,
            input.y + input.height * 0.5
        ),
        HitKind::Client
    );
}

#[test]
fn auxiliary_panel_nchittest_drags_header_without_swallowing_close_edge() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    assert_eq!(
        auxiliary_panel_nchittest_kind(viewport, 180.0, 26.0),
        HitKind::Caption
    );
    assert_eq!(
        auxiliary_panel_nchittest_kind(viewport, 680.0, 26.0),
        HitKind::Client
    );
    assert_eq!(
        auxiliary_panel_nchittest_kind(viewport, 180.0, 92.0),
        HitKind::Client
    );
    assert_eq!(
        auxiliary_panel_nchittest_kind(viewport, -1.0, 26.0),
        HitKind::Transparent
    );
}

#[test]
fn bulk_manager_nchittest_keeps_header_search_and_close_interactive() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    let search = bento_nano_app::business::bulk_manager_panel::bulk_manager_search_rect(viewport);
    let close = bento_nano_app::business::bulk_manager_panel::bulk_manager_close_rect(viewport);

    assert_eq!(
        bulk_manager_nchittest_kind(
            viewport,
            search.x + search.width * 0.5,
            search.y + search.height * 0.5
        ),
        HitKind::Client
    );
    assert_eq!(
        bulk_manager_nchittest_kind(
            viewport,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        HitKind::Client
    );
    assert_eq!(
        bulk_manager_nchittest_kind(viewport, 180.0, 26.0),
        HitKind::Caption
    );
    assert_eq!(
        bulk_manager_nchittest_kind(viewport, -1.0, 26.0),
        HitKind::Transparent
    );
}

#[test]
fn hit_test_zone_returns_topmost_when_overlapping() {
    let app = app_with_zones(vec![
        Zone::new(ZoneId(1), Cow::Borrowed("a"), 0, 0, 100, 100),
        Zone::new(ZoneId(2), Cow::Borrowed("b"), 50, 50, 100, 100),
    ]);
    // Inside the overlap region — id 2 wins (drawn last).
    assert_eq!(hit_test_zone(&app, 75.0, 75.0), Some(ZoneId(2)));
    // Only id 1 covers (10, 10).
    assert_eq!(hit_test_zone(&app, 10.0, 10.0), Some(ZoneId(1)));
    // Empty space.
    assert_eq!(hit_test_zone(&app, 400.0, 300.0), None);
}

#[test]
fn hit_test_zone_skips_hidden_zones() {
    let mut hidden = Zone::new(ZoneId(2), Cow::Borrowed("hidden"), 50, 50, 100, 100);
    hidden.set_visible(false);
    let app = app_with_zones(vec![
        Zone::new(ZoneId(1), Cow::Borrowed("visible"), 0, 0, 100, 100),
        hidden,
    ]);

    // Wave C — visible zone collapses to its pill rect (≤96×36 at the
    // zone origin); the resize corner is only surfaced for expanded
    // zones, so the legacy `(145, 145)` corner is no longer a hit.
    assert_eq!(hit_test_zone(&app, 20.0, 20.0), Some(ZoneId(1)));
    assert_eq!(hit_test_zone_resize_corner(&app, 145.0, 145.0), None);
}

#[test]
fn hit_test_zone_skips_stacked_children() {
    let mut app = app_with_zones(vec![
        Zone::new(ZoneId(1), Cow::Borrowed("anchor"), 0, 0, 150, 150),
        Zone::new(ZoneId(2), Cow::Borrowed("child"), 50, 50, 150, 150),
    ]);
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));

    // #4 (2026-06-02) — a COLLAPSED stack anchor renders as the compact
    // stack pill at its origin (≤96×36), NOT the full expanded body, so a
    // point near the origin hits the anchor and the stacked child (ZoneId 2)
    // is never independently hit-testable. The legacy `(75, 75)` was inside
    // the old always-expanded anchor body; the pill no longer reaches it.
    assert_eq!(hit_test_zone(&app, 8.0, 8.0), Some(ZoneId(1)));
    assert_eq!(hit_test_zone(&app, 75.0, 75.0), None);
    // A collapsed pill (incl. a stack-anchor pill) has no resize corner.
    assert_eq!(hit_test_zone_resize_corner(&app, 195.0, 195.0), None);
}

#[test]
fn hovered_stack_anchor_has_no_hidden_panel_hit_targets() {
    let mut anchor = Zone::new(ZoneId(1), Cow::Borrowed("anchor"), 0, 0, 220, 160);
    let _item = anchor
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Anchor.lnk".to_owned()),
            Cow::Borrowed("hash-anchor"),
        )
        .expect("anchor item");
    let mut app = app_with_zones(vec![
        anchor,
        Zone::new(ZoneId(2), Cow::Borrowed("child"), 50, 50, 150, 150),
    ]);
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
    // Default Hover mode sets `zone_body_visible_for_mode(anchor) == true`,
    // but the 2026-06-02 stack contract says hover shows compact pill +
    // bloom only. The shared `zone_pill_body_visible` SSoT must therefore
    // keep every expanded-panel hit target absent until explicit selection.
    app.hovered_zone.set(Some(ZoneId(1)));

    assert!(!app.zone_pill_body_visible(app.zones.get(ZoneId(1)).unwrap()));
    assert_eq!(hit_test_zone(&app, 120.0, 120.0), None);
    assert_eq!(hit_test_zone_item(&app, 24.0, 70.0), None);
    assert_eq!(hit_test_zone_resize_corner(&app, 215.0, 155.0), None);
    assert_eq!(hit_test_zone_header_button(&app, 196.0, 24.0), None);
}

#[test]
fn hit_test_zone_item_skips_hidden_zone_items() {
    let mut hidden = Zone::new(ZoneId(8), Cow::Borrowed("hidden"), 10, 10, 240, 180);
    hidden.set_visible(false);
    hidden
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Hidden.lnk".to_owned()),
            Cow::Borrowed("hash"),
        )
        .expect("item id");
    let app = app_with_zones(vec![hidden]);

    // P3.8: row 0 starts at zone_top(10) + 56-DIP header/content offset =
    // 66 and column 0 starts at zone_left(10) + 16-DIP inset = 26. The
    // point is inside the painted card, but the zone is hidden, so still
    // None.
    assert_eq!(hit_test_zone_item(&app, 28.0, 70.0), None);
}

#[test]
fn hit_test_zone_item_returns_item_under_visible_card() {
    let mut zone = Zone::new(ZoneId(1), Cow::Borrowed("z"), 10, 10, 240, 180);
    let item_id = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/App.lnk".to_owned()),
            Cow::Borrowed("hash"),
        )
        .expect("item id");
    let app = app_with_zones(vec![zone]);
    // Wave C — items are reachable only when the zone is expanded.
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

    // P3.8: grid row 0 begins at zone_top(10) + 56-DIP header/content
    // offset = 66; x starts at zone_left(10) + 16-DIP inset = 26.
    let hit = hit_test_zone_item(&app, 28.0, 70.0).expect("item hit");
    assert_eq!(hit.0, ZoneId(1));
    assert_eq!(hit.1, item_id);
    assert_eq!(hit.2, "C:/Users/BentoDeskTest/Desktop/App.lnk");
    assert_eq!(hit_test_zone_item(&app, 400.0, 300.0), None);
}

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
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

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
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

    assert_ne!(
        hit_test_zone_item(&app, 28.0, 70.0).map(|hit| hit.1),
        Some(third)
    );
    app.set_zone_content_scroll(ZoneId(3), 86.0);
    assert_eq!(
        hit_test_zone_item(&app, 28.0, 70.0).map(|hit| hit.1),
        Some(third)
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
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

    // The renderer auto-places the wide first card across slots 0-1. The
    // second item is therefore painted in effective slot 2, not at its raw
    // persisted grid_x=1. The shell hit-test must use that same item-aware
    // helper or a click on the visible second card starts no drag.
    let second_hit = hit_test_zone_item(&app, 261.0, 427.0).expect("second item hit");
    assert_eq!(second_hit.0, ZoneId(4));
    assert_eq!(second_hit.1, second);
    assert_eq!(second_hit.2, "C:/Users/BentoDeskTest/Desktop/item-02.txt");

    let third_hit = hit_test_zone_item(&app, 335.0, 427.0).expect("third item hit");
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
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);
    // Inside body but outside corner.
    assert_eq!(hit_test_zone_resize_corner(&app, 150.0, 150.0), None);
    // Inside the 12×12 corner box (right=300, bottom=200).
    assert_eq!(
        hit_test_zone_resize_corner(&app, 295.0, 195.0),
        Some(ZoneId(7))
    );
    // Edge boundary excluded (`<` not `<=`).
    assert_eq!(hit_test_zone_resize_corner(&app, 300.0, 200.0), None);
}

#[test]
fn hit_test_zone_header_button_search_and_close_when_expanded() {
    // GROUP-4 (2026-06-01) — the expanded PanelHeader search + close
    // buttons. Geometry is the paint==hit SSoT, so click the centre of
    // each layout rect and assert the right button.
    let zone = Zone::new(ZoneId(9), Cow::Borrowed("z"), 100, 100, 400, 200);
    let app = app_with_zones(vec![zone]);
    // Header buttons only exist on an expanded (body-visible) zone.
    app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

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
