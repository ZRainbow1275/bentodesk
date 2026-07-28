//! Legacy stealth-layout migration and startup reconciliation.

use super::*;

// ─── Legacy migration ───────────────────────────────────────────────────

/// Resolve the old `hidden_items/` storage directory under app data.
fn legacy_hidden_items_dir(app_data: &Path) -> PathBuf {
    app_data.join("hidden_items")
}

/// Old manifest entry from the AppData/hidden_items/ "move-mode" era.
#[derive(Debug, Clone, Deserialize)]
struct LegacyMoveManifestEntry {
    original_path: String,
    hidden_path: String,
    #[serde(rename = "hidden_at")]
    _hidden_at: String,
}

/// Migrate from BOTH old architectures:
///
/// 1. AppData/hidden_items/ directory (old "file move" mode)
/// 2. Files hidden via `attrib +h +s` (old "reference mode")
///
/// `attrib_items` is the caller-provided set of layout entries that may
/// be in attrib-mode (1.x walked `state.layout` directly; the nano port
/// expects the caller to pre-collect candidates). Returns `(migrated,
/// new_hidden_paths)` so the caller can update its layout store with the
/// rewritten `hidden_path` values.
pub fn cleanup_legacy_hidden_dir(
    config: &StealthConfig,
    attrib_items: &[(String, String, String)],
) -> MigrationResult {
    let mut total_migrated = 0u32;
    let mut new_paths: Vec<MigrationUpdate> = Vec::new();

    // -- Phase 1: AppData/hidden_items/ "move mode" ------------------
    let old_dir = legacy_hidden_items_dir(config.app_data_path());
    if old_dir.exists() {
        tracing::info!("=== Legacy migration Phase 1: AppData/hidden_items/ ===");
        total_migrated += migrate_old_move_dir(config, &old_dir)?;
    }

    // -- Phase 2: attrib-mode files in the layout --------------------
    let (phase2_count, phase2_paths) = migrate_attrib_hidden_files(config, attrib_items)?;
    total_migrated += phase2_count;
    new_paths.extend(phase2_paths);

    // -- Phase 3: Clean old manifest.json from app data dir ----------
    let app_data_manifest = config.app_data_path().join("manifest.json");
    if app_data_manifest.exists() {
        tracing::info!("Removing old app-data manifest.json");
        let _ = std::fs::remove_file(&app_data_manifest);
        let _ = std::fs::remove_file(app_data_manifest.with_extension("json.tmp"));
    }

    if total_migrated > 0 {
        tracing::info!(
            "=== Legacy migration complete: {} files migrated total ===",
            total_migrated
        );
    }

    Ok((total_migrated, new_paths))
}

fn migrate_old_move_dir(config: &StealthConfig, old_dir: &Path) -> Result<u32, StealthError> {
    let old_manifest = load_legacy_move_manifest(old_dir);

    let entries = match std::fs::read_dir(old_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Cannot read legacy hidden_items/ directory: {}", e);
            return Ok(0);
        }
    };

    let desktop = PathBuf::from(config.desktop_path.as_str());
    let mut migrated = 0u32;

    for entry in entries.flatten() {
        let file_path = entry.path();

        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name == "manifest.json" || name == "manifest.json.tmp" {
                continue;
            }
        }
        if file_path.is_dir() {
            continue;
        }

        let file_path_str = file_path.to_string_lossy().into_owned();

        let original = old_manifest
            .iter()
            .find(|e| paths_equal_str(&e.hidden_path, &file_path_str))
            .map(|e| e.original_path.clone());

        let dest = if let Some(orig) = &original {
            PathBuf::from(orig)
        } else {
            match file_path.file_name() {
                Some(name) => desktop.join(name),
                None => {
                    tracing::warn!(
                        "Legacy migration: cannot determine destination for {}",
                        file_path.display()
                    );
                    continue;
                }
            }
        };

        if dest.exists() {
            tracing::warn!(
                "Legacy migration: destination already exists, skipping: {}",
                dest.display()
            );
            continue;
        }

        if let Some(parent) = dest.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        let success = match std::fs::rename(&file_path, &dest) {
            Ok(()) => true,
            Err(rename_err) => match std::fs::copy(&file_path, &dest) {
                Ok(_) => match std::fs::remove_file(&file_path) {
                    Ok(()) => true,
                    Err(rm_err) => {
                        tracing::error!(
                            "Legacy migration: copy ok but delete of source failed for {}: {} (rename was: {}). Removing copy to stay safe.",
                            file_path.display(),
                            rm_err,
                            rename_err
                        );
                        if let Err(cleanup_err) = std::fs::remove_file(&dest) {
                            tracing::error!(
                                "Legacy migration: failed to clean up orphan copy at {}: {} — manual cleanup required",
                                dest.display(),
                                cleanup_err
                            );
                        }
                        false
                    }
                },
                Err(e) => {
                    tracing::error!(
                        "Legacy migration failed: {} -> {}: {}",
                        file_path.display(),
                        dest.display(),
                        e
                    );
                    false
                }
            },
        };

        if success {
            tracing::info!(
                "Legacy Phase1 migrated: {} -> {}",
                file_path.display(),
                dest.display()
            );
            migrated += 1;
        }
    }

    if migrated > 0 || is_dir_empty_except_manifest(old_dir) {
        let _ = std::fs::remove_file(old_dir.join("manifest.json"));
        let _ = std::fs::remove_file(old_dir.join("manifest.json.tmp"));
        match std::fs::remove_dir(old_dir) {
            Ok(()) => tracing::info!("Removed legacy hidden_items/ directory"),
            Err(e) => tracing::warn!("Could not remove legacy hidden_items/ directory: {}", e),
        }
    }

    Ok(migrated)
}

