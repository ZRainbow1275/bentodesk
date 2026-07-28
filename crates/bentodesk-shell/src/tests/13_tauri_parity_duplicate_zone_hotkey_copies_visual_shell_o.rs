#[test]
fn tauri_parity_duplicate_zone_hotkey_copies_visual_shell_only() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 480.0,
            height: 320.0,
        };
        let mut zone = Zone::new(ZoneId(7), "Source", 12, 18, 160, 96);
        zone.set_icon(Cow::Borrowed("archive"));
        zone.set_accent_color(Some(Cow::Borrowed("#ffcc00")));
        zone.set_locked(true);
        zone.live_folder_path = Some(Cow::Borrowed(r"C:\Users\Alice\Desktop\Live"));
        let _ = zone.add_item(
            Cow::Borrowed(r"C:\Users\Alice\Desktop\a.txt"),
            Cow::Borrowed("hash-a"),
        );
        app.zones.add(zone);
        app.selected_zone.set(Some(ZoneId(7)));
    }

    assert!(duplicate_selected_zone(&root));
    let app = root.app.borrow();
    let duplicate_id = app
        .selected_zone
        .get()
        .expect("duplicate should become selected");
    assert_ne!(duplicate_id, ZoneId(7));
    let duplicate = app.zones.get(duplicate_id).expect("duplicate zone");
    assert_eq!(duplicate.title.as_ref(), "Source *");
    assert_eq!(duplicate.icon.as_ref(), "archive");
    assert_eq!(duplicate.accent_color.as_deref(), Some("#ffcc00"));
    assert!(!duplicate.locked);
    assert!(duplicate.live_folder_path.is_none());
    assert!(duplicate.stack_members.is_empty());
    assert!(duplicate.items.is_empty());
    assert!(app.dirty.get());
}

#[test]
fn tauri_parity_focus_hotkeys_cycle_visible_top_level_zones() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones.add(Zone::new(ZoneId(1), "One", 0, 0, 100, 100));
        let mut hidden = Zone::new(ZoneId(2), "Hidden", 0, 0, 100, 100);
        hidden.set_visible(false);
        app.zones.add(hidden);
        app.zones.add(Zone::new(ZoneId(3), "Three", 0, 0, 100, 100));
        app.selected_zone.set(Some(ZoneId(1)));
    }

    assert!(focus_visible_zone(&root, true));
    assert_eq!(root.app.borrow().selected_zone.get(), Some(ZoneId(3)));
    assert!(focus_visible_zone(&root, true));
    assert_eq!(root.app.borrow().selected_zone.get(), Some(ZoneId(1)));
    assert!(focus_visible_zone(&root, false));
    assert_eq!(root.app.borrow().selected_zone.get(), Some(ZoneId(3)));
}

#[test]
fn global_hotkey_metadata_maps_registered_ids_to_commands() {
    let root = test_app_root();
    let toggle_id = global_hotkey_id(super::hotkey::HotkeyCommand::ToggleMain).expect("toggle id");
    let create_id = global_hotkey_id(super::hotkey::HotkeyCommand::CreateZone).expect("create id");
    assert!(global_hotkey_id(super::hotkey::HotkeyCommand::Escape).is_none());
    root.global_hotkeys
        .borrow_mut()
        .push(super::GlobalHotkeyRegistration {
            id: toggle_id,
            command: super::hotkey::HotkeyCommand::ToggleMain,
        });
    root.global_hotkeys
        .borrow_mut()
        .push(super::GlobalHotkeyRegistration {
            id: create_id,
            command: super::hotkey::HotkeyCommand::CreateZone,
        });

    assert_eq!(
        global_hotkey_command(&root, toggle_id),
        Some(super::hotkey::HotkeyCommand::ToggleMain)
    );
    assert_eq!(
        global_hotkey_command(&root, create_id),
        Some(super::hotkey::HotkeyCommand::CreateZone)
    );
    assert_eq!(global_hotkey_command(&root, create_id + 100), None);
}

#[test]
fn global_hotkey_modifiers_include_norepeat_and_selected_mods() {
    let flags = global_hotkey_modifiers(super::hotkey::ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    });
    assert_ne!(
        flags & windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_NOREPEAT,
        0
    );
    assert_ne!(
        flags & windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL,
        0
    );
    assert_ne!(
        flags & windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_SHIFT,
        0
    );
    assert_eq!(
        flags & windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_ALT,
        0
    );
}

