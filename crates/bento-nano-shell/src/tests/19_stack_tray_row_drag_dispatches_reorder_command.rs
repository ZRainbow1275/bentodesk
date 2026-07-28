#[test]
fn stack_tray_row_drag_dispatches_reorder_command() {
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
            .add(Zone::new(ZoneId(2), "Child A", 420, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(3), "Child B", 640, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(app.zones.stack(ZoneId(1), ZoneId(3)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
    }
    let (from_row, target_row) = {
        let app = root.app.borrow();
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        (
            stack_tray::stack_tray_row_rect(app.viewport, anchor, 3, 2),
            stack_tray::stack_tray_row_rect(app.viewport, anchor, 3, 1),
        )
    };

    assert!(handle_stack_tray_lbutton_down(
        &root,
        from_row.x + 4.0,
        from_row.y + 4.0
    ));
    assert!(handle_stack_tray_lbutton_up(
        &root,
        std::ptr::null_mut(),
        target_row.x + 4.0,
        target_row.y + 4.0
    ));

    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::ReorderStackMember(ZoneId(1), ZoneId(3), 1))
    ));
}

#[test]
fn reorder_stack_member_command_mutates_real_order() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(2), "Child A", 420, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(3), "Child B", 640, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(app.zones.stack(ZoneId(1), ZoneId(3)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
    }

    root.dispatcher
        .push(Command::ReorderStackMember(ZoneId(1), ZoneId(3), 1));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert_eq!(
        app.zones
            .stack_member_ids(ZoneId(1))
            .map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(3), ZoneId(2)])
    );
    assert!(app.dirty.get());
    assert!(matches!(
        app.stack_tray.borrow().as_ref(),
        Some(state) if state.selected_member_id == ZoneId(3)
    ));
}

#[test]
fn stack_tray_detach_command_mutates_real_stack() {
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
            .add(Zone::new(ZoneId(2), "Child A", 420, 100, 180, 130));
        app.zones
            .add(Zone::new(ZoneId(3), "Child B", 640, 100, 180, 130));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(app.zones.stack(ZoneId(1), ZoneId(3)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(2)));
    }

    root.dispatcher
        .push(Command::DetachStackMember(ZoneId(1), ZoneId(2)));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert!(matches!(app.zones.get(ZoneId(2)), Some(zone) if zone.stack_parent.is_none()));
    assert_eq!(app.zones.stack_anchor_for(ZoneId(3)), Some(ZoneId(1)));
    assert!(app.dirty.get());
    assert!(matches!(
        app.stack_tray.borrow().as_ref(),
        Some(state) if state.anchor_zone_id == ZoneId(1) && state.selected_member_id == ZoneId(1)
    ));
}

#[test]
fn dissolve_stack_command_scatters_released_members() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 640.0,
            height: 480.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Anchor", 100, 80, 120, 90));
        app.zones
            .add(Zone::new(ZoneId(2), "Child A", 100, 80, 120, 90));
        app.zones
            .add(Zone::new(ZoneId(3), "Child B", 100, 80, 120, 90));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));
        assert!(app.zones.stack(ZoneId(1), ZoneId(3)));
        app.stack_tray
            .borrow_mut()
            .replace(stack_tray::StackTrayState::new(ZoneId(1), ZoneId(1)));
    }

    root.dispatcher.push(Command::DissolveStack(ZoneId(1)));
    consume_dispatcher(&root, std::ptr::null_mut());

    let app = root.app.borrow();
    assert!(app.zones.stack_member_ids(ZoneId(1)).is_none());
    assert_eq!(
        app.zones.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((100, 80))
    );
    assert_eq!(
        app.zones.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((236, 80))
    );
    assert_eq!(
        app.zones.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
        Some((372, 80))
    );
    assert!(app.stack_tray.borrow().is_none());
    assert!(app.dirty.get());
}

#[test]
fn locale_setting_command_persists_display_locale_key() {
    assert_eq!(
        locale_setting_command_for("en-US"),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("display.locale"),
            value: bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("en-US")),
        }
    );
}

#[test]
fn persisted_locale_rejects_unknown_wire_value() {
    assert!(!apply_locale_wire("fr-FR"));
}