/// Migrate attrib-mode files into subfolder mode.
///
/// `attrib_items` is `(zone_id, item_id, original_path)` triples for layout
/// entries the caller has already filtered to attrib mode. Returns the
/// migrated count + the list of `(zone_id, item_id, new_hidden_path)` so
/// the caller can update its layout store.
fn migrate_attrib_hidden_files(
    config: &StealthConfig,
    attrib_items: &[(String, String, String)],
) -> MigrationResult {
    if attrib_items.is_empty() {
        return Ok((0, Vec::new()));
    }

    tracing::info!(
        "=== Legacy migration Phase 2: migrating {} attrib-hidden files to subfolder mode ===",
        attrib_items.len()
    );

    let mut migrated = 0u32;
    let mut updates: Vec<MigrationUpdate> = Vec::new();

    for (zone_id, item_id, orig_path) in attrib_items {
        // Remove hidden attribute by clearing the bits (no `attrib.exe` spawn).
        clear_hidden_attribute(Path::new(orig_path));

        let source = Path::new(orig_path);
        if !source.exists() {
            tracing::warn!(
                "Attrib migration: file disappeared after unhide: {}",
                orig_path
            );
            continue;
        }

        let zone_dir = match crate::stealth::hide::zone_hidden_dir_for(config, zone_id) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Attrib migration: zone_hidden_dir_for failed: {e}");
                continue;
            }
        };
        let dest = crate::stealth::hide::unique_hidden_path(&zone_dir, source);

        let success = match std::fs::rename(source, &dest) {
            Ok(()) => true,
            Err(rename_err) => match std::fs::copy(source, &dest) {
                Ok(_) => match std::fs::remove_file(source) {
                    Ok(()) => true,
                    Err(rm_err) => {
                        tracing::error!(
                            "Attrib migration: copy ok but delete of source failed for {}: {} (rename was: {}). Removing copy to stay safe.",
                            orig_path,
                            rm_err,
                            rename_err
                        );
                        if let Err(cleanup_err) = std::fs::remove_file(&dest) {
                            tracing::error!(
                                "Attrib migration: failed to clean up orphan copy at {:?}: {} — manual cleanup required",
                                dest,
                                cleanup_err
                            );
                        }
                        false
                    }
                },
                Err(copy_err) => {
                    tracing::error!(
                        "Attrib migration failed for {}: rename={} copy={}",
                        orig_path,
                        rename_err,
                        copy_err
                    );
                    false
                }
            },
        };

        if success {
            let hidden_path_str = dest.to_string_lossy().into_owned();
            let file_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            let display_name = Path::new(orig_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let global_dir = crate::stealth::hide::hidden_dir_for(config)?;
            if let Err(e) = manifest_add(
                &global_dir,
                ManifestAddParams {
                    original_path: orig_path,
                    hidden_path: &hidden_path_str,
                    zone_id,
                    file_size_bytes: file_size,
                    display_name: &display_name,
                    icon_x: None,
                    icon_y: None,
                    file_type: "",
                },
            ) {
                tracing::warn!("Attrib migration: manifest_add failed: {e}");
            }
            tracing::info!(
                "Attrib migration: {} -> {} (zone={})",
                orig_path,
                dest.display(),
                zone_id
            );
            updates.push((zone_id.clone(), item_id.clone(), hidden_path_str));
            migrated += 1;
        }
    }

    Ok((migrated, updates))
}

