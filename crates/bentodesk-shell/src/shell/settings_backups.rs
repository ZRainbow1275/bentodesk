//! Native shell owner: `settings_backups`.

use super::*;

pub(super) fn settings_backup_id_now() -> SmolStr {
    SmolStr::new(bentodesk_backend::time::now_compact_rfc3339())
}

pub(super) fn settings_backup_status_text(
    zh_prefix: &str,
    en_prefix: &str,
    detail: impl std::fmt::Display,
) -> SmolStr {
    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        SmolStr::new(format!("{zh_prefix}：{detail}"))
    } else {
        SmolStr::new(format!("{en_prefix}: {detail}"))
    }
}

pub(super) fn settings_backup_file_name(backup_id: &str) -> String {
    format!("vault-{backup_id}.bin")
}

#[derive(Debug)]
pub(super) struct SettingsBackupCandidate {
    path: PathBuf,
    entry: SettingsBackupEntry,
    modified: SystemTime,
}

pub(super) fn settings_backup_dir_for_vault_path(
    vault_path: &Path,
) -> Result<PathBuf, SettingsBackupError> {
    let Some(parent) = vault_path.parent() else {
        return Err(SettingsBackupError::MissingVaultParent);
    };
    Ok(parent.join("backups"))
}

pub(super) fn settings_backup_id_from_file_name(file_name: &str) -> Option<&str> {
    file_name
        .strip_prefix("vault-")
        .and_then(|value| value.strip_suffix(".bin"))
        .filter(|value| !value.is_empty())
}

pub(super) fn collect_settings_backup_candidates(
    vault_path: &Path,
) -> Result<Vec<SettingsBackupCandidate>, SettingsBackupError> {
    let backup_dir = settings_backup_dir_for_vault_path(vault_path)?;
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }
    let read_dir = std::fs::read_dir(&backup_dir).map_err(|source| SettingsBackupError::Io {
        op: "read backup dir",
        path: backup_dir.clone(),
        source,
    })?;
    let mut backups = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|source| SettingsBackupError::Io {
            op: "read backup entry",
            path: backup_dir.clone(),
            source,
        })?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(backup_id) = settings_backup_id_from_file_name(file_name) else {
            continue;
        };
        let metadata = entry.metadata().map_err(|source| SettingsBackupError::Io {
            op: "read backup metadata",
            path: path.clone(),
            source,
        })?;
        if !metadata.is_file() {
            continue;
        }
        let file_name = SmolStr::new(file_name);
        let backup_id = SmolStr::new(backup_id);
        backups.push(SettingsBackupCandidate {
            path,
            entry: SettingsBackupEntry {
                id: backup_id,
                file_name,
                size_bytes: metadata.len(),
            },
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
        });
    }
    backups.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.entry.file_name.cmp(&left.entry.file_name))
    });
    Ok(backups)
}

pub(super) fn list_settings_backups_for_vault_path(
    vault_path: &Path,
) -> Result<Vec<SettingsBackupEntry>, SettingsBackupError> {
    Ok(collect_settings_backup_candidates(vault_path)?
        .into_iter()
        .map(|candidate| candidate.entry)
        .collect())
}

pub(super) fn settings_backup_retained_count(
    vault: &bentodesk_backend::config_vault::Vault,
) -> usize {
    match vault.get_setting("backup.max_retained") {
        Some(bentodesk_backend::config_vault::SettingValue::Int(value)) => usize::try_from(value)
            .ok()
            .filter(|value| (1..=MAX_BACKUP_RETAINED).contains(value))
            .unwrap_or(DEFAULT_BACKUP_RETAINED),
        _ => DEFAULT_BACKUP_RETAINED,
    }
}