#[test]
fn updater_frequency_setting_uses_tauri_wire_key() {
    assert_eq!(
        update_frequency_setting_command_for(UpdateCheckFrequency::Manual),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("updates.check_frequency"),
            value: bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Manual")),
        }
    );
    assert_eq!(
        next_update_frequency(UpdateCheckFrequency::Weekly),
        UpdateCheckFrequency::Manual
    );
    assert_eq!(
        update_frequency_from_wire("Daily"),
        Some(UpdateCheckFrequency::Daily)
    );
    assert_eq!(update_frequency_from_wire("Hourly"), None);
    assert!(should_start_background_update_check(
        UpdateCheckFrequency::Daily
    ));
    assert_eq!(
        update_check_interval(UpdateCheckFrequency::Daily),
        Some(std::time::Duration::from_secs(24 * 60 * 60))
    );
    assert!(should_start_background_update_check(
        UpdateCheckFrequency::Weekly
    ));
    assert_eq!(
        update_check_interval(UpdateCheckFrequency::Weekly),
        Some(std::time::Duration::from_secs(7 * 24 * 60 * 60))
    );
    assert!(!should_start_background_update_check(
        UpdateCheckFrequency::Manual
    ));
    assert_eq!(update_check_interval(UpdateCheckFrequency::Manual), None);
}

#[test]
fn updater_events_update_visible_settings_status() {
    let app = AppState::new();
    let available_event = UpdateEvent::Available {
        info: UpdateInfo {
            version: smol_str::SmolStr::new_static("2.1.0"),
            current_version: smol_str::SmolStr::new_static("2.0.0"),
            date: None,
            body: None,
            artifact_url: None,
            artifact_sha256: None,
            signature: None,
        },
    };
    app.update_auto_download.set(true);
    assert!(updater_event_should_auto_download(&app, &available_event));
    app.update_auto_download.set(false);
    assert!(!updater_event_should_auto_download(&app, &available_event));
    app.update_auto_download.set(true);
    assert!(!updater_event_should_auto_download(
        &app,
        &UpdateEvent::Progress {
            progress: UpdateProgress {
                chunk_len: 1,
                total_bytes: Some(1),
            },
        }
    ));
    apply_update_event_to_app(&app, available_event);
    assert_eq!(
        *app.settings_updater_status.borrow(),
        SettingsUpdaterStatus::Available {
            version: smol_str::SmolStr::new_static("2.1.0")
        }
    );
    assert_eq!(
        app.settings_updater_status
            .borrow()
            .version_for_skip()
            .as_deref(),
        Some("2.1.0")
    );

    apply_update_event_to_app(
        &app,
        UpdateEvent::Progress {
            progress: UpdateProgress {
                chunk_len: 4096,
                total_bytes: Some(8192),
            },
        },
    );
    assert_eq!(
        app.settings_updater_status.borrow().summary().as_str(),
        "Downloading 4096/8192 B"
    );

    apply_update_event_to_app(
        &app,
        UpdateEvent::Installing {
            info: UpdateInfo {
                version: smol_str::SmolStr::new_static("2.1.0"),
                current_version: smol_str::SmolStr::new_static("2.0.0"),
                date: None,
                body: None,
                artifact_url: None,
                artifact_sha256: None,
                signature: None,
            },
        },
    );
    assert_eq!(
        app.settings_updater_status.borrow().summary().as_str(),
        "Installing 2.1.0"
    );

    apply_update_event_to_app(
        &app,
        UpdateEvent::Error {
            kind: smol_str::SmolStr::new_static("download"),
            message: "T-091.1 deferred".to_owned(),
        },
    );
    assert!(matches!(
        &*app.settings_updater_status.borrow(),
        SettingsUpdaterStatus::Error(message) if message.as_str().contains("download:")
    ));
}

#[test]
fn updater_visible_action_dispatches_download_until_ready_then_install() {
    let root = test_app_root();

    queue_update_action(&root);
    let mut drained = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 0);
    {
        let app = root.app.borrow();
        assert!(matches!(
            &*app.settings_updater_status.borrow(),
            SettingsUpdaterStatus::Error(message)
                if message.as_str() == "No update action is available"
        ));
        *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Available {
            version: smol_str::SmolStr::new_static("2.1.0"),
        };
    }

    queue_update_action(&root);
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(drained.first(), Some(Command::DownloadUpdate)));

    {
        let app = root.app.borrow();
        *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Ready {
            version: smol_str::SmolStr::new_static("2.1.0"),
        };
    }
    queue_update_action(&root);
    drained.clear();
    assert_eq!(root.dispatcher.drain_into(&mut drained), 1);
    assert!(matches!(
        drained.first(),
        Some(Command::InstallUpdateAndRestart)
    ));
}

