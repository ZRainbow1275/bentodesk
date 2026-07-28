//! Native shell owner: `recovery_commands`.

use super::*;

pub(super) fn run_startup_recovery_bundle_heal(root: &AppRoot, zones_path: &Path) {
    let result = startup_heal_recovery_bundle(root, zones_path);
    let app = root.app.borrow();
    match result {
        Ok(Some(outcome)) => {
            let summary = &outcome.summary;
            let (
                icon_restore_attempted,
                icons_restored,
                icons_skipped,
                icons_failed,
                auto_arrange_toggled,
                icon_restore_error,
            ) = outcome.icon_restore.log_fields();
            tracing::info!(
                target: "bentodesk::recovery",
                path = %summary.path.display(),
                zones = summary.zone_count,
                vault = summary.vault_included,
                manifests = summary.safety_manifest_count,
                icon_backup = summary.icon_backup_included,
                user_data_files = summary.user_data_file_count,
                user_data_bytes = summary.user_data_len_bytes,
                user_data_restored = outcome.user_data_restore.restored_files,
                icon_restore_attempted,
                icons_restored,
                icons_skipped,
                icons_failed,
                auto_arrange_toggled,
                icon_restore_error = %icon_restore_error,
                "startup recovery bundle heal restored state"
            );
            log_static(
                format!(
                    "recovery: startup heal restored zones={} bundle={}{}{}\n",
                    summary.zone_count,
                    summary.path.display(),
                    recovery_user_data_status_suffix(outcome.user_data_restore),
                    outcome.icon_restore.status_suffix()
                )
                .as_str(),
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    format!(
                        "启动恢复已完成：{} 个区域{}{}",
                        summary.zone_count,
                        localized_recovery_user_data_status_suffix(outcome.user_data_restore),
                        outcome.icon_restore.localized_status_suffix()
                    ),
                    format!(
                        "Startup recovery restored: {} zone(s){}{}",
                        summary.zone_count,
                        localized_recovery_user_data_status_suffix(outcome.user_data_restore),
                        outcome.icon_restore.localized_status_suffix()
                    ),
                )));
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::recovery",
                error = %error,
                "startup recovery bundle heal failed"
            );
            log_static(format!("recovery: startup heal failed error={error}\n").as_str());
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(localized_current(
                    format!("启动恢复失败：{error}"),
                    format!("Startup recovery failed: {error}"),
                )));
        }
    }
}

pub(super) fn run_recovery_bundle_capture(root: &AppRoot) {
    let result = capture_recovery_bundle(root);
    let app = root.app.borrow();
    match result {
        Ok(summary) => {
            tracing::info!(
                target: "bentodesk::recovery",
                path = %summary.path.display(),
                zones = summary.zone_count,
                bytes = summary.zones_len_bytes,
                checksum = %summary.zones_checksum,
                manifests = summary.safety_manifest_count,
                icon_backup = summary.icon_backup_included,
                user_data_files = summary.user_data_file_count,
                user_data_bytes = summary.user_data_len_bytes,
                "recovery bundle captured"
            );
            log_static(
                format!(
                    "recovery: capture bundle zones={} bundle={} icon_backup={} user_data_files={} user_data_bytes={}\n",
                    summary.zone_count,
                    summary.path.display(),
                    summary.icon_backup_included,
                    summary.user_data_file_count,
                    summary.user_data_len_bytes
                )
                .as_str(),
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    format!(
                        "恢复包已保存：{} 个区域{}{}{}{}",
                        summary.zone_count,
                        if summary.vault_included {
                            " + 设置"
                        } else {
                            ""
                        },
                        if summary.safety_manifest_count > 0 {
                            " + 安全清单"
                        } else {
                            ""
                        },
                        if summary.icon_backup_included {
                            " + 图标"
                        } else {
                            ""
                        },
                        if summary.user_data_file_count > 0 {
                            " + 用户数据"
                        } else {
                            ""
                        }
                    ),
                    format!(
                        "Bundle saved: {} zone(s){}{}{}{}",
                        summary.zone_count,
                        if summary.vault_included {
                            " + settings"
                        } else {
                            ""
                        },
                        if summary.safety_manifest_count > 0 {
                            " + manifest"
                        } else {
                            ""
                        },
                        if summary.icon_backup_included {
                            " + icons"
                        } else {
                            ""
                        },
                        if summary.user_data_file_count > 0 {
                            " + user data"
                        } else {
                            ""
                        }
                    ),
                )));
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::recovery",
                error = %error,
                "recovery bundle capture failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(localized_current(
                    format!("恢复包保存失败：{error}"),
                    format!("Bundle failed: {error}"),
                )));
        }
    }
}

