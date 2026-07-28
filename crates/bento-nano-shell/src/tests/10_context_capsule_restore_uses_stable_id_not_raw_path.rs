#[test]
fn context_capsule_restore_uses_stable_id_not_raw_path() {
    let zones_path = scratch_zones_path("path-escape");
    let mut zones = ZoneList::new();
    zones.add(Zone::new(ZoneId(9), "Safe", 0, 0, 80, 60));
    let entry = capture_context_capsule_for_path(&zones_path, &zones, "Safe")
        .expect("capture context capsule");

    let err = restore_context_capsule_for_path(&zones_path, "../vault.bin")
        .expect_err("path-shaped id must not resolve");
    assert!(
        err.to_string()
            .contains("context capsule not found: ../vault.bin")
    );
    assert!(restore_context_capsule_for_path(&zones_path, entry.id.as_str()).is_ok());
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn bulk_manager_rows_reflect_live_zone_geometry() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 400.0,
        height: 200.0,
    };
    let mut zone = Zone::new(ZoneId(4), "Ops", 40, 20, 200, 100);
    zone.set_accent_color(Some(std::borrow::Cow::Borrowed("#3b82f6")));
    let _ = zone.add_item("C:/Users/BentoDeskTest/Desktop/a.txt", "hash");
    app.zones.add(zone);

    let rows = bulk_manager_rows_from_app(&app);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, ZoneId(4));
    assert_eq!(rows[0].display_name.as_str(), "Ops");
    assert_eq!(rows[0].item_count, 1);
    assert_eq!(rows[0].accent_hex.as_str(), "#3b82f6");
    assert!(rows[0].visible);
    assert!(!rows[0].locked);
    assert_eq!(rows[0].icon_slug.as_str(), DEFAULT_ZONE_ICON);
    assert_eq!(rows[0].capsule_size.as_str(), "medium");
    assert_eq!(rows[0].display_mode.as_str(), "inherit");
    assert_eq!(rows[0].width_percent, 50);
    assert_eq!(rows[0].height_percent, 50);
    assert_eq!(rows[0].position_x_percent, 10);
    assert_eq!(rows[0].position_y_percent, 10);
}

#[test]
fn bulk_manager_rows_include_hidden_zone_state() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 400.0,
        height: 200.0,
    };
    let mut zone = Zone::new(ZoneId(9), "Hidden", 40, 20, 200, 100);
    zone.set_visible(false);
    app.zones.add(zone);

    let rows = bulk_manager_rows_from_app(&app);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, ZoneId(9));
    assert!(!rows[0].visible);
}

#[test]
fn bulk_manager_rows_prefer_alias_and_show_bulk_metadata() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 400.0,
        height: 200.0,
    };
    let mut zone = Zone::new(ZoneId(12), "Canonical", 40, 20, 200, 100);
    zone.set_alias(Some(Cow::Borrowed("Display alias")));
    zone.set_locked(true);
    zone.set_icon(Cow::Borrowed("star"));
    zone.set_capsule_size(Cow::Borrowed("large"));
    zone.set_display_mode(Some(Cow::Borrowed("click")));
    app.zones.add(zone);

    let rows = bulk_manager_rows_from_app(&app);
    assert_eq!(rows[0].display_name.as_str(), "Display alias");
    assert!(rows[0].locked);
    assert_eq!(rows[0].icon_slug.as_str(), "star");
    assert_eq!(rows[0].capsule_size.as_str(), "large");
    assert_eq!(rows[0].display_mode.as_str(), "click");
}

#[test]
fn bulk_zone_visibility_mutates_only_matched_zones() {
    let mut app = AppState::new();
    app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
    app.zones.add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));

    let (changed, matched) = apply_bulk_zone_visibility(&mut app, &[ZoneId(1), ZoneId(99)], false);
    assert_eq!((changed, matched), (1, 1));
    assert!(!app.zones.get(ZoneId(1)).expect("zone 1").visible);
    assert!(app.zones.get(ZoneId(2)).expect("zone 2").visible);

    let (changed_again, matched_again) =
        apply_bulk_zone_visibility(&mut app, &[ZoneId(1), ZoneId(2)], false);
    assert_eq!((changed_again, matched_again), (1, 2));
    assert!(!app.zones.get(ZoneId(1)).expect("zone 1").visible);
    assert!(!app.zones.get(ZoneId(2)).expect("zone 2").visible);
}

