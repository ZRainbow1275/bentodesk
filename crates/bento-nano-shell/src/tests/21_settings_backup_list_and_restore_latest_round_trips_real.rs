#[test]
fn settings_backup_list_and_restore_latest_round_trips_real_vault_file() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "bento-nano-settings-restore-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let vault_path = dir.join("vault.bin");
    let mut vault = Vault::open(&vault_path).expect("open test vault");

    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("zh-CN")),
    );
    vault.flush().expect("seed first locale");
    create_settings_backup_from_vault(&mut vault, "100-old").expect("create first backup");

    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("en-US")),
    );
    vault.flush().expect("seed second locale");
    create_settings_backup_from_vault(&mut vault, "200-new").expect("create second backup");

    let listed = list_settings_backups_for_vault_path(&vault_path).expect("list backups");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id.as_str(), "200-new");
    assert!(listed[0].size_bytes > 0);

    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("de-DE")),
    );
    vault.flush().expect("seed current drift");

    let (restored, entries) =
        restore_latest_settings_backup_from_vault(&mut vault).expect("restore latest backup");
    assert_eq!(restored.id.as_str(), "200-new");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        vault.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("en-US")
        ))
    );
    assert_eq!(
        vault.get_setting(SETTING_BACKUP_LAST_RESTORED),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("200-new")
        ))
    );

    let reopened = Vault::open(&vault_path).expect("reopen restored vault");
    assert_eq!(
        reopened.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("en-US")
        ))
    );
    assert_eq!(
        reopened.get_setting(SETTING_BACKUP_LAST_RESTORED),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("200-new")
        ))
    );

    vault.set_setting(
        "display.locale",
        bento_nano_backend::config_vault::SettingValue::Str(smol_str::SmolStr::new_static("de-DE")),
    );
    vault.flush().expect("seed current drift again");

    let (selected, selected_entries) =
        restore_settings_backup_by_id_from_vault(&mut vault, "100-old")
            .expect("restore selected backup");
    assert_eq!(selected.id.as_str(), "100-old");
    assert_eq!(selected_entries.len(), 2);
    assert_eq!(
        vault.get_setting("display.locale"),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("zh-CN")
        ))
    );
    assert_eq!(
        vault.get_setting(SETTING_BACKUP_LAST_RESTORED),
        Some(bento_nano_backend::config_vault::SettingValue::Str(
            smol_str::SmolStr::new_static("100-old")
        ))
    );
    let missing = restore_settings_backup_by_id_from_vault(&mut vault, "404-missing")
        .expect_err("missing selected backup must be visible error");
    assert_eq!(
        missing.to_string(),
        "settings backup not found: 404-missing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auxiliary_escape_does_not_hide_main_window() {
    assert_eq!(auxiliary_escape_action(WindowKind::Main, 0x1B), None);
    assert_eq!(
        auxiliary_escape_action(WindowKind::CapsulePicker, 0x1B),
        Some(AuxiliaryEscapeAction::HideAuxWindow)
    );
    assert_eq!(
        auxiliary_escape_action(WindowKind::About, 0x1B),
        Some(AuxiliaryEscapeAction::CloseAbout)
    );
    assert_eq!(auxiliary_escape_action(WindowKind::BulkManager, 0x71), None);
}