pub(super) fn run_recovery_diagnostics_export(root: &AppRoot) {
    let result = export_recovery_diagnostics(root);
    let app = root.app.borrow();
    match result {
        Ok(report) => {
            tracing::info!(
                target: "bentodesk::recovery",
                path = %report.diagnostics_path,
                bundle = %report.bundle_path,
                zones = report.zones.zone_count,
                bytes = report.zones.len_bytes,
                checksum = %report.zones.checksum,
                manifests = report.safety_manifests.len(),
                icon_backup = report.icon_backup.included,
                user_data_files = report.user_data_files.len(),
                "recovery diagnostics exported"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    format!(
                        "诊断已导出：{} 个区域{}{}{}{}",
                        report.zones.zone_count,
                        if report.vault.included {
                            " + 设置"
                        } else {
                            ""
                        },
                        if report.safety_manifests.is_empty() {
                            ""
                        } else {
                            " + 安全清单"
                        },
                        if report.icon_backup.included {
                            " + 图标"
                        } else {
                            ""
                        },
                        if report.user_data_files.is_empty() {
                            ""
                        } else {
                            " + 用户数据"
                        }
                    ),
                    format!(
                        "Diagnostics exported: {} zone(s){}{}{}{}",
                        report.zones.zone_count,
                        if report.vault.included {
                            " + settings"
                        } else {
                            ""
                        },
                        if report.safety_manifests.is_empty() {
                            ""
                        } else {
                            " + manifest"
                        },
                        if report.icon_backup.included {
                            " + icons"
                        } else {
                            ""
                        },
                        if report.user_data_files.is_empty() {
                            ""
                        } else {
                            " + user data"
                        }
                    ),
                )));
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::recovery",
                error = %error,
                "recovery diagnostics export failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(localized_current(
                    format!("诊断导出失败：{error}"),
                    format!("Diagnostics failed: {error}"),
                )));
        }
    }
}

pub(super) fn run_recovery_bundle_restore(root: &AppRoot) {
    let result = restore_recovery_bundle(root);
    let app = root.app.borrow();
    match result {
        Ok(outcome) => {
            let summary = &outcome.summary;
            let (
                icon_restore_attempted,
                icons_restored,
                icons_skipped,
                icons_failed,
                auto_arrange_toggled,
                icon_restore_error,
            ) = outcome.icon_restore.log_fields();
            tracing::info!(
                target: "bentodesk::recovery",
                path = %summary.path.display(),
                zones = summary.zone_count,
                checksum = %summary.zones_checksum,
                manifests = summary.safety_manifest_count,
                icon_backup = summary.icon_backup_included,
                user_data_files = summary.user_data_file_count,
                user_data_bytes = summary.user_data_len_bytes,
                user_data_restored = outcome.user_data_restore.restored_files,
                icon_restore_attempted,
                icons_restored,
                icons_skipped,
                icons_failed,
                auto_arrange_toggled,
                icon_restore_error = %icon_restore_error,
                "recovery bundle restored"
            );
            log_static(
                format!(
                    "recovery: restore bundle zones={} bundle={}{}{}\n",
                    summary.zone_count,
                    summary.path.display(),
                    recovery_user_data_status_suffix(outcome.user_data_restore),
                    outcome.icon_restore.status_suffix()
                )
                .as_str(),
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    format!(
                        "恢复包已载入：{} 个区域{}{}{}{}{}",
                        summary.zone_count,
                        if summary.vault_included {
                            " + 设置"
                        } else {
                            ""
                        },
                        if summary.safety_manifest_count > 0 {
                            " + 安全清单"
                        } else {
                            ""
                        },
                        if summary.icon_backup_included {
                            " + 图标"
                        } else {
                            ""
                        },
                        localized_recovery_user_data_status_suffix(outcome.user_data_restore),
                        outcome.icon_restore.localized_status_suffix()
                    ),
                    format!(
                        "Bundle restored: {} zone(s){}{}{}{}{}",
                        summary.zone_count,
                        if summary.vault_included {
                            " + settings"
                        } else {
                            ""
                        },
                        if summary.safety_manifest_count > 0 {
                            " + manifest"
                        } else {
                            ""
                        },
                        if summary.icon_backup_included {
                            " + icons"
                        } else {
                            ""
                        },
                        localized_recovery_user_data_status_suffix(outcome.user_data_restore),
                        outcome.icon_restore.localized_status_suffix()
                    ),
                )));
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::recovery",
                error = %error,
                "recovery bundle restore failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(localized_current(
                    format!("恢复包载入失败：{error}"),
                    format!("Bundle restore failed: {error}"),
                )));
        }
    }
}

pub(super) fn backend_setting_value_from_app(
    value: &bentodesk_app::SettingValue,
) -> bentodesk_backend::config_vault::SettingValue {
    match value {
        bentodesk_app::SettingValue::Bool(value) => {
            bentodesk_backend::config_vault::SettingValue::Bool(*value)
        }
        bentodesk_app::SettingValue::Int(value) => {
            bentodesk_backend::config_vault::SettingValue::Int(*value)
        }
        bentodesk_app::SettingValue::Float(value) => {
            bentodesk_backend::config_vault::SettingValue::Float(*value)
        }
        bentodesk_app::SettingValue::Str(value) => {
            bentodesk_backend::config_vault::SettingValue::Str(value.clone())
        }
    }
}

