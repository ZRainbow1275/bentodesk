#[test]
fn bulk_layout_algorithm_matches_tauri_row_column_shape() {
    let row = compute_bulk_layout_positions(BulkLayoutAlgorithm::Row, 5);
    for pair in row.windows(2) {
        assert!((pair[1].1 - pair[0].1).abs() < 0.01);
        assert!(pair[1].0 > pair[0].0);
    }

    let column = compute_bulk_layout_positions(BulkLayoutAlgorithm::Column, 5);
    for pair in column.windows(2) {
        assert!((pair[1].0 - pair[0].0).abs() < 0.01);
        assert!(pair[1].1 > pair[0].1);
    }
}

fn bulk_layout_capsule_rect(app: &AppState, id: ZoneId) -> (i32, i32, i32, i32) {
    let zone = app.zones.get(id).expect("layout zone");
    bento_nano_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, zone)
}

fn assert_bulk_layout_capsules_inside_viewport(app: &AppState, ids: &[ZoneId]) {
    let width = app.viewport.width.floor() as i32;
    let height = app.viewport.height.floor() as i32;
    for id in ids {
        let (x, y, w, h) = bulk_layout_capsule_rect(app, *id);
        assert!(x >= 0, "zone {} x outside viewport: {x}", id.0);
        assert!(y >= 0, "zone {} y outside viewport: {y}", id.0);
        assert!(
            x + w <= width,
            "zone {} right outside viewport: {x}+{w}>{width}",
            id.0
        );
        assert!(
            y + h <= height,
            "zone {} bottom outside viewport: {y}+{h}>{height}",
            id.0
        );
    }
}

#[test]
fn bulk_layout_algorithm_organic_stays_inside_viewport() {
    let points = compute_bulk_layout_positions(BulkLayoutAlgorithm::Organic, 10);
    assert_eq!(points.len(), 10);
    for (x, y) in points {
        assert!((5.0..=95.0).contains(&x), "x out of range: {x}");
        assert!((5.0..=95.0).contains(&y), "y out of range: {y}");
    }
}

#[test]
fn bulk_layout_all_algorithms_fit_mixed_capsules_and_stack_anchor_on_screen() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 640.0,
        height: 360.0,
    };
    let mut ids = Vec::new();
    for (id, size, shape) in [
        (1, "small", "pill"),
        (2, "medium", "rounded"),
        (3, "large", "minimal"),
        (4, "small", "circle"),
        (5, "medium", "circle"),
        (6, "large", "circle"),
        (7, "large", "pill"),
    ] {
        let zone_id = ZoneId(id);
        let mut zone = Zone::new(zone_id, format!("Zone {id}"), 0, 0, 420, 260);
        zone.set_capsule(size, shape);
        app.zones.add(zone);
        ids.push(zone_id);
    }
    let anchor_id = ZoneId(8);
    let child_id = ZoneId(9);
    let mut anchor = Zone::new(anchor_id, "Stack", 0, 0, 420, 260);
    anchor.stack_members.push(child_id);
    let mut child = Zone::new(child_id, "Stack child", 0, 0, 420, 260);
    child.stack_parent = Some(anchor_id);
    app.zones.add(anchor);
    app.zones.add(child);
    ids.push(anchor_id);

    for algorithm in [
        BulkLayoutAlgorithm::Grid,
        BulkLayoutAlgorithm::Row,
        BulkLayoutAlgorithm::Column,
        BulkLayoutAlgorithm::Spiral,
        BulkLayoutAlgorithm::Organic,
    ] {
        let (_, matched) = apply_bulk_layout_algorithm(&mut app, &ids, algorithm);
        assert_eq!(matched, ids.len(), "matched count for {algorithm:?}");
        assert_bulk_layout_capsules_inside_viewport(&app, &ids);
    }
}

