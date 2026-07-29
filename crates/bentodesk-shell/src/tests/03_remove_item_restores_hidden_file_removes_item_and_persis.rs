#[test]
fn remove_item_restores_hidden_file_removes_item_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-remove-restore");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    let hidden_dir = state_dir.join(".bentodesk").join("46");
    std::fs::create_dir_all(&desktop_dir).expect("desktop dir");
    std::fs::create_dir_all(&hidden_dir).expect("hidden dir");
    let original = desktop_dir.join("restore-me.txt");
    let hidden = hidden_dir.join("restore-me.txt");
    std::fs::write(&hidden, b"restore").expect("hidden file");
    let original_path = original.to_string_lossy().to_string();
    let hidden_path = hidden.to_string_lossy().to_string();
    let zone_id = ZoneId(46);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Restore", 0, 0, 240, 160);
        let item_id = zone
            .add_item_with_metadata(
                Cow::Owned(hidden_path.clone()),
                Some(original_path.as_str()),
                Cow::Borrowed("hash"),
                Some(Cow::Owned(original_path.clone())),
                Some(Cow::Owned(hidden_path.clone())),
            )
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    root.dispatcher.push(Command::RemoveItem(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    {
        let app = root.app.borrow();
        assert!(!app.dirty.get(), "dispatcher should flush removed item");
        assert!(app.zones.item(zone_id, item_id).is_none());
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Removed item: restore-me.txt")
        );
    }
    assert!(
        original.exists(),
        "hidden file should be restored to Desktop"
    );
    assert!(!hidden.exists(), "hidden mirror should be moved out");
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    assert!(reloaded.item(zone_id, item_id).is_none());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn remove_item_keeps_item_when_hidden_restore_fails_and_reports_status() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-remove-restore-fail");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop_dir = state_dir.join("Desktop");
    let hidden_dir = state_dir.join(".bentodesk").join("47");
    std::fs::create_dir_all(&desktop_dir).expect("desktop dir");
    std::fs::create_dir_all(&hidden_dir).expect("hidden dir");
    let original = desktop_dir.join("missing-hidden.txt");
    let hidden = hidden_dir.join("missing-hidden.txt");
    let original_path = original.to_string_lossy().to_string();
    let hidden_path = hidden.to_string_lossy().to_string();
    let zone_id = ZoneId(47);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Restore", 0, 0, 240, 160);
        let item_id = zone
            .add_item_with_metadata(
                Cow::Owned(hidden_path.clone()),
                Some(original_path.as_str()),
                Cow::Borrowed("hash"),
                Some(Cow::Owned(original_path.clone())),
                Some(Cow::Owned(hidden_path)),
            )
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    root.dispatcher.push(Command::RemoveItem(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert!(!app.dirty.get(), "failed restore must not mutate layout");
    assert!(app.zones.item(zone_id, item_id).is_some());
    assert!(!original.exists());
    assert!(!hidden.exists());
    assert_eq!(
        app.item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Remove item failed: missing-hidden.txt")
    );

    drop(app);
    let _ = std::fs::remove_dir_all(state_dir);
}

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
        app.zones.add(Zone::new(to_zone_id, "To", 260, 0, 240, 160));
        item_id
    };

    root.dispatcher.push(Command::MoveItemToZone(
        from_zone_id,
        to_zone_id,
        bentodesk_app::ItemId(item_id.0),
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
        let item = app
            .zones
            .item(to_zone_id, item_id)
            .expect("moved item in target zone");
        assert_eq!(item.path.as_ref(), expected_hidden_path.as_str());
        assert_eq!(
            item.hidden_path.as_deref(),
            Some(expected_hidden_path.as_str())
        );
        assert_eq!(item.original_path.as_deref(), Some(original_path.as_str()));
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
    let item = reloaded
        .item(to_zone_id, item_id)
        .expect("persisted moved item");
    assert_eq!(item.path.as_ref(), expected_hidden_path.as_str());
    assert_eq!(
        item.hidden_path.as_deref(),
        Some(expected_hidden_path.as_str())
    );

    let _ = std::fs::remove_dir_all(state_dir);
}
#[test]
fn move_item_command_updates_grid_position_status_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-move-grid");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch dir");
    let source = state_dir.join("grid-move.txt");
    std::fs::write(&source, b"grid").expect("source file");
    let source_path = source.to_string_lossy().to_string();
    let zone_id = ZoneId(50);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Grid", 0, 0, 240, 160);
        let item_id = zone
            .add_item(Cow::Owned(source_path.clone()), Cow::Borrowed("hash"))
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    root.dispatcher.push(Command::MoveItem(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
        DispatchPoint::new(3, 4),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    {
        let app = root.app.borrow();
        assert!(
            !app.dirty.get(),
            "dispatcher should flush moved grid position"
        );
        let item = app.zones.item(zone_id, item_id).expect("moved item");
        assert_eq!((item.x, item.y), (3, 4));
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Moved item: grid-move.txt (3, 4)")
        );
    }
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    let item = reloaded.item(zone_id, item_id).expect("persisted item");
    assert_eq!((item.x, item.y), (3, 4));

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn toggle_item_wide_updates_status_and_persists() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-toggle-wide");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch dir");
    let source = state_dir.join("wide.txt");
    std::fs::write(&source, b"wide").expect("source file");
    let source_path = source.to_string_lossy().to_string();
    let zone_id = ZoneId(51);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Grid", 0, 0, 240, 160);
        let item_id = zone
            .add_item(Cow::Owned(source_path.clone()), Cow::Borrowed("hash"))
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    root.dispatcher.push(Command::ToggleItemWide(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    {
        let app = root.app.borrow();
        assert!(!app.dirty.get(), "dispatcher should flush wide toggle");
        let item = app.zones.item(zone_id, item_id).expect("wide item");
        assert!(item.is_wide);
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Item wide enabled: wide.txt")
        );
    }
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    assert!(
        reloaded
            .item(zone_id, item_id)
            .expect("persisted item")
            .is_wide
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn item_drag_out_reports_visible_outcomes() {
    let root = test_app_root();
    let path = r"C:\Users\BentoDeskTest\Desktop\drag-me.txt".to_owned();
    let mut captured_files = Vec::<String>::new();

    assert!(start_item_drag_out_with(&root, path.clone(), |files| {
        captured_files = files.to_vec();
        Ok(bentodesk_backend::drag_drop::DragOutcome::Dropped)
    }));
    assert_eq!(captured_files, vec![path.clone()]);
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Dragged out: drag-me.txt")
    );

    assert!(start_item_drag_out_with(&root, path.clone(), |_files| {
        Ok(bentodesk_backend::drag_drop::DragOutcome::Cancelled)
    }));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Drag out cancelled: drag-me.txt")
    );

    assert!(start_item_drag_out_with(&root, path, |_files| {
        Err(bentodesk_backend::drag_drop::DragDropError::NoFiles)
    }));
    assert_eq!(
        root.app
            .borrow()
            .item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Drag out failed: drag-me.txt: no files to drag")
    );
}

#[test]
fn successful_drag_out_uses_the_actual_ole_effect() {
    let root = test_app_root();
    let zone_id = ZoneId(83);
    let (move_item_id, copy_item_id) = {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(zone_id, "Drag Out", 0, 0, 280, 180);
        let move_item_id = zone
            .add_item(
                Cow::Borrowed("C:/Users/BentoDeskTest/Desktop/move-out.txt"),
                Cow::Borrowed("move-hash"),
            )
            .expect("move item");
        let copy_item_id = zone
            .add_item(
                Cow::Borrowed("C:/Users/BentoDeskTest/Desktop/copy-out.txt"),
                Cow::Borrowed("copy-hash"),
            )
            .expect("copy item");
        app.zones.add(zone);
        (move_item_id, copy_item_id)
    };

    let move_request = PendingItemDragOut {
        zone_id,
        item_id: move_item_id,
        path: SmolStr::new("C:/Users/BentoDeskTest/Desktop/move-out.txt"),
        copy_only: false,
    };
    finalize_item_drag_out(
        &root,
        &move_request,
        "move-out.txt",
        bentodesk_backend::drag_drop::DragOutcome::Moved,
    );
    {
        let app = root.app.borrow();
        assert!(
            app.zones.item(zone_id, move_item_id).is_none(),
            "ordinary successful drag-out must remove the source Zone item"
        );
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Moved out: move-out.txt")
        );
    }

    let copy_request = PendingItemDragOut {
        zone_id,
        item_id: copy_item_id,
        path: SmolStr::new("C:/Users/BentoDeskTest/Desktop/copy-out.txt"),
        copy_only: false,
    };
    finalize_item_drag_out(
        &root,
        &copy_request,
        "copy-out.txt",
        bentodesk_backend::drag_drop::DragOutcome::Copied,
    );
    let app = root.app.borrow();
    assert!(
        app.zones.item(zone_id, copy_item_id).is_some(),
        "a COPY effect from any target must keep the source item"
    );
    assert_eq!(
        app.item_operation_status
            .borrow()
            .as_ref()
            .map(SmolStr::as_str),
        Some("Copied out: copy-out.txt")
    );
}

