//! Native shell owner: `persisted_settings`.

use super::*;

pub(super) fn current_minibar_pins_csv(root: &AppRoot) -> Option<SmolStr> {
    let app = root.app.borrow();
    let minibars = app.minibars.borrow();
    let mut csv = String::new();
    for (zone_id, _) in minibars.iter() {
        if !csv.is_empty() {
            csv.push(',');
        }
        let _ = core::fmt::Write::write_fmt(&mut csv, format_args!("{}", zone_id.0));
    }
    if csv.is_empty() {
        None
    } else {
        Some(SmolStr::new(csv))
    }
}

pub(super) fn list_pinned_minibar_labels(
    root: &AppRoot,
) -> smallvec::SmallVec<[SmolStr; BUSINESS_MAX_MINIBARS]> {
    let app = root.app.borrow();
    app.minibars
        .borrow()
        .iter()
        .map(|(zone_id, _bar)| SmolStr::new(format!("minibar-{}", zone_id.0)))
        .collect()
}

pub(super) fn show_pinned_minibar_list_status(root: &AppRoot) {
    let labels = list_pinned_minibar_labels(root);
    if labels.is_empty() {
        set_item_operation_status(
            root,
            localized_current("暂无固定迷你栏", "No pinned minibars"),
        );
        log_static("minibar: ListPinnedMinibars status=No pinned minibars\n");
        return;
    }
    let mut status = localized_current("已固定迷你栏：", "Pinned minibars: ").to_string();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            status.push_str(", ");
        }
        status.push_str(label.as_str());
    }
    log_static(format!("minibar: ListPinnedMinibars status={status}\n").as_str());
    set_item_operation_status(root, SmolStr::new(status));
}

pub(super) fn write_minibar_pins_to_vault(
    root: &AppRoot,
    vault: &mut bento_nano_backend::config_vault::Vault,
) {
    match current_minibar_pins_csv(root) {
        Some(value) => vault.set_setting(
            SETTING_MINIBAR_PINNED_ZONES,
            bento_nano_backend::config_vault::SettingValue::Str(value),
        ),
        None => {
            let _ = vault.remove_setting(SETTING_MINIBAR_PINNED_ZONES);
        }
    }
}

pub(super) fn parse_minibar_pin_ids(
    value: &str,
) -> smallvec::SmallVec<[ZoneId; BUSINESS_MAX_MINIBARS]> {
    let mut ids = smallvec::SmallVec::<[ZoneId; BUSINESS_MAX_MINIBARS]>::new();
    for raw in value.split(',') {
        if ids.len() >= BUSINESS_MAX_MINIBARS {
            break;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(id) = trimmed.parse::<u64>() else {
            tracing::warn!(
                target: "bentodesk::minibar",
                value = trimmed,
                "minibar.pinned_zones restore skipped: invalid zone id"
            );
            continue;
        };
        let zone_id = ZoneId(id);
        if zone_id == ZoneId::INVALID || ids.contains(&zone_id) {
            continue;
        }
        ids.push(zone_id);
    }
    ids
}

pub(super) fn restore_minibar_pins_from_wire_value<F>(
    root: &AppRoot,
    value: &str,
    mut pin_zone: F,
) -> usize
where
    F: FnMut(&AppRoot, ZoneId) -> bool,
{
    let mut restored = 0usize;
    for zone_id in parse_minibar_pin_ids(value) {
        if pin_zone(root, zone_id) {
            restored += 1;
        }
    }
    restored
}

pub(super) fn persist_minibar_pins_to_vault(root: &AppRoot) {
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        tracing::warn!(
            target: "bentodesk::minibar",
            "minibar pins not persisted: vault not initialised"
        );
        return;
    };
    match mtx.lock() {
        Ok(mut vault) => {
            if vault.is_locked_passphrase() {
                tracing::warn!(
                    target: "bentodesk::minibar",
                    "minibar pins not persisted: vault locked"
                );
                return;
            }
            write_minibar_pins_to_vault(root, &mut vault);
            if let Err(error) = vault.flush() {
                tracing::warn!(
                    target: "bentodesk::minibar",
                    error = %error,
                    "minibar pins flush failed"
                );
            }
        }
        Err(_poisoned) => {
            tracing::warn!(
                target: "bentodesk::minibar",
                "minibar pins not persisted: vault mutex poisoned"
            );
        }
    }
}