#[test]
fn bulk_layout_row_uses_equal_edge_gaps_and_a_shared_top() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1000.0,
        height: 500.0,
    };
    let ids = [ZoneId(1), ZoneId(2), ZoneId(3)];
    for (id, size) in ids.into_iter().zip(["small", "medium", "large"]) {
        let mut zone = Zone::new(id, "Row", 0, 0, 400, 260);
        zone.set_capsule_size(size);
        app.zones.add(zone);
    }

    apply_bulk_layout_algorithm(&mut app, &ids, BulkLayoutAlgorithm::Row);
    let rects = ids.map(|id| bulk_layout_capsule_rect(&app, id));
    assert_eq!(rects[0].1, rects[1].1);
    assert_eq!(rects[1].1, rects[2].1);
    let first_gap = rects[1].0 - (rects[0].0 + rects[0].2);
    let second_gap = rects[2].0 - (rects[1].0 + rects[1].2);
    assert!(
        (first_gap - second_gap).abs() <= 1,
        "row edge gaps differ: {first_gap} vs {second_gap}"
    );
    assert_bulk_layout_capsules_inside_viewport(&app, &ids);
}

#[test]
fn bulk_layout_column_uses_equal_edge_gaps_and_a_shared_left() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 600.0,
        height: 500.0,
    };
    let ids = [ZoneId(1), ZoneId(2), ZoneId(3)];
    for (id, size) in ids.into_iter().zip(["small", "medium", "large"]) {
        let mut zone = Zone::new(id, "Column", 0, 0, 400, 260);
        zone.set_capsule(size, "circle");
        app.zones.add(zone);
    }

    apply_bulk_layout_algorithm(&mut app, &ids, BulkLayoutAlgorithm::Column);
    let rects = ids.map(|id| bulk_layout_capsule_rect(&app, id));
    assert_eq!(rects[0].0, rects[1].0);
    assert_eq!(rects[1].0, rects[2].0);
    let first_gap = rects[1].1 - (rects[0].1 + rects[0].3);
    let second_gap = rects[2].1 - (rects[1].1 + rects[1].3);
    assert!(
        (first_gap - second_gap).abs() <= 1,
        "column edge gaps differ: {first_gap} vs {second_gap}"
    );
    assert_bulk_layout_capsules_inside_viewport(&app, &ids);
}

#[test]
fn bulk_layout_grid_center_aligns_mixed_capsules_in_rows_and_columns() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 800.0,
        height: 500.0,
    };
    let ids = [ZoneId(1), ZoneId(2), ZoneId(3), ZoneId(4)];
    for (id, size, shape) in [
        (ids[0], "small", "pill"),
        (ids[1], "large", "circle"),
        (ids[2], "large", "pill"),
        (ids[3], "small", "circle"),
    ] {
        let mut zone = Zone::new(id, "Grid", 0, 0, 400, 260);
        zone.set_capsule(size, shape);
        app.zones.add(zone);
    }

    apply_bulk_layout_algorithm(&mut app, &ids, BulkLayoutAlgorithm::Grid);
    let rects = ids.map(|id| bulk_layout_capsule_rect(&app, id));
    let center_x = |rect: (i32, i32, i32, i32)| rect.0 * 2 + rect.2;
    let center_y = |rect: (i32, i32, i32, i32)| rect.1 * 2 + rect.3;
    assert!((center_x(rects[0]) - center_x(rects[2])).abs() <= 1);
    assert!((center_x(rects[1]) - center_x(rects[3])).abs() <= 1);
    assert!((center_y(rects[0]) - center_y(rects[1])).abs() <= 1);
    assert!((center_y(rects[2]) - center_y(rects[3])).abs() <= 1);
    assert_bulk_layout_capsules_inside_viewport(&app, &ids);
}

#[test]
fn bulk_layout_overflow_row_overlaps_instead_of_leaving_the_viewport() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 400.0,
        height: 220.0,
    };
    let ids = [ZoneId(1), ZoneId(2), ZoneId(3), ZoneId(4)];
    for id in ids {
        let mut zone = Zone::new(id, "Wide", 0, 0, 400, 260);
        zone.set_capsule_size("large");
        app.zones.add(zone);
    }

    apply_bulk_layout_algorithm(&mut app, &ids, BulkLayoutAlgorithm::Row);
    let xs = ids.map(|id| app.zones.get(id).expect("zone").x);
    assert!(xs.windows(2).all(|pair| pair[1] > pair[0]));
    assert_bulk_layout_capsules_inside_viewport(&app, &ids);
}