pub(super) fn next_palette_accent(current: Option<&str>) -> Option<SmolStr> {
    let table = palette_picker::swatch_table();
    match current {
        None => table.first().map(|swatch| swatch.hex.clone()),
        Some(value) => match table.iter().position(|swatch| swatch.hex.as_str() == value) {
            Some(idx) if idx + 1 < table.len() => Some(table[idx + 1].hex.clone()),
            Some(_) => None,
            None => table.first().map(|swatch| swatch.hex.clone()),
        },
    }
}

pub(super) const DEFAULT_ACCENT_COLORREF: COLORREF = 0x00F6_823B;

pub(super) fn accent_hex_to_colorref(hex: &str) -> Option<COLORREF> {
    let bytes = hex.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return None;
    }
    let r = hex_pair_to_u8(bytes[1], bytes[2])?;
    let g = hex_pair_to_u8(bytes[3], bytes[4])?;
    let b = hex_pair_to_u8(bytes[5], bytes[6])?;
    Some(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

pub(super) fn hex_pair_to_u8(high: u8, low: u8) -> Option<u8> {
    Some((hex_nibble(high)? << 4) | hex_nibble(low)?)
}

pub(super) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn colorref_to_accent_hex(color: COLORREF) -> SmolStr {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let r = (color & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = ((color >> 16) & 0xFF) as u8;
    let text = [
        b'#',
        HEX[(r >> 4) as usize],
        HEX[(r & 0x0F) as usize],
        HEX[(g >> 4) as usize],
        HEX[(g & 0x0F) as usize],
        HEX[(b >> 4) as usize],
        HEX[(b & 0x0F) as usize],
    ];
    // SAFETY: `text` is built only from `#` and lowercase ASCII hex nibbles.
    let hex = unsafe { core::str::from_utf8_unchecked(&text) };
    SmolStr::new(hex)
}

pub(super) fn settings_accent_custom_colors() -> [COLORREF; 16] {
    let mut colors = [DEFAULT_ACCENT_COLORREF; 16];
    let mut idx = 0usize;
    while idx < bentodesk_app::theme_picker::ACCENT_SWATCH_COUNT {
        if let Some(hex) = bentodesk_app::theme_picker::accent_swatch_hex(idx)
            && let Some(color) = accent_hex_to_colorref(hex)
        {
            colors[idx] = color;
        }
        idx += 1;
    }
    colors
}

pub(super) fn choose_native_accent_color(hwnd: HWND, initial_hex: &str) -> Option<SmolStr> {
    let mut custom_colors = settings_accent_custom_colors();
    let initial = accent_hex_to_colorref(initial_hex).unwrap_or(DEFAULT_ACCENT_COLORREF);
    // SAFETY: CHOOSECOLORW is a plain C struct. We set lStructSize and every
    // pointer field that ChooseColorW reads; `custom_colors` stays live until
    // the modal dialog returns.
    let mut dialog = unsafe { core::mem::zeroed::<CHOOSECOLORW>() };
    dialog.lStructSize = core::mem::size_of::<CHOOSECOLORW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.rgbResult = initial;
    dialog.lpCustColors = custom_colors.as_mut_ptr();
    dialog.Flags = CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR;

    // SAFETY: dialog points to a valid CHOOSECOLORW and its custom-colour
    // array remains allocated for the whole synchronous common-dialog call.
    let accepted = unsafe { ChooseColorW(&mut dialog) };
    if accepted != 0 {
        return Some(colorref_to_accent_hex(dialog.rgbResult));
    }

    // SAFETY: CommDlgExtendedError is the documented way to distinguish a
    // cancelled common dialog from a ChooseColorW failure.
    let error = unsafe { CommDlgExtendedError() };
    if error != 0 {
        log_static(format!("settings: native accent picker failed error={error}\n").as_str());
    } else {
        log_static("settings: native accent picker cancelled\n");
    }
    None
}

pub(super) fn open_settings_native_accent_picker(root: &AppRoot, hwnd: HWND) -> bool {
    let initial = {
        let app = root.app.borrow();
        app.settings_accent_editor_value()
    };
    let selected = choose_native_accent_color(hwnd, initial.as_str());
    arm_settings_owned_dialog_release_guard(root);
    let Some(hex) = selected else {
        return false;
    };
    log_static(format!("settings: OpenAccentColorPicker selected={hex}\n").as_str());
    {
        let app = root.app.borrow();
        app.set_settings_accent_color_from_picker(hex);
        app.passphrase_entry_active.set(false);
    }
    true
}