#[test]
fn bulk_update_payload_mutates_all_supported_fields() {
    let mut app = AppState::new();
    app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
    let mut two = Zone::new(ZoneId(2), "Two", 40, 40, 100, 100);
    two.set_alias(Some(Cow::Borrowed("Will Clear")));
    two.set_display_mode(Some(Cow::Borrowed("click")));
    app.zones.add(two);

    let (changed, matched) = apply_bulk_zone_updates(
        &mut app,
        &[
            BulkZoneUpdate {
                id: ZoneId(1),
                position: Some(DispatchPoint::new(33, 44)),
                size: Some(DispatchSize::new(18, 22)),
                accent_color: Some(SmolStr::new_static("#abcdef")),
                capsule_size: Some(SmolStr::new_static("large")),
                locked: Some(true),
                alias: Some(SmolStr::new_static("  Trimmed  ")),
                display_mode: Some(Some(SmolStr::new_static(" hover "))),
                icon: Some(SmolStr::new_static("  star  ")),
            },
            BulkZoneUpdate {
                id: ZoneId(2),
                alias: Some(SmolStr::new_static("   ")),
                display_mode: Some(None),
                icon: Some(SmolStr::new_static("   ")),
                ..BulkZoneUpdate::default()
            },
            BulkZoneUpdate {
                id: ZoneId(99),
                locked: Some(true),
                ..BulkZoneUpdate::default()
            },
        ],
    );

    assert_eq!((changed, matched), (2, 2));
    let one = app.zones.get(ZoneId(1)).expect("zone 1");
    assert_eq!((one.x, one.y), (33, 44));
    assert_eq!(
        (one.w, one.h),
        (80, 60),
        "size clamps to selected-stack minimums"
    );
    assert_eq!(one.accent_color.as_deref(), Some("#abcdef"));
    assert_eq!(one.capsule_size.as_ref(), "large");
    assert!(one.locked);
    assert_eq!(one.alias.as_deref(), Some("Trimmed"));
    assert_eq!(one.display_mode.as_deref(), Some("hover"));
    assert_eq!(one.icon.as_ref(), "star");
    let two = app.zones.get(ZoneId(2)).expect("zone 2");
    assert!(two.alias.is_none());
    assert!(two.display_mode.is_none());
    assert_eq!(
        two.icon.as_ref(),
        DEFAULT_ZONE_ICON,
        "whitespace icon is a no-op"
    );
}

#[test]
fn bulk_update_payload_reports_noop_and_missing_ids() {
    let mut app = AppState::new();
    let mut zone = Zone::new(ZoneId(1), "One", 10, 20, 120, 80);
    zone.set_alias(Some(Cow::Borrowed("Alias")));
    zone.set_display_mode(Some(Cow::Borrowed("hover")));
    app.zones.add(zone);

    let (changed, matched) = apply_bulk_zone_updates(
        &mut app,
        &[
            BulkZoneUpdate {
                id: ZoneId(1),
                position: Some(DispatchPoint::new(10, 20)),
                size: Some(DispatchSize::new(120, 80)),
                alias: Some(SmolStr::new_static("Alias")),
                display_mode: Some(Some(SmolStr::new_static("hover"))),
                ..BulkZoneUpdate::default()
            },
            BulkZoneUpdate {
                id: ZoneId(77),
                position: Some(DispatchPoint::new(1, 2)),
                ..BulkZoneUpdate::default()
            },
        ],
    );

    assert_eq!((changed, matched), (0, 1));
}

#[test]
fn bulk_update_command_records_coalesced_timeline_pair() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("bulk-update-timeline");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 10, 20, 120, 80));
    }

    for alias in ["First alias", "Second alias"] {
        root.dispatcher
            .push(Command::BulkUpdateZones(vec![BulkZoneUpdate {
                id: ZoneId(1),
                alias: Some(SmolStr::new_static(alias)),
                ..BulkZoneUpdate::default()
            }]));
        consume_dispatcher(&root, std::ptr::null_mut());
    }

    let timeline_dir = timeline_dir_for_zones_path(&zones_path).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger.as_str(), "bulk_pre_apply");
    assert_eq!(entries[1].trigger.as_str(), "bulk_update_zones");
    assert_eq!(entries[1].delta.zones_updated, 1);
    assert_eq!(
        entries[1].coalesce_key.as_deref(),
        Some("mutation:post:bulk_update_zones:1")
    );
    let zone = entries[1]
        .snapshot
        .zones
        .iter()
        .find(|zone| zone.id.as_str() == "1")
        .expect("zone 1");
    assert_eq!(zone.alias.as_deref(), Some("Second alias"));
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 2);
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn bulk_metadata_producer_builds_reachable_update_payloads() {
    let mut app = AppState::new();
    let mut one = Zone::new(ZoneId(1), "One", 0, 0, 100, 100);
    one.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
    let mut two = Zone::new(ZoneId(2), "Two", 0, 0, 100, 100);
    two.set_alias(Some(Cow::Borrowed("Alias")));
    two.set_display_mode(Some(Cow::Borrowed("click")));
    app.zones.add(one);
    app.zones.add(two);
    let rows = bulk_manager_rows_from_app(&app);
    app.bulk_manager.borrow_mut().set_zones(rows);
    app.bulk_manager.borrow_mut().toggle_selection(ZoneId(2));

    let updates = bulk_metadata_updates_for_target_ids(&app);
    assert_eq!(updates.len(), 1);
    let update = &updates[0];
    assert_eq!(update.id, ZoneId(2));
    assert_eq!(update.alias.as_deref(), Some(""));
    assert_eq!(update.display_mode, Some(None));
    assert_eq!(update.locked, Some(true));
    assert!(update.accent_color.is_some());
    assert!(update.capsule_size.is_some());
    assert!(update.icon.is_some());
}

