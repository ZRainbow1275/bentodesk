#[test]
fn rules_wizard_pointer_close_button_clears_pending_action_without_command() {
    let root = test_app_root();
    root.app.borrow_mut().viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    let close = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::Close);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        close.x + 1.0,
        close.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    assert!(
        root.app
            .borrow()
            .rules_wizard
            .borrow_mut()
            .take_action()
            .is_none()
    );
}

#[test]
fn rules_state_dir_uses_zones_parent_directory() {
    let dir = std::env::temp_dir().join("bentodesk-rules-parent-test");
    let zones_path = dir.join("zones.bin");
    assert_eq!(
        rules_state_dir_for_zones_path(&zones_path).expect("rules state dir"),
        dir
    );
}

#[test]
fn rules_save_stamps_empty_id_and_persists_json() {
    let dir = std::env::temp_dir().join(format!("bentodesk-rules-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");

    let saved = save_rule_for_state_dir(&dir, sample_rule("")).expect("save rule");
    assert!(saved.id.as_str().starts_with("rule-"));
    let loaded = bentodesk_backend::rules::load_all(&dir);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, saved.id);
    assert_eq!(loaded[0].name, "Move logs");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rules_preview_zones_preserve_live_item_assignments() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 800.0,
        height: 600.0,
    };
    let mut zone = Zone::new(ZoneId(12), "Archive", 80, 60, 240, 180);
    zone.live_folder_path = Some(std::borrow::Cow::Borrowed("C:/Users/BentoDeskTest/Documents/Live"));
    zone.items.push(ZoneItem {
        id: ZoneItemId(3),
        name: std::borrow::Cow::Borrowed("a.log"),
        path: std::borrow::Cow::Borrowed("C:/Users/BentoDeskTest/Desktop/a.log"),
        icon_hash: std::borrow::Cow::Borrowed("hash"),
        x: 2,
        y: 1,
        is_wide: true,
        file_missing: false,
        original_path: Some(std::borrow::Cow::Borrowed("C:/Users/BentoDeskTest/Desktop/a.log")),
        hidden_path: Some(std::borrow::Cow::Borrowed("C:/Data/.bentodesk/12/a.log")),
        tags: smallvec::SmallVec::new(),
    });
    app.zones.add(zone);

    let zones = rules_preview_zones_from_app(&app);
    assert_eq!(zones.len(), 1);
    assert_eq!(zones[0].id.as_str(), "12");
    assert_eq!(zones[0].items.len(), 1);
    assert_eq!(zones[0].items[0].zone_id.as_str(), "12");
    assert_eq!(zones[0].items[0].grid_position.col_span, 2);
    assert_eq!(
        zones[0].items[0].original_path.as_deref(),
        Some("C:/Users/BentoDeskTest/Desktop/a.log")
    );
    assert_eq!(
        zones[0].live_folder_path.as_deref(),
        Some("C:/Users/BentoDeskTest/Documents/Live")
    );
}

#[test]
fn zone_context_bind_live_folder_choice_maps_to_real_picker_action() {
    assert!(matches!(
        zone_context_action_for_choice(ZONE_CONTEXT_BIND_LIVE_FOLDER_ID, &[]),
        Some(ZoneContextAction::OpenLiveFolderPicker)
    ));
}

#[test]
fn live_folder_rehydrate_binds_persisted_paths_once_after_startup_load() {
    let root = test_app_root();
    root.live_folder_rehydrated.set(false);
    {
        let mut app = root.app.borrow_mut();
        let mut live_zone = Zone::new(ZoneId(32), "Persisted Live", 0, 0, 240, 160);
        live_zone.live_folder_path = Some(Cow::Borrowed("C:/Users/BentoDeskTest/Desktop/Live"));
        app.zones.add(live_zone);
        app.zones
            .add(Zone::new(ZoneId(33), "Regular", 20, 20, 240, 160));
    }

    let mut bound = Vec::<(ZoneId, String)>::new();
    let mut refreshed = Vec::<ZoneId>::new();
    let changed = rehydrate_live_folder_bindings_with(
        &root,
        |zone_id, folder| {
            bound.push((zone_id, folder.display().to_string()));
            Ok(())
        },
        |_root, zone_id| {
            refreshed.push(zone_id);
            Ok(true)
        },
    );

    assert!(changed);
    assert_eq!(
        bound,
        vec![(ZoneId(32), "C:/Users/BentoDeskTest/Desktop/Live".to_string())]
    );
    assert_eq!(refreshed, vec![ZoneId(32)]);
    assert!(root.live_folder_rehydrated.get());

    let second = rehydrate_live_folder_bindings_with(
        &root,
        |_zone_id, _folder| panic!("rehydrate must be one-shot"),
        |_root, _zone_id| panic!("rehydrate must be one-shot"),
    );
    assert!(!second);
}