pub(super) fn restore_persisted_minibars_from_vault(root: &AppRoot) {
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        return;
    };
    let setting = match mtx.lock() {
        Ok(vault) if vault.is_locked_passphrase() => return,
        Ok(vault) => vault.get_setting(SETTING_MINIBAR_PINNED_ZONES),
        Err(_poisoned) => {
            tracing::warn!(
                target: "bentodesk::minibar",
                "minibar restore skipped: vault mutex poisoned"
            );
            return;
        }
    };
    match setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(value)) => {
            let _ = restore_minibar_pins_from_wire_value(root, value.as_str(), pin_zone_as_minibar);
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::minibar",
                "minibar.pinned_zones restore skipped: non-string setting"
            );
        }
        None => {}
    }
}

pub(super) fn apply_persisted_settings_from_vault(root: &AppRoot) {
    if let Err(error) = apply_available_themes_to_app(root) {
        set_theme_setting_error(
            root,
            localized_current(
                format!("无法恢复主题列表：{error}"),
                format!("Theme list restore failed: {error}"),
            ),
        );
    }
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        return;
    };
    let Ok(vault) = mtx.lock() else {
        tracing::warn!(
            target: "bentodesk::vault",
            "settings restore skipped: vault mutex poisoned"
        );
        return;
    };
    let locale_setting = vault.get_setting(SETTING_DISPLAY_LOCALE);
    let update_frequency_setting = vault.get_setting(SETTING_UPDATES_CHECK_FREQUENCY);
    let update_auto_download_setting = vault.get_setting(SETTING_UPDATES_AUTO_DOWNLOAD);
    let update_skipped_version_setting = vault.get_setting(SETTING_UPDATES_SKIPPED_VERSION);
    let stealth_enabled_setting = vault.get_setting(SETTING_STEALTH_ENABLED);
    let encryption_mode_setting = vault.get_setting(SETTING_ENCRYPTION_MODE);
    let theme_base_accent_setting = vault.get_setting(SETTING_THEME_BASE_ACCENT);
    let active_theme_setting = vault.get_setting(SETTING_ACTIVE_THEME);
    let zone_display_mode_setting = vault.get_setting(SETTING_ZONE_DISPLAY_MODE);
    let debug_overlay_setting = vault.get_setting(SETTING_DEBUG_OVERLAY);
    let keybinding_settings = hotkey::HOTKEY_ACTIONS
        .iter()
        .filter_map(|action| {
            let key = format!("{KEYBINDING_PREFIX}{action}");
            match vault.get_setting(&key) {
                Some(bento_nano_backend::config_vault::SettingValue::Str(chord)) => {
                    Some((*action, chord))
                }
                Some(_) => {
                    tracing::warn!(
                        target: "bentodesk::hotkey",
                        %key,
                        "keybinding restore skipped: non-string setting"
                    );
                    None
                }
                None => None,
            }
        })
        .collect::<smallvec::SmallVec<[(&'static str, SmolStr); 8]>>();
    let vault_is_locked_passphrase = vault.is_locked_passphrase();
    drop(vault);

    if vault_is_locked_passphrase {
        let app = root.app.borrow();
        app.encryption_mode.set(SettingsEncryptionMode::Passphrase);
        app.passphrase_unlock_required.set(true);
        app.settings_encryption_status
            .borrow_mut()
            .replace(SettingsBackupStatus::Error(localized_current(
                "需要输入口令解锁设置",
                "Passphrase unlock required",
            )));
        return;
    }

    for (action, chord) in keybinding_settings {
        let _ = apply_hotkey_binding(root, action, chord.as_str());
    }

    match locale_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(locale)) => {
            if !apply_locale_wire(locale.as_str()) {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %locale,
                    "display.locale restore skipped: unsupported locale value"
                );
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "display.locale restore skipped: non-string setting"
            );
        }
        None => {}
    }

    let app = root.app.borrow();
    app.passphrase_unlock_required.set(false);
    match update_frequency_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(frequency)) => {
            if let Some(parsed) = update_frequency_from_wire(frequency.as_str()) {
                app.update_check_frequency.set(parsed);
            } else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %frequency,
                    "updates.check_frequency restore skipped: unsupported value"
                );
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "updates.check_frequency restore skipped: non-string setting"
            );
        }
        None => {}
    }
    match update_auto_download_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Bool(value)) => {
            app.update_auto_download.set(value);
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "updates.auto_download restore skipped: non-bool setting"
            );
        }
        None => {}
    }
    match update_skipped_version_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(version)) => {
            root.updater.skip_version(version.clone());
            *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Skipped { version };
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "updates.skipped_version restore skipped: non-string setting"
            );
        }
        None => {}
    }
    match stealth_enabled_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Bool(value)) => {
            app.stealth_enabled.set(value);
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "stealth.enabled restore skipped: non-bool setting"
            );
        }
        None => {}
    }
    match debug_overlay_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Bool(value)) => {
            app.debug_overlay.borrow_mut().visible = value;
            log_static(format!("debug_overlay: restored persisted visible={value}\n").as_str());
            tracing::info!(
                target: "bentodesk::debug_overlay",
                "debug_overlay: restored persisted visible={}",
                value
            );
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "debug_overlay restore skipped: non-bool setting"
            );
        }
        None => {}
    }
    match encryption_mode_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(mode)) => {
            if let Some(parsed) = encryption_mode_from_wire(mode.as_str()) {
                app.encryption_mode.set(parsed);
            } else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %mode,
                    "encryption.mode restore skipped: unsupported or deferred value"
                );
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "encryption.mode restore skipped: non-string setting"
            );
        }
        None => {}
    }
    match theme_base_accent_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(accent)) => {
            if let Some(parsed) = theme_base_accent_from_wire(accent.as_str()) {
                let _changed = apply_theme_base_accent_to_app(&app, Some(parsed));
            } else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %accent,
                    "theme.base_accent restore skipped: unsupported palette swatch"
                );
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "theme.base_accent restore skipped: non-string setting"
            );
        }
        None => {}
    }
    match zone_display_mode_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(mode)) => {
            if let Some(parsed) = zone_display_mode_from_wire(mode.as_str()) {
                app.zone_display_mode.set(parsed);
                log_static(format!("zone_display_mode restored: {}\n", parsed.as_wire()).as_str());
            } else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %mode,
                    "zone_display_mode restore skipped: unsupported value"
                );
                app.settings_theme_status
                    .borrow_mut()
                    .replace(SettingsBackupStatus::Error(localized_current(
                        format!("已忽略无效的显示模式：{mode}"),
                        format!("Display mode ignored: {mode}"),
                    )));
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "zone_display_mode restore skipped: non-string setting"
            );
        }
        None => {}
    }
    drop(app);
    match active_theme_setting {
        Some(bento_nano_backend::config_vault::SettingValue::Str(theme_id)) => {
            if let Err(error) = apply_active_theme_to_app(root, theme_id.clone()) {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %theme_id,
                    error = %error,
                    "active_theme restore skipped"
                );
                set_theme_setting_error(
                    root,
                    localized_current(
                        format!("已忽略无效主题：{theme_id}"),
                        format!("Theme ignored: {theme_id}"),
                    ),
                );
            }
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                "active_theme restore skipped: non-string setting"
            );
        }
        None => {}
    }
    // M1a 2026-05-29 — restore the 5 General-section toggles from the
    // vault. Calling this AFTER the locale / updater / theme branches keeps
    // the warn-on-poisoned-vault flow consistent with the existing reads
    // and lets a partial-failure (e.g., missing keys) fall through to the
    // AppState defaults defined in `AppState::new`.
    apply_general_settings_from_vault(&root.app.borrow());
}

