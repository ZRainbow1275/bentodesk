#[test]
fn active_theme_applies_preextracted_plugin_provider_theme() {
    let root = test_app_root();
    let scratch = scratch_zones_path("active-theme-plugin");
    let state_dir = scratch.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }
    let plugin_dir = state_dir.join("plugins").join("com.test.shell-theme");
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    std::fs::write(
        plugin_dir.join("manifest.json"),
        r#"{
  "id": "com.test.shell-theme",
  "name": "Shell Theme",
  "version": "1.0.0",
  "type": "theme",
  "author": "Tester",
  "description": "Shell-visible selected-stack theme provider",
  "min_app_version": null,
  "icon": null
}"#,
    )
    .expect("manifest");
    std::fs::write(
        plugin_dir.join("theme.json"),
        r##"{
  "id": "shell-purple",
  "name": "Shell Purple",
  "is_builtin": false,
  "colors": {
    "accent": "#a855f7",
    "background": "rgba(30, 10, 50, 0.8)",
    "text": "#f5f3ff",
    "border": "rgba(168, 85, 247, 0.2)"
  },
  "capsule": {
    "shape": "rounded",
    "size": "medium",
    "blur_radius": 18.0
  },
  "animation": {
    "expand_duration_ms": 200,
    "collapse_duration_ms": 180
  },
  "glassmorphism": {
    "blur": 18.0,
    "opacity": 0.8,
    "saturation": 1.4
  }
}"##,
    )
    .expect("theme");

    let options = load_available_theme_options(&root).expect("load themes");
    assert!(
        options
            .iter()
            .any(|theme| theme.id.as_str() == "shell-purple")
    );
    assert!(
        apply_active_theme_to_app(&root, SmolStr::new_static("shell-purple"))
            .expect("apply plugin theme")
    );
    {
        let app = root.app.borrow();
        assert_eq!(app.active_theme_id.borrow().as_str(), "shell-purple");
        assert_eq!(app.active_theme_name.borrow().as_str(), "Shell Purple");
    }

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn settings_plugin_commands_list_toggle_and_uninstall_real_registry_entries() {
    let root = test_app_root();
    let scratch = scratch_zones_path("settings-plugin-lifecycle");
    let state_dir = scratch.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }
    assert_eq!(state_dir_for_root(&root), state_dir);
    let plugin_dir = state_dir.join("plugins").join("com.test.lifecycle-theme");
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    std::fs::write(
        plugin_dir.join("manifest.json"),
        r#"{
  "id": "com.test.lifecycle-theme",
  "name": "Lifecycle Theme",
  "version": "1.0.0",
  "type": "theme",
  "author": "Tester",
  "description": "Selected-stack lifecycle plugin",
  "min_app_version": null,
  "icon": null
}"#,
    )
    .expect("manifest");
    std::fs::write(
        plugin_dir.join("theme.json"),
        r##"{
  "id": "lifecycle-theme",
  "name": "Lifecycle Theme",
  "is_builtin": false,
  "colors": {
    "accent": "#22c55e",
    "background": "rgba(4, 30, 20, 0.8)",
    "text": "#ecfdf5",
    "border": "rgba(34, 197, 94, 0.2)"
  },
  "capsule": {
    "shape": "rounded",
    "size": "medium",
    "blur_radius": 16.0
  },
  "animation": {
    "expand_duration_ms": 180,
    "collapse_duration_ms": 160
  },
  "glassmorphism": {
    "blur": 16.0,
    "opacity": 0.8,
    "saturation": 1.3
  }
}"##,
    )
    .expect("theme");

    root.dispatcher.push(Command::ListPlugins);
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        let entries = app.settings_plugin_entries.borrow();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id.as_str(), "com.test.lifecycle-theme");
        assert!(entries[0].enabled);
        assert!(
            app.settings_plugin_status.borrow().is_none(),
            "a normal list refresh must stay visually quiet"
        );
    }

    root.dispatcher.push(Command::TogglePlugin(
        SmolStr::new_static("com.test.lifecycle-theme"),
        false,
    ));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        let entries = app.settings_plugin_entries.borrow();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].enabled);
        assert!(matches!(
            app.settings_plugin_status.borrow().as_ref(),
            Some(SettingsBackupStatus::Success(_))
        ));
    }
    let reloaded = bento_nano_backend::plugins::PluginRegistry::load(state_dir).expect("registry");
    assert_eq!(
        reloaded
            .find("com.test.lifecycle-theme")
            .map(|plugin| plugin.enabled),
        Some(false)
    );

    root.dispatcher
        .push(Command::UninstallPlugin(SmolStr::new_static(
            "com.test.lifecycle-theme",
        )));
    consume_dispatcher(&root, std::ptr::null_mut());
    {
        let app = root.app.borrow();
        assert!(app.settings_plugin_entries.borrow().is_empty());
        assert!(matches!(
            app.settings_plugin_status.borrow().as_ref(),
            Some(SettingsBackupStatus::Success(_))
        ));
    }
    assert!(!plugin_dir.exists());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn settings_install_plugin_command_extracts_archive_and_refreshes_theme_providers() {
    let root = test_app_root();
    let scratch = scratch_zones_path("settings-plugin-install-archive");
    let state_dir = scratch.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }
    let archive_path = state_dir.join("archive-theme.bdplugin");
    write_plugin_archive(
        &archive_path,
        "com.test.archive-theme",
        "Archive Theme Plugin",
        "archive-theme",
        "Archive Theme",
    );

    root.dispatcher.push(Command::InstallPlugin(SmolStr::new(
        archive_path.to_string_lossy(),
    )));
    consume_dispatcher(&root, std::ptr::null_mut());

    let install_dir = state_dir.join("plugins").join("com.test.archive-theme");
    assert!(install_dir.join("manifest.json").is_file());
    assert!(install_dir.join("theme.json").is_file());
    {
        let app = root.app.borrow();
        let entries = app.settings_plugin_entries.borrow();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id.as_str(), "com.test.archive-theme");
        assert_eq!(entries[0].name.as_str(), "Archive Theme Plugin");
        assert!(entries[0].enabled);
        assert!(matches!(
            app.settings_plugin_status.borrow().as_ref(),
            Some(SettingsBackupStatus::Success(message))
                if message.contains("Archive Theme Plugin")
        ));
        assert!(
            app.available_themes
                .borrow()
                .iter()
                .any(|theme| theme.id.as_str() == "archive-theme"),
            "install refresh should expose plugin-provided theme option"
        );
    }

    let registry = bento_nano_backend::plugins::PluginRegistry::load(state_dir).expect("registry");
    assert!(registry.find("com.test.archive-theme").is_some());

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn refresh_settings_plugins_for_root_updates_app_state_without_mock_rows() {
    let root = test_app_root();
    let scratch = scratch_zones_path("settings-plugin-empty-list");
    let state_dir = scratch.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }

    assert!(list_plugins_for_root(&root).expect("list").is_empty());
    assert!(!refresh_settings_plugins_for_root(&root).expect("refresh"));
    assert!(
        root.app
            .borrow()
            .settings_plugin_entries
            .borrow()
            .is_empty()
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn theme_import_copies_user_json_into_selected_stack_theme_dir() {
    let root = test_app_root();
    let scratch = scratch_zones_path("theme-import");
    let state_dir = scratch.parent().expect("scratch parent");
    std::fs::create_dir_all(state_dir).expect("scratch");
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = scratch.clone();
    }
    let source_dir = state_dir.join("downloads");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    let source_path = source_dir.join("theme-from-browser.json");
    std::fs::write(
        &source_path,
        r##"{
  "id": "imported-amber",
  "name": "Imported Amber",
  "is_builtin": false,
  "colors": {
    "accent": "#f59e0b",
    "background": "rgba(69, 26, 3, 0.75)",
    "text": "#fffbeb",
    "border": "rgba(245, 158, 11, 0.24)"
  },
  "capsule": {
    "shape": "rounded",
    "size": "medium",
    "blur_radius": 18.0
  },
  "animation": {
    "expand_duration_ms": 210,
    "collapse_duration_ms": 170
  },
  "glassmorphism": {
    "blur": 18.0,
    "opacity": 0.78,
    "saturation": 1.35
  }
}"##,
    )
    .expect("theme source");

    let imported = import_theme_for_root(&root, &source_path).expect("import theme");
    assert_eq!(imported.id.as_str(), "imported-amber");
    assert!(
        state_dir
            .join("themes")
            .join("imported-amber.json")
            .is_file()
    );
    let options = load_available_theme_options(&root).expect("load imported themes");
    assert!(
        options
            .iter()
            .any(|theme| theme.id.as_str() == "imported-amber")
    );

    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn zone_display_mode_setting_uses_tauri_wire_key_and_applies() {
    assert_eq!(
        zone_display_mode_from_wire("hover"),
        Some(ZoneDisplayMode::Hover)
    );
    assert_eq!(
        zone_display_mode_from_wire("always"),
        Some(ZoneDisplayMode::Always)
    );
    assert_eq!(
        zone_display_mode_from_wire("click"),
        Some(ZoneDisplayMode::Click)
    );
    assert_eq!(zone_display_mode_from_wire("popup"), None);

    let app = AppState::new();
    assert!(apply_setting_value_to_app(
        &app,
        SETTING_ZONE_DISPLAY_MODE,
        &bento_nano_app::SettingValue::Str(SmolStr::new_static("click")),
    ));
    assert_eq!(app.zone_display_mode.get(), ZoneDisplayMode::Click);
    assert!(!apply_setting_value_to_app(
        &app,
        SETTING_ZONE_DISPLAY_MODE,
        &bento_nano_app::SettingValue::Str(SmolStr::new_static("popup")),
    ));
    assert_eq!(app.zone_display_mode.get(), ZoneDisplayMode::Click);

    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-zone-display-mode-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut vault = Vault::open(&path).expect("open test vault");
    assert!(
        persist_zone_display_mode_to_vault(&mut vault, ZoneDisplayMode::Always)
            .expect("persist display mode")
    );
    assert_eq!(
        vault.get_setting(SETTING_ZONE_DISPLAY_MODE),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            SmolStr::new_static("always")
        ))
    );
    assert!(
        !persist_setting_to_vault(
            &mut vault,
            SETTING_ZONE_DISPLAY_MODE,
            &bento_nano_app::SettingValue::Str(SmolStr::new_static("popup")),
        )
        .expect("reject invalid mode without error")
    );
    assert_eq!(
        vault.get_setting(SETTING_ZONE_DISPLAY_MODE),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            SmolStr::new_static("always")
        ))
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn encryption_mode_setting_changes_real_vault_mode_before_runtime_apply() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-encryption-mode-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut vault = Vault::open(&path).expect("open test vault");

    assert!(
        persist_setting_to_vault(
            &mut vault,
            SETTING_ENCRYPTION_MODE,
            &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("None")),
        )
        .expect("persist none")
    );
    assert_eq!(vault.mode_tag(), ModeTag::None);
    assert_eq!(
        vault.get_setting(SETTING_ENCRYPTION_MODE),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("None")
        ))
    );

    vault
        .set_mode(BackendEncryptionMode::None)
        .expect("test reset mode");

    assert!(
        !persist_setting_to_vault(
            &mut vault,
            SETTING_ENCRYPTION_MODE,
            &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("Passphrase")),
        )
        .expect("reject passphrase cleanly")
    );
    assert_eq!(vault.mode_tag(), ModeTag::None);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn passphrase_encryption_command_path_encrypts_and_verifies_vault() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-passphrase-mode-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut vault = Vault::open(&path).expect("open test vault");
    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("zh-CN")),
    );

    persist_passphrase_to_vault(&mut vault, "correct horse").expect("persist passphrase");
    assert_eq!(vault.mode_tag(), ModeTag::Passphrase);
    assert!(vault.verify_passphrase("correct horse"));
    assert_eq!(
        vault.get_setting(SETTING_ENCRYPTION_MODE),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("Passphrase")
        ))
    );

    let wrong = Vault::open_with_passphrase(&path, "wrong horse");
    assert!(wrong.is_err(), "wrong passphrase must not unlock the vault");
    let reopened =
        Vault::open_with_passphrase(&path, "correct horse").expect("open with passphrase");
    assert_eq!(
        reopened.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn locked_passphrase_vault_rejects_plain_setting_until_unlock_succeeds() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "bento-nano-passphrase-unlock-{}.vault",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let mut vault = Vault::open(&path).expect("open test vault");
        vault.set_setting(
            "display.locale",
            bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static(
                "zh-CN",
            )),
        );
        persist_passphrase_to_vault(&mut vault, "correct horse").expect("persist passphrase");
    }

    let mut locked = Vault::open(&path).expect("open locked vault");
    assert!(locked.is_locked_passphrase());
    assert_eq!(locked.mode_tag(), ModeTag::Passphrase);
    assert!(
        persist_setting_to_vault(
            &mut locked,
            SETTING_DISPLAY_LOCALE,
            &bento_nano_app::SettingValue::Str(smol_str::SmolStr::new_static("en-US")),
        )
        .is_err()
    );
    assert!(unlock_passphrase_vault(&mut locked, "wrong horse").is_err());
    unlock_passphrase_vault(&mut locked, "correct horse").expect("unlock vault");
    assert!(!locked.is_locked_passphrase());
    assert_eq!(
        locked.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn settings_backup_create_now_writes_real_backup_file() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("bento-nano-settings-backup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let vault_path = dir.join("vault.bin");
    let mut vault = Vault::open(&vault_path).expect("open test vault");
    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("zh-CN")),
    );
    vault.flush().expect("seed vault");

    let backup_id = "12345-test";
    let backup_path =
        create_settings_backup_from_vault(&mut vault, backup_id).expect("create backup");

    assert_eq!(
        backup_path.file_name().and_then(|name| name.to_str()),
        Some(settings_backup_file_name(backup_id).as_str())
    );
    assert!(backup_path.exists(), "backup file must be copied to disk");
    assert_eq!(
        vault.get_setting(SETTING_BACKUP_LAST_CREATED),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static(backup_id)
        ))
    );

    let reopened = Vault::open(&backup_path).expect("open copied backup");
    assert_eq!(
        reopened.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );

    let _ = std::fs::remove_dir_all(&dir);
}