#[test]
fn live_folder_rehydrate_reports_binding_failures_visibly() {
    let root = test_app_root();
    root.live_folder_rehydrated.set(false);
    {
        let mut app = root.app.borrow_mut();
        let mut live_zone = Zone::new(ZoneId(35), "Broken Live", 0, 0, 240, 160);
        live_zone.live_folder_path = Some(Cow::Borrowed("C:/missing/live"));
        app.zones.add(live_zone);
    }

    let changed = rehydrate_live_folder_bindings_with(
        &root,
        |_zone_id, _folder| Err("watch failed".to_string()),
        |_root, _zone_id| Ok(false),
    );

    assert!(changed);
    assert!(
        root.app
            .borrow()
            .rules_wizard_status
            .borrow()
            .as_ref()
            .expect("visible status")
            .contains("Live folder rehydrate failed for zone 35")
    );
}

#[test]
fn live_folder_refresh_rebuilds_zone_items_and_preserves_tags() {
    let root = test_app_root();
    let scratch =
        std::env::temp_dir().join(format!("bentodesk-live-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let a = scratch.join("a.log");
    let b = scratch.join("b.txt");
    let hidden = scratch.join(".ignored");
    std::fs::write(&a, b"a").expect("a");
    std::fs::write(&b, b"b").expect("b");
    std::fs::write(&hidden, b"hidden").expect("hidden");

    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(31), "Live", 0, 0, 240, 160);
        zone.live_folder_path = Some(Cow::Owned(scratch.to_string_lossy().to_string()));
        zone.items.push(ZoneItem {
            id: ZoneItemId(1),
            name: Cow::Borrowed("a.log"),
            path: Cow::Owned(a.to_string_lossy().to_string()),
            icon_hash: Cow::Borrowed("hash"),
            x: 0,
            y: 0,
            is_wide: false,
            file_missing: false,
            original_path: None,
            hidden_path: None,
            tags: smallvec::smallvec![Cow::Borrowed("urgent")],
        });
        app.zones.add(zone);
    }

    assert!(refresh_live_folder_zone(&root, ZoneId(31)).expect("refresh"));
    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(31)).expect("zone");
    assert_eq!(zone.items.len(), 2);
    assert_eq!(zone.items[0].name.as_ref(), "a.log");
    assert_eq!(zone.items[1].name.as_ref(), "b.txt");
    assert_eq!(zone.items[0].tags[0].as_ref(), "urgent");
    assert!(app.dirty.get());
    assert!(
        app.rules_wizard_status
            .borrow()
            .as_ref()
            .expect("status")
            .contains("Live folder refreshed zone 31")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn live_folder_refresh_no_change_reports_visible_main_status_without_dirtying() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-live-folder-no-change-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let alpha = scratch.join("alpha.txt");
    std::fs::write(&alpha, b"alpha").expect("alpha");

    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(36), "Live", 0, 0, 240, 160);
        zone.live_folder_path = Some(Cow::Owned(scratch.to_string_lossy().to_string()));
        app.zones.add(zone);
    }

    assert!(refresh_live_folder_zone(&root, ZoneId(36)).expect("initial refresh"));
    root.app.borrow().dirty.set(false);
    assert!(refresh_live_folder_zone(&root, ZoneId(36)).expect("unchanged refresh"));

    let app = root.app.borrow();
    assert!(!app.dirty.get());
    assert!(
        app.rules_wizard_status
            .borrow()
            .as_ref()
            .expect("rules status")
            .contains("no changes")
    );
    assert!(
        app.item_operation_status
            .borrow()
            .as_ref()
            .expect("main status")
            .contains("no changes")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn live_folder_items_participate_in_rules_in_zone_conditions() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-live-folder-rules-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let live_file = scratch.join("case.log");
    std::fs::write(&live_file, b"log").expect("live file");

    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(33), "Live Rules", 0, 0, 240, 160);
        zone.live_folder_path = Some(Cow::Owned(scratch.to_string_lossy().to_string()));
        app.zones.add(zone);
    }
    refresh_live_folder_zone(&root, ZoneId(33)).expect("refresh");

    let mut rule = sample_rule("rule-live-in-zone");
    rule.conditions = ConditionGroup::All(vec![ConditionNode::Leaf(Condition::InZone(
        SmolStr::new_static("33"),
    ))]);
    rule.actions = vec![Action::Tag(vec![SmolStr::new_static("live")])];
    let preview_zones = {
        let app = root.app.borrow();
        rules_preview_zones_from_app(&app)
    };
    let plan = bentodesk_backend::rules::executor::build_plan(
        &rule,
        scratch.to_str().expect("desktop"),
        &preview_zones,
        None,
    )
    .expect("plan");
    assert_eq!(plan.matched.len(), 1);
    assert_eq!(plan.matched[0].path, live_file.to_string_lossy());

    let report = apply_rules_execution_plan(&root, &plan);
    assert_eq!(report.matched, 1);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    refresh_live_folder_zone(&root, ZoneId(33)).expect("refresh with preserved tags");
    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(33)).expect("zone");
    assert_eq!(zone.items[0].tags[0].as_ref(), "live");
    drop(app);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_move_to_zone_rejects_live_folder_target_without_mutating() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-live-folder-move-reject-{}",
        std::process::id()
    ));
    let source_dir = scratch.join("source");
    let live_dir = scratch.join("live");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&source_dir).expect("source");
    std::fs::create_dir_all(&live_dir).expect("live");
    let source = source_dir.join("move.log");
    std::fs::write(&source, b"log").expect("source file");

    {
        let mut app = root.app.borrow_mut();
        let mut zone = Zone::new(ZoneId(34), "Read-only Live", 0, 0, 240, 160);
        zone.live_folder_path = Some(Cow::Owned(live_dir.to_string_lossy().to_string()));
        app.zones.add(zone);
    }

    let plan = ExecutionPlan {
        rule_id: SmolStr::new_static("rule-live-move"),
        matched: vec![file_info(&source)],
        effects: vec![ActionEffect::MoveToZone {
            zone_id: SmolStr::new_static("34"),
            files: vec![file_info(&source)],
        }],
    };
    let report = apply_rules_execution_plan(&root, &plan);
    assert_eq!(report.matched, 1);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("read-only live folder mirror"));
    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(34)).expect("zone");
    assert!(zone.items.is_empty());
    assert!(source.exists());
    drop(app);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_item_type_detects_common_windows_shell_types() {
    assert_eq!(
        rules_item_type_for_path("C:/Users/BentoDeskTest/Desktop/a.lnk"),
        ItemType::Shortcut
    );
    assert_eq!(
        rules_item_type_for_path("C:/Tools/app.exe"),
        ItemType::Application
    );
    assert_eq!(
        rules_item_type_for_path("C:/Users/BentoDeskTest/Desktop/a.txt"),
        ItemType::File
    );
    let stamped = stamp_rule_id_if_empty(sample_rule(""));
    assert!(!stamped.id.as_str().is_empty());
}