pub(super) fn apply_locale_wire(locale: &str) -> bool {
    match locale {
        "en-US" => {
            bento_nano_style::set_locale(&bento_nano_style::EN_US);
            true
        }
        "zh-CN" => {
            bento_nano_style::set_locale(&bento_nano_style::ZH_CN);
            true
        }
        _ => false,
    }
}

pub(super) fn apply_setting_value_to_app(
    app: &AppState,
    key: &str,
    value: &bento_nano_app::SettingValue,
) -> bool {
    match (key, value) {
        (SETTING_UPDATES_CHECK_FREQUENCY, bento_nano_app::SettingValue::Str(frequency)) => {
            let Some(parsed) = update_frequency_from_wire(frequency.as_str()) else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %frequency,
                    "updates.check_frequency rejected: unsupported value"
                );
                return false;
            };
            let changed = app.update_check_frequency.get() != parsed;
            app.update_check_frequency.set(parsed);
            changed
        }
        (SETTING_UPDATES_AUTO_DOWNLOAD, bento_nano_app::SettingValue::Bool(value)) => {
            let changed = app.update_auto_download.get() != *value;
            app.update_auto_download.set(*value);
            changed
        }
        (SETTING_STEALTH_ENABLED, bento_nano_app::SettingValue::Bool(value)) => {
            let changed = app.stealth_enabled.get() != *value;
            app.stealth_enabled.set(*value);
            changed
        }
        (SETTING_DEBUG_OVERLAY, bento_nano_app::SettingValue::Bool(value)) => {
            let mut overlay = app.debug_overlay.borrow_mut();
            let changed = overlay.visible != *value;
            overlay.visible = *value;
            changed
        }
        (SETTING_ENCRYPTION_MODE, bento_nano_app::SettingValue::Str(mode)) => {
            let Some(parsed) = encryption_mode_from_wire(mode.as_str()) else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %mode,
                    "encryption.mode rejected: unsupported or deferred value"
                );
                return false;
            };
            if parsed == SettingsEncryptionMode::Passphrase {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %mode,
                    "encryption.mode rejected: Passphrase requires dedicated command"
                );
                return false;
            }
            let changed = app.encryption_mode.get() != parsed;
            app.encryption_mode.set(parsed);
            changed
        }
        (SETTING_ZONE_DISPLAY_MODE, bento_nano_app::SettingValue::Str(mode)) => {
            let Some(parsed) = zone_display_mode_from_wire(mode.as_str()) else {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %mode,
                    "zone_display_mode rejected: unsupported value"
                );
                app.settings_theme_status
                    .borrow_mut()
                    .replace(SettingsBackupStatus::Error(localized_current(
                        format!("不支持的显示模式：{mode}"),
                        format!("Display mode rejected: {mode}"),
                    )));
                return false;
            };
            let changed = app.set_zone_display_mode(parsed);
            app.settings_theme_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    format!("显示模式：{}", parsed.label()),
                    format!("Display mode: {}", parsed.label()),
                )));
            changed
        }
        _ => false,
    }
}

