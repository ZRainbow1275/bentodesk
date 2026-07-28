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
        position: bento_nano_backend::layout::RelativePosition {
            x_percent,
            y_percent,
        },
        expanded_size: bento_nano_backend::layout::RelativeSize {
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

    let rect = drop_preview_rect_for_zone(&zone, Some(drag), false, 0.0, 0.0).expect("preview");

    // P3.8 paint-hit parity: drag-preview placement uses the same grid SSoTs
    // as painted cards. For a 240px zone, the 64-DIP readable-card floor
    // reflows the requested 4 columns into 3 effective columns:
    // cell_w = (240 - 16*2 - 8*2) / 3 = 64; col stride = 72.
    // last_x=130 lands in col 1, last_y=116 lands in row 0 because row 0
    // starts at zone_top(20) + ITEM_GRID_TOP_OFFSET_PX(56) = 76.
    assert!((rect.x - 98.0).abs() < 0.01);
    assert!((rect.y - 76.0).abs() < 0.01);
    assert!((rect.width - 64.0).abs() < 0.01);
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

    let preview = drop_preview_rect_for_zone(&zone, Some(drag), false, 0.0, 0.0).expect("preview");
    let resident_card = item_card_rect_for_item(&zone, &zone.items[1]);

    assert_eq!(preview, resident_card);
    assert_ne!(drag.zone_id, zone.id);
    assert_ne!(drag.item_id, zone.items[1].id);
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
    app.viewport = bento_nano_style::Size {
        width: 120.0,
        height: 96.0,
    };
    let drag = ActiveItemDragVisual {
        zone_id: ZoneId(1),
        item_id: ZoneItemId(1),
        last_x: 400.0,
        last_y: 400.0,
    };
    let source = bento_nano_style::Rect {
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
    let thumbnail = bento_nano_style::Rect {
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
    let thumbnail = bento_nano_style::Rect {
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
    let row = bento_nano_style::Rect {
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
