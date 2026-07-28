//! Native shell owner: `recovery_vault`.

use super::*;

pub(super) fn recovery_vault_snapshot_from_vault(
    vault: &mut bentodesk_backend::config_vault::Vault,
) -> Result<Option<(PathBuf, Vec<u8>)>, RecoveryBundleShellError> {
    vault.flush()?;
    let path = vault.path().to_path_buf();
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|source| RecoveryBundleShellError::Io {
        op: "read recovery vault",
        path: path.clone(),
        source,
    })?;
    Ok(Some((path, bytes)))
}

pub(super) fn recovery_vault_snapshot()
-> Result<Option<(PathBuf, Vec<u8>)>, RecoveryBundleShellError> {
    let Some(mtx) = bentodesk_backend::config_vault::Vault::global() else {
        return Ok(None);
    };
    let mut vault = mtx
        .lock()
        .map_err(|_poisoned| RecoveryBundleShellError::Io {
            op: "lock recovery vault",
            path: PathBuf::from("vault mutex"),
            source: std::io::Error::other("vault mutex poisoned"),
        })?;
    recovery_vault_snapshot_from_vault(&mut vault)
}

pub(super) fn restore_recovery_vault_payload_to_vault(
    vault: &mut bentodesk_backend::config_vault::Vault,
    vault_bin: &[u8],
) -> Result<(), RecoveryBundleShellError> {
    let vault_path = vault.path().to_path_buf();
    let temp_path = vault_path.with_extension("recovery.tmp");
    std::fs::write(&temp_path, vault_bin).map_err(|source| RecoveryBundleShellError::Io {
        op: "write recovery vault temp",
        path: temp_path.clone(),
        source,
    })?;
    let _validated = bentodesk_backend::config_vault::Vault::open(&temp_path)?;
    std::fs::copy(&temp_path, &vault_path).map_err(|source| RecoveryBundleShellError::Io {
        op: "restore recovery vault",
        path: vault_path.clone(),
        source,
    })?;
    let _ = std::fs::remove_file(&temp_path);
    let restored = bentodesk_backend::config_vault::Vault::open(&vault_path)?;
    *vault = restored;
    Ok(())
}

pub(super) fn restore_recovery_vault_payload(
    payload: &bentodesk_backend::recovery_bundle::RecoveredVaultPayload,
) -> Result<(), RecoveryBundleShellError> {
    let Some(mtx) = bentodesk_backend::config_vault::Vault::global() else {
        return Err(RecoveryBundleShellError::MissingVault);
    };
    let mut vault = mtx
        .lock()
        .map_err(|_poisoned| RecoveryBundleShellError::Io {
            op: "lock recovery vault restore",
            path: PathBuf::from("vault mutex"),
            source: std::io::Error::other("vault mutex poisoned"),
        })?;
    restore_recovery_vault_payload_to_vault(&mut vault, &payload.vault_bin)
}

pub(super) fn recovery_manifest_snapshots(
    root: &AppRoot,
) -> Result<Vec<bentodesk_backend::recovery_bundle::RecoverySafetyManifest>, RecoveryBundleShellError>
{
    let (zones, app_data_dir) = {
        let app = root.app.borrow();
        let app_data_dir = app
            .zones_path
            .parent()
            .map(|path| path.to_path_buf())
            .ok_or(RecoveryBundleShellError::MissingZonesPath)?;
        (app.zones.clone(), app_data_dir)
    };
    let mut desktop_paths: Vec<PathBuf> = Vec::new();
    for zone in zones.iter() {
        for item in &zone.items {
            let Some(original_path) = item.original_path.as_deref() else {
                continue;
            };
            let Some(parent) = Path::new(original_path).parent() else {
                continue;
            };
            let desktop_path = parent.to_path_buf();
            if !desktop_paths
                .iter()
                .any(|existing| existing == &desktop_path)
            {
                desktop_paths.push(desktop_path);
            }
        }
    }

    let mut snapshots = Vec::new();
    for desktop_path in desktop_paths {
        let manifest_dir = desktop_path.join(".bentodesk");
        if !manifest_dir.join("manifest.json").exists() {
            continue;
        }
        let manifest = bentodesk_backend::stealth::load_manifest(&manifest_dir)?;
        let config = bentodesk_backend::stealth::StealthConfig {
            desktop_path: SmolStr::new(desktop_path.to_string_lossy().as_ref()),
            app_data_dir: SmolStr::new(app_data_dir.to_string_lossy().as_ref()),
        };
        let expected_manifest_dir = bentodesk_backend::stealth::hidden_dir_for(&config)?;
        if expected_manifest_dir != manifest_dir {
            continue;
        }
        snapshots.push(bentodesk_backend::recovery_bundle::RecoverySafetyManifest {
            desktop_path: desktop_path.display().to_string(),
            manifest,
        });
    }
    Ok(snapshots)
}