#[test]
fn successful_drag_out_removes_stealth_item_after_shell_moved_hidden_file() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-drag-out-stealth-move");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("state dir");
    let original = state_dir.join("Desktop").join("moved-out.url");
    std::fs::create_dir_all(original.parent().expect("desktop")).expect("desktop");
    std::fs::write(&original, b"drag-out bytes").expect("source");
    let config = bentodesk_backend::stealth::StealthConfig {
        desktop_path: SmolStr::new(original.parent().expect("desktop").to_string_lossy()),
        app_data_dir: SmolStr::new(state_dir.to_string_lossy()),
    };
    let (original_path, hidden_path) = bentodesk_backend::stealth::hide_file(
        &config,
        &original.to_string_lossy(),
        "91",
        "File",
        None,
        None,
        None,
    )
    .expect("hide with recovery manifest");
    let hidden = std::path::PathBuf::from(&hidden_path);
    let zone_id = ZoneId(91);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Stealth Drag Out", 0, 0, 280, 180);
        let item_id = zone
            .add_item_with_metadata(
                Cow::Owned(hidden_path.clone()),
                Some(original_path.as_str()),
                Cow::Borrowed("url-hash"),
                Some(Cow::Owned(original_path.clone())),
                Some(Cow::Owned(hidden_path.clone())),
            )
            .expect("stealth item");
        app.zones.add(zone);
        item_id
    };

    // Mirrors the state after Explorer accepted DROPEFFECT_MOVE.
    let external = state_dir.join("External").join("moved-out.url");
    std::fs::create_dir_all(external.parent().expect("external")).expect("external");
    std::fs::rename(&hidden, &external).expect("shell moved hidden source");
    assert!(!hidden.exists());
    assert!(!original.exists());
    finalize_item_drag_out(
        &root,
        &PendingItemDragOut {
            zone_id,
            item_id,
            path: SmolStr::new(hidden_path),
            copy_only: false,
        },
        "moved-out.url",
        bentodesk_backend::drag_drop::DragOutcome::Moved,
    );

    {
        let app = root.app.borrow();
        assert!(
            app.zones.item(zone_id, item_id).is_none(),
            "Shell move completion must remove the stale Zone card"
        );
        assert!(
            !app.dirty.get(),
            "drag-out completion must persist immediately"
        );
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Moved out: moved-out.url")
        );
    }
    let reloaded = storage::read_zones(&zones_path).expect("persisted zones");
    assert!(reloaded.item(zone_id, item_id).is_none());
    assert_eq!(
        bentodesk_backend::stealth::load_manifest(
            &original.parent().expect("desktop").join(".bentodesk")
        )
        .expect("manifest")
        .entries
        .len(),
        0,
        "successful external MOVE must not leave stale recovery metadata"
    );
    assert_eq!(
        std::fs::read(external).expect("external payload"),
        b"drag-out bytes"
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn item_drag_out_suspends_own_ole_drop_target() {
    let root = test_app_root();
    let path = r"C:\Users\BentoDeskTest\Desktop\drag-me.txt".to_owned();
    let mut observed_guard = false;

    assert!(!root.item_drag_out_active.get());
    assert!(start_item_drag_out_with(&root, path, |_files| {
        observed_guard = root.item_drag_out_active.get();
        Ok(bentodesk_backend::drag_drop::DragOutcome::Dropped)
    }));

    assert!(
        observed_guard,
        "BentoDesk must reject its own OLE drop target while acting as the drag source"
    );
    assert!(
        !root.item_drag_out_active.get(),
        "drag-out guard must clear after OLE returns"
    );
}

#[test]
fn item_drag_out_starts_when_pointer_leaves_bento_zone() {
    let root = test_app_root();
    seed_test_zone(&root, 1, "Docs");
    root.app.borrow_mut().viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let app = root.app.borrow();

    assert!(
        !should_start_item_drag_out(&app, 40.0, 60.0, false),
        "dragging over the source zone remains an internal item move"
    );
    assert!(
        should_start_item_drag_out(&app, 260.0, 60.0, false),
        "blank desktop inside the main overlay must start external OLE drag-out"
    );
    assert!(
        should_start_item_drag_out(&app, -1.0, 60.0, false),
        "outside the overlay remains an external OLE drag-out path"
    );
    assert!(
        should_start_item_drag_out(&app, 40.0, 60.0, true),
        "Ctrl-drag follows Explorer copy semantics and enters OLE drag-out immediately"
    );
}

#[test]
fn normalized_rename_leaf_accepts_leaf_name_only() {
    assert_eq!(
        normalized_rename_leaf("  report-final.pdf  "),
        Ok("report-final.pdf".to_owned())
    );
    assert_eq!(normalized_rename_leaf(""), Err("empty name"));
    assert_eq!(normalized_rename_leaf(".."), Err("reserved name"));
    assert_eq!(
        normalized_rename_leaf("folder/report.pdf"),
        Err("invalid filename character")
    );
    assert_eq!(
        normalized_rename_leaf("report?.pdf"),
        Err("invalid filename character")
    );
    assert_eq!(
        normalized_rename_leaf("report. "),
        Err("trailing dot/space")
    );
}

#[test]
fn item_file_display_path_prefers_original_visible_path() {
    let mut item = ZoneItem::new(
        ZoneItemId(7),
        Cow::Owned("C:/Users/BentoDeskTest/Desktop/.bentodesk/z/report.pdf".to_owned()),
        Cow::Owned("hash".to_owned()),
        0,
        0,
    );
    item.original_path = Some(Cow::Owned(
        "C:/Users/BentoDeskTest/Desktop/report.pdf".to_owned(),
    ));
    item.hidden_path = Some(Cow::Owned(
        "C:/Users/BentoDeskTest/Desktop/.bentodesk/z/report.pdf".to_owned(),
    ));

    assert_eq!(
        item_file_display_path(&item),
        "C:/Users/BentoDeskTest/Desktop/report.pdf"
    );
}

#[test]
fn item_rename_command_renames_real_file_and_persists_zones_bin() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("item-rename");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch dir");
    let source = state_dir.join("report.txt");
    std::fs::write(&source, b"contract").expect("source file");
    let renamed = state_dir.join("report-final.txt");
    let source_path = source.to_string_lossy().to_string();
    let renamed_path = renamed.to_string_lossy().to_string();
    let zone_id = ZoneId(41);
    let item_id = {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(zone_id, "Docs", 0, 0, 240, 160);
        let item_id = zone
            .add_item(Cow::Owned(source_path), Cow::Borrowed("hash"))
            .expect("item id");
        app.zones.add(zone);
        item_id
    };

    root.dispatcher.push(Command::RenameItemFile(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
        SmolStr::new_static("report-final.txt"),
    ));
    consume_dispatcher(&root, std::ptr::null_mut());

    assert!(!source.exists());
    assert!(renamed.exists());
    {
        let app = root.app.borrow();
        assert!(!app.dirty.get(), "dispatcher must flush dirty zones");
        let item = app
            .zones
            .item(zone_id, item_id)
            .expect("renamed item still tracked");
        assert_eq!(item.path.as_ref(), renamed_path.as_str());
        assert_eq!(
            app.item_operation_status
                .borrow()
                .as_ref()
                .map(SmolStr::as_str),
            Some("Renamed file: report-final.txt")
        );
    }
    let reloaded = storage::read_zones(&zones_path).expect("read persisted zones");
    assert_eq!(
        reloaded
            .item(zone_id, item_id)
            .expect("persisted renamed item")
            .path
            .as_ref(),
        renamed_path.as_str()
    );

    let _ = std::fs::remove_dir_all(state_dir);
}