pub(super) fn create_settings_backup_from_vault(
    vault: &mut bentodesk_backend::config_vault::Vault,
    backup_id: &str,
) -> Result<PathBuf, SettingsBackupError> {
    let source = vault.path().to_path_buf();
    let backup_dir = settings_backup_dir_for_vault_path(&source)?;
    std::fs::create_dir_all(&backup_dir).map_err(|source| SettingsBackupError::Io {
        op: "create backup dir",
        path: backup_dir.clone(),
        source,
    })?;

    vault.set_setting(
        SETTING_BACKUP_LAST_CREATED,
        bentodesk_backend::config_vault::SettingValue::Str(SmolStr::new(backup_id)),
    );
    vault.flush()?;

    let backup_path = backup_dir.join(settings_backup_file_name(backup_id));
    std::fs::copy(&source, &backup_path).map_err(|source| SettingsBackupError::Io {
        op: "copy vault backup",
        path: backup_path.clone(),
        source,
    })?;
    prune_settings_backups(&backup_dir, settings_backup_retained_count(vault))?;
    Ok(backup_path)
}

pub(super) fn prune_settings_backups(
    backup_dir: &Path,
    retained: usize,
) -> Result<(), SettingsBackupError> {
    let read_dir = std::fs::read_dir(backup_dir).map_err(|source| SettingsBackupError::Io {
        op: "read backup dir",
        path: backup_dir.to_path_buf(),
        source,
    })?;
    let mut backups = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|source| SettingsBackupError::Io {
            op: "read backup entry",
            path: backup_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("vault-") && name.ends_with(".bin") {
            backups.push(path);
        }
    }
    backups.sort();
    let remove_count = backups.len().saturating_sub(retained.max(1));
    for path in backups.into_iter().take(remove_count) {
        std::fs::remove_file(&path).map_err(|source| SettingsBackupError::Io {
            op: "remove old backup",
            path,
            source,
        })?;
    }
    Ok(())
}

pub(super) fn restore_latest_settings_backup_from_vault(
    vault: &mut bentodesk_backend::config_vault::Vault,
) -> Result<(SettingsBackupEntry, Vec<SettingsBackupEntry>), SettingsBackupError> {
    let vault_path = vault.path().to_path_buf();
    let backups = collect_settings_backup_candidates(&vault_path)?;
    let Some(latest) = backups.first() else {
        return Err(SettingsBackupError::NoBackups);
    };
    let latest_id = latest.entry.id.clone();
    restore_settings_backup_by_id_from_vault(vault, latest_id.as_str())
}

pub(super) fn restore_settings_backup_by_id_from_vault(
    vault: &mut bentodesk_backend::config_vault::Vault,
    backup_id: &str,
) -> Result<(SettingsBackupEntry, Vec<SettingsBackupEntry>), SettingsBackupError> {
    let vault_path = vault.path().to_path_buf();
    let backups = collect_settings_backup_candidates(&vault_path)?;
    if backups.is_empty() {
        return Err(SettingsBackupError::NoBackups);
    }
    let Some(selected) = backups
        .iter()
        .find(|candidate| candidate.entry.id.as_str() == backup_id)
    else {
        return Err(SettingsBackupError::BackupNotFound(SmolStr::new(backup_id)));
    };
    let selected_path = selected.path.clone();
    let selected_entry = selected.entry.clone();

    bentodesk_backend::config_vault::Vault::open(&selected_path)?;
    std::fs::copy(&selected_path, &vault_path).map_err(|source| SettingsBackupError::Io {
        op: "restore backup copy",
        path: vault_path.clone(),
        source,
    })?;

    let mut restored = bentodesk_backend::config_vault::Vault::open(&vault_path)?;
    restored.set_setting(
        SETTING_BACKUP_LAST_RESTORED,
        bentodesk_backend::config_vault::SettingValue::Str(selected_entry.id.clone()),
    );
    restored.flush()?;
    *vault = restored;

    let backups = collect_settings_backup_candidates(&vault_path)?;
    Ok((
        selected_entry,
        backups
            .into_iter()
            .map(|candidate| candidate.entry)
            .collect(),
    ))
}

pub(super) fn run_settings_backup_list(root: &AppRoot) {
    let result = match bentodesk_backend::config_vault::Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(vault) => list_settings_backups_for_vault_path(vault.path()),
            Err(_poisoned) => Err(SettingsBackupError::Io {
                op: "lock backup list vault",
                path: PathBuf::from("vault mutex"),
                source: std::io::Error::other("vault mutex poisoned"),
            }),
        },
        None => Err(SettingsBackupError::Io {
            op: "list backups",
            path: PathBuf::from("global vault"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "config vault not initialised",
            ),
        }),
    };

    let app = root.app.borrow();
    match result {
        Ok(entries) => {
            app.settings_backup_entries.replace(entries);
            // Refresh-on-open is background synchronisation, not a user action.
            // Match Tauri: an ordinary successful list must not leak a green
            // diagnostic line into the Settings UI.
            app.settings_backup_status.borrow_mut().take();
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::vault",
                error = %e,
                "settings backup list failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(settings_backup_status_text(
                    "读取备份失败",
                    "Backup list failed",
                    e,
                )));
        }
    }
}