#[test]
fn bulk_layout_target_ids_use_selection_or_listed_rows() {
    let app = AppState::new();
    {
        let mut rows_app = AppState::new();
        rows_app.viewport = Size {
            width: 400.0,
            height: 200.0,
        };
        rows_app
            .zones
            .add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        rows_app
            .zones
            .add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&rows_app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }

    assert_eq!(bulk_layout_target_ids(&app), vec![ZoneId(1), ZoneId(2)]);

    app.bulk_manager.borrow_mut().toggle_selection(ZoneId(2));
    assert_eq!(bulk_layout_target_ids(&app), vec![ZoneId(2)]);
}

#[test]
fn timeline_manual_checkpoint_persists_to_real_store() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("timeline-save");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones
            .add(Zone::new(ZoneId(7), "Archive", 80, 60, 240, 180));
    }

    let checkpoint = save_timeline_checkpoint(
        &root,
        None,
        Some(smol_str::SmolStr::new_static("manual save")),
    )
    .expect("save checkpoint");
    let timeline_dir = timeline_dir_for_zones_path(&zones_path).expect("timeline dir");
    let loaded = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir)
        .load(checkpoint.id.as_str())
        .expect("checkpoint load");
    assert!(loaded.pinned);
    assert_eq!(loaded.snapshot.zones.len(), 1);
    assert_eq!(loaded.snapshot.zones[0].id.as_str(), "7");
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 1);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn timeline_manual_checkpoints_do_not_coalesce() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("timeline-manual-no-coalesce");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones
            .add(Zone::new(ZoneId(7), "Archive", 80, 60, 240, 180));
    }

    let first = save_timeline_checkpoint(
        &root,
        None,
        Some(smol_str::SmolStr::new_static("manual save")),
    )
    .expect("first checkpoint");
    let second = save_timeline_checkpoint(
        &root,
        None,
        Some(smol_str::SmolStr::new_static("manual save")),
    )
    .expect("second checkpoint");

    assert_ne!(first.id, second.id);
    let timeline_dir = timeline_dir_for_zones_path(&zones_path).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.pinned));
    assert!(entries.iter().all(|entry| entry.coalesce_key.is_none()));
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 2);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn timeline_pointer_row_click_selects_and_loads_checkpoint() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("timeline-pointer-row");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        app.zones
            .add(Zone::new(ZoneId(7), "Archive", 80, 60, 240, 180));
    }
    let _ = save_timeline_checkpoint(&root, None, Some(smol_str::SmolStr::new_static("first")))
        .expect("first checkpoint");
    let _ = save_timeline_checkpoint(&root, None, Some(smol_str::SmolStr::new_static("second")))
        .expect("second checkpoint");
    let row =
        bento_nano_app::business::timeline::panel::timeline_row_rect(root.app.borrow().viewport, 1);

    assert!(handle_timeline_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 1.0,
        row.y + 1.0
    ));
    let (cursor, selected, active) = {
        let app = root.app.borrow();
        let panel = app.timeline_panel.borrow();
        (
            panel.cursor_index(),
            panel.selected_id(),
            panel.active().map(|checkpoint| checkpoint.id.clone()),
        )
    };
    assert_eq!(cursor, 1);
    assert_eq!(active, selected);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn timeline_pointer_save_button_dispatches_manual_checkpoint() {
    let root = test_app_root();
    root.app.borrow_mut().viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    let save = bento_nano_app::business::timeline::panel::TIMELINE_ACTION_BUTTONS
        .iter()
        .find(|spec| {
            spec.hit == bento_nano_app::business::timeline::panel::TimelinePointerHit::Save
        })
        .copied()
        .expect("save button");
    let rect = bento_nano_app::business::timeline::panel::timeline_button_rect(
        root.app.borrow().viewport,
        save,
    );

    assert!(handle_timeline_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::SaveCheckpoint { id: None, label: Some(label) })
            if matches!(label.as_str(), "manual save" | "手动保存")
    ));
}

