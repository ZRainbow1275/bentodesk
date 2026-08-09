#[test]
fn reported_move_keeps_card_when_source_still_exists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-drag-out-unoptimized-move");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("state dir");
    let source = state_dir.join("source-still-present.txt");
    std::fs::write(&source, b"source bytes").expect("source");
    let zone_id = ZoneId(84);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Safe Drag Out", 0, 0, 280, 180);
        let item_id = zone
            .add_item(
                Cow::Owned(source.to_string_lossy().into_owned()),
                Cow::Borrowed("source-hash"),
            )
            .expect("item");
        app.zones.add(zone);
        item_id
    };

    finalize_item_drag_out(
        &root,
        &PendingItemDragOut {
            zone_id,
            item_id,
            path: SmolStr::new(source.to_string_lossy()),
            copy_only: false,
        },
        "source-still-present.txt",
        bentodesk_backend::drag_drop::DragOutcome::Moved,
    );

    let app = root.app.borrow();
    assert!(app.zones.item(zone_id, item_id).is_some());
    assert_eq!(
        app.item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Copied out: source-still-present.txt")
    );
    assert!(source.exists());
    drop(app);
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn shell_move_requires_an_exact_not_found_result_before_model_removal() {
    assert!(matches!(
        source_missing_from_metadata(Err(std::io::Error::from(std::io::ErrorKind::NotFound,))),
        Ok(true)
    ));
    assert_eq!(
        source_missing_from_metadata(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )))
        .expect_err("metadata failure must keep the model")
        .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let metadata = std::fs::symlink_metadata(std::env::current_exe().expect("test executable"))
        .expect("test executable metadata");
    assert!(matches!(
        source_missing_from_metadata(Ok(metadata)),
        Ok(false)
    ));
}
