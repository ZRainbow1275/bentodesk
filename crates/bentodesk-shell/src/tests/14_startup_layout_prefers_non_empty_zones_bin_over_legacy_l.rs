#[test]
fn startup_layout_prefers_non_empty_zones_bin_over_legacy_layout() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-prefers-zones-bin");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let legacy_layout = legacy_layout_with_zone("152", "Legacy Should Not Win", "legacy.txt");
    legacy_layout
        .save(&state_dir.join("layout.json"))
        .expect("write legacy layout");
    let mut selected_zones = ZoneList::new();
    selected_zones.add(Zone::new(
        ZoneId(151),
        "Selected Should Win",
        40,
        50,
        260,
        180,
    ));
    storage::write_zones_atomic(&zones_path, &selected_zones).expect("persist zones");

    let outcome = load_startup_zones_or_migrate_legacy(&root, &zones_path)
        .expect("startup load")
        .expect("selected zones loaded");

    assert_eq!(outcome.source, StartupLayoutLoadSource::SelectedZonesBin);
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(151)).is_some());
    assert!(app.zones.get(ZoneId(152)).is_none());
    drop(app);
    let persisted = storage::read_zones(&zones_path).expect("read zones");
    assert!(persisted.get(ZoneId(151)).is_some());
    assert!(persisted.get(ZoneId(152)).is_none());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn startup_layout_imports_legacy_when_zones_bin_is_empty() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-empty-zones-bin-legacy");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    storage::write_zones_atomic(&zones_path, &ZoneList::new()).expect("persist empty zones");
    let legacy_layout = legacy_layout_with_zone("161", "Legacy Empty Bin Import", "legacy.txt");
    legacy_layout
        .save(&state_dir.join("layout.json"))
        .expect("write legacy layout");

    let outcome = load_startup_zones_or_migrate_legacy(&root, &zones_path)
        .expect("startup migration")
        .expect("legacy layout migrated");

    assert_eq!(outcome.source, StartupLayoutLoadSource::LegacyLayoutJson);
    let persisted = storage::read_zones(&zones_path).expect("read migrated zones");
    assert!(persisted.get(ZoneId(161)).is_some());
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(161)).is_some());
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn startup_layout_imports_legacy_layout_backup_when_primary_missing() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-legacy-layout-backup");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let legacy_path = state_dir.join("layout.json");
    let backup_path = state_dir.join("layout.json.bak");
    let legacy_layout = legacy_layout_with_zone("171", "Legacy Backup Import", "backup.txt");
    legacy_layout
        .save(&backup_path)
        .expect("write legacy backup");
    assert!(!legacy_path.exists());

    let outcome = load_startup_zones_or_migrate_legacy(&root, &zones_path)
        .expect("startup migration from backup")
        .expect("legacy backup migrated");

    assert_eq!(outcome.source, StartupLayoutLoadSource::LegacyLayoutJson);
    let persisted = storage::read_zones(&zones_path).expect("read migrated zones");
    let zone = persisted.get(ZoneId(171)).expect("migrated backup zone");
    assert_eq!(zone.title.as_ref(), "Legacy Backup Import");
    assert_eq!(
        zone.items
            .first()
            .and_then(|item| item.hidden_path.as_deref())
            .filter(|path| path.ends_with(r"\.bentodesk\171\backup.txt")),
        Some(r"C:\Users\Alice\AppData\Roaming\BentoDesk\.bentodesk\171\backup.txt")
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_startup_heal_restores_missing_zones_file() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-startup-missing");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(121), "Startup Missing", 40, 50, 260, 180));
    }
    capture_recovery_bundle(&root).expect("capture recovery bundle");
    assert!(!zones_path.exists());
    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.dirty.set(false);
    }

    let outcome = startup_heal_recovery_bundle(&root, &zones_path)
        .expect("startup heal")
        .expect("bundle restored");
    assert_eq!(outcome.summary.zone_count, 1);
    assert!(matches!(
        outcome.icon_restore,
        RecoveryIconRestoreOutcome::NotIncluded
    ));
    let persisted = storage::read_zones(&zones_path).expect("read healed zones");
    assert!(persisted.get(ZoneId(121)).is_some());
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(121)).is_some());
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_startup_heal_quarantines_corrupt_zones_file() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-startup-corrupt");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(122), "Startup Corrupt", 40, 50, 260, 180));
    }
    capture_recovery_bundle(&root).expect("capture recovery bundle");
    std::fs::write(&zones_path, b"not a zones.bin").expect("write corrupt zones");
    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.dirty.set(false);
    }

    let outcome = startup_heal_recovery_bundle(&root, &zones_path)
        .expect("startup heal")
        .expect("bundle restored");
    assert_eq!(outcome.summary.zone_count, 1);
    assert!(matches!(
        outcome.icon_restore,
        RecoveryIconRestoreOutcome::NotIncluded
    ));
    let persisted = storage::read_zones(&zones_path).expect("read healed zones");
    assert!(persisted.get(ZoneId(122)).is_some());
    let quarantined = std::fs::read_dir(state_dir)
        .expect("read scratch")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("zones.bin.corrupt-")
        });
    assert!(quarantined, "corrupt zones.bin should be quarantined");
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(122)).is_some());
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_tamper_rejects_before_mutating_live_zones() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-tamper");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(41), "Trusted", 40, 50, 260, 180));
    }
    capture_recovery_bundle(&root).expect("capture recovery bundle");

    let data_root = bentodesk_backend::recovery_bundle::data_root_for_state_file(&zones_path)
        .expect("data root");
    let mut bundle = bentodesk_backend::recovery_bundle::load_bundle(&data_root)
        .expect("load bundle")
        .expect("bundle exists");
    bundle.zones_bin_b64 = bentodesk_backend::config_vault::wire::base64_encode(b"tampered");
    bentodesk_backend::recovery_bundle::write_bundle(&data_root, &bundle)
        .expect("write tampered bundle");

    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.zones
            .add(Zone::new(ZoneId(2), "Current Live", 10, 10, 120, 80));
        app.dirty.set(false);
    }

    let error = restore_recovery_bundle(&root).expect_err("tampered bundle must fail");
    assert!(error.to_string().contains("recovery bundle"));
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(2)).is_some());
    assert!(app.zones.get(ZoneId(41)).is_none());
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