#[test]
fn timeline_pointer_restore_button_dispatches_selected_checkpoint() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("timeline-pointer-restore");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        app.zones
            .add(Zone::new(ZoneId(9), "Original", 80, 60, 240, 180));
    }
    let _ = save_timeline_checkpoint(&root, None, Some(smol_str::SmolStr::new_static("baseline")))
        .expect("save checkpoint");
    let selected = root
        .app
        .borrow()
        .timeline_panel
        .borrow()
        .selected_id()
        .expect("selected checkpoint");
    let restore = bento_nano_app::business::timeline::panel::TIMELINE_ACTION_BUTTONS
        .iter()
        .find(|spec| {
            spec.hit == bento_nano_app::business::timeline::panel::TimelinePointerHit::Restore
        })
        .copied()
        .expect("restore button");
    let rect = bento_nano_app::business::timeline::panel::timeline_button_rect(
        root.app.borrow().viewport,
        restore,
    );

    assert!(handle_timeline_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    {
        let app = root.app.borrow();
        let panel = app.timeline_panel.borrow();
        let expected_status =
            format!("Press Restore again to replace the current layout with checkpoint {selected}");
        assert_eq!(
            panel.restore_confirmation().map(|status| status.as_str()),
            Some(selected.as_str())
        );
        assert_eq!(
            panel.status().map(|status| status.as_str()),
            Some(expected_status.as_str())
        );
    }

    assert!(handle_timeline_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::RestoreCheckpoint(checkpoint_id)) if checkpoint_id == &selected
    ));
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn timeline_restore_checkpoint_replaces_zones_and_bumps_allocator() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("timeline-restore");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones
            .add(Zone::new(ZoneId(9), "Original", 80, 60, 240, 180));
    }
    let checkpoint =
        save_timeline_checkpoint(&root, None, Some(smol_str::SmolStr::new_static("baseline")))
            .expect("save checkpoint");
    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.zones
            .add(Zone::new(ZoneId(2), "Mutated", 10, 10, 120, 80));
        app.next_zone_id.set(3);
        app.dirty.set(false);
    }

    let restored =
        restore_timeline_checkpoint(&root, checkpoint.id.as_str()).expect("restore checkpoint");
    assert_eq!(restored, checkpoint.id);
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(9)).is_some());
    assert!(app.zones.get(ZoneId(2)).is_none());
    assert!(app.dirty.get());
    assert!(app.next_zone_id.get() >= 10);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn snapshot_picker_save_load_delete_round_trip() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("snapshot-round-trip");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones
            .add(Zone::new(ZoneId(11), "Snapshot Source", 80, 60, 240, 180));
    }

    let snapshot = save_layout_snapshot(
        &root,
        Some(smol_str::SmolStr::new_static("Manual snapshot")),
    )
    .expect("save snapshot");
    let snapshot_dir = snapshot_dir_for_zones_path(&zones_path).expect("snapshot dir");
    let manager = bento_nano_backend::layout::SnapshotManager::new(snapshot_dir.clone());
    let loaded = manager.load(snapshot.id.as_str()).expect("load snapshot");
    assert_eq!(loaded.name, "Manual snapshot");
    assert_eq!(loaded.zones.len(), 1);
    assert_eq!(
        root.app.borrow().snapshot_picker.borrow().entries().len(),
        1
    );

    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.zones
            .add(Zone::new(ZoneId(2), "Mutated", 10, 10, 120, 80));
        app.next_zone_id.set(3);
        app.dirty.set(false);
    }

    let restored = super::load_layout_snapshot(&root, snapshot.id.as_str())
        .expect("load selected-stack snapshot");
    assert_eq!(restored.id, snapshot.id);
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(11)).is_some());
    assert!(app.zones.get(ZoneId(2)).is_none());
    assert!(app.dirty.get());
    assert!(app.next_zone_id.get() >= 12);
    drop(app);

    super::delete_layout_snapshot(&root, snapshot.id.as_str()).expect("delete snapshot");
    assert!(manager.list().expect("list snapshots").is_empty());
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn snapshot_picker_pointer_row_click_selects_snapshot() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("snapshot-pointer-row");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        app.zones
            .add(Zone::new(ZoneId(21), "Snapshot Source", 80, 60, 240, 180));
    }
    let _ = save_layout_snapshot(&root, Some(smol_str::SmolStr::new_static("First")))
        .expect("first snapshot");
    let _ = save_layout_snapshot(&root, Some(smol_str::SmolStr::new_static("Second")))
        .expect("second snapshot");
    let row = bento_nano_app::business::timeline::snapshot_picker::snapshot_picker_row_rect(
        root.app.borrow().viewport,
        1,
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 1.0,
        row.y + 1.0
    ));
    assert_eq!(root.app.borrow().snapshot_picker.borrow().cursor_index(), 1);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn snapshot_picker_pointer_save_button_dispatches_snapshot_command() {
    let root = test_app_root();
    root.app.borrow_mut().viewport = Size {
        width: 640.0,
        height: 520.0,
    };
    let save = bento_nano_app::business::timeline::snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS
            .iter()
            .find(|spec| {
                spec.hit
                    == bento_nano_app::business::timeline::snapshot_picker::SnapshotPickerPointerHit::Save
            })
            .copied()
            .expect("save button");
    let rect = bento_nano_app::business::timeline::snapshot_picker::snapshot_picker_button_rect(
        root.app.borrow().viewport,
        save,
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::SaveSnapshot { name: Some(name) }) if name.starts_with("Manual snapshot")
    ));
}