#[test]
fn settings_bool_commands_use_stable_dotted_keys() {
    assert_eq!(
        bool_setting_command_for(SETTING_UPDATES_AUTO_DOWNLOAD, false),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("updates.auto_download"),
            value: bento_nano_app::SettingValue::Bool(false),
        }
    );
    assert_eq!(
        bool_setting_command_for(SETTING_STEALTH_ENABLED, true),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("stealth.enabled"),
            value: bento_nano_app::SettingValue::Bool(true),
        }
    );
    assert_eq!(
        bool_setting_command_for(SETTING_DEBUG_OVERLAY, true),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("debug_overlay"),
            value: bento_nano_app::SettingValue::Bool(true),
        }
    );
}

#[test]
fn encryption_mode_setting_uses_stable_dotted_key_and_safe_modes_only() {
    assert_eq!(
        encryption_mode_setting_command_for(SettingsEncryptionMode::Dpapi),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("encryption.mode"),
            value: bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Dpapi")),
        }
    );
    // M7 (2026-06-01) — `next_encryption_mode` removed (the orphan cycle is
    // replaced by the §10 3-button grid that sets each mode explicitly).
    // The §10 None/Dpapi buttons route through this command helper, so pin
    // the "None" wire string too.
    assert_eq!(
        encryption_mode_setting_command_for(SettingsEncryptionMode::None),
        Command::SetSetting {
            key: smol_str::SmolStr::new_static("encryption.mode"),
            value: bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("None")),
        }
    );
    assert_eq!(
        encryption_mode_from_wire("None"),
        Some(SettingsEncryptionMode::None)
    );
    assert_eq!(
        encryption_mode_from_wire("Dpapi"),
        Some(SettingsEncryptionMode::Dpapi)
    );
    assert_eq!(
        encryption_mode_from_wire("Passphrase"),
        Some(SettingsEncryptionMode::Passphrase)
    );
}

#[test]
fn setting_runtime_apply_updates_visible_state_only_for_valid_payloads() {
    let app = AppState::new();
    assert_eq!(
        app.update_check_frequency.get(),
        UpdateCheckFrequency::Weekly
    );
    assert!(app.update_auto_download.get());
    assert!(app.stealth_enabled.get());
    assert_eq!(app.encryption_mode.get(), SettingsEncryptionMode::None);

    assert!(apply_setting_value_to_app(
        &app,
        SETTING_UPDATES_CHECK_FREQUENCY,
        &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Daily")),
    ));
    assert_eq!(
        app.update_check_frequency.get(),
        UpdateCheckFrequency::Daily
    );

    assert!(apply_setting_value_to_app(
        &app,
        SETTING_UPDATES_AUTO_DOWNLOAD,
        &bento_nano_app::SettingValue::Bool(false),
    ));
    assert!(!app.update_auto_download.get());

    assert!(apply_setting_value_to_app(
        &app,
        SETTING_STEALTH_ENABLED,
        &bento_nano_app::SettingValue::Bool(false),
    ));
    assert!(!app.stealth_enabled.get());

    assert!(apply_setting_value_to_app(
        &app,
        SETTING_DEBUG_OVERLAY,
        &bento_nano_app::SettingValue::Bool(true),
    ));
    assert!(app.debug_overlay.borrow().visible);

    assert!(apply_setting_value_to_app(
        &app,
        SETTING_ENCRYPTION_MODE,
        &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Dpapi")),
    ));
    assert_eq!(app.encryption_mode.get(), SettingsEncryptionMode::Dpapi);

    assert!(!apply_setting_value_to_app(
        &app,
        SETTING_UPDATES_CHECK_FREQUENCY,
        &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Hourly")),
    ));
    assert!(!apply_setting_value_to_app(
        &app,
        SETTING_ENCRYPTION_MODE,
        &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Passphrase")),
    ));
}

