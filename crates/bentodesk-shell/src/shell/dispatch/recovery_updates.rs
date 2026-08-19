//! Command handlers for the `recovery_updates` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    _hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::CreateSettingsBackup => {
            run_settings_backup(root);
            effects.needs_redraw = true;
        }
        Command::ListSettingsBackups => {
            run_settings_backup_list(root);
            effects.needs_redraw = true;
        }
        Command::RestoreLatestSettingsBackup => {
            run_settings_backup_restore_latest(root);
            effects.needs_redraw = true;
        }
        Command::RestoreSettingsBackup(backup_id) => {
            run_settings_backup_restore_selected(root, backup_id.as_str());
            effects.needs_redraw = true;
        }
        Command::CreateRecoveryBundle => {
            run_recovery_bundle_capture(root);
            effects.needs_redraw = true;
        }
        Command::ExportRecoveryDiagnostics => {
            run_recovery_diagnostics_export(root);
            effects.needs_redraw = true;
        }
        Command::RestoreRecoveryBundle => {
            run_recovery_bundle_restore(root);
            effects.needs_redraw = true;
        }
        Command::SetEncryptionPassphrase(passphrase) => {
            let result = match bentodesk_backend::config_vault::Vault::global() {
                Some(mtx) => match mtx.lock() {
                    Ok(mut vault) => persist_passphrase_to_vault(&mut vault, passphrase.as_str()),
                    Err(_poisoned) => {
                        Err(bentodesk_backend::config_vault::VaultError::NoPassphraseSet)
                    }
                },
                None => Err(bentodesk_backend::config_vault::VaultError::NoPassphraseSet),
            };
            let app = root.app.borrow();
            match result {
                Ok(()) => {
                    app.encryption_mode.set(SettingsEncryptionMode::Passphrase);
                    app.passphrase_unlock_required.set(false);
                    app.settings_encryption_status.borrow_mut().replace(
                        SettingsBackupStatus::Success(localized_current(
                            "口令已验证",
                            "Passphrase verified",
                        )),
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "bentodesk::vault",
                        error = %e,
                        "SetEncryptionPassphrase failed"
                    );
                    app.settings_encryption_status.borrow_mut().replace(
                        SettingsBackupStatus::Error(localized_current(
                            format!("口令设置失败：{e}"),
                            format!("Passphrase failed: {e}"),
                        )),
                    );
                }
            }
            effects.needs_redraw = true;
        }
        Command::UnlockEncryptionPassphrase(passphrase) => {
            let result = match bentodesk_backend::config_vault::Vault::global() {
                Some(mtx) => match mtx.lock() {
                    Ok(mut vault) => unlock_passphrase_vault(&mut vault, passphrase.as_str()),
                    Err(_poisoned) => {
                        Err(bentodesk_backend::config_vault::VaultError::NoPassphraseSet)
                    }
                },
                None => Err(bentodesk_backend::config_vault::VaultError::NoPassphraseSet),
            };
            if result.is_ok() {
                apply_persisted_settings_from_vault(root);
            }
            let app = root.app.borrow();
            match result {
                Ok(()) => {
                    app.encryption_mode.set(SettingsEncryptionMode::Passphrase);
                    app.passphrase_unlock_required.set(false);
                    app.settings_encryption_status.borrow_mut().replace(
                        SettingsBackupStatus::Success(localized_current(
                            "口令已解锁",
                            "Passphrase unlocked",
                        )),
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "bentodesk::vault",
                        error = %e,
                        "UnlockEncryptionPassphrase failed"
                    );
                    app.encryption_mode.set(SettingsEncryptionMode::Passphrase);
                    app.passphrase_unlock_required.set(true);
                    app.settings_encryption_status.borrow_mut().replace(
                        SettingsBackupStatus::Error(localized_current(
                            format!("口令解锁失败：{e}"),
                            format!("Passphrase unlock failed: {e}"),
                        )),
                    );
                }
            }
            effects.needs_redraw = true;
        }
        Command::CheckForUpdates => {
            let start = root.updater.try_start_check();
            match start {
                bentodesk_backend::updater::UpdateStart::Started
                | bentodesk_backend::updater::UpdateStart::Joined(
                    bentodesk_backend::updater::UpdateOperation::Checking,
                ) => {
                    let app = root.app.borrow();
                    *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Checking;
                    log_static("updater: CheckForUpdates started/joined\n");
                }
                bentodesk_backend::updater::UpdateStart::Rejected(operation)
                | bentodesk_backend::updater::UpdateStart::Joined(operation) => {
                    log_static(
                        format!("updater: CheckForUpdates rejected operation={operation:?}\n")
                            .as_str(),
                    );
                }
            }
            effects.needs_redraw = true;
        }
        Command::DownloadUpdate => {
            let start = root.updater.try_start_download();
            match start {
                bentodesk_backend::updater::UpdateStart::Started
                | bentodesk_backend::updater::UpdateStart::Joined(
                    bentodesk_backend::updater::UpdateOperation::Downloading,
                ) => {
                    let app = root.app.borrow();
                    *app.settings_updater_status.borrow_mut() =
                        SettingsUpdaterStatus::Downloading {
                            chunk_len: 0,
                            total_bytes: None,
                        };
                    log_static("updater: DownloadUpdate started/joined\n");
                }
                bentodesk_backend::updater::UpdateStart::Rejected(operation)
                | bentodesk_backend::updater::UpdateStart::Joined(operation) => {
                    log_static(
                        format!("updater: DownloadUpdate rejected operation={operation:?}\n")
                            .as_str(),
                    );
                }
            }
            effects.needs_redraw = true;
        }
        Command::InstallUpdateAndRestart => {
            let result = root.updater.install();
            let _ = drain_updater_events(root);
            match result {
                Ok(()) => {
                    log_static("updater: InstallUpdateAndRestart launched installer\n");
                    effects.quit_after_drain = true;
                }
                Err(error) => {
                    let app = root.app.borrow();
                    set_update_error(
                        &app,
                        "无法安装更新",
                        "Update installation unavailable",
                        &error,
                    );
                    log_static(
                        format!("updater: InstallUpdateAndRestart failed error={error}\n").as_str(),
                    );
                }
            }
            effects.needs_redraw = true;
        }
        Command::SkipUpdateVersion(version) => {
            if version.as_str().is_empty() {
                let app = root.app.borrow();
                *app.settings_updater_status.borrow_mut() =
                    SettingsUpdaterStatus::Error(localized_current(
                        "当前没有可跳过的更新版本",
                        "No update version is available to skip",
                    ));
            } else {
                root.updater.skip_version(version.clone());
                persist_skipped_update_to_vault(&version);
                let app = root.app.borrow();
                *app.settings_updater_status.borrow_mut() =
                    SettingsUpdaterStatus::Skipped { version };
            }
            effects.needs_redraw = true;
        }
        _ => unreachable!("command routed to the wrong recovery_updates dispatcher"),
    }
}