#[test]
fn keybinding_setting_applies_runtime_table_and_rejects_conflicts() {
    let root = test_app_root();
    assert!(apply_hotkey_setting_to_runtime(
        &root,
        "keybinding.timeline.open",
        &bentodesk_app::SettingValue::Str(SmolStr::new_static("Ctrl+Shift+T")),
    ));
    assert!(!apply_hotkey_setting_to_runtime(
        &root,
        "keybinding.snapshot.open",
        &bentodesk_app::SettingValue::Str(SmolStr::new_static("Ctrl+Shift+T")),
    ));
    assert_eq!(
        super::hotkey::lookup_in(
            &root.hotkey_bindings.borrow(),
            0x54,
            super::hotkey::ModFlags {
                ctrl: true,
                shift: true,
                alt: false,
            },
        ),
        Some(super::hotkey::HotkeyCommand::OpenTimeline)
    );
}

#[test]
fn keybinding_reset_removes_persisted_override() {
    let zones_path = scratch_zones_path("keybinding-reset");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let vault_path = state_dir.join("vault.bin");
    let mut vault = bentodesk_backend::config_vault::Vault::open(&vault_path).expect("open vault");
    vault.set_setting(
        "keybinding.timeline.open",
        bentodesk_backend::config_vault::SettingValue::Str(SmolStr::new_static("Ctrl+Shift+T")),
    );
    vault.flush().expect("flush override");

    assert!(
        persist_keybinding_reset_to_vault(&mut vault, super::hotkey::ACTION_OPEN_TIMELINE)
            .expect("reset keybinding")
    );
    let reopened =
        bentodesk_backend::config_vault::Vault::open(&vault_path).expect("reopen vault");
    assert_eq!(reopened.get_setting("keybinding.timeline.open"), None);

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn snapshot_delete_rejects_path_shaped_ids() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("snapshot-path-shaped");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        app.zones
            .add(Zone::new(ZoneId(5), "Safe", 80, 60, 240, 180));
    }
    let snapshot =
        save_layout_snapshot(&root, Some(smol_str::SmolStr::new_static("Safe snapshot")))
            .expect("save snapshot");
    let error = super::delete_layout_snapshot(&root, "../vault.bin").expect_err("reject path");
    assert!(matches!(
        error,
        super::SnapshotPickerError::SnapshotNotFound(_)
    ));
    let snapshot_dir = snapshot_dir_for_zones_path(&zones_path).expect("snapshot dir");
    let manager = bentodesk_backend::layout::SnapshotManager::new(snapshot_dir);
    assert!(manager.load(snapshot.id.as_str()).is_ok());
    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn recovery_bundle_capture_restore_round_trips_live_zones() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-round-trip");
    std::fs::create_dir_all(zones_path.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(31), "Recovery Source", 40, 50, 260, 180));
    }

    let summary = capture_recovery_bundle(&root).expect("capture recovery bundle");
    assert_eq!(summary.zone_count, 1);
    assert!(summary.path.exists(), "recovery bundle JSON must exist");

    {
        let mut app = root.app.borrow_mut();
        app.zones = ZoneList::new();
        app.zones
            .add(Zone::new(ZoneId(2), "Mutated", 10, 10, 120, 80));
        app.next_zone_id.set(3);
        app.dirty.set(false);
    }

    let restored = restore_recovery_bundle(&root).expect("restore recovery bundle");
    assert_eq!(restored.summary.zones_checksum, summary.zones_checksum);
    assert!(matches!(
        restored.icon_restore,
        RecoveryIconRestoreOutcome::NotIncluded
    ));
    {
        let app = root.app.borrow();
        assert!(app.zones.get(ZoneId(31)).is_some());
        assert!(app.zones.get(ZoneId(2)).is_none());
        assert!(app.dirty.get());
        assert!(app.next_zone_id.get() >= 32);
        bentodesk_platform::storage::write_zones_atomic(&app.zones_path, &app.zones)
            .expect("persist restored zones");
    }
    let persisted = bentodesk_platform::storage::read_zones(&zones_path)
        .expect("read persisted restored zones");
    assert!(persisted.get(ZoneId(31)).is_some());

    let _ = std::fs::remove_dir_all(zones_path.parent().expect("scratch parent"));
}