fn sample_rule(id: &str) -> bentodesk_backend::rules::Rule {
    bentodesk_backend::rules::Rule {
        id: smol_str::SmolStr::new(id),
        name: "Move logs".to_string(),
        enabled: true,
        conditions: ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(vec![
            smol_str::SmolStr::new_static("log"),
        ]))]),
        actions: vec![Action::MoveToZone(smol_str::SmolStr::new_static("archive"))],
        run_mode: RunMode::OnDemand,
        last_run: None,
        run_count: 0,
    }
}

fn rules_wizard_button_rect_for(
    root: &AppRoot,
    hit: rules_wizard::RulesWizardPointerHit,
) -> bentodesk_style::Rect {
    let spec = rules_wizard::RULES_WIZARD_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == hit)
        .copied()
        .expect("rules wizard button");
    rules_wizard::rules_wizard_button_rect(root.app.borrow().viewport, spec)
}

#[test]
fn rules_wizard_pointer_row_click_selects_persisted_rule() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut first = sample_rule("rule-one");
        first.name = "First rule".to_string();
        let mut second = sample_rule("rule-two");
        second.name = "Second rule".to_string();
        *app.rules_wizard_rules.borrow_mut() = vec![first, second];
    }
    let rect = rules_wizard::rules_wizard_rule_row_rect(root.app.borrow().viewport, 1);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let app = root.app.borrow();
    assert_eq!(app.rules_wizard_rule_cursor.get(), 1);
    assert_eq!(
        app.rules_wizard_status.borrow().as_deref(),
        Some("Selected rule 2")
    );
}