pub(super) fn restore_recovery_safety_manifests(
    manifests: &[bentodesk_backend::recovery_bundle::RecoverySafetyManifest],
) -> Result<(), RecoveryBundleShellError> {
    for snapshot in manifests {
        bentodesk_backend::stealth::persist_manifest_snapshot(
            &snapshot.desktop_path,
            &snapshot.manifest,
        )?;
    }
    Ok(())
}

pub(super) fn recovery_icon_backup(
    data_root: &Path,
) -> Result<Option<bentodesk_backend::icon_positions::SavedIconLayout>, RecoveryBundleShellError> {
    bentodesk_backend::icon_positions::load_from_file(data_root).map_err(Into::into)
}

pub(super) fn restore_recovery_icon_backup_with<F>(
    data_root: &Path,
    icon_backup: Option<&bentodesk_backend::icon_positions::SavedIconLayout>,
    restore_layout: F,
) -> RecoveryIconRestoreOutcome
where
    F: FnOnce(
        &bentodesk_backend::icon_positions::SavedIconLayout,
    ) -> Result<
        bentodesk_backend::icon_positions::RestoreResult,
        bentodesk_backend::icon_positions::IconPositionError,
    >,
{
    let Some(icon_backup) = icon_backup else {
        return RecoveryIconRestoreOutcome::NotIncluded;
    };

    if let Err(error) = bentodesk_backend::icon_positions::persist_to_file(icon_backup, data_root) {
        return RecoveryIconRestoreOutcome::Failed(SmolStr::new(format!("sidecar: {error}")));
    }

    match restore_layout(icon_backup) {
        Ok(result) => RecoveryIconRestoreOutcome::Restored(result),
        Err(error) => RecoveryIconRestoreOutcome::Failed(SmolStr::new(format!("live: {error}"))),
    }
}

pub(super) fn restore_recovery_icon_backup(
    data_root: &Path,
    icon_backup: Option<&bentodesk_backend::icon_positions::SavedIconLayout>,
) -> RecoveryIconRestoreOutcome {
    restore_recovery_icon_backup_with(
        data_root,
        icon_backup,
        bentodesk_backend::icon_positions::restore_layout,
    )
}

pub(super) fn capture_recovery_bundle(
    root: &AppRoot,
) -> Result<bentodesk_backend::recovery_bundle::RecoveryBundleSummary, RecoveryBundleShellError> {
    let (zones_path, zones, zone_count) = {
        let app = root.app.borrow();
        if app.zones_path.as_os_str().is_empty() {
            return Err(RecoveryBundleShellError::MissingZonesPath);
        }
        let zone_count = match u32::try_from(app.zones.len()) {
            Ok(count) => count,
            Err(_) => return Err(RecoveryBundleShellError::ZoneCountOverflow(app.zones.len())),
        };
        (app.zones_path.clone(), app.zones.clone(), zone_count)
    };
    let data_root = bentodesk_backend::recovery_bundle::data_root_for_state_file(&zones_path)?;
    let zones_bin = storage::encode(&zones);
    let vault_snapshot = recovery_vault_snapshot()?;
    let vault_payload = vault_snapshot
        .as_ref()
        .map(|(path, bytes)| (path.as_path(), bytes.as_slice()));
    let safety_manifests = recovery_manifest_snapshots(root)?;
    let icon_backup = recovery_icon_backup(&data_root)?;
    let user_data_files = bentodesk_backend::recovery_bundle::collect_user_data_files(&data_root)?;
    bentodesk_backend::recovery_bundle::refresh_bundle_with_user_data(
        &data_root,
        &zones_path,
        &zones_bin,
        zone_count,
        bentodesk_backend::recovery_bundle::RecoveryBundleSidecars {
            vault: vault_payload,
            safety_manifests: &safety_manifests,
            icon_backup,
            user_data_files: &user_data_files,
        },
    )
    .map_err(Into::into)
}