/// Clear `FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM` from `path` via
/// direct `SetFileAttributesW` call. Replaces 1.x's `attrib.exe` spawn.
///
/// Uses `windows-sys` (plain Win32) per spec §3.1.1.
#[cfg(windows)]
fn clear_hidden_attribute(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, SetFileAttributesW};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide` is null-terminated UTF-16 valid for the call.
    let existing = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if existing == crate::stealth::INVALID_FILE_ATTRIBUTES {
        return;
    }

    // Clear HIDDEN (0x2) + SYSTEM (0x4) bits while preserving everything else.
    let new_attrs = existing & !(0x0000_0002u32 | 0x0000_0004u32);
    if new_attrs == existing {
        return;
    }

    // SAFETY: `wide` valid for the call; flag mask is a plain u32. Result
    // ignored — best-effort un-hide before the move.
    let _ = unsafe { SetFileAttributesW(wide.as_ptr(), new_attrs) };
}

#[cfg(not(windows))]
fn clear_hidden_attribute(_path: &Path) {}

/// Load the old-format manifest from the AppData/hidden_items/ directory.
fn load_legacy_move_manifest(hidden_dir: &Path) -> Vec<LegacyMoveManifestEntry> {
    let path = hidden_dir.join("manifest.json");
    if !path.exists() {
        return Vec::new();
    }

    #[derive(Debug, Deserialize)]
    struct LegacyManifest {
        entries: Vec<LegacyMoveManifestEntry>,
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<LegacyManifest>(&content)
            .map(|m| m.entries)
            .unwrap_or_else(|e| {
                tracing::warn!("Could not parse legacy manifest: {}", e);
                Vec::new()
            }),
        Err(e) => {
            tracing::warn!("Could not read legacy manifest: {}", e);
            Vec::new()
        }
    }
}

fn is_dir_empty_except_manifest(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name != "manifest.json" && name != "manifest.json.tmp" {
                        return false;
                    }
                }
            }
            true
        }
        Err(_) => false,
    }
}

/// Reapply hidden-folder attributes on startup. Returns the count of files
/// currently inside `.bentodesk/` (across all zone subdirs) for logging.
pub fn reapply_hidden_on_startup(config: &StealthConfig) -> Result<u32, StealthError> {
    let hdir = crate::stealth::hide::hidden_dir_for(config)?;
    ensure_stealth(&hdir);

    let mut count = 0u32;
    if let Ok(entries) = std::fs::read_dir(&hdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    count += sub_entries.flatten().filter(|e| !e.path().is_dir()).count() as u32;
                }
            } else if name_str != "manifest.json"
                && name_str != "manifest.json.tmp"
                && name_str != "manifest.json.bak"
            {
                count += 1;
            }
        }
    }

    if count > 0 {
        tracing::info!(
            "Startup: .bentodesk/ contains {} hidden files (across zone subdirs), folder attrib +h +s ensured",
            count
        );
    }

    Ok(count)
}

/// Migrate flat `.bentodesk/` files into zone subdirectories.
///
/// `hidden_to_zone` is the caller-provided lookup `(hidden_path, zone_id)`
/// derived from the layout. Returns the migrated count + the list of
/// `(old_hidden_path, new_hidden_path, zone_id)` tuples so the caller can
/// rewrite layout entries.
pub fn migrate_flat_to_zone_dirs(
    config: &StealthConfig,
    hidden_to_zone: &[(String, String)],
) -> MigrationResult {
    let hdir = crate::stealth::hide::hidden_dir_for(config)?;

    let flat_files: Vec<PathBuf> = match std::fs::read_dir(&hdir) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| {
                let path = e.path();
                if path.is_dir() {
                    return false;
                }
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                name_str != "manifest.json"
                    && name_str != "manifest.json.tmp"
                    && name_str != "manifest.json.bak"
            })
            .map(|e| e.path())
            .collect(),
        Err(_) => return Ok((0, Vec::new())),
    };

    if flat_files.is_empty() {
        return Ok((0, Vec::new()));
    }

    tracing::info!(
        "=== Zone isolation migration: {} flat files to migrate ===",
        flat_files.len()
    );

    let mut migrated = 0u32;
    let mut updates: Vec<MigrationUpdate> = Vec::new();

    for file_path in &flat_files {
        let file_path_str = file_path.to_string_lossy().into_owned();

        let zone_id = hidden_to_zone
            .iter()
            .find(|(hp, _)| paths_equal_str(hp, &file_path_str))
            .map(|(_, zid)| zid.clone());

        let zone_id = match zone_id {
            Some(z) => z,
            None => {
                tracing::warn!(
                    "Zone migration: no zone found for flat file {:?}, leaving in place",
                    file_path
                );
                continue;
            }
        };

        let zone_dir = crate::stealth::hide::zone_hidden_dir_for(config, &zone_id)?;
        let file_name = match file_path.file_name() {
            Some(n) => n,
            None => continue,
        };
        let dest = zone_dir.join(file_name);

        if dest.exists() {
            tracing::warn!(
                "Zone migration: destination already exists {:?}, skipping {:?}",
                dest,
                file_path
            );
            continue;
        }

        match std::fs::rename(file_path, &dest) {
            Ok(()) => {
                let new_hidden_str = dest.to_string_lossy().into_owned();

                let mut manifest = load_manifest(&hdir)?;
                for entry in &mut manifest.entries {
                    if paths_equal_str(&entry.hidden_path, &file_path_str) {
                        entry.hidden_path = new_hidden_str.clone();
                        entry.zone_id = zone_id.clone();
                    }
                }
                save_manifest(&hdir, &manifest)?;

                tracing::info!(
                    "Zone migration: {:?} -> {:?} (zone={})",
                    file_path,
                    dest,
                    zone_id
                );
                updates.push((file_path_str, new_hidden_str, zone_id));
                migrated += 1;
            }
            Err(e) => {
                tracing::error!(
                    "Zone migration failed for {:?} -> {:?}: {}",
                    file_path,
                    dest,
                    e
                );
            }
        }
    }

    Ok((migrated, updates))
}
