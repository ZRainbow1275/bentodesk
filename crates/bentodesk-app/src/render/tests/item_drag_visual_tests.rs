use super::*;
use std::borrow::Cow;

fn snapshot_zone(
    visible: bool,
    x_percent: f64,
    y_percent: f64,
    w_percent: f64,
    h_percent: f64,
) -> SnapshotZone {
    SnapshotZone {
        id: smol_str::SmolStr::new_static("z1"),
        name: "Zone".to_owned(),
        icon: smol_str::SmolStr::new_static("folder"),
        position: bentodesk_backend::layout::RelativePosition {
            x_percent,
            y_percent,
        },
        expanded_size: bentodesk_backend::layout::RelativeSize {
            w_percent,
            h_percent,
        },
        items: Vec::new(),
        accent_color: Some(smol_str::SmolStr::new_static("#3b82f6")),
        sort_order: 0,
        auto_group: None,
        grid_columns: 4,
        created_at: smol_str::SmolStr::new_static(""),
        updated_at: smol_str::SmolStr::new_static(""),
        capsule_size: smol_str::SmolStr::new_static("medium"),
        capsule_shape: smol_str::SmolStr::new_static("pill"),
        locked: false,
        visible,
        stack_id: None,
        stack_order: 0,
        alias: None,
        display_mode: None,
        live_folder_path: None,
    }
}

#[test]
fn drop_preview_uses_renderer_grid_geometry() {
    let zone = Zone::new(ZoneId(7), Cow::Borrowed("z"), 10, 20, 240, 180);
    let drag = ActiveItemDragVisual {
        zone_id: ZoneId(1),
        item_id: ZoneItemId(1),
        last_x: 130.0,
        last_y: 116.0,
    };

    let panel = bentodesk_style::Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };
    let rect = drop_preview_rect_for_zone(&zone, panel, Some(drag), false, 0.0, 0.0, |_| true)
        .expect("preview");

    // The target Zone is empty, so CSS auto-flow paints the inserted card in
    // slot zero even when the pointer is farther right. Preview must match
    // that post-drop renderer position rather than promise a free-form slot.
    assert!((rect.x - 26.0).abs() < 0.01);
    assert!((rect.y - 76.0).abs() < 0.01);
    assert!((rect.width - 46.0).abs() < 0.01);
    assert!((rect.height - item_grid::ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
}

#[test]
fn drop_preview_targets_occupied_non_source_cell() {
    let mut zone = Zone::new(ZoneId(7), Cow::Borrowed("z"), 10, 20, 240, 180);
    zone.items.push(ZoneItem::new(
        ZoneItemId(8),
        "C:/Users/BentoDeskTest/Desktop/source-neighbor.lnk",
        "",
        0,
        0,
    ));
    zone.items.push(ZoneItem::new(
        ZoneItemId(9),
        "C:/Users/BentoDeskTest/Desktop/target.lnk",
        "",
        0,
        0,
    ));
    let drag = ActiveItemDragVisual {
        zone_id: ZoneId(8),
        item_id: ZoneItemId(1),
        last_x: 130.0,
        last_y: 116.0,
    };

    let panel = bentodesk_style::Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };
    let preview = drop_preview_rect_for_zone(&zone, panel, Some(drag), false, 0.0, 0.0, |_| true)
        .expect("preview");
    let resident_card =
        highlight_overlay::item_card_rect_for_item_in_panel(&zone, &zone.items[1], panel);

    assert_eq!(preview, resident_card);
    assert_ne!(drag.zone_id, zone.id);
    assert_ne!(drag.item_id, zone.items[1].id);
}

#[test]
fn inline_search_hides_drop_preview_for_a_nonmatching_dragged_item() {
    let mut source = Zone::new(ZoneId(1), Cow::Borrowed("source"), 10, 20, 240, 180);
    source.items.push(ZoneItem::new(
        ZoneItemId(1),
        "C:/Users/BentoDeskTest/Desktop/hidden.txt",
        "",
        0,
        0,
    ));
    let target = Zone::new(ZoneId(2), Cow::Borrowed("target"), 300, 20, 240, 180);
    let mut app = AppState::new();
    app.zones.add(source);
    app.zones.add(target);
    let panel = Rect {
        x: 300.0,
        y: 20.0,
        width: 240.0,
        height: 180.0,
    };
    let drag = ActiveItemDragVisual {
        zone_id: ZoneId(1),
        item_id: ZoneItemId(1),
        last_x: 330.0,
        last_y: 90.0,
    };
    let target = app.zones.get(ZoneId(2)).expect("target");

    assert!(
        drop_preview_rect_for_visible_drag(&app, target, panel, Some(drag), 0.0, 44.0, |item| item
            .name
            .contains("match"),)
        .is_none()
    );
    assert!(
        drop_preview_rect_for_visible_drag(&app, target, panel, Some(drag), 0.0, 44.0, |_| true,)
            .is_some()
    );
}

