#[test]
fn move_item_to_zone_moves_hidden_file_between_zone_dirs_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-move-zone-hidden");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    let source_hidden_dir = desktop_dir.join(".bentodesk").join("48");
    std::fs::create_dir_all(&source_hidden_dir).expect("source hidden dir");
    let original = desktop_dir.join("move-zone.txt");
    let hidden = source_hidden_dir.join("move-zone.txt");
    std::fs::write(&hidden, b"move").expect("hidden file");
    let original_path = original.to_string_lossy().to_string();
    let hidden_path = hidden.to_string_lossy().to_string();
    let from_zone_id = ZoneId(48);
    let to_zone_id = ZoneId(49);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut from_zone = Zone::new(from_zone_id, "From", 0, 0, 240, 160);
        let item_id = from_zone
            .add_item_with_metadata(
                Cow::Owned(hidden_path.clone()),
                Some(original_path.as_str()),
                Cow::Borrowed("hash"),
                Some(Cow::Owned(original_path.clone())),
                Some(Cow::Owned(hidden_path.clone())),
            )
            .expect("item id");
        app.zones.add(from_zone);
        let mut to_zone = Zone::new(to_zone_id, "To", 260, 0, 240, 260);
        for id in [1, 10, 11, 12] {
            to_zone.items.push(ZoneItem::new(
                ZoneItemId(id),
                format!("C:/Desktop/resident-{id}.txt"),
                "",
                0,
                0,
            ));
        }
        app.zones.add(to_zone);
        item_id
    };

    let (drop_point, target_index, preview) = {
        let app = root.app.borrow();
        let zone = app.zones.get(to_zone_id).expect("target zone");
        let panel = bentodesk_style::Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        };
        let cell = bentodesk_app::business::highlight_overlay::item_card_rect_for_grid_in_panel(
            zone, 3, 0, false, panel,
        );
        let (grid_x, grid_y, target_index, preview) =
            bentodesk_app::business::highlight_overlay::item_drop_target_for_panel(
                zone,
                panel,
                None,
                false,
                (cell.x + 2.0, cell.y + 2.0),
                0.0,
                |_| true,
            )
            .expect("cross-zone target");
        (DispatchPoint::new(grid_x, grid_y), target_index, preview)
    };

    root.dispatcher.push(Command::MoveItemToZone(
        from_zone_id,
        to_zone_id,
        bentodesk_app::ItemId(item_id.0),
        Some((drop_point, target_index)),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    let expected_hidden = desktop_dir
        .join(".bentodesk")
        .join("49")
        .join("move-zone.txt");
    let expected_hidden_path = expected_hidden.to_string_lossy().to_string();
    {
        let app = root.app.borrow();
        assert!(!app.dirty.get(), "dispatcher should flush moved item");
        assert!(app.zones.item(from_zone_id, item_id).is_none());
        let zone = app.zones.get(to_zone_id).expect("target zone");
        let item = zone
            .items
            .iter()
            .find(|item| item.path.as_ref() == expected_hidden_path)
            .expect("moved item in target zone");
        assert_ne!(item.id, item_id, "target collision must remint the item id");
        assert_eq!(item.id, ZoneItemId(13));
        assert_eq!(
            zone.items.iter().filter(|candidate| candidate.id == item_id).count(),
            1,
            "resident target id must remain unique"
        );
        assert_eq!(item.path.as_ref(), expected_hidden_path.as_str());
        assert_eq!(
            item.hidden_path.as_deref(),
            Some(expected_hidden_path.as_str())
        );
        assert_eq!(item.original_path.as_deref(), Some(original_path.as_str()));
        assert_eq!((item.x, item.y), (drop_point.x, drop_point.y));
        assert_eq!(
            app.zones
                .get(to_zone_id)
                .and_then(|zone| zone.items.get(target_index))
                .map(|item| item.id),
            Some(ZoneItemId(13))
        );
        let panel = bentodesk_style::Rect {
            x: zone.x as f32,
            y: zone.y as f32,
            width: zone.w as f32,
            height: zone.h as f32,
        };
        assert_eq!(
            bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
                zone, item, panel,
            ),
            preview,
            "cross-zone preview must equal post-dispatch paint"
        );
        let resident = zone.item(item_id).expect("resident item remains addressable");
        assert_ne!(
            bentodesk_app::business::highlight_overlay::item_card_rect_for_item_in_panel(
                zone, resident, panel,
            ),
            preview,
            "resident and moved item must not overlap after remint"
        );
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Moved hidden item to zone: move-zone.txt")
        );
    }
    assert!(!hidden.exists(), "source zone hidden file should move away");
    assert!(
        expected_hidden.exists(),
        "target zone hidden file should exist"
    );
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    let zone = reloaded.get(to_zone_id).expect("persisted target zone");
    let item = zone
        .items
        .iter()
        .find(|item| item.path.as_ref() == expected_hidden_path)
        .expect("persisted moved item");
    assert_eq!(item.id, ZoneItemId(13));
    assert_eq!(
        zone.items
            .iter()
            .map(|item| item.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        zone.items.len(),
        "persisted target item ids must be unique"
    );
    assert_eq!(item.path.as_ref(), expected_hidden_path.as_str());
    assert_eq!(
        item.hidden_path.as_deref(),
        Some(expected_hidden_path.as_str())
    );
    assert_eq!((item.x, item.y), (drop_point.x, drop_point.y));

    let _ = std::fs::remove_dir_all(state_dir);
}