#[test]
fn rules_wizard_delete_requires_second_matching_delete() {
    let root = test_app_root();
    {
        let app = root.app.borrow();
        *app.rules_wizard_rules.borrow_mut() = vec![sample_rule("rule-one")];
        app.rules_wizard_rule_cursor.set(0);
    }

    let _ = handle_rules_wizard_keydown(&root, VK_D_KEY, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert_eq!(
            app.rules_wizard_delete_confirm.borrow().as_deref(),
            Some("rule-one")
        );
        assert_eq!(
            app.rules_wizard_status.borrow().as_deref(),
            Some("Press Delete again to permanently remove rule rule-one")
        );
    }
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);

    let _ = handle_rules_wizard_keydown(&root, VK_D_KEY, std::ptr::null_mut());
    assert!(
        root.app
            .borrow()
            .rules_wizard_delete_confirm
            .borrow()
            .is_none()
    );
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.pop(),
        Some(Command::DeleteRule(rule_id)) if rule_id.as_str() == "rule-one"
    ));
}

#[test]
fn rules_wizard_pointer_row_click_uses_scrolled_rule_window() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut rules = Vec::new();
        for index in 0..8 {
            let mut rule = sample_rule(&format!("rule-{index:02}"));
            rule.name = format!("Rule {index:02}");
            rules.push(rule);
        }
        *app.rules_wizard_rules.borrow_mut() = rules;
    }
    for _ in 0..6 {
        let _ = handle_rules_wizard_keydown(&root, VK_DOWN_KEY, std::ptr::null_mut());
    }
    assert_eq!(root.app.borrow().rules_wizard_rule_cursor.get(), 6);
    let rect = rules_wizard::rules_wizard_rule_row_rect(root.app.borrow().viewport, 0);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    let app = root.app.borrow();
    assert_eq!(app.rules_wizard_rule_cursor.get(), 1);
    assert_eq!(
        app.rules_wizard_status.borrow().as_deref(),
        Some("Selected rule 2")
    );
}

#[test]
fn rules_wizard_condition_buttons_edit_multiple_condition_rows() {
    let root = test_app_root();
    root.app.borrow_mut().viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    let add =
        rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::AddCondition);
    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        add.x + 1.0,
        add.y + 1.0
    ));
    assert_eq!(
        root.app.borrow().rules_wizard.borrow().condition_cursor(),
        1
    );
    assert!(handle_rules_wizard_char(&root, 'x' as u32));

    let next =
        rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::NextCondition);
    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        next.x + 1.0,
        next.y + 1.0
    ));
    assert_eq!(
        root.app.borrow().rules_wizard.borrow().condition_cursor(),
        0
    );
    assert!(handle_rules_wizard_char(&root, 'a' as u32));

    let combine = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::Combine);
    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        combine.x + 1.0,
        combine.y + 1.0
    ));
    assert_eq!(
        root.app.borrow().rules_wizard.borrow().combine(),
        rules_wizard::CombineMode::Any
    );

    let remove =
        rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::RemoveCondition);
    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        remove.x + 1.0,
        remove.y + 1.0
    ));
    let app = root.app.borrow();
    let wizard = app.rules_wizard.borrow();
    assert_eq!(wizard.conditions().len(), 1);
    assert_eq!(wizard.condition_cursor(), 0);
    assert_eq!(wizard.conditions()[0].value, "x");
    assert_eq!(
        app.rules_wizard_status.borrow().as_deref(),
        Some("Removed condition 1")
    );
}

#[test]
fn rules_wizard_condition_row_click_selects_target_condition_for_typing() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut wizard = app.rules_wizard.borrow_mut();
        wizard.add_condition();
        wizard.add_condition();
        wizard.set_condition_cursor(0);
    }
    let rect = rules_wizard::rules_wizard_condition_row_rect(root.app.borrow().viewport, 1);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        rect.x + 1.0,
        rect.y + 1.0
    ));
    assert_eq!(
        root.app.borrow().rules_wizard.borrow().condition_cursor(),
        1
    );
    assert_eq!(
        root.app.borrow().rules_wizard_status.borrow().as_deref(),
        Some("Editing condition 2 of 3")
    );

    assert!(handle_rules_wizard_char(&root, 'b' as u32));
    let app = root.app.borrow();
    let wizard = app.rules_wizard.borrow();
    assert_eq!(wizard.conditions()[0].value, "");
    assert_eq!(wizard.conditions()[1].value, "b");
    assert_eq!(wizard.conditions()[2].value, "");
}

