//! Synchronized disaster-recovery bundle for multi-file state.
//!
//! Per-file `.bak` recovery already protects individual JSON files. This module
//! adds an extra atomic bundle that captures the latest known-good layout,
//! settings, safety manifest, and optional icon backup together so diagnostics
//! and future recovery flows have a single coherent checkpoint.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::settings::AppSettings;
use crate::error::BentoDeskError;
use crate::hidden_items::SafetyManifest;
use crate::icon_positions::SavedIconLayout;
use crate::layout::persistence::LayoutData;
use crate::storage;
use crate::AppState;

const RECOVERY_BUNDLE_FILENAME: &str = "recovery_bundle.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryBundle {
    pub captured_at: String,
    pub data_root: String,
    pub layout: LayoutData,
    pub settings: AppSettings,
    pub manifest: Option<SafetyManifest>,
    pub icon_backup: Option<SavedIconLayout>,
}

pub struct RecoveredStartupState {
    pub settings: AppSettings,
    pub layout: LayoutData,
    pub icon_backup: Option<SavedIconLayout>,
}

pub fn bundle_path(data_root: &Path) -> PathBuf {
    data_root.join(RECOVERY_BUNDLE_FILENAME)
}

pub fn load_bundle(data_root: &Path) -> Result<Option<RecoveryBundle>, BentoDeskError> {
    storage::read_json_with_recovery(&bundle_path(data_root), "Recovery bundle")
}

pub fn data_root_from_handle(handle: &tauri::AppHandle) -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        let portable_dir = exe_path.parent().map(|p| p.join("data"));
        if let Some(ref dir) = portable_dir {
            if dir.exists() {
                return dir.clone();
            }
        }
    }

    tauri::Manager::path(handle)
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn write_bundle(data_root: &Path, bundle: &RecoveryBundle) -> Result<(), BentoDeskError> {
    storage::write_json_atomic(&bundle_path(data_root), bundle)
}

pub fn refresh_from_state(state: &AppState) -> Result<(), BentoDeskError> {
    let layout = state
        .layout
        .lock()
        .map_err(|e| {
            BentoDeskError::Generic(format!("Failed to lock layout for recovery bundle: {e}"))
        })?
        .clone();
    let settings = state
        .settings
        .lock()
        .map_err(|e| {
            BentoDeskError::Generic(format!("Failed to lock settings for recovery bundle: {e}"))
        })?
        .clone();
    let icon_backup = state
        .icon_backup
        .lock()
        .map_err(|e| {
            BentoDeskError::Generic(format!(
                "Failed to lock icon backup for recovery bundle: {e}"
            ))
        })?
        .clone();

    let data_root = data_root_from_handle(&state.app_handle);
    let manifest_path = crate::hidden_items::hidden_dir(&state.app_handle).join("manifest.json");
    let manifest =
        storage::read_json_with_recovery::<SafetyManifest>(&manifest_path, "Safety manifest")?;

    let bundle = RecoveryBundle {
        captured_at: chrono::Utc::now().to_rfc3339(),
        data_root: data_root.display().to_string(),
        layout,
        settings,
        manifest,
        icon_backup,
    };

    write_bundle(&data_root, &bundle)
}

pub fn recover_icon_backup_from_bundle(
    handle: &tauri::AppHandle,
) -> Result<Option<SavedIconLayout>, BentoDeskError> {
    let data_root = data_root_from_handle(handle);
    let Some(bundle) = load_bundle(&data_root)? else {
        return Ok(None);
    };
    let Some(icon_backup) = bundle.icon_backup else {
        return Ok(None);
    };

    crate::icon_positions::persist_to_file(&icon_backup, &data_root)?;
    Ok(Some(icon_backup))
}

pub fn heal_manifest_from_bundle_if_needed(
    data_root: &Path,
    desktop_path: &str,
) -> Result<bool, BentoDeskError> {
    let manifest_path = std::path::Path::new(desktop_path)
        .join(".bentodesk")
        .join("manifest.json");

    match storage::read_json_with_recovery::<SafetyManifest>(&manifest_path, "Safety manifest") {
        Ok(Some(_)) => return Ok(false),
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %err,
                "Safety manifest missing or unreadable at startup; attempting synchronized recovery bundle fallback"
            );
        }
    }

    let Some(bundle) = load_bundle(data_root)? else {
        return Ok(false);
    };
    let Some(manifest) = bundle.manifest else {
        return Ok(false);
    };

    crate::hidden_items::persist_manifest_snapshot_to_desktop_path(desktop_path, &manifest)?;
    Ok(true)
}

fn persist_bundle_sidecars(
    bundle: &RecoveryBundle,
    data_root: &Path,
) -> Result<(), BentoDeskError> {
    if let Some(ref icon_backup) = bundle.icon_backup {
        crate::icon_positions::persist_to_file(icon_backup, data_root)?;
    }

    if let Some(ref manifest) = bundle.manifest {
        crate::hidden_items::persist_manifest_snapshot_to_desktop_path(
            &bundle.settings.desktop_path,
            manifest,
        )?;
    }

    Ok(())
}