#[test]
fn four_quadrant_drop_target_and_preview_use_effective_panel() {
    for (name, x, y, anchor_right, anchor_bottom) in [
        ("left-top", 20, 20, false, false),
        ("right-top", 720, 20, true, false),
        ("left-bottom", 20, 520, false, true),
        ("right-bottom", 720, 520, true, true),
    ] {
        let mut app = AppState::new();
        app.viewport = bentodesk_style::Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones.add(Zone::new(
            ZoneId(70),
            Cow::Borrowed("Directional"),
            x,
            y,
            320,
            240,
        ));
        app.set_zone_display_mode(crate::ZoneDisplayMode::Always);
        let zone = app.zones.get(ZoneId(70)).expect("zone");
        let placement = app.zone_expanded_placement(zone);
        assert_eq!(placement.anchor_right, anchor_right, "{name}");
        assert_eq!(placement.anchor_bottom, anchor_bottom, "{name}");
        let panel = app.zone_effective_rect_at(zone, 0);
        let drag = ActiveItemDragVisual {
            zone_id: ZoneId(1),
            item_id: ZoneItemId(1),
            last_x: panel.x + 24.0,
            last_y: panel.y + 72.0,
        };

        assert_eq!(
            hit_test_render_zone(&app, drag.last_x, drag.last_y, 0),
            Some(zone.id),
            "{name} drop target"
        );
        let preview =
            drop_preview_rect_for_zone(zone, panel, Some(drag), false, 0.0, 0.0, |_| true)
                .expect("directional preview");
        assert!(
            preview.x >= panel.x && preview.right() <= panel.right(),
            "{name}"
        );
        assert!(
            preview.y >= panel.y && preview.bottom() <= panel.bottom(),
            "{name}"
        );
    }
}

#[test]
fn live_folder_badge_text_preserves_visible_path_and_compacts_long_paths() {
    let short = live_folder_badge_text("C:/Users/BentoDeskTest/Documents/Live");
    assert_eq!(
        short.as_str(),
        "Live: C:/Users/BentoDeskTest/Documents/Live"
    );

    let long = live_folder_badge_text(
        "C:/Users/BentoDeskTest/Documents/VeryLongLiveFolderPath/with/many/segments/that/should/still/show/both/prefix/and/suffix",
    );
    assert!(long.as_str().starts_with("Live: C:/Users/BentoDeskTest/"));
    assert!(long.as_str().contains('…'));
    assert!(long.as_str().ends_with("show/both/prefix/and/suffix"));
}

#[test]
fn drag_ghost_is_clamped_to_viewport() {
    let mut app = AppState::new();
    app.viewport = bentodesk_style::Size {
        width: 120.0,
        height: 96.0,
    };
    let drag = ActiveItemDragVisual {
        zone_id: ZoneId(1),
        item_id: ZoneItemId(1),
        last_x: 400.0,
        last_y: 400.0,
    };
    let source = bentodesk_style::Rect {
        x: 0.0,
        y: 0.0,
        width: 80.0,
        height: 64.0,
    };

    let ghost = drag_ghost_rect(&app, drag, source);

    assert_eq!(ghost.x, 40.0);
    assert_eq!(ghost.y, 32.0);
}

#[test]
fn snapshot_thumbnail_maps_zone_percentages_into_canvas() {
    let thumbnail = bentodesk_style::Rect {
        x: 10.0,
        y: 20.0,
        width: 160.0,
        height: 96.0,
    };
    let zone = snapshot_zone(true, 50.0, 25.0, 25.0, 50.0);

    let rect = snapshot_zone_thumbnail_rect(&zone, thumbnail).expect("visible zone");

    assert!((rect.x - 90.0).abs() < 0.01);
    assert!((rect.y - 48.0).abs() < 0.01);
    assert!((rect.width - 36.0).abs() < 0.01);
    assert!((rect.height - 40.0).abs() < 0.01);
}

#[test]
fn snapshot_thumbnail_skips_hidden_and_out_of_bounds_zones() {
    let thumbnail = bentodesk_style::Rect {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 90.0,
    };

    assert!(
        snapshot_zone_thumbnail_rect(&snapshot_zone(false, 0.0, 0.0, 20.0, 20.0), thumbnail)
            .is_none()
    );
    assert!(
        snapshot_zone_thumbnail_rect(&snapshot_zone(true, 100.0, 100.0, 20.0, 20.0), thumbnail)
            .is_none()
    );
}

#[test]
fn snapshot_row_preview_stays_inside_row() {
    let row = bentodesk_style::Rect {
        x: 20.0,
        y: 40.0,
        width: 300.0,
        height: 44.0,
    };

    let rect = snapshot_row_preview_rect(row);

    assert!(rect.x >= row.x);
    assert!(rect.y >= row.y);
    assert!(rect.right() <= row.right());
    assert!(rect.bottom() <= row.bottom());
    assert!((rect.width / rect.height - timeline_panel::THUMBNAIL_ASPECT_RATIO).abs() < 0.01);
}
