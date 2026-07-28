#[test]
fn rule_execution_timeline_keeps_distinct_coalesce_keys() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-rules-timeline-distinct-{}",
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

    for (rule_id, path) in [
        (smol_str::SmolStr::new_static("rule-a"), &first_file),
        (smol_str::SmolStr::new_static("rule-b"), &second_file),
    ] {
        let before = capture_current_timeline_snapshot(&root, "before rule run");
        let info = file_info(path);
        let plan = ExecutionPlan {
            rule_id,
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
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.trigger.as_str() == "rule_file_change_applied")
            .count(),
        2
    );
    assert_eq!(root.app.borrow().timeline_panel.borrow().entries().len(), 4);

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_move_to_folder_moves_real_file_and_reports_errors() {
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-rules-move-folder-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let source_dir = scratch.join("source");
    let target_dir = scratch.join("target");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    let source = source_dir.join("move.log");
    std::fs::write(&source, b"log").expect("source file");

    let mut errors = Vec::new();
    let description = apply_rules_move_to_folder(
        target_dir.to_str().expect("target path"),
        &[file_info(&source)],
        &mut errors,
    );
    assert!(errors.is_empty(), "errors: {:?}", errors);
    assert!(description.contains("Moved 1 file"));
    assert!(!source.exists());
    assert!(target_dir.join("move.log").exists());
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_run_stats_increment_persisted_rule() {
    let dir = std::env::temp_dir().join(format!("bento-nano-rules-stats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    bento_nano_backend::rules::save_all(&dir, &[sample_rule("rule-stats")]).expect("seed rule");

    persist_rule_run_stats(&dir, "rule-stats").expect("persist stats");
    let loaded = bento_nano_backend::rules::load_all(&dir);
    assert_eq!(loaded[0].run_count, 1);
    assert!(loaded[0].last_run.is_some());
    assert_eq!(
        rules_zone_id_from_wire(&smol_str::SmolStr::new_static("7")),
        Some(ZoneId(7))
    );
    assert_eq!(
        rules_zone_id_from_wire(&smol_str::SmolStr::new_static("not-a-zone")),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn file_change_event(event_type: &'static str, path: &std::path::Path) -> FileChangedEvent {
    FileChangedEvent {
        event_type: smol_str::SmolStr::new_static(event_type),
        path: path.to_string_lossy().to_string(),
        old_path: None,
    }
}

#[test]
fn rules_on_file_change_event_runs_matching_rule_for_changed_file_only() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-rules-file-change-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let changed_file = scratch.join("watcher.log");
    let unrelated_file = scratch.join("unrelated.log");
    std::fs::write(&changed_file, b"changed").expect("changed file");
    std::fs::write(&unrelated_file, b"unrelated").expect("unrelated file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "File Change Target", 0, 0, 120, 80));
    }

    let mut rule = sample_rule("rule-file-change");
    rule.run_mode = RunMode::OnFileChange;
    rule.actions = vec![Action::MoveToZone(smol_str::SmolStr::new_static("21"))];
    bento_nano_backend::rules::save_all(&scratch, &[rule]).expect("seed rule");

    let outcome = run_on_file_change_rules(&root, &file_change_event("create", &changed_file))
        .expect("run file-change rules");
    assert_eq!(outcome.eligible_rules, 1);
    assert_eq!(outcome.triggered_rules, 1);
    assert_eq!(outcome.matched_files, 1);
    assert_eq!(outcome.action_count, 1);
    assert_eq!(outcome.error_count, 0);

    {
        let app = root.app.borrow();
        let zone = app.zones.get(ZoneId(21)).expect("target zone");
        assert_eq!(zone.items.len(), 1);
        assert_eq!(zone.items[0].name.as_ref(), "watcher.log");
        assert_eq!(zone.items[0].path.as_ref(), changed_file.to_str().unwrap());
        let status = app
            .rules_wizard_status
            .borrow()
            .as_ref()
            .expect("visible rules status")
            .to_string();
        assert!(status.contains("File change create ran 1/1 OnFileChange rules"));
        assert!(status.contains("matched 1"));
    }

    let loaded = bento_nano_backend::rules::load_all(&scratch);
    assert_eq!(loaded[0].run_count, 1);
    assert!(loaded[0].last_run.is_some());

    let timeline_dir =
        timeline_dir_for_zones_path(&scratch.join("zones.bin")).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger.as_str(), "rule_pre_apply");
    assert_eq!(entries[1].trigger.as_str(), "rule_file_change_applied");
    assert_eq!(entries[1].delta.items_added, 1);

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_on_file_change_event_does_not_run_on_demand_rules() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-rules-file-change-on-demand-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let changed_file = scratch.join("manual.log");
    std::fs::write(&changed_file, b"manual").expect("changed file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "Manual Only", 0, 0, 120, 80));
    }

    let mut rule = sample_rule("rule-on-demand");
    rule.actions = vec![Action::MoveToZone(smol_str::SmolStr::new_static("21"))];
    bento_nano_backend::rules::save_all(&scratch, &[rule]).expect("seed rule");

    let outcome = run_on_file_change_rules(&root, &file_change_event("create", &changed_file))
        .expect("run file-change rules");
    assert_eq!(outcome.eligible_rules, 0);
    assert_eq!(outcome.triggered_rules, 0);
    assert_eq!(outcome.matched_files, 0);

    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(21)).expect("target zone");
    assert!(zone.items.is_empty());
    assert!(app.rules_wizard_status.borrow().is_none());
    drop(app);

    let loaded = bento_nano_backend::rules::load_all(&scratch);
    assert_eq!(loaded[0].run_count, 0);
    assert!(loaded[0].last_run.is_none());

    let timeline_dir =
        timeline_dir_for_zones_path(&scratch.join("zones.bin")).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert!(entries.is_empty());

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_interval_scheduler_event_runs_due_interval_rule() {
    let root = test_app_root();
    let scratch =
        std::env::temp_dir().join(format!("bento-nano-rules-interval-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let due_file = scratch.join("due.log");
    std::fs::write(&due_file, b"due").expect("due file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "Interval Target", 0, 0, 120, 80));
    }

    let mut rule = sample_rule("rule-interval");
    rule.run_mode = RunMode::Interval { minutes: 60 };
    rule.actions = vec![Action::MoveToZone(smol_str::SmolStr::new_static("21"))];
    bento_nano_backend::rules::save_all(&scratch, &[rule]).expect("seed rule");

    let outcome = run_interval_rule_for_scheduler_event_with_desktop(
        &root,
        &SchedulerEvent::RuleDue {
            rule_id: smol_str::SmolStr::new_static("rule-interval"),
        },
        &scratch,
    )
    .expect("run interval rule");
    assert_eq!(outcome.eligible_rules, 1);
    assert_eq!(outcome.triggered_rules, 1);
    assert_eq!(outcome.matched_files, 1);
    assert_eq!(outcome.action_count, 1);
    assert_eq!(outcome.error_count, 0);

    {
        let app = root.app.borrow();
        let zone = app.zones.get(ZoneId(21)).expect("target zone");
        assert_eq!(zone.items.len(), 1);
        assert_eq!(zone.items[0].name.as_ref(), "due.log");
        assert_eq!(zone.items[0].path.as_ref(), due_file.to_str().unwrap());
        let status = app
            .rules_wizard_status
            .borrow()
            .as_ref()
            .expect("visible rules status")
            .to_string();
        assert!(status.contains("Interval scheduler ran 1/1 Interval rules"));
        assert!(status.contains("matched 1"));
        assert!(status.contains("rule=rule-interval"));
    }

    let loaded = bento_nano_backend::rules::load_all(&scratch);
    assert_eq!(loaded[0].run_count, 1);
    assert!(loaded[0].last_run.is_some());

    let timeline_dir =
        timeline_dir_for_zones_path(&scratch.join("zones.bin")).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger.as_str(), "rule_pre_apply");
    assert_eq!(entries[1].trigger.as_str(), "rule_interval_applied");
    assert_eq!(entries[1].delta.items_added, 1);

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn rules_interval_scheduler_event_ignores_non_interval_rule() {
    let root = test_app_root();
    let scratch = std::env::temp_dir().join(format!(
        "bento-nano-rules-interval-on-demand-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let due_file = scratch.join("manual.log");
    std::fs::write(&due_file, b"manual").expect("manual file");

    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.join("zones.bin");
        app.stealth_enabled.set(false);
        app.zones
            .add(Zone::new(ZoneId(21), "Manual Only", 0, 0, 120, 80));
    }

    let mut rule = sample_rule("rule-on-demand-scheduler");
    rule.actions = vec![Action::MoveToZone(smol_str::SmolStr::new_static("21"))];
    bento_nano_backend::rules::save_all(&scratch, &[rule]).expect("seed rule");

    let outcome = run_interval_rule_for_scheduler_event_with_desktop(
        &root,
        &SchedulerEvent::RuleDue {
            rule_id: smol_str::SmolStr::new_static("rule-on-demand-scheduler"),
        },
        &scratch,
    )
    .expect("ignore non-interval rule");
    assert_eq!(outcome.eligible_rules, 0);
    assert_eq!(outcome.triggered_rules, 0);
    assert_eq!(outcome.matched_files, 0);

    let app = root.app.borrow();
    let zone = app.zones.get(ZoneId(21)).expect("target zone");
    assert!(zone.items.is_empty());
    assert!(app.rules_wizard_status.borrow().is_none());
    drop(app);

    let loaded = bento_nano_backend::rules::load_all(&scratch);
    assert_eq!(loaded[0].run_count, 0);
    assert!(loaded[0].last_run.is_none());

    let timeline_dir =
        timeline_dir_for_zones_path(&scratch.join("zones.bin")).expect("timeline dir");
    let entries = bento_nano_backend::timeline::CheckpointStore::new(timeline_dir).load_all();
    assert!(entries.is_empty());

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn icon_picker_cycle_uses_builtin_registry_order() {
    assert_eq!(next_icon_slug("folder").as_str(), "document");
    assert_eq!(next_icon_slug("settings").as_str(), "folder");
    assert_eq!(next_icon_slug("unknown").as_str(), "folder");
}

#[test]
fn icon_picker_cycle_accepts_tauri_hyphenated_aliases() {
    assert_eq!(next_icon_slug("arrow-right").as_str(), "trash");
    assert_eq!(next_icon_slug("external-link").as_str(), "folder_open");
    assert_eq!(next_icon_slug("folder-open").as_str(), "camera");
}

#[test]
fn palette_picker_cycle_covers_swatch_table_and_clear_state() {
    assert_eq!(next_palette_accent(None).as_deref(), Some("#64748b"));
    assert_eq!(
        next_palette_accent(Some("#64748b")).as_deref(),
        Some("#3b82f6")
    );
    assert_eq!(next_palette_accent(Some("#06b6d4")).as_deref(), None);
    assert_eq!(
        next_palette_accent(Some("#not-real")).as_deref(),
        Some("#64748b")
    );
}

#[test]
fn icon_picker_hit_test_selects_visible_registry_slot() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let slot = icon_picker_slot_rect(viewport, 1);
    let hit = icon_picker_hit_test(
        viewport,
        slot.x + slot.width * 0.5,
        slot.y + slot.height * 0.5,
        ALL_ICON_KINDS.len(),
    );
    assert_eq!(hit, Some(IconPickerHit::Icon(1)));
    assert_eq!(
        hit.and_then(icon_picker_slug_for_hit).as_deref(),
        Some("document")
    );
}

#[test]
fn palette_picker_hit_test_maps_swatch_and_clear_payloads() {
    let viewport = Size {
        width: 320.0,
        height: 240.0,
    };
    let swatch = palette_picker_swatch_rect(viewport, 1);
    let swatch_hit = palette_picker_hit_test(
        viewport,
        swatch.x + swatch.width * 0.5,
        swatch.y + swatch.height * 0.5,
        palette_picker::swatch_table().len(),
    );
    assert_eq!(swatch_hit, Some(PalettePickerHit::Swatch(1)));
    assert_eq!(
        swatch_hit
            .and_then(palette_picker_accent_for_hit)
            .flatten()
            .as_deref(),
        Some("#3b82f6")
    );

    let clear = palette_picker_clear_rect(viewport);
    let clear_hit = palette_picker_hit_test(
        viewport,
        clear.x + clear.width * 0.5,
        clear.y + clear.height * 0.5,
        palette_picker::swatch_table().len(),
    );
    assert_eq!(clear_hit, Some(PalettePickerHit::Clear));
    assert_eq!(
        clear_hit.and_then(palette_picker_accent_for_hit),
        Some(None)
    );
}

#[test]
fn zone_editor_hit_test_maps_visible_pointer_controls() {
    let viewport = Size {
        width: 480.0,
        height: 460.0,
    };
    let close = zone_editor_close_rect(viewport);
    assert_eq!(
        zone_editor_hit_test(viewport, close.x + 6.0, close.y + 6.0),
        Some(ZoneEditorHit::Close)
    );
    let grid = zone_editor_grid_rect(viewport);
    assert_eq!(
        zone_editor_hit_test(viewport, grid.x + 6.0, grid.y + 6.0),
        Some(ZoneEditorHit::GridColumns(2))
    );
    let accent = zone_editor_accent_option_rect(viewport, 2).expect("accent option");
    assert_eq!(
        zone_editor_hit_test(
            viewport,
            accent.x + accent.width * 0.5,
            accent.y + accent.height * 0.5
        ),
        Some(ZoneEditorHit::AccentSwatch(1))
    );

    let save = zone_editor_save_rect(viewport);
    assert_eq!(
        zone_editor_hit_test(viewport, save.x + 6.0, save.y + 6.0),
        Some(ZoneEditorHit::Save)
    );

    let cancel = zone_editor_cancel_rect(viewport);
    assert_eq!(
        zone_editor_hit_test(viewport, cancel.x + 6.0, cancel.y + 6.0),
        Some(ZoneEditorHit::Cancel)
    );
}

#[test]
fn stack_tray_keyboard_selection_wraps_members() {
    let members = [ZoneId(1), ZoneId(2), ZoneId(3)];

    assert_eq!(
        next_stack_tray_member(ZoneId(1), &members, false),
        ZoneId(3)
    );
    assert_eq!(next_stack_tray_member(ZoneId(3), &members, true), ZoneId(1));
    assert_eq!(next_stack_tray_member(ZoneId(2), &members, true), ZoneId(3));
}

#[test]
fn drag_selection_release_never_reexpands_the_dragged_zone() {
    let app = AppState::new();
    app.zone_drag_body_visible_at_start
        .set(Some((ZoneId(2), false)));
    app.zone_drag_selected_before_start.set(Some(ZoneId(1)));

    assert_eq!(
        drag_selection_release(&app, true),
        DragSelectionRelease::Restore(Some(ZoneId(1)))
    );
    assert_eq!(
        drag_selection_release(&app, false),
        DragSelectionRelease::KeepCurrent
    );

    app.zone_drag_body_visible_at_start
        .set(Some((ZoneId(2), true)));
    app.zone_drag_selected_before_start.set(Some(ZoneId(2)));
    assert_eq!(
        drag_selection_release(&app, true),
        DragSelectionRelease::Restore(None)
    );
}

#[test]
fn ordinary_zone_click_expands_only_in_click_mode() {
    let mut app = AppState::new();
    let zone_id = ZoneId(21);
    app.zones
        .add(Zone::new(zone_id, "Mode gate", 0, 0, 160, 120));

    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    assert!(!zone_accepts_click_expand(&app, zone_id, false));
    app.set_zone_display_mode(ZoneDisplayMode::Always);
    assert!(!zone_accepts_click_expand(&app, zone_id, false));
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    assert!(zone_accepts_click_expand(&app, zone_id, false));
    assert!(!zone_accepts_click_expand(&app, zone_id, true));
}

#[test]
fn stack_tray_pointer_row_dispatches_preview_command() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
    }
    let row = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_tray_row_rect(app.viewport, anchor, 2, 1)
    };

    assert!(handle_stack_tray_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 4.0,
        row.y + 4.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::PreviewStackMember(ZoneId(1), ZoneId(2)))
    ));
}

#[test]
fn stack_bloom_hover_keeps_anchor_when_pointer_moves_to_petal() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child", 420, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(update_stack_bloom_hover(&app, Some(ZoneId(1)), 1_000));
        app.stack_bloom_progress.set(1.0);
    }
    let petal = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        stack_tray::stack_bloom_petal_rects(app.viewport, anchor, 2)[1]
    };

    let hover = {
        let app = root.app.borrow();
        stack_bloom_hover_anchor_for_point(&app, petal.x + 4.0, petal.y + 4.0)
    };

    assert_eq!(hover, Some(ZoneId(1)));
}