pub(super) fn restore_recovery_bundle(
    root: &AppRoot,
) -> Result<RecoveryBundleRestoreOutcome, RecoveryBundleShellError> {
    let zones_path = {
        let app = root.app.borrow();
        if app.zones_path.as_os_str().is_empty() {
            return Err(RecoveryBundleShellError::MissingZonesPath);
        }
        app.zones_path.clone()
    };
    let data_root = bentodesk_backend::recovery_bundle::data_root_for_state_file(&zones_path)?;
    let payload = bentodesk_backend::recovery_bundle::recover_zones_payload(&data_root)?
        .ok_or(RecoveryBundleShellError::MissingBundle)?;
    let restored_zones = storage::decode(&payload.zones_bin)?;
    if let Some(vault_payload) = payload.vault.as_ref() {
        restore_recovery_vault_payload(vault_payload)?;
    }
    restore_recovery_safety_manifests(&payload.safety_manifests)?;
    let icon_restore = restore_recovery_icon_backup(&data_root, payload.icon_backup.as_ref());
    let user_data_restore = bentodesk_backend::recovery_bundle::restore_user_data_files(
        &data_root,
        &payload.user_data_files,
    )?;
    let vault_restored = payload.vault.is_some();
    {
        let mut app = root.app.borrow_mut();
        app.zones = restored_zones;
        bump_next_zone_id_from_zones(&app);
        app.mark_dirty();
    }
    if vault_restored {
        apply_persisted_settings_from_vault(root);
    }
    Ok(RecoveryBundleRestoreOutcome {
        summary: payload.summary,
        icon_restore,
        user_data_restore,
    })
}

pub(super) fn export_recovery_diagnostics(
    root: &AppRoot,
) -> Result<bentodesk_backend::recovery_bundle::RecoveryDiagnosticsReport, RecoveryBundleShellError>
{
    let zones_path = {
        let app = root.app.borrow();
        if app.zones_path.as_os_str().is_empty() {
            return Err(RecoveryBundleShellError::MissingZonesPath);
        }
        app.zones_path.clone()
    };
    let data_root = bentodesk_backend::recovery_bundle::data_root_for_state_file(&zones_path)?;
    bentodesk_backend::recovery_bundle::export_diagnostics_report(&data_root)?
        .ok_or(RecoveryBundleShellError::MissingBundle)
}

pub(super) fn startup_heal_recovery_bundle(
    root: &AppRoot,
    zones_path: &Path,
) -> Result<Option<RecoveryBundleRestoreOutcome>, RecoveryBundleShellError> {
    let needs_heal = if zones_path.exists() {
        match storage::read_zones(zones_path) {
            Ok(_) => false,
            Err(PlatformError::Storage(_)) => {
                storage::quarantine_corrupt(zones_path)?;
                true
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        true
    };
    if !needs_heal {
        return Ok(None);
    }
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.to_path_buf();
    }
    let outcome = match restore_recovery_bundle(root) {
        Ok(outcome) => outcome,
        Err(RecoveryBundleShellError::MissingBundle) => return Ok(None),
        Err(error) => return Err(error),
    };
    {
        let app = root.app.borrow();
        storage::write_zones_atomic(&app.zones_path, &app.zones)?;
        app.dirty.set(false);
    }
    Ok(Some(outcome))
}