#[test]
fn snapshot_picker_pointer_load_button_dispatches_selected_snapshot() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("snapshot-pointer-load");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        app.zones
            .add(Zone::new(ZoneId(31), "Snapshot Source", 80, 60, 240, 180));
    }
    let snapshot = save_layout_snapshot(&root, Some(smol_str::SmolStr::new_static("Load me")))
        .expect("save snapshot");
    let load = bento_nano_app::business::timeline::snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS
            .iter()
            .find(|spec| {
                spec.hit
                    == bento_nano_app::business::timeline::snapshot_picker::SnapshotPickerPointerHit::Load
            })
            .copied()
            .expect("load button");
    let rect = bento_nano_app::business::timeline::snapshot_picker::snapshot_picker_button_rect(
        root.app.borrow().viewport,
        load,
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::LoadSnapshot(snapshot_id)) if snapshot_id == &snapshot.id
    ));
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn snapshot_picker_pointer_delete_button_uses_two_step_confirmation() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("snapshot-pointer-delete");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        app.zones
            .add(Zone::new(ZoneId(41), "Snapshot Source", 80, 60, 240, 180));
    }
    let snapshot = save_layout_snapshot(&root, Some(smol_str::SmolStr::new_static("Delete me")))
        .expect("save snapshot");
    let delete = bento_nano_app::business::timeline::snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS
            .iter()
            .find(|spec| {
                spec.hit
                    == bento_nano_app::business::timeline::snapshot_picker::SnapshotPickerPointerHit::Delete
            })
            .copied()
            .expect("delete button");
    let rect = bento_nano_app::business::timeline::snapshot_picker::snapshot_picker_button_rect(
        root.app.borrow().viewport,
        delete,
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    assert!(
        root.app
            .borrow()
            .snapshot_picker
            .borrow()
            .row_action()
            .is_awaiting_for(snapshot.id.as_str())
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::DeleteSnapshot(snapshot_id)) if snapshot_id == &snapshot.id
    ));
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn snapshot_picker_pointer_timeline_button_dispatches_open_timeline() {
    let root = test_app_root();
    root.app.borrow_mut().viewport = Size {
        width: 640.0,
        height: 520.0,
    };
    let timeline =
            bento_nano_app::business::timeline::snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS
                .iter()
                .find(|spec| {
                    spec.hit
                        == bento_nano_app::business::timeline::snapshot_picker::SnapshotPickerPointerHit::Timeline
                })
                .copied()
                .expect("timeline button");
    let rect = bento_nano_app::business::timeline::snapshot_picker::snapshot_picker_button_rect(
        root.app.borrow().viewport,
        timeline,
    );

    assert!(handle_snapshot_picker_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(drained.first(), Some(Command::OpenTimeline)));
}

#[test]
fn runtime_hotkey_override_replaces_default_binding() {
    let root = test_app_root();
    let ctrl_t = super::hotkey::ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    let ctrl_shift_t = super::hotkey::ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    };
    assert_eq!(
        super::hotkey::lookup_in(&root.hotkey_bindings.borrow(), 0x54, ctrl_t),
        Some(super::hotkey::HotkeyCommand::OpenTimeline)
    );

    assert!(apply_hotkey_binding(
        &root,
        super::hotkey::ACTION_OPEN_TIMELINE,
        "Ctrl+Shift+T"
    ));

    assert_eq!(
        super::hotkey::lookup_in(&root.hotkey_bindings.borrow(), 0x54, ctrl_t),
        None
    );
    assert_eq!(
        super::hotkey::lookup_in(&root.hotkey_bindings.borrow(), 0x54, ctrl_shift_t),
        Some(super::hotkey::HotkeyCommand::OpenTimeline)
    );
}