#[test]
fn theme_base_accent_validates_applies_persists_and_clears() {
    assert_eq!(
        theme_base_accent_from_wire("#3b82f6").as_deref(),
        Some("#3b82f6")
    );
    assert_eq!(theme_base_accent_from_wire("#not-real"), None);

    let app = AppState::new();
    assert!(apply_theme_base_accent_to_app(
        &app,
        Some(smol_str::SmolStr::new_static("#3b82f6"))
    ));
    assert_eq!(app.theme_base_accent.borrow().as_deref(), Some("#3b82f6"));
    assert!(!apply_theme_base_accent_to_app(
        &app,
        Some(smol_str::SmolStr::new_static("#3b82f6"))
    ));
    assert!(apply_theme_base_accent_to_app(&app, None));
    assert_eq!(app.theme_base_accent.borrow().as_deref(), None);

    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-theme-base-accent-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut vault = Vault::open(&path).expect("open test vault");
    let accent = smol_str::SmolStr::new_static("#3b82f6");
    assert!(
        persist_theme_base_accent_to_vault(&mut vault, Some(&accent)).expect("persist theme base")
    );
    assert_eq!(
        vault.get_setting(SETTING_THEME_BASE_ACCENT),
        Some(bento_nano_backend::config_vault::SettingValue::Str(accent))
    );
    assert!(persist_theme_base_accent_to_vault(&mut vault, None).expect("clear theme base"));
    assert_eq!(vault.get_setting(SETTING_THEME_BASE_ACCENT), None);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn settings_accent_vault_persist_writes_and_clears_modern_and_legacy_keys() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-settings-accent-clear-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut vault = Vault::open(&path).expect("open test vault");
    let accent = smol_str::SmolStr::new_static("#abcdef");

    persist_settings_accent_to_vault(&mut vault, Some(&accent), false);
    assert_eq!(
        vault.get_setting(SETTING_APPEARANCE_ACCENT_COLOR),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            accent.clone()
        ))
    );
    assert_eq!(
        vault.get_setting(SETTING_THEME_BASE_ACCENT),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            accent.clone()
        ))
    );

    persist_settings_accent_to_vault(&mut vault, Some(&accent), true);
    assert_eq!(vault.get_setting(SETTING_APPEARANCE_ACCENT_COLOR), None);
    assert_eq!(vault.get_setting(SETTING_THEME_BASE_ACCENT), None);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn active_theme_loads_applies_persists_and_rejects_unknown() {
    let root = test_app_root();
    let scratch = scratch_zones_path("active-theme");
    std::fs::create_dir_all(scratch.parent().expect("scratch parent")).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }

    let options = load_available_theme_options(&root).expect("load built-in themes");
    assert!(
        options
            .iter()
            .any(|theme| theme.id.as_str() == "ocean-blue")
    );

    assert!(
        apply_active_theme_to_app(&root, SmolStr::new_static("ocean-blue"))
            .expect("apply built-in theme")
    );
    {
        let app = root.app.borrow();
        assert_eq!(app.active_theme_id.borrow().as_str(), "ocean-blue");
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_OCEAN_BLUE
        );
    }
    assert!(active_theme_id_is_builtin("light"));
    assert!(
        apply_active_theme_to_app(&root, SmolStr::new_static("light"))
            .expect("apply app built-in light")
    );
    {
        let app = root.app.borrow();
        assert_eq!(app.active_theme_id.borrow().as_str(), "light");
    }

    let mut vault_path = std::env::temp_dir();
    vault_path.push(format!(
        "bento-nano-active-theme-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&vault_path);
    let mut vault = Vault::open(&vault_path).expect("open test vault");
    let active_id = SmolStr::new_static("light");
    assert!(persist_active_theme_to_vault(&mut vault, &active_id).expect("persist theme"));
    assert_eq!(
        vault.get_setting(SETTING_ACTIVE_THEME),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            active_id
        ))
    );

    assert!(apply_active_theme_to_app(&root, SmolStr::new_static("missing-theme")).is_err());
    assert_eq!(root.app.borrow().active_theme_id.borrow().as_str(), "light");

    let _ = std::fs::remove_file(&vault_path);
    let _ = std::fs::remove_dir_all(scratch.parent().expect("scratch parent"));
}