#[test]
fn recovery_bundle_vault_snapshot_flushes_and_reads_real_vault_bytes() {
    let zones_path = scratch_zones_path("recovery-vault-snapshot");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let vault_path = state_dir.join("vault.bin");
    let mut vault = Vault::open(&vault_path).expect("open vault");
    vault.set_setting(
        SETTING_DISPLAY_LOCALE,
        bentodesk_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("zh-CN")),
    );

    let (captured_path, vault_bin) = recovery_vault_snapshot_from_vault(&mut vault)
        .expect("capture vault")
        .expect("vault payload");
    assert_eq!(captured_path, vault_path);
    assert!(!vault_bin.is_empty());
    let reopened = Vault::open(&vault_path).expect("reopen captured vault");
    assert_eq!(
        reopened.get_setting(SETTING_DISPLAY_LOCALE),
        Some(bentodesk_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_vault_restore_replaces_active_vault_file() {
    let zones_path = scratch_zones_path("recovery-vault-restore");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let active_path = state_dir.join("vault.bin");
    let source_path = state_dir.join("source-vault.bin");

    let mut active = Vault::open(&active_path).expect("open active vault");
    active.set_setting(
        SETTING_DISPLAY_LOCALE,
        bentodesk_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("en-US")),
    );
    active.flush().expect("flush active");

    let mut source = Vault::open(&source_path).expect("open source vault");
    source.set_setting(
        SETTING_DISPLAY_LOCALE,
        bentodesk_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("zh-CN")),
    );
    source.flush().expect("flush source");
    let source_bytes = std::fs::read(&source_path).expect("read source vault");

    restore_recovery_vault_payload_to_vault(&mut active, &source_bytes)
        .expect("restore vault payload");
    assert_eq!(
        active.get_setting(SETTING_DISPLAY_LOCALE),
        Some(bentodesk_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );
    let reopened = Vault::open(&active_path).expect("reopen active vault");
    assert_eq!(
        reopened.get_setting(SETTING_DISPLAY_LOCALE),
        Some(bentodesk_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_capture_restore_round_trips_safety_manifest() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-manifest");
    let state_dir = zones_path.parent().expect("scratch parent");
    let desktop = state_dir.join("Desktop");
    let hidden_dir = desktop.join(".bentodesk");
    std::fs::create_dir_all(&hidden_dir).expect("hidden dir");
    let original_path = desktop.join("doc.txt");
    let hidden_path = hidden_dir.join("1").join("doc.txt");
    let manifest = bentodesk_backend::stealth::SafetyManifest {
        schema_version: bentodesk_backend::stealth::MANIFEST_SCHEMA_VERSION.to_string(),
        entries: vec![bentodesk_backend::stealth::ManifestEntry {
            original_path: original_path.display().to_string(),
            hidden_path: hidden_path.display().to_string(),
            zone_id: "1".to_string(),
            file_size_bytes: 42,
            hidden_at: "2026-05-08T00:00:00Z".to_string(),
            display_name: "doc.txt".to_string(),
            icon_x: Some(10),
            icon_y: Some(20),
            file_type: "File".to_string(),
        }],
        zones: Vec::new(),
        screen_width: 1920,
        screen_height: 1080,
        last_updated: "2026-05-08T00:00:00Z".to_string(),
    };
    bentodesk_backend::stealth::save_manifest(&hidden_dir, &manifest).expect("save manifest");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        let mut zone = Zone::new(ZoneId(1), "Manifest Source", 40, 50, 260, 180);
        zone.add_item_with_metadata(
            hidden_path.display().to_string(),
            Some(original_path.to_string_lossy().as_ref()),
            "hash",
            Some(std::borrow::Cow::Owned(original_path.display().to_string())),
            Some(std::borrow::Cow::Owned(hidden_path.display().to_string())),
        )
        .expect("add item");
        app.zones.add(zone);
    }

    let summary = capture_recovery_bundle(&root).expect("capture recovery bundle");
    assert_eq!(summary.safety_manifest_count, 1);
    std::fs::remove_file(hidden_dir.join("manifest.json")).expect("remove manifest");

    restore_recovery_bundle(&root).expect("restore recovery bundle");
    let restored = bentodesk_backend::stealth::load_manifest(&hidden_dir).expect("load restored");
    assert_eq!(restored.entries.len(), 1);
    assert_eq!(restored.entries[0].display_name, "doc.txt");

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_capture_restore_round_trips_icon_backup_sidecar() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-icon-backup");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let data_root = bentodesk_backend::recovery_bundle::data_root_for_state_file(&zones_path)
        .expect("data root");
    let layout = bentodesk_backend::icon_positions::SavedIconLayout {
        icons: vec![bentodesk_backend::icon_positions::IconPosition {
            name: "Contract.pdf".to_string(),
            x: 120,
            y: 240,
        }],
        saved_at: "2026-05-09T00:00:00Z".to_string(),
        resolution: bentodesk_backend::icon_positions::Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.25,
    };
    bentodesk_backend::icon_positions::persist_to_file(&layout, &data_root)
        .expect("persist icon backup");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones.add(Zone::new(
            ZoneId(91),
            "Icon Backup Source",
            40,
            50,
            260,
            180,
        ));
    }

    let summary = capture_recovery_bundle(&root).expect("capture recovery bundle");
    assert!(summary.icon_backup_included);
    std::fs::remove_file(data_root.join("icon_layout_backup.json")).expect("remove icon sidecar");

    let payload = bentodesk_backend::recovery_bundle::recover_zones_payload(&data_root)
        .expect("recover zones payload")
        .expect("payload exists");
    let outcome = restore_recovery_icon_backup_with(
        &data_root,
        payload.icon_backup.as_ref(),
        |saved_layout| {
            assert_eq!(saved_layout.icons.len(), 1);
            assert_eq!(saved_layout.icons[0].name, layout.icons[0].name);
            Ok(bentodesk_backend::icon_positions::RestoreResult {
                restored: 1,
                skipped: 0,
                failed: 0,
                auto_arrange_toggled: false,
            })
        },
    );
    match outcome {
        RecoveryIconRestoreOutcome::Restored(result) => {
            assert_eq!(result.restored, 1);
            assert_eq!(result.skipped, 0);
            assert_eq!(result.failed, 0);
        }
        other => panic!("expected live icon restore outcome, got {other:?}"),
    }
    let restored = bentodesk_backend::icon_positions::load_from_file(&data_root)
        .expect("load restored icon backup")
        .expect("restored icon backup");
    assert_eq!(restored.icons.len(), 1);
    assert_eq!(restored.icons[0].name, layout.icons[0].name);
    assert_eq!(restored.icons[0].x, layout.icons[0].x);
    assert_eq!(restored.icons[0].y, layout.icons[0].y);
    assert_eq!(restored.saved_at, layout.saved_at);
    assert_eq!(restored.resolution.width, layout.resolution.width);
    assert_eq!(restored.resolution.height, layout.resolution.height);
    assert!((restored.dpi - layout.dpi).abs() < f64::EPSILON);

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_bundle_capture_restore_round_trips_user_data_sidecars() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-user-data-sidecars");
    let state_dir = zones_path.parent().expect("scratch parent");
    let timeline_dir = state_dir.join("timeline");
    let snapshot_dir = state_dir.join("snapshots");
    std::fs::create_dir_all(&timeline_dir).expect("timeline dir");
    std::fs::create_dir_all(&snapshot_dir).expect("snapshot dir");
    std::fs::write(state_dir.join("rules.json"), br#"[{"id":"rule-live"}]"#).expect("write rules");
    std::fs::write(
        timeline_dir.join("checkpoint-live.json"),
        br#"{"id":"checkpoint-live"}"#,
    )
    .expect("write timeline");
    std::fs::write(
        snapshot_dir.join("snapshot-live.json"),
        br#"{"id":"snapshot-live"}"#,
    )
    .expect("write snapshot");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones
            .add(Zone::new(ZoneId(92), "User Data Source", 40, 50, 260, 180));
    }

    let summary = capture_recovery_bundle(&root).expect("capture recovery bundle");
    assert_eq!(summary.user_data_file_count, 3);
    assert!(summary.user_data_len_bytes > 0);

    std::fs::remove_file(state_dir.join("rules.json")).expect("remove rules");
    std::fs::remove_file(timeline_dir.join("checkpoint-live.json")).expect("remove timeline");
    std::fs::remove_file(snapshot_dir.join("snapshot-live.json")).expect("remove snapshot");

    let outcome = restore_recovery_bundle(&root).expect("restore recovery bundle");
    assert_eq!(outcome.user_data_restore.restored_files, 3);
    assert_eq!(
        std::fs::read(state_dir.join("rules.json")).expect("read rules"),
        br#"[{"id":"rule-live"}]"#
    );
    assert_eq!(
        std::fs::read(timeline_dir.join("checkpoint-live.json")).expect("read timeline"),
        br#"{"id":"checkpoint-live"}"#
    );
    assert_eq!(
        std::fs::read(snapshot_dir.join("snapshot-live.json")).expect("read snapshot"),
        br#"{"id":"snapshot-live"}"#
    );

    let report = export_recovery_diagnostics(&root).expect("export diagnostics");
    assert_eq!(report.user_data_files.len(), 3);

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn recovery_diagnostics_export_writes_visible_bundle_report() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("recovery-diagnostics");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.clone();
        app.zones.add(Zone::new(
            ZoneId(101),
            "Diagnostics Source",
            40,
            50,
            260,
            180,
        ));
    }

    let summary = capture_recovery_bundle(&root).expect("capture recovery bundle");
    let report = export_recovery_diagnostics(&root).expect("export diagnostics");

    assert_eq!(report.zones.zone_count, summary.zone_count);
    assert_eq!(report.zones.checksum, summary.zones_checksum);
    assert_eq!(report.vault.included, summary.vault_included);
    let report_path = bentodesk_backend::recovery_bundle::diagnostics_path(state_dir);
    assert_eq!(report.diagnostics_path, report_path.display().to_string());
    assert!(report_path.exists(), "diagnostics report must be persisted");

    let persisted = bentodesk_backend::storage::read_json_with_recovery::<
        bentodesk_backend::recovery_bundle::RecoveryDiagnosticsReport,
    >(&report_path, "Recovery diagnostics")
    .expect("read diagnostics report")
    .expect("diagnostics report exists");
    assert_eq!(persisted.zones.zone_count, 1);
    assert_eq!(persisted.zones.path, zones_path.display().to_string());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn startup_layout_loads_existing_zones_bin_before_first_render() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-load-zones-bin");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let mut selected_zones = ZoneList::new();
    selected_zones.add(Zone::new(
        ZoneId(131),
        "Selected Stack Existing",
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
    assert_eq!(outcome.zone_count, 1);
    assert!(!outcome.persisted);
    let app = root.app.borrow();
    assert_eq!(app.zones_path, zones_path);
    assert!(app.zones.get(ZoneId(131)).is_some());
    assert_eq!(app.next_zone_id.get(), 132);
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn startup_layout_migrates_legacy_tauri_layout_json_to_zones_bin() {
    let root = test_app_root();
    let zones_path = scratch_zones_path("startup-migrate-legacy-layout");
    let state_dir = zones_path.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    let legacy_path = state_dir.join("layout.json");
    let legacy_layout = legacy_layout_with_zone("141", "Legacy Documents", "legacy.txt");
    legacy_layout
        .save(&legacy_path)
        .expect("write legacy layout");

    let outcome = load_startup_zones_or_migrate_legacy(&root, &zones_path)
        .expect("startup migration")
        .expect("legacy layout migrated");

    assert_eq!(outcome.source, StartupLayoutLoadSource::LegacyLayoutJson);
    assert_eq!(outcome.zone_count, 1);
    assert!(outcome.persisted);
    let persisted = storage::read_zones(&zones_path).expect("read migrated zones");
    let migrated_zone = persisted.get(ZoneId(141)).expect("migrated zone");
    assert_eq!(migrated_zone.title.as_ref(), "Legacy Documents");
    assert_eq!(migrated_zone.icon.as_ref(), "folder");
    assert_eq!(migrated_zone.grid_columns, 4);
    assert_eq!(migrated_zone.capsule_size.as_ref(), "large");
    assert_eq!(migrated_zone.capsule_shape.as_ref(), "rounded");
    assert_eq!(migrated_zone.alias.as_deref(), Some("Legacy Alias"));
    // Wave G1 (2026-05-20) — legacy `layout.json` files used to encode
    // `display_mode = "always"` for every zone. `read_zones` now strips
    // that stale value to `None` (app-level default `Hover`, the
    // Tauri-faithful "collapsed pill at rest" behaviour) so legacy
    // imports collapse correctly on first launch. Users who genuinely
    // want "always-expanded" re-toggle it from Settings.
    assert!(
        migrated_zone.display_mode.is_none(),
        "Wave G1 stale-always migration must strip legacy 'always' on load, got {:?}",
        migrated_zone.display_mode,
    );
    assert_eq!(
        migrated_zone.live_folder_path.as_deref(),
        Some(r"C:\Users\Alice\Desktop\Live")
    );
    let migrated_item = migrated_zone.items.first().expect("migrated item");
    assert_eq!(migrated_item.id, ZoneItemId(14101));
    assert_eq!(migrated_item.name.as_ref(), "legacy.txt");
    assert_eq!(migrated_item.x, 2);
    assert_eq!(migrated_item.y, 3);
    assert!(migrated_item.is_wide);
    assert_eq!(
        migrated_item.original_path.as_deref(),
        Some(r"C:\Users\Alice\Desktop\legacy.txt")
    );
    assert!(
        migrated_item
            .hidden_path
            .as_deref()
            .is_some_and(|path| path.ends_with(r"\.bentodesk\141\legacy.txt"))
    );
    assert!(
        migrated_item
            .tags
            .iter()
            .any(|tag| tag.as_ref() == "legacy")
    );
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(141)).is_some());
    assert_eq!(app.next_zone_id.get(), 142);
    assert!(!app.dirty.get());

    let _ = std::fs::remove_dir_all(state_dir);
}
