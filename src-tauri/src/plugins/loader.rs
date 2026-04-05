//! Plugin installation, uninstallation, and lifecycle management.
//!
//! `.bdplugin` files are ZIP archives containing a `manifest.json` at their
//! root plus any plugin-specific assets (e.g. `theme.json` for theme plugins).
//! Installation extracts to a temporary directory, validates, then atomically
//! renames into place.

use std::path::{Path, PathBuf};

use crate::error::BentoDeskError;

use super::manifest::{PluginManifest, PluginType};
use super::registry::{InstalledPlugin, PluginRegistry};

/// Directory within app_data where plugins are extracted.
fn plugins_dir(app_data: &Path) -> PathBuf {
    app_data.join("plugins")
}

/// Install a plugin from a `.bdplugin` (ZIP) file.
///
/// Steps:
/// 1. Open and validate the ZIP archive
/// 2. Extract to a temporary directory (with zip-slip protection)
/// 3. Read and validate `manifest.json`
/// 4. Check for ID conflicts in the registry
/// 5. For Theme plugins, verify `theme.json` exists
/// 6. Rename temp dir to final install path
/// 7. Update the registry
pub fn install_from_zip(
    zip_path: &Path,
    app_data: &Path,
) -> Result<InstalledPlugin, BentoDeskError> {
    // 1. Open ZIP
    let zip_file = std::fs::File::open(zip_path).map_err(|e| {
        BentoDeskError::PluginError(format!(
            "Cannot open plugin file '{}': {e}",
            zip_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| {
        BentoDeskError::PluginError(format!(
            "Invalid plugin archive '{}': {e}",
            zip_path.display()
        ))
    })?;

    // 2. Extract to temp directory
    let tmp_id = uuid::Uuid::new_v4();
    let tmp_dir = plugins_dir(app_data).join(format!(".tmp-{tmp_id}"));
    std::fs::create_dir_all(&tmp_dir)?;

    let extract_result = extract_zip_safely(&mut archive, &tmp_dir);
    if let Err(e) = extract_result {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    // 3. Read and validate manifest.json
    let manifest_path = tmp_dir.join("manifest.json");
    let manifest = match read_and_validate_manifest(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    // 4. Check ID conflict
    let mut registry = PluginRegistry::load(app_data)?;
    if registry.find(&manifest.id).is_some() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(BentoDeskError::PluginError(format!(
            "Plugin '{}' is already installed",
            manifest.id
        )));
    }

    // 5. Theme-specific: verify theme.json exists
    if manifest.plugin_type == PluginType::Theme {
        let theme_json = tmp_dir.join("theme.json");
        if !theme_json.exists() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(BentoDeskError::PluginError(
                "Theme plugin must contain a theme.json file".into(),
            ));
        }
    }

    // 6. Rename temp dir to final install path
    let final_dir = plugins_dir(app_data).join(&manifest.id);
    if final_dir.exists() {
        // Stale directory from a previous failed install — clean it up.
        std::fs::remove_dir_all(&final_dir).map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            BentoDeskError::PluginError(format!(
                "Cannot clean stale plugin directory '{}': {e}",
                final_dir.display()
            ))
        })?;
    }
    if let Err(e) = std::fs::rename(&tmp_dir, &final_dir) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(BentoDeskError::PluginError(format!(
            "Failed to finalize plugin installation: {e}"
        )));
    }

    // 7. Update registry
    let installed = InstalledPlugin {
        id: manifest.id.clone(),
        name: manifest.name,
        version: manifest.version,
        plugin_type: manifest.plugin_type,
        author: manifest.author,
        description: manifest.description,
        enabled: true,
        installed_at: chrono::Utc::now().to_rfc3339(),
        install_path: final_dir.to_string_lossy().into_owned(),
    };
    registry.plugins.push(installed.clone());
    registry.save(app_data)?;

    tracing::info!("Plugin '{}' installed successfully", manifest.id);
    Ok(installed)
}