pub(super) fn run_settings_backup_restore_latest(root: &AppRoot) {
    let result = match bentodesk_backend::config_vault::Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(mut vault) => restore_latest_settings_backup_from_vault(&mut vault),
            Err(_poisoned) => Err(SettingsBackupError::Io {
                op: "lock backup restore vault",
                path: PathBuf::from("vault mutex"),
                source: std::io::Error::other("vault mutex poisoned"),
            }),
        },
        None => Err(SettingsBackupError::Io {
            op: "restore backup",
            path: PathBuf::from("global vault"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "config vault not initialised",
            ),
        }),
    };

    let restored = result.is_ok();
    if restored {
        apply_persisted_settings_from_vault(root);
    }

    let app = root.app.borrow();
    match result {
        Ok((entry, entries)) => {
            app.settings_backup_entries.replace(entries);
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(settings_backup_status_text(
                    "备份已恢复",
                    "Backup restored",
                    entry.file_name,
                )));
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::vault",
                error = %e,
                "settings backup restore failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(settings_backup_status_text(
                    "恢复备份失败",
                    "Backup restore failed",
                    e,
                )));
        }
    }
}

pub(super) fn run_settings_backup_restore_selected(root: &AppRoot, backup_id: &str) {
    let result = match bentodesk_backend::config_vault::Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(mut vault) => restore_settings_backup_by_id_from_vault(&mut vault, backup_id),
            Err(_poisoned) => Err(SettingsBackupError::Io {
                op: "lock selected backup restore vault",
                path: PathBuf::from("vault mutex"),
                source: std::io::Error::other("vault mutex poisoned"),
            }),
        },
        None => Err(SettingsBackupError::Io {
            op: "restore selected backup",
            path: PathBuf::from("global vault"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "config vault not initialised",
            ),
        }),
    };

    let restored = result.is_ok();
    if restored {
        apply_persisted_settings_from_vault(root);
    }

    let app = root.app.borrow();
    match result {
        Ok((entry, entries)) => {
            app.settings_backup_entries.replace(entries);
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(settings_backup_status_text(
                    "备份已恢复",
                    "Backup restored",
                    entry.file_name,
                )));
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::vault",
                backup_id,
                error = %e,
                "selected settings backup restore failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(settings_backup_status_text(
                    "恢复备份失败",
                    "Backup restore failed",
                    e,
                )));
        }
    }
}

pub(super) fn run_settings_backup(root: &AppRoot) {
    let result = match bentodesk_backend::config_vault::Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(mut vault) => {
                let backup_id = settings_backup_id_now();
                create_settings_backup_from_vault(&mut vault, backup_id.as_str()).and_then(|path| {
                    let entries = list_settings_backups_for_vault_path(vault.path())?;
                    Ok((path, entries))
                })
            }
            Err(_poisoned) => Err(SettingsBackupError::Io {
                op: "lock backup create vault",
                path: PathBuf::from("vault mutex"),
                source: std::io::Error::other("vault mutex poisoned"),
            }),
        },
        None => Err(SettingsBackupError::Io {
            op: "create backup",
            path: PathBuf::from("global vault"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "config vault not initialised",
            ),
        }),
    };
    let app = root.app.borrow();
    match result {
        Ok((path, entries)) => {
            tracing::info!(
                target: "bentodesk::vault",
                backup = %path.display(),
                "settings backup created"
            );
            app.settings_backup_entries.replace(entries);
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(settings_backup_status_text(
                    "备份已创建",
                    "Backup created",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("vault backup"),
                )));
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::vault",
                error = %e,
                "settings backup failed"
            );
            app.settings_backup_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Error(settings_backup_status_text(
                    "创建备份失败",
                    "Backup failed",
                    e,
                )));
        }
    }
}