pub fn recover_startup_state_from_bundle(
    handle: &tauri::AppHandle,
) -> Result<Option<RecoveredStartupState>, BentoDeskError> {
    let data_root = data_root_from_handle(handle);
    let Some(bundle) = load_bundle(&data_root)? else {
        return Ok(None);
    };

    bundle.settings.save(handle)?;
    bundle.layout.save(handle)?;
    persist_bundle_sidecars(&bundle, &data_root)?;

    Ok(Some(RecoveredStartupState {
        settings: bundle.settings,
        layout: bundle.layout,
        icon_backup: bundle.icon_backup,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::SafetyProfile;
    use crate::hidden_items::ManifestEntry;
    use crate::layout::resolution::Resolution;

    fn sample_bundle(data_root: &Path) -> RecoveryBundle {
        RecoveryBundle {
            captured_at: "2026-03-27T00:00:00Z".to_string(),
            data_root: data_root.display().to_string(),
            layout: LayoutData::default(),
            settings: AppSettings {
                safety_profile: SafetyProfile::Expanded,
                ..AppSettings::default()
            },
            manifest: Some(SafetyManifest {
                schema_version: crate::hidden_items::MANIFEST_SCHEMA_VERSION.to_string(),
                entries: vec![ManifestEntry {
                    original_path: "D:/Desktop/example.txt".to_string(),
                    hidden_path: "D:/Desktop/.bentodesk/zone-a/example.txt".to_string(),
                    zone_id: "zone-a".to_string(),
                    file_size_bytes: 128,
                    hidden_at: "2026-03-27T00:00:00Z".to_string(),
                    display_name: "example.txt".to_string(),
                    icon_x: Some(10),
                    icon_y: Some(20),
                    file_type: "File".to_string(),
                }],
                zones: Vec::new(),
                screen_width: 1920,
                screen_height: 1080,
                last_updated: "2026-03-27T00:00:00Z".to_string(),
            }),
            icon_backup: Some(SavedIconLayout {
                icons: vec![crate::icon_positions::IconPosition {
                    name: "example.txt".to_string(),
                    x: 16,
                    y: 24,
                }],
                saved_at: "2026-03-27T00:00:00Z".to_string(),
                resolution: Resolution {
                    width: 1920,
                    height: 1080,
                },
                dpi: 1.0,
            }),
        }
    }

    #[test]
    fn missing_bundle_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_bundle(dir.path()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn bundle_roundtrip_preserves_optional_sections() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = sample_bundle(dir.path());

        write_bundle(dir.path(), &bundle).unwrap();
        let loaded = load_bundle(dir.path()).unwrap().expect("bundle");

        assert_eq!(loaded.captured_at, bundle.captured_at);
        assert_eq!(loaded.data_root, bundle.data_root);
        assert!(matches!(
            loaded.settings.safety_profile,
            SafetyProfile::Expanded
        ));
        assert!(loaded.manifest.is_some());
        assert!(loaded.icon_backup.is_some());
    }

    #[test]
    fn persisting_bundle_sidecars_restores_icon_backup_and_manifest() {
        let data_root = tempfile::tempdir().unwrap();
        let desktop_root = tempfile::tempdir().unwrap();
        let mut bundle = sample_bundle(data_root.path());
        bundle.settings.desktop_path = desktop_root.path().display().to_string();

        persist_bundle_sidecars(&bundle, data_root.path()).unwrap();

        let icon_backup_path = crate::icon_positions::backup_file_path(data_root.path());
        assert!(icon_backup_path.exists());
        let restored_icon_backup: SavedIconLayout =
            serde_json::from_str(&std::fs::read_to_string(icon_backup_path).unwrap()).unwrap();
        assert_eq!(
            restored_icon_backup.icons.len(),
            bundle.icon_backup.as_ref().unwrap().icons.len()
        );
        assert_eq!(restored_icon_backup.icons[0].name, "example.txt");

        let manifest_path = desktop_root.path().join(".bentodesk").join("manifest.json");
        assert!(manifest_path.exists());
        let restored_manifest: SafetyManifest =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        assert_eq!(
            restored_manifest.entries.len(),
            bundle.manifest.as_ref().unwrap().entries.len()
        );
        assert_eq!(restored_manifest.entries[0].display_name, "example.txt");
    }

    #[test]
    fn heal_manifest_from_bundle_if_needed_restores_missing_manifest() {
        let data_root = tempfile::tempdir().unwrap();
        let desktop_root = tempfile::tempdir().unwrap();
        let mut bundle = sample_bundle(data_root.path());
        bundle.settings.desktop_path = desktop_root.path().display().to_string();

        write_bundle(data_root.path(), &bundle).unwrap();

        let healed =
            heal_manifest_from_bundle_if_needed(data_root.path(), &bundle.settings.desktop_path)
                .unwrap();

        assert!(healed);
        let manifest_path = desktop_root.path().join(".bentodesk").join("manifest.json");
        assert!(manifest_path.exists());

        let restored_manifest: SafetyManifest =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        assert_eq!(restored_manifest.entries.len(), 1);
        assert_eq!(restored_manifest.entries[0].display_name, "example.txt");
    }
}