#[test]
fn rules_wizard_pointer_edit_button_loads_selected_rule() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut first = sample_rule("rule-one");
        first.name = "First rule".to_string();
        let mut second = sample_rule("rule-two");
        second.name = "Second pointer rule".to_string();
        *app.rules_wizard_rules.borrow_mut() = vec![first, second];
    }
    let row = rules_wizard::rules_wizard_rule_row_rect(root.app.borrow().viewport, 1);
    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        row.x + 1.0,
        row.y + 1.0
    ));
    let edit = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::Edit);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        edit.x + 1.0,
        edit.y + 1.0
    ));
    let app = root.app.borrow();
    assert_eq!(app.rules_wizard.borrow().name(), "Second pointer rule");
    assert_eq!(
        app.rules_wizard_status.borrow().as_deref(),
        Some("Editing rule 'Second pointer rule'")
    );
}

#[test]
fn rules_wizard_pointer_next_save_button_dispatches_preview_request() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut wizard = app.rules_wizard.borrow_mut();
        wizard.set_condition_kind(0, rules_wizard::PredicateKind::ExtensionIn);
        wizard.set_condition_value(0, "log");
        wizard.set_action_kind(rules_wizard::ActionKind::Notify);
        wizard.set_action_value("notice");
        wizard.set_name("Pointer preview rule");
        wizard.click_next();
    }
    let next_save =
        rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::NextSave);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        next_save.x + 1.0,
        next_save.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::PreviewRuleHits(rule)) if rule.name == "Pointer preview rule"
    ));
}

#[test]
fn rules_wizard_pointer_next_save_button_dispatches_save_rule_on_review() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        let mut wizard = app.rules_wizard.borrow_mut();
        wizard.set_condition_kind(0, rules_wizard::PredicateKind::ExtensionIn);
        wizard.set_condition_value(0, "log");
        wizard.set_action_kind(rules_wizard::ActionKind::Notify);
        wizard.set_action_value("notice");
        wizard.set_name("Pointer saved rule");
        wizard.click_next();
        wizard.click_next();
        let _ = wizard.take_action();
        wizard.click_next();
        wizard.click_next();
    }
    let next_save =
        rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::NextSave);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        next_save.x + 1.0,
        next_save.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::SaveRule(rule)) if rule.name == "Pointer saved rule"
    ));
}

#[test]
fn rules_wizard_pointer_run_button_dispatches_selected_rule() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        *app.rules_wizard_rules.borrow_mut() = vec![sample_rule("rule-run")];
    }
    let run = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::Run);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        run.x + 1.0,
        run.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::RunRuleNow(rule_id)) if rule_id.as_str() == "rule-run"
    ));
}

#[test]
fn rules_wizard_pointer_delete_button_dispatches_selected_rule() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 820.0,
            height: 620.0,
        };
        *app.rules_wizard_rules.borrow_mut() = vec![sample_rule("rule-delete")];
    }
    let delete = rules_wizard_button_rect_for(&root, rules_wizard::RulesWizardPointerHit::Delete);

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        delete.x + 1.0,
        delete.y + 1.0
    ));
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    {
        let app = root.app.borrow();
        assert_eq!(
            app.rules_wizard_delete_confirm.borrow().as_deref(),
            Some("rule-delete")
        );
        assert_eq!(
            app.rules_wizard_status.borrow().as_deref(),
            Some("Press Delete again to permanently remove rule rule-delete")
        );
    }

    assert!(handle_rules_wizard_lbutton_up(
        &root,
        std::ptr::null_mut(),
        delete.x + 1.0,
        delete.y + 1.0
    ));
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::DeleteRule(rule_id)) if rule_id.as_str() == "rule-delete"
    ));
}