#[test]
fn rules_run_plan_move_to_zone_mutates_live_zone_state() {
    let root = test_app_root();
    let scratch =
        std::env::temp_dir().join(format!("bentodesk-rules-run-zone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let source = scratch.join("a.log");
    std::fs::write(&source, b"log").expect("source file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(7), "Archive", 0, 0, 120, 80));
    }

    let info = file_info(&source);
    let plan = ExecutionPlan {
        rule_id: smol_str::SmolStr::new_static("rule-1"),
        matched: vec![info.clone()],
        effects: vec![ActionEffect::MoveToZone {
            zone_id: smol_str::SmolStr::new_static("7"),
            files: vec![info],
        }],
    };
    let report = apply_rules_execution_plan(&root, &plan);
    assert_eq!(report.matched, 1);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    assert_eq!(report.actions_taken.len(), 1);

    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(7)).expect("target zone");
    assert_eq!(zone.items.len(), 1);
    assert_eq!(zone.items[0].name.as_ref(), "a.log");
    assert_eq!(zone.items[0].path.as_ref(), source.to_str().unwrap());
    assert_eq!(zone.items[0].original_path.as_deref(), None);
    assert!(app.dirty.get());
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_tag_action_updates_live_and_persisted_item_metadata() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!("bentodesk-rules-tag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let source = scratch.join("tagged.log");
    std::fs::write(&source, b"log").expect("source file");
    let zones_path = scratch.join("zones.bin");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones.add(Zone::new(ZoneId(7), "Tagged", 0, 0, 120, 80));
        app.zones
            .add_item(
                ZoneId(7),
                std::borrow::Cow::Owned(source.to_string_lossy().to_string()),
                std::borrow::Cow::Borrowed("hash"),
            )
            .expect("item id");
    }

    let mut rule = sample_rule("rule-tag");
    rule.actions = vec![Action::Tag(vec![
        smol_str::SmolStr::new_static("urgent"),
        smol_str::SmolStr::new_static("work"),
        smol_str::SmolStr::new_static("urgent"),
    ])];
    let preview_zones = {
        let app = root.app.borrow();
        rules_preview_zones_from_app(&app)
    };
    let plan = bentodesk_backend::rules::executor::build_plan(
        &rule,
        source.parent().expect("parent").to_str().expect("desktop"),
        &preview_zones,
        None,
    )
    .expect("tag plan");
    let report = apply_rules_execution_plan(&root, &plan);
    assert_eq!(report.matched, 1);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    assert_eq!(report.actions_taken.len(), 1);
    assert!(report.actions_taken[0].contains("Tagged 1 item"));

    {
        let app = root.app.borrow();
        let zone = app.zones.get(ZoneId(7)).expect("target zone");
        let tags: Vec<&str> = zone.items[0].tags.iter().map(|tag| tag.as_ref()).collect();
        assert_eq!(tags, vec!["urgent", "work"]);
        assert!(app.dirty.get());
        storage::write_zones_atomic(&zones_path, &app.zones).expect("persist zones");
    }

    let reloaded = storage::read_zones(&zones_path).expect("reload zones");
    let reloaded_tags: Vec<&str> = reloaded.get(ZoneId(7)).expect("zone").items[0]
        .tags
        .iter()
        .map(|tag| tag.as_ref())
        .collect();
    assert_eq!(reloaded_tags, vec!["urgent", "work"]);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_tag_action_reports_missing_zone_item() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-rules-tag-missing-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let source = scratch.join("untagged.log");
    std::fs::write(&source, b"log").expect("source file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.zones.add(Zone::new(ZoneId(7), "Tagged", 0, 0, 120, 80));
    }

    let info = file_info(&source);
    let plan = ExecutionPlan {
        rule_id: smol_str::SmolStr::new_static("rule-tag-missing"),
        matched: vec![info.clone()],
        effects: vec![ActionEffect::Tag {
            tags: vec![smol_str::SmolStr::new_static("urgent")],
            files: vec![info],
        }],
    };
    let report = apply_rules_execution_plan(&root, &plan);
    assert_eq!(report.matched, 1);
    assert_eq!(report.actions_taken.len(), 1);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("no zone item found"));
    assert!(!root.app.borrow().dirty.get());
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_run_records_timeline_pre_and_post_checkpoints() {
    let root = test_app_root();
    let scratch =
        std::env::temp_dir().join(format!("bentodesk-rules-timeline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let source = scratch.join("timeline.log");
    std::fs::write(&source, b"log").expect("source file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "Rules Target", 0, 0, 120, 80));
    }

    let before = capture_current_timeline_snapshot(&root, "before rule run");
    let info = file_info(&source);
    let plan = ExecutionPlan {
        rule_id: smol_str::SmolStr::new_static("rule-timeline"),
        matched: vec![info.clone()],
        effects: vec![ActionEffect::MoveToZone {
            zone_id: smol_str::SmolStr::new_static("21"),
            files: vec![info],
        }],
    };
    let report = apply_rules_execution_plan(&root, &plan);
    record_rule_execution_timeline_pair(&root, before, &report);

    let timeline_dir = {
        let app = root.app.borrow();
        timeline_dir_for_zones_path(&app.zones_path).expect("timeline dir")
    };
    let entries = bentodesk_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger.as_str(), "rule_pre_apply");
    assert_eq!(entries[0].snapshot.zones[0].items.len(), 0);
    assert_eq!(entries[1].trigger.as_str(), "rule_applied");
    assert_eq!(entries[1].snapshot.zones[0].items.len(), 1);
    assert_eq!(entries[1].delta.items_added, 1);
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 2);

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rule_execution_timeline_coalesces_same_trigger_and_key() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bentodesk-rules-timeline-coalesce-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let first_file = scratch.join("first.log");
    let second_file = scratch.join("second.log");
    std::fs::write(&first_file, b"first").expect("first file");
    std::fs::write(&second_file, b"second").expect("second file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "Rules Target", 0, 0, 120, 80));
    }

    for path in [&first_file, &second_file] {
        let before = capture_current_timeline_snapshot(&root, "before rule run");
        let info = file_info(path);
        let plan = ExecutionPlan {
            rule_id: smol_str::SmolStr::new_static("rule-coalesce"),
            matched: vec![info.clone()],
            effects: vec![ActionEffect::MoveToZone {
                zone_id: smol_str::SmolStr::new_static("21"),
                files: vec![info],
            }],
        };
        let mut report = apply_rules_execution_plan(&root, &plan);
        report.checkpoint_trigger = SmolStr::new_static("rule_file_change_applied");
        record_rule_execution_timeline_pair(&root, before, &report);
    }

    let timeline_dir =
        timeline_dir_for_zones_path(&scratch.join("zones.bin")).expect("timeline dir");
    let entries = bentodesk_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger.as_str(), "rule_pre_apply");
    assert_eq!(entries[0].snapshot.zones[0].items.len(), 0);
    assert_eq!(entries[1].trigger.as_str(), "rule_file_change_applied");
    assert_eq!(entries[1].snapshot.zones[0].items.len(), 2);
    assert_eq!(entries[1].delta.items_added, 2);
    assert_eq!(
        entries[1].coalesce_key.as_deref(),
        Some("rules:post:rule_file_change_applied:rule-coalesce")
    );
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 2);

    let _ = std::fs::remove_dir_all(&scratch);
}