#[test]
fn bulk_manager_pointer_row_click_toggles_selection() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        app.zones.add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    let row = bento_nano_app::business::bulk_manager_panel::bulk_manager_row_rect(
        root.app.borrow().viewport,
        1,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 2.0,
        row.y + 2.0
    ));
    let app = root.app.borrow();
    assert_eq!(app.bulk_manager.borrow().cursor_index(), 1);
    assert_eq!(app.bulk_manager.borrow().selected(), &[ZoneId(2)]);
}

#[test]
fn bulk_manager_pointer_row_click_uses_scrolled_visible_window() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        for index in 0..10 {
            app.zones.add(Zone::new(
                ZoneId(index + 1),
                format!("Zone {index:02}"),
                0,
                0,
                100,
                100,
            ));
        }
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    for _ in 0..8 {
        let _ = handle_bulk_manager_keydown(&root, VK_DOWN_KEY, std::ptr::null_mut());
    }
    assert_eq!(root.app.borrow().bulk_manager.borrow().cursor_index(), 8);
    let row = bento_nano_app::business::bulk_manager_panel::bulk_manager_row_rect(
        root.app.borrow().viewport,
        7,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 2.0,
        row.y + 2.0
    ));
    let app = root.app.borrow();
    assert_eq!(app.bulk_manager.borrow().cursor_index(), 8);
    assert_eq!(app.bulk_manager.borrow().selected(), &[ZoneId(9)]);
}

#[test]
fn bulk_manager_search_input_filters_rows_without_triggering_shortcuts() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        app.zones
            .add(Zone::new(ZoneId(2), "Projects", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
    }
    let search = bento_nano_app::business::bulk_manager_panel::bulk_manager_search_rect(
        root.app.borrow().viewport,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        search.x + 2.0,
        search.y + 2.0
    ));
    assert!(root.app.borrow().bulk_manager.borrow().search_focused());
    let _ = handle_bulk_manager_keydown(&root, VK_P_KEY, std::ptr::null_mut());
    assert!(handle_bulk_manager_char(&root, u32::from('p')));

    let app = root.app.borrow();
    assert_eq!(app.bulk_manager.borrow().search(), "p");
    assert_eq!(app.bulk_manager.borrow().visible_count(), 1);
    assert!(app.bulk_manager.borrow().selected().is_empty());
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
}

#[test]
fn bulk_manager_search_backspace_and_escape_are_local_to_filter() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        app.zones
            .add(Zone::new(ZoneId(2), "Projects", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().focus_search();
        app.bulk_manager.borrow_mut().set_search("pr");
    }

    let _ = handle_bulk_manager_keydown(&root, VK_BACKSPACE, std::ptr::null_mut());
    assert_eq!(root.app.borrow().bulk_manager.borrow().search(), "p");
    let _ = handle_bulk_manager_keydown(&root, VK_ESCAPE_KEY, std::ptr::null_mut());
    assert!(!root.app.borrow().bulk_manager.borrow().search_focused());
    assert_eq!(root.app.borrow().bulk_manager.borrow().search(), "p");
}