pub(super) fn apply_update_event_to_app(app: &AppState, event: UpdateEvent) {
    let next_status = match event {
        UpdateEvent::Available { info } => SettingsUpdaterStatus::Available {
            version: info.version,
        },
        UpdateEvent::Progress { progress } => SettingsUpdaterStatus::Downloading {
            chunk_len: progress.chunk_len,
            total_bytes: progress.total_bytes,
        },
        UpdateEvent::Ready { info } => SettingsUpdaterStatus::Ready {
            version: info.version,
        },
        UpdateEvent::Installing { info } => SettingsUpdaterStatus::Installing {
            version: info.version,
        },
        UpdateEvent::Error { kind, message } => SettingsUpdaterStatus::Error(localized_current(
            format!("更新失败（{kind}）：{message}"),
            format!("{kind}: {message}"),
        )),
    };
    *app.settings_updater_status.borrow_mut() = next_status;
}

pub(super) fn updater_event_should_auto_download(app: &AppState, event: &UpdateEvent) -> bool {
    app.update_auto_download.get() && matches!(event, UpdateEvent::Available { .. })
}

pub(super) fn set_update_error(
    app: &AppState,
    action_zh: &str,
    action_en: &str,
    error: &dyn core::fmt::Display,
) {
    *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Error(localized_current(
        format!("{action_zh}：{error}"),
        format!("{action_en}: {error}"),
    ));
}