/// Uninstall a plugin by ID.
///
/// Removes the plugin directory and its registry entry.
pub fn uninstall(id: &str, app_data: &Path) -> Result<(), BentoDeskError> {
    let mut registry = PluginRegistry::load(app_data)?;

    let plugin = registry.find(id).ok_or_else(|| {
        BentoDeskError::PluginError(format!("Plugin '{id}' is not installed"))
    })?;

    // Remove plugin directory
    let install_path = PathBuf::from(&plugin.install_path);
    if install_path.exists() {
        std::fs::remove_dir_all(&install_path).map_err(|e| {
            BentoDeskError::PluginError(format!(
                "Failed to remove plugin directory '{}': {e}",
                install_path.display()
            ))
        })?;
    }

    // Remove from registry and save
    registry.remove(id);
    registry.save(app_data)?;

    tracing::info!("Plugin '{id}' uninstalled successfully");
    Ok(())
}

/// Toggle a plugin's enabled state.
pub fn toggle_enabled(
    id: &str,
    enabled: bool,
    app_data: &Path,
) -> Result<InstalledPlugin, BentoDeskError> {
    let mut registry = PluginRegistry::load(app_data)?;

    let plugin = registry.find_mut(id).ok_or_else(|| {
        BentoDeskError::PluginError(format!("Plugin '{id}' is not installed"))
    })?;

    plugin.enabled = enabled;
    let updated = plugin.clone();
    registry.save(app_data)?;

    tracing::info!("Plugin '{id}' enabled={enabled}");
    Ok(updated)
}

// ─── Helpers ───────────────────────────────────────────────────

/// Extract all entries from a ZIP archive into `dest_dir` with zip-slip
/// protection: every extracted path must resolve to within `dest_dir`.
fn extract_zip_safely(
    archive: &mut zip::ZipArchive<std::fs::File>,
    dest_dir: &Path,
) -> Result<(), BentoDeskError> {
    let canonical_dest = std::fs::canonicalize(dest_dir).map_err(|e| {
        BentoDeskError::PluginError(format!(
            "Cannot canonicalize dest dir '{}': {e}",
            dest_dir.display()
        ))
    })?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| {
            BentoDeskError::PluginError(format!("Cannot read ZIP entry {i}: {e}"))
        })?;

        let entry_name = entry
            .enclosed_name()
            .ok_or_else(|| {
                BentoDeskError::PluginError(format!(
                    "ZIP entry has unsafe path: {}",
                    entry.name()
                ))
            })?
            .to_owned();

        let target = dest_dir.join(&entry_name);

        // Zip-slip defence: canonicalize the parent and ensure it's within dest.
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Verify the resolved path is within the destination.
            let canonical_parent = std::fs::canonicalize(
                target.parent().unwrap_or(dest_dir),
            )
            .map_err(|e| {
                BentoDeskError::PluginError(format!(
                    "Cannot canonicalize path for '{}': {e}",
                    entry_name.display()
                ))
            })?;
            if !canonical_parent.starts_with(&canonical_dest) {
                return Err(BentoDeskError::PluginError(format!(
                    "Zip-slip detected: entry '{}' escapes destination directory",
                    entry_name.display()
                )));
            }

            let mut outfile = std::fs::File::create(&target)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}

/// Read `manifest.json` from a plugin directory, deserialize, and validate.
fn read_and_validate_manifest(path: &Path) -> Result<PluginManifest, BentoDeskError> {
    if !path.exists() {
        return Err(BentoDeskError::PluginError(
            "Plugin archive must contain a manifest.json at its root".into(),
        ));
    }

    let content = std::fs::read_to_string(path).map_err(|e| {
        BentoDeskError::PluginError(format!("Cannot read manifest.json: {e}"))
    })?;

    let manifest: PluginManifest = serde_json::from_str(&content).map_err(|e| {
        BentoDeskError::PluginError(format!("Invalid manifest.json: {e}"))
    })?;

    manifest.validate()?;
    Ok(manifest)
}