#[test]
fn bulk_manager_sort_header_click_toggles_sort_key_and_row_order() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "Large", 0, 0, 300, 200));
        app.zones.add(Zone::new(ZoneId(2), "Small", 0, 0, 50, 50));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().focus_search();
    }
    {
        let app = root.app.borrow();
        let manager = app.bulk_manager.borrow();
        let ids: Vec<ZoneId> = manager.visible_rows().iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![ZoneId(1), ZoneId(2)]);
    }
    let size_header = {
        let app = root.app.borrow();
        bento_nano_app::business::bulk_manager_panel::bulk_manager_sort_header_rect(
            app.viewport,
            SortKey::Size,
        )
    };

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        size_header.x + (size_header.width / 2.0),
        size_header.y + (size_header.height / 2.0)
    ));
    {
        let app = root.app.borrow();
        let manager = app.bulk_manager.borrow();
        assert_eq!(manager.sort_key(), SortKey::Size);
        assert_eq!(
            manager.sort_direction(),
            bento_nano_app::business::bulk_manager_panel::SortDirection::Ascending
        );
        assert!(!manager.search_focused());
        let ids: Vec<ZoneId> = manager.visible_rows().iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![ZoneId(2), ZoneId(1)]);
        assert_eq!(
            app.bulk_manager_status
                .borrow()
                .as_ref()
                .map(|status| status.as_str()),
            Some("Sorted Bulk rows by Size (ascending)")
        );
    }

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        size_header.x + (size_header.width / 2.0),
        size_header.y + (size_header.height / 2.0)
    ));
    {
        let app = root.app.borrow();
        let manager = app.bulk_manager.borrow();
        assert_eq!(
            manager.sort_direction(),
            bento_nano_app::business::bulk_manager_panel::SortDirection::Descending
        );
        let ids: Vec<ZoneId> = manager.visible_rows().iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![ZoneId(1), ZoneId(2)]);
        assert_eq!(
            app.bulk_manager_status
                .borrow()
                .as_ref()
                .map(|status| status.as_str()),
            Some("Sorted Bulk rows by Size (descending)")
        );
    }

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
}

#[test]
fn bulk_manager_pointer_update_button_dispatches_bulk_update() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
    }
    let update = bento_nano_app::business::bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| {
            spec.hit == bento_nano_app::business::bulk_manager_panel::BulkManagerPointerHit::Update
        })
        .copied()
        .expect("update button");
    let rect = bento_nano_app::business::bulk_manager_panel::bulk_manager_button_rect(
        root.app.borrow().viewport,
        update,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 2.0,
        rect.y + 2.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::BulkUpdateZones(updates)) if updates.len() == 1 && updates[0].id == ZoneId(1)
    ));
}

#[test]
fn bulk_manager_icon_button_opens_non_zone_icon_picker() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 720.0,
            height: 540.0,
        };
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
    }
    let icon = bento_nano_app::business::bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| {
            spec.hit
                == bento_nano_app::business::bulk_manager_panel::BulkManagerPointerHit::IconPicker
        })
        .copied()
        .expect("icon picker button");
    let rect = bento_nano_app::business::bulk_manager_panel::bulk_manager_button_rect(
        root.app.borrow().viewport,
        icon,
    );

    assert!(handle_bulk_manager_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 2.0,
        rect.y + 2.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::OpenIconPicker { zone_id: None })
    ));
}

#[test]
fn bulk_icon_picker_commit_dispatches_bulk_update_for_selected_zones() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        app.zones.add(Zone::new(ZoneId(2), "Two", 0, 0, 100, 100));
        let rows = bulk_manager_rows_from_app(&app);
        app.bulk_manager.borrow_mut().set_zones(rows);
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(1));
        app.bulk_manager.borrow_mut().toggle_selection(ZoneId(2));
        app.icon_picker.borrow_mut().replace(IconPickerSession {
            zone_id: None,
            selected_icon: SmolStr::new_static("star"),
        });
    }

    save_icon_picker(&root);

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::BulkUpdateZones(updates))
            if updates.len() == 2
                && updates.iter().all(|update| update.icon.as_deref() == Some("star"))
    ));
}

#[test]
fn icon_picker_enter_dispatches_normalized_set_zone_icon_for_zone_target() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones.add(Zone::new(ZoneId(7), "Docs", 0, 0, 100, 100));
        app.icon_picker.borrow_mut().replace(IconPickerSession {
            zone_id: Some(ZoneId(7)),
            selected_icon: SmolStr::new_static("external-link"),
        });
    }

    let result = handle_icon_picker_keydown(&root, VK_ENTER, std::ptr::null_mut());

    assert_eq!(result, 0);
    assert!(root.app.borrow().icon_picker.borrow().is_none());
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::SetZoneIcon(ZoneId(7), icon)) if icon.as_str() == "external_link"
    ));
}