pub(super) fn persist_setting_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    key: &str,
    value: &bento_nano_app::SettingValue,
) -> Result<bool, bento_nano_backend::config_vault::VaultError> {
    if vault.is_locked_passphrase() {
        return Err(bento_nano_backend::config_vault::VaultError::NoPassphraseSet);
    }
    let backend_value = backend_setting_value_from_app(value);
    if key == SETTING_ENCRYPTION_MODE {
        let Some(mode) = (match value {
            bento_nano_app::SettingValue::Str(mode) => encryption_mode_from_wire(mode.as_str()),
            _ => None,
        }) else {
            tracing::warn!(
                target: "bentodesk::vault",
                %key,
                "SetSetting rejected: encryption.mode requires None/Dpapi/Passphrase string"
            );
            return Ok(false);
        };
        if mode == SettingsEncryptionMode::Passphrase {
            tracing::warn!(
                target: "bentodesk::vault",
                %key,
                "SetSetting rejected: Passphrase requires SetEncryptionPassphrase"
            );
            return Ok(false);
        }
        vault.set_setting(key, backend_value);
        vault.set_mode(backend_encryption_mode_from_app(mode))?;
        vault.flush()?;
        return Ok(true);
    }
    if key == SETTING_ZONE_DISPLAY_MODE {
        let Some(mode) = (match value {
            bento_nano_app::SettingValue::Str(mode) => zone_display_mode_from_wire(mode.as_str()),
            _ => None,
        }) else {
            tracing::warn!(
                target: "bentodesk::vault",
                %key,
                "SetSetting rejected: zone_display_mode requires hover/always/click string"
            );
            return Ok(false);
        };
        return persist_zone_display_mode_to_vault(vault, mode);
    }

    vault.set_setting(key, backend_value);
    vault.flush()?;
    Ok(true)
}

pub(super) fn persist_passphrase_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    passphrase: &str,
) -> Result<(), bento_nano_backend::config_vault::VaultError> {
    vault.set_setting(
        SETTING_ENCRYPTION_MODE,
        bento_nano_backend::config_vault::SettingValue::Str(SmolStr::new_static("Passphrase")),
    );
    vault.set_mode(
        bento_nano_backend::config_vault::EncryptionMode::Passphrase {
            passphrase: SmolStr::new(passphrase),
        },
    )?;
    vault.flush()?;
    let reopened =
        bento_nano_backend::config_vault::Vault::open_with_passphrase(vault.path(), passphrase)?;
    *vault = reopened;
    Ok(())
}

pub(super) fn unlock_passphrase_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    passphrase: &str,
) -> Result<(), bento_nano_backend::config_vault::VaultError> {
    let reopened =
        bento_nano_backend::config_vault::Vault::open_with_passphrase(vault.path(), passphrase)?;
    *vault = reopened;
    Ok(())
}
