//! Hidden items management -- "Desktop Subfolder Mode".
//!
//! Files added to zones are **moved** into a hidden `.bentodesk/` subfolder
//! located on the **same drive** as the Desktop directory. This is superior
//! to the old `attrib +h` approach because:
//!
//! 1. Same-drive = instant `fs::rename`, no cross-drive copy needed.
//! 2. Subfolder contents do NOT appear as desktop icons.
//! 3. Only the `.bentodesk/` folder itself needs `attrib +h +s`.
//! 4. Easy recovery -- files are just sitting in a known directory.
//!
//! Safety invariants:
//! - NEVER delete user files.
//! - If `fs::rename` fails, the file stays visible at its original location (safe default).
//! - On exit, all hidden files are moved back to their original Desktop paths.
//! - A **safety manifest** (`manifest.json`) inside `.bentodesk/` tracks every
//!   hidden file, providing a recovery path even if layout.json is lost.
//!
//! ## Migration
//!
//! On first run after upgrading:
//! - `cleanup_legacy_hidden_dir()` detects files in the old AppData `hidden_items/`
//!   directory AND files that were hidden via `attrib +h +s`, migrating them all
//!   to the new subfolder-based architecture.

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

// --- Safety Manifest --------------------------------------------------------

/// A single entry in the safety manifest — tracks one hidden file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestEntry {
    /// The original file path on the Desktop.
    pub original_path: String,
    /// The current path inside `.bentodesk/{zone_id}/`.
    pub hidden_path: String,
    /// The zone that owns this hidden file.
    #[serde(default)]
    pub zone_id: String,
    /// File size in bytes at the time of hiding (for staleness detection).
    #[serde(default)]
    pub file_size_bytes: u64,
    /// ISO 8601 timestamp when the file was hidden.
    pub hidden_at: String,
    /// Display name shown in the zone.
    #[serde(default)]
    pub display_name: String,
    /// Desktop icon X position before hiding (for exact position restore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_x: Option<i32>,
    /// Desktop icon Y position before hiding (for exact position restore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_y: Option<i32>,
    /// File type (e.g. "File", "Shortcut", "Folder", "Application").
    #[serde(default)]
    pub file_type: String,
}

/// Zone metadata snapshot — stored in manifest for complete backup.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestZone {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Position as percentage of screen.
    #[serde(default)]
    pub x_percent: f64,
    #[serde(default)]
    pub y_percent: f64,
    /// Expanded size as percentage of screen.
    #[serde(default)]
    pub w_percent: f64,
    #[serde(default)]
    pub h_percent: f64,
    #[serde(default)]
    pub sort_order: u32,
    #[serde(default)]
    pub grid_columns: u32,
    #[serde(default)]
    pub item_count: usize,
}

/// The safety manifest — a complete independent backup of all hidden files
/// AND zone metadata. This provides full recovery even if layout.json is lost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SafetyManifest {
    pub entries: Vec<ManifestEntry>,
    /// Zone metadata snapshots — updated on every manifest_save.
    #[serde(default)]
    pub zones: Vec<ManifestZone>,
    /// Screen resolution at the time of the last update.
    #[serde(default)]
    pub screen_width: u32,
    #[serde(default)]
    pub screen_height: u32,
    /// ISO 8601 timestamp of the last manifest update.
    #[serde(default)]
    pub last_updated: String,
}

impl Default for SafetyManifest {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            zones: Vec::new(),
            screen_width: 0,
            screen_height: 0,
            last_updated: String::new(),
        }
    }
}

// --- Hidden Directory -------------------------------------------------------

/// Resolve the `.bentodesk/` hidden directory path. Located at
/// `{desktop_path}/.bentodesk/`. Creates the directory and applies
/// `attrib +h +s` on it if it does not yet exist.
pub fn hidden_dir(app_handle: &AppHandle) -> PathBuf {
    let state = app_handle.state::<crate::AppState>();
    let desktop_path = {
        let settings = state.settings.lock().expect("settings lock poisoned");
        settings.desktop_path.clone()
    };

    let dir = PathBuf::from(&desktop_path).join(".bentodesk");

    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!("Failed to create .bentodesk/ directory at {:?}: {}", dir, e);
            return dir;
        }
        tracing::info!("Created .bentodesk/ directory: {:?}", dir);
    }

    // Ensure the folder has hidden+system attributes
    set_hidden_attribute_on_dir(&dir);

    dir
}

/// Resolve the `.bentodesk/{zone_id}/` sub-directory for zone-level isolation.
/// Creates the directory if it does not yet exist. The parent `.bentodesk/`
/// directory is created (and hidden) if needed.
pub fn zone_hidden_dir(app_handle: &AppHandle, zone_id: &str) -> PathBuf {
    let parent = hidden_dir(app_handle);
    let dir = parent.join(zone_id);

    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!(
                "Failed to create zone hidden dir {:?}: {}",
                dir,
                e
            );
            return dir;
        }
        tracing::info!("Created zone hidden dir: {:?}", dir);
    }

    dir
}

/// Remove an empty zone sub-directory inside `.bentodesk/`.
/// All files must have been restored before calling this.
/// Returns `true` if the directory was removed (or didn't exist).
pub fn cleanup_zone_dir(app_handle: &AppHandle, zone_id: &str) -> bool {
    let parent = hidden_dir(app_handle);
    let dir = parent.join(zone_id);

    if !dir.exists() {
        return true;
    }

    // Safety: only remove if empty (no user files left)
    match std::fs::read_dir(&dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                tracing::warn!(
                    "cleanup_zone_dir: zone dir {:?} is not empty, will not remove",
                    dir
                );
                return false;
            }
        }
        Err(e) => {
            tracing::error!("cleanup_zone_dir: cannot read zone dir {:?}: {}", dir, e);
            return false;
        }
    }

    match std::fs::remove_dir(&dir) {
        Ok(()) => {
            tracing::info!("Removed empty zone dir: {:?}", dir);
            true
        }
        Err(e) => {
            tracing::warn!("Failed to remove zone dir {:?}: {}", dir, e);
            false
        }
    }
}

/// Apply `attrib +h +s` to a **directory** (the `.bentodesk/` folder).
fn set_hidden_attribute_on_dir(dir: &Path) {
    let dir_str = dir.to_string_lossy();
    match std::process::Command::new("attrib")
        .args(["+h", "+s", &*dir_str])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                tracing::debug!("attrib +h +s on dir succeeded: {}", dir_str);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "attrib +h +s on dir failed for {}: exit={}, stderr={}",
                    dir_str,
                    output.status,
                    stderr.trim()
                );
            }
        }
        Err(e) => {
            tracing::error!("Failed to run attrib on dir {}: {}", dir_str, e);
        }
    }
}

/// Remove `attrib +h +s` from a single file (used during legacy migration).
fn remove_hidden_attribute(file_path: &str) -> bool {
    match std::process::Command::new("attrib")
        .args(["-h", "-s", file_path])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                tracing::debug!("attrib -h -s succeeded: {}", file_path);
                true
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "attrib -h -s failed for {}: exit={}, stderr={}",
                    file_path,
                    output.status,
                    stderr.trim()
                );
                false
            }
        }
        Err(e) => {
            tracing::error!("Failed to run attrib -h -s for {}: {}", file_path, e);
            false
        }
    }
}

// --- Manifest I/O -----------------------------------------------------------

/// Load the safety manifest from the `.bentodesk/` directory.
fn load_manifest(dir: &Path) -> SafetyManifest {
    let path = dir.join("manifest.json");
    if !path.exists() {
        return SafetyManifest::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            tracing::error!("Manifest corrupt, using empty manifest: {}", e);
            SafetyManifest::default()
        }),
        Err(e) => {
            tracing::error!("Failed to read manifest: {}", e);
            SafetyManifest::default()
        }
    }
}

/// Save the safety manifest atomically (write to temp, then rename).
fn save_manifest(dir: &Path, manifest: &SafetyManifest) {
    let path = dir.join("manifest.json");
    let temp_path = dir.join("manifest.json.tmp");

    let content = match serde_json::to_string_pretty(manifest) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to serialize manifest: {}", e);
            return;
        }
    };

    if let Err(e) = std::fs::write(&temp_path, &content) {
        tracing::error!("Failed to write temp manifest: {}", e);
        return;
    }

    if let Err(e) = std::fs::rename(&temp_path, &path) {
        tracing::error!("Failed to rename temp manifest: {}", e);
        // Fallback: direct write
        if let Err(e2) = std::fs::write(&path, &content) {
            tracing::error!("Fallback direct write also failed: {}", e2);
        }
    }
}

/// Append an entry to the safety manifest with full metadata.
fn manifest_add(
    dir: &Path,
    original_path: &str,
    hidden_path: &str,
    zone_id: &str,
    file_size_bytes: u64,
    display_name: &str,
    icon_x: Option<i32>,
    icon_y: Option<i32>,
    file_type: &str,
) {
    let mut manifest = load_manifest(dir);
    // Avoid duplicate entries for the same original path
    manifest
        .entries
        .retain(|e| !paths_equal_str(&e.original_path, original_path));
    manifest.entries.push(ManifestEntry {
        original_path: original_path.to_string(),
        hidden_path: hidden_path.to_string(),
        zone_id: zone_id.to_string(),
        file_size_bytes,
        hidden_at: chrono::Utc::now().to_rfc3339(),
        display_name: display_name.to_string(),
        icon_x,
        icon_y,
        file_type: file_type.to_string(),
    });
    save_manifest(dir, &manifest);
    tracing::debug!(
        "Manifest: added entry ({}, zone={}, icon_pos=({:?},{:?})), total={}",
        original_path,
        zone_id,
        icon_x,
        icon_y,
        manifest.entries.len()
    );
}

/// Sync zone metadata into the manifest. Called after layout changes to keep
/// the manifest as a complete backup of zone configuration.
pub fn sync_zone_metadata(app_handle: &AppHandle) {
    let dir = hidden_dir(app_handle);
    let mut manifest = load_manifest(&dir);

    // Read current zone data from AppState
    if let Some(state) = app_handle.try_state::<crate::AppState>() {
        if let Ok(layout) = state.layout.lock() {
            manifest.zones = layout
                .zones
                .iter()
                .map(|z| ManifestZone {
                    id: z.id.clone(),
                    name: z.name.clone(),
                    icon: z.icon.clone(),
                    x_percent: z.position.x_percent,
                    y_percent: z.position.y_percent,
                    w_percent: z.expanded_size.w_percent,
                    h_percent: z.expanded_size.h_percent,
                    sort_order: z.sort_order as u32,
                    grid_columns: z.grid_columns,
                    item_count: z.items.len(),
                })
                .collect();
        }
    }

    // Add screen resolution
    let res = crate::layout::resolution::get_current_resolution();
    manifest.screen_width = res.width;
    manifest.screen_height = res.height;
    manifest.last_updated = chrono::Utc::now().to_rfc3339();

    save_manifest(&dir, &manifest);
}

/// Remove an entry from the safety manifest (matched by original_path).
fn manifest_remove(dir: &Path, original_path: &str) {
    let mut manifest = load_manifest(dir);
    let before = manifest.entries.len();
    manifest
        .entries
        .retain(|e| !paths_equal_str(&e.original_path, original_path));
    let removed = before - manifest.entries.len();
    if removed > 0 {
        save_manifest(dir, &manifest);
        tracing::debug!(
            "Manifest: removed {} entry(ies) for {}, remaining={}",
            removed,
            original_path,
            manifest.entries.len()
        );
    }
}

// --- Path Helpers -----------------------------------------------------------

/// Case-insensitive path comparison for Windows.
fn paths_equal_str(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

/// Strip the Windows extended-length path prefix (`\\?\`).
#[allow(dead_code)]
fn strip_unc_prefix(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p.to_path_buf()
    }
}

/// Check whether a file path is on the user's Desktop directory.
#[allow(dead_code)]
pub fn is_desktop_path(path: &str) -> bool {
    is_desktop_path_with_custom(path, None)
}

/// Check whether a file path is on the user's Desktop directory,
/// also checking against a custom desktop path from settings.
pub fn is_desktop_path_with_custom(path: &str, custom_desktop: Option<&str>) -> bool {
    let file_parent = Path::new(path).parent().unwrap_or(Path::new(path));

    // Check 1: against dirs::desktop_dir() (system Shell folder)
    if let Some(desktop) = dirs::desktop_dir() {
        if paths_match(file_parent, &desktop) {
            return true;
        }
    }

    // Check 2: against custom desktop_path from settings
    if let Some(custom) = custom_desktop {
        if !custom.is_empty() && paths_match(file_parent, Path::new(custom)) {
            return true;
        }
    }

    // Check 3: simple string comparison as fallback (case-insensitive on Windows)
    if let Some(parent_str) = file_parent.to_str() {
        if let Some(desktop) = dirs::desktop_dir() {
            if let Some(desktop_str) = desktop.to_str() {
                if parent_str.eq_ignore_ascii_case(desktop_str) {
                    return true;
                }
            }
        }
        if let Some(custom) = custom_desktop {
            if !custom.is_empty() && parent_str.eq_ignore_ascii_case(custom) {
                return true;
            }
        }
    }

    false
}

/// Compare two paths after canonicalization and UNC prefix stripping.
#[allow(dead_code)]
fn paths_match(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (a.canonicalize(), b.canonicalize()) {
        strip_unc_prefix(&ca) == strip_unc_prefix(&cb)
    } else {
        false
    }
}

/// Generate a unique filename inside the hidden directory. If a file with the
/// same name already exists, append a UUID suffix before the extension.
fn unique_hidden_path(hidden_dir: &Path, original: &Path) -> PathBuf {
    let file_name = original
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let candidate = hidden_dir.join(&file_name);
    if !candidate.exists() {
        return candidate;
    }

    // Name conflict -- append UUID suffix
    let stem = original
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = original
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let uuid_suffix = &uuid::Uuid::new_v4().to_string()[..8];

    hidden_dir.join(format!("{}_{}{}", stem, uuid_suffix, ext))
}

// --- Hide / Restore (Subfolder Mode) ----------------------------------------

/// Hide a file by moving it into the `.bentodesk/{zone_id}/` subfolder.
///
/// Returns `(original_path, hidden_path)` on success, or `None` if the file
/// does not exist or the move fails. On failure, the file stays visible at
/// its original location (safe default).
pub fn hide_file(app_handle: &AppHandle, file_path: &str, zone_id: &str) -> Option<(String, String)> {
    let source = Path::new(file_path);
    if !source.exists() {
        tracing::warn!("Cannot hide file -- does not exist: {}", file_path);
        return None;
    }

    let hdir = zone_hidden_dir(app_handle, zone_id);
    let dest = unique_hidden_path(&hdir, source);

    // Same-drive rename should be instant
    if let Err(e) = std::fs::rename(source, &dest) {
        tracing::warn!(
            "fs::rename failed for hide ({} -> {:?}): {}. Trying copy+delete fallback.",
            file_path,
            dest,
            e
        );
        // Cross-drive fallback (should be rare since .bentodesk is on the desktop drive)
        match std::fs::copy(source, &dest) {
            Ok(_) => {
                if let Err(e2) = std::fs::remove_file(source) {
                    tracing::error!(
                        "Copy succeeded but delete of original failed: {}. Removing copy to stay safe.",
                        e2
                    );
                    let _ = std::fs::remove_file(&dest);
                    return None;
                }
            }
            Err(e2) => {
                tracing::error!(
                    "Copy fallback also failed ({} -> {:?}): {}",
                    file_path,
                    dest,
                    e2
                );
                return None;
            }
        }
    }

    // Record file size
    let file_size = std::fs::metadata(&dest)
        .map(|m| m.len())
        .unwrap_or(0);

    let hidden_path_str = dest.to_string_lossy().to_string();

    // Derive display name for manifest
    let display_name = source.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Track in global manifest (stored in parent .bentodesk/ dir)
    let global_dir = hidden_dir(app_handle);
    manifest_add(
        &global_dir, file_path, &hidden_path_str, zone_id, file_size,
        &display_name, None, None, "",
    );

    tracing::info!(
        "Hidden desktop item (zone '{}'): {} -> {}",
        zone_id,
        file_path,
        hidden_path_str
    );

    Some((file_path.to_string(), hidden_path_str))
}

/// Restore a hidden file by moving it back from `.bentodesk/` to its original
/// Desktop path.
///
/// Returns `true` on success, `false` on failure.
pub fn restore_file(original_path: &str, hidden_path: &str) -> bool {
    let source = Path::new(hidden_path);
    let dest = Path::new(original_path);

    if !source.exists() {
        tracing::warn!(
            "Cannot restore -- hidden file does not exist: {}",
            hidden_path
        );
        return false;
    }

    // Ensure destination directory exists
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    // Don't overwrite if something already exists at the original path
    if dest.exists() {
        tracing::warn!(
            "Restore destination already exists, skipping: {}",
            original_path
        );
        // The file exists at the destination already, consider this "restored"
        // Remove the hidden copy to avoid duplication
        let _ = std::fs::remove_file(source);
        return true;
    }

    // Same-drive rename
    if let Err(e) = std::fs::rename(source, dest) {
        tracing::warn!(
            "fs::rename failed for restore ({} -> {}): {}. Trying copy+delete fallback.",
            hidden_path,
            original_path,
            e
        );
        // Cross-drive fallback
        match std::fs::copy(source, dest) {
            Ok(_) => {
                let _ = std::fs::remove_file(source);
            }
            Err(e2) => {
                tracing::error!(
                    "Restore failed ({} -> {}): {}",
                    hidden_path,
                    original_path,
                    e2
                );
                return false;
            }
        }
    }

    tracing::info!(
        "Restored desktop item (subfolder): {} -> {}",
        hidden_path,
        original_path
    );
    true
}

/// Restore a hidden file and update the safety manifest.
pub fn restore_file_tracked(app_handle: &AppHandle, original_path: &str, hidden_path: &str) -> bool {
    let success = restore_file(original_path, hidden_path);
    if success {
        let hdir = hidden_dir(app_handle);
        manifest_remove(&hdir, original_path);
    }
    success
}

// --- Bulk Restore (Exit Safety) ---------------------------------------------

/// Restore ALL hidden items. 3-tier strategy:
///
/// 1. **Layout** -- iterate zones/items and move files back.
/// 2. **Manifest** -- cross-check against safety manifest for items missed by layout.
/// 3. **Directory scan** -- scan `.bentodesk/` for any orphaned files not tracked
///    by layout or manifest, and move them back to the Desktop.
///
/// Called on application exit to leave the Desktop in its original state.
pub fn restore_all_hidden(app_handle: &AppHandle) {
    let hdir = hidden_dir(app_handle);

    tracing::info!("=== restore_all_hidden: starting (subfolder mode) ===");

    // -- Tier 1: Restore from layout -----------------------------------------
    let state = app_handle.state::<crate::AppState>();

    let (layout, desktop_path) = {
        let l = match state.layout.lock() {
            Ok(l) => l.clone(),
            Err(e) => {
                tracing::error!("Failed to lock layout for restore: {}", e);
                crate::layout::persistence::LayoutData::default()
            }
        };
        let dp = match state.settings.lock() {
            Ok(s) => s.desktop_path.clone(),
            Err(_) => String::new(),
        };
        (l, dp)
    };

    let mut restored = 0u32;
    let mut failed = 0u32;
    let mut attempted_originals: Vec<String> = Vec::new();

    for zone in &layout.zones {
        for item in &zone.items {
            if let (Some(ref orig), Some(ref hidden)) = (&item.original_path, &item.hidden_path) {
                attempted_originals.push(orig.clone());
                if restore_file(orig, hidden) {
                    restored += 1;
                } else {
                    failed += 1;
                }
            }
        }
    }

    tracing::info!(
        "  Layout tier: restored={}, failed={}",
        restored,
        failed
    );

    // -- Tier 2: Restore from manifest ---------------------------------------
    let manifest = load_manifest(&hdir);
    let mut manifest_restored = 0u32;
    let mut manifest_failed = 0u32;

    for entry in &manifest.entries {
        // Skip entries already attempted in tier 1
        if attempted_originals
            .iter()
            .any(|p| paths_equal_str(p, &entry.original_path))
        {
            continue;
        }

        tracing::info!(
            "  Manifest restore (missed by layout): orig={}, hidden={}",
            entry.original_path,
            entry.hidden_path
        );
        if restore_file(&entry.original_path, &entry.hidden_path) {
            manifest_restored += 1;
        } else {
            manifest_failed += 1;
        }
    }

    if manifest_restored > 0 || manifest_failed > 0 {
        tracing::info!(
            "  Manifest tier: restored={}, failed={}",
            manifest_restored,
            manifest_failed
        );
    }

    // -- Tier 3: Directory scan (orphaned files) -----------------------------
    // Walk ALL subdirectories (zone subdirs) AND top-level orphaned files.
    let mut scan_restored = 0u32;
    if hdir.exists() {
        scan_restored += scan_and_restore_orphans(&hdir, &desktop_path);

        // Also walk zone subdirectories
        if let Ok(entries) = std::fs::read_dir(&hdir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    // Skip non-zone dirs that might exist
                    if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') {
                            continue;
                        }
                    }
                    scan_restored += scan_and_restore_orphans(&entry_path, &desktop_path);

                    // Remove zone subdir if now empty
                    if is_dir_empty(&entry_path) {
                        let _ = std::fs::remove_dir(&entry_path);
                    }
                }
            }
        }
    }

    if scan_restored > 0 {
        tracing::info!("  Scan tier: restored {} orphaned files", scan_restored);
    }

    // Clear the manifest
    save_manifest(&hdir, &SafetyManifest::default());

    let total = restored + manifest_restored + scan_restored;
    tracing::info!(
        "=== restore_all_hidden: complete -- {} total files restored ===",
        total
    );
}

/// Restore items for a specific zone. Called before deleting a zone.
///
/// Returns the number of items successfully restored.
pub fn restore_zone_items(app_handle: &AppHandle, items: &[crate::layout::persistence::BentoItem]) -> u32 {
    let hdir = hidden_dir(app_handle);
    let mut restored = 0u32;

    for item in items {
        if let (Some(ref orig), Some(ref hidden)) = (&item.original_path, &item.hidden_path) {
            tracing::info!("  Zone delete restore: {} -> {}", hidden, orig);
            if restore_file(orig, hidden) {
                manifest_remove(&hdir, orig);
                restored += 1;
            } else {
                tracing::error!("  Zone delete restore FAILED for: {}", orig);
            }
        }
    }

    restored
}

/// Ensure `.bentodesk/` folder has hidden+system attributes on startup.
/// In subfolder mode, we do NOT need to re-hide individual files.
/// Also counts files across all zone subdirectories for logging.
pub fn reapply_hidden_on_startup(app_handle: &AppHandle) -> u32 {
    let hdir = hidden_dir(app_handle);
    set_hidden_attribute_on_dir(&hdir);

    // Count how many files are in the hidden dir and all zone subdirs (for logging)
    let mut count = 0u32;
    if let Ok(entries) = std::fs::read_dir(&hdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if path.is_dir() {
                // Count files inside zone subdirectory
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    count += sub_entries.flatten().filter(|e| !e.path().is_dir()).count() as u32;
                }
            } else if name_str != "manifest.json" && name_str != "manifest.json.tmp" {
                // Legacy top-level file (pre-zone-isolation)
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

    count
}

// --- Reference Verification -------------------------------------------------

/// Check all referenced hidden files still exist. Returns a list of original
/// paths whose hidden files are missing.
pub fn verify_references(app_handle: &AppHandle) -> Vec<String> {
    let state = app_handle.state::<crate::AppState>();
    let layout = match state.layout.lock() {
        Ok(l) => l.clone(),
        Err(e) => {
            tracing::error!("Failed to lock layout for reference verification: {}", e);
            return Vec::new();
        }
    };

    let mut missing = Vec::new();

    for zone in &layout.zones {
        for item in &zone.items {
            if let Some(ref hidden) = item.hidden_path {
                if !Path::new(hidden).exists() {
                    tracing::warn!(
                        "Reference verification: hidden file missing -- zone='{}', item='{}', hidden_path='{}'",
                        zone.name,
                        item.name,
                        hidden
                    );
                    if let Some(ref orig) = item.original_path {
                        missing.push(orig.clone());
                    }
                }
            }
        }
    }

    if !missing.is_empty() {
        tracing::info!(
            "Reference verification: {} hidden file(s) missing",
            missing.len()
        );
    }

    missing
}

// --- Legacy Migration -------------------------------------------------------

/// Resolve the old `hidden_items/` storage directory under the app data dir.
fn legacy_hidden_items_dir(app_handle: &AppHandle) -> PathBuf {
    let base = tauri::Manager::path(app_handle)
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("hidden_items")
}

/// Old manifest from the AppData/hidden_items/ directory.
#[derive(Debug, Clone, serde::Deserialize)]
struct LegacyMoveManifestEntry {
    original_path: String,
    hidden_path: String,
    #[allow(dead_code)]
    hidden_at: String,
}


/// Migrate from BOTH old architectures:
/// 1. AppData/hidden_items/ directory (old "file move" mode)
/// 2. Files hidden via `attrib +h +s` (old "reference mode")
///
/// Returns the number of files migrated.
pub fn cleanup_legacy_hidden_dir(app_handle: &AppHandle) -> u32 {
    let mut total_migrated = 0u32;

    // --- Phase 1: Migrate from old AppData/hidden_items/ directory ----------
    let old_dir = legacy_hidden_items_dir(app_handle);
    if old_dir.exists() {
        tracing::info!("=== Legacy migration Phase 1: checking AppData/hidden_items/ ===");
        total_migrated += migrate_old_move_dir(app_handle, &old_dir);
    }

    // --- Phase 2: Migrate attrib-hidden files from the layout ---------------
    total_migrated += migrate_attrib_hidden_files(app_handle);

    // --- Phase 3: Clean old manifest.json from app data dir -----------------
    let app_data_manifest = {
        let base = tauri::Manager::path(app_handle)
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join("manifest.json")
    };
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

    total_migrated
}

/// Migrate files from the old AppData/hidden_items/ directory.
fn migrate_old_move_dir(app_handle: &AppHandle, old_dir: &Path) -> u32 {
    // Load the old manifest
    let old_manifest = load_legacy_move_manifest(old_dir);

    let entries = match std::fs::read_dir(old_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Cannot read legacy hidden_items/ directory: {}", e);
            return 0;
        }
    };

    let desktop = {
        let state = app_handle.state::<crate::AppState>();
        let result = match state.settings.lock() {
            Ok(s) => Some(PathBuf::from(&s.desktop_path)),
            Err(_) => dirs::desktop_dir(),
        };
        result
    };

    let mut migrated = 0u32;

    for entry in entries.flatten() {
        let file_path = entry.path();

        // Skip manifest files and directories
        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name == "manifest.json" || name == "manifest.json.tmp" {
                continue;
            }
        }
        if file_path.is_dir() {
            continue;
        }

        let file_path_str = file_path.to_string_lossy().to_string();

        // Find original path from old manifest
        let original = old_manifest
            .iter()
            .find(|e| paths_equal_str(&e.hidden_path, &file_path_str))
            .map(|e| e.original_path.clone());

        let dest = if let Some(ref orig) = original {
            PathBuf::from(orig)
        } else {
            // Untracked -- restore to Desktop
            match (&desktop, file_path.file_name()) {
                (Some(desktop_dir), Some(name)) => desktop_dir.join(name),
                _ => {
                    tracing::warn!(
                        "Legacy migration: cannot determine destination for {}",
                        file_path.display()
                    );
                    continue;
                }
            }
        };

        // Don't overwrite existing files
        if dest.exists() {
            tracing::warn!(
                "Legacy migration: destination already exists, skipping: {}",
                dest.display()
            );
            continue;
        }

        // Ensure parent exists
        if let Some(parent) = dest.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        let success = match std::fs::rename(&file_path, &dest) {
            Ok(()) => true,
            Err(_) => {
                match std::fs::copy(&file_path, &dest) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&file_path);
                        true
                    }
                    Err(e) => {
                        tracing::error!(
                            "Legacy migration failed: {} -> {}: {}",
                            file_path.display(),
                            dest.display(),
                            e
                        );
                        false
                    }
                }
            }
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

    // Try to remove the legacy directory
    if migrated > 0 || is_dir_empty_except_manifest(old_dir) {
        let _ = std::fs::remove_file(old_dir.join("manifest.json"));
        let _ = std::fs::remove_file(old_dir.join("manifest.json.tmp"));
        match std::fs::remove_dir(old_dir) {
            Ok(()) => tracing::info!("Removed legacy hidden_items/ directory"),
            Err(e) => tracing::warn!("Could not remove legacy hidden_items/ directory: {}", e),
        }
    }

    migrated
}

/// Migrate files that were hidden via `attrib +h +s` in the old "reference
/// mode". These files are still at their original Desktop path but have the
/// hidden attribute set. We need to:
/// 1. Remove `attrib -h -s` from them.
/// 2. Move them into `.bentodesk/`.
/// 3. Update layout items with the new hidden_path.
///
/// This is called AFTER layout is loaded but BEFORE reapply_hidden_on_startup.
fn migrate_attrib_hidden_files(app_handle: &AppHandle) -> u32 {
    let state = app_handle.state::<crate::AppState>();
    let layout = match state.layout.lock() {
        Ok(l) => l.clone(),
        Err(e) => {
            tracing::error!("Failed to lock layout for attrib migration: {}", e);
            return 0;
        }
    };

    // Detect attrib-mode items: original_path == hidden_path (or hidden_path is
    // None but original_path is set), meaning the file was never moved.
    let mut items_to_migrate: Vec<(String, String, String)> = Vec::new(); // (zone_id, item_id, original_path)

    for zone in &layout.zones {
        for item in &zone.items {
            if let Some(ref orig) = item.original_path {
                let is_attrib_mode = match &item.hidden_path {
                    Some(hp) => paths_equal_str(hp, orig),
                    None => true,
                };
                if is_attrib_mode && Path::new(orig).exists() {
                    items_to_migrate.push((zone.id.clone(), item.id.clone(), orig.clone()));
                }
            }
        }
    }

    if items_to_migrate.is_empty() {
        return 0;
    }

    tracing::info!(
        "=== Legacy migration Phase 2: migrating {} attrib-hidden files to subfolder mode ===",
        items_to_migrate.len()
    );

    let mut migrated = 0u32;

    for (zone_id, item_id, orig_path) in &items_to_migrate {
        // Remove hidden attribute first so we can work with the file
        remove_hidden_attribute(orig_path);

        let source = Path::new(orig_path);
        if !source.exists() {
            tracing::warn!("Attrib migration: file disappeared after unhide: {}", orig_path);
            continue;
        }

        // Move directly into zone subdirectory
        let zone_dir = zone_hidden_dir(app_handle, zone_id);
        let dest = unique_hidden_path(&zone_dir, source);

        let success = match std::fs::rename(source, &dest) {
            Ok(()) => true,
            Err(_) => {
                match std::fs::copy(source, &dest) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(source);
                        true
                    }
                    Err(e) => {
                        tracing::error!(
                            "Attrib migration failed for {}: {}",
                            orig_path,
                            e
                        );
                        false
                    }
                }
            }
        };

        if success {
            let hidden_path_str = dest.to_string_lossy().to_string();
            let file_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            let display_name = std::path::Path::new(orig_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let global_dir = hidden_dir(app_handle);
            manifest_add(&global_dir, orig_path, &hidden_path_str, zone_id, file_size,
                &display_name, None, None, "");

            // Update layout item with new hidden_path and path
            {
                let mut layout = state.layout.lock().expect("layout lock poisoned");
                if let Some(zone) = layout.zones.iter_mut().find(|z| z.id == *zone_id) {
                    if let Some(item) = zone.items.iter_mut().find(|i| i.id == *item_id) {
                        item.hidden_path = Some(hidden_path_str.clone());
                        item.path = hidden_path_str.clone();
                    }
                }
                layout.last_modified = chrono::Utc::now().to_rfc3339();
            }

            tracing::info!(
                "Attrib migration: {} -> {} (zone={})",
                orig_path,
                dest.display(),
                zone_id
            );
            migrated += 1;
        }
    }

    if migrated > 0 {
        state.persist_layout();
        tracing::info!(
            "Legacy Phase2: migrated {} attrib-hidden files to subfolder mode",
            migrated
        );
    }

    migrated
}

/// Load the old-format manifest (hidden_items/ directory, move mode).
fn load_legacy_move_manifest(hidden_dir: &Path) -> Vec<LegacyMoveManifestEntry> {
    let path = hidden_dir.join("manifest.json");
    if !path.exists() {
        return Vec::new();
    }

    #[derive(Debug, serde::Deserialize)]
    struct LegacyManifest {
        entries: Vec<LegacyMoveManifestEntry>,
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            serde_json::from_str::<LegacyManifest>(&content)
                .map(|m| m.entries)
                .unwrap_or_else(|e| {
                    tracing::warn!("Could not parse legacy manifest: {}", e);
                    Vec::new()
                })
        }
        Err(e) => {
            tracing::warn!("Could not read legacy manifest: {}", e);
            Vec::new()
        }
    }
}

// --- Zone-Level Isolation Migration ------------------------------------------

/// Migrate flat `.bentodesk/` files into zone subdirectories.
///
/// Before zone-level isolation, all hidden files lived directly in `.bentodesk/`.
/// This function detects such files, looks up which zone references them in the
/// layout, and moves each file into `.bentodesk/{zone_id}/`.
///
/// Returns the number of files migrated.
pub fn migrate_flat_to_zone_dirs(app_handle: &AppHandle) -> u32 {
    let hdir = hidden_dir(app_handle);
    let state = app_handle.state::<crate::AppState>();

    // Collect top-level files in .bentodesk/ (excluding manifest & subdirs)
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
                name_str != "manifest.json" && name_str != "manifest.json.tmp"
            })
            .map(|e| e.path())
            .collect(),
        Err(_) => return 0,
    };

    if flat_files.is_empty() {
        return 0;
    }

    tracing::info!(
        "=== Zone isolation migration: {} flat files to migrate ===",
        flat_files.len()
    );

    // Build a lookup: hidden_path -> zone_id from current layout
    let hidden_to_zone: Vec<(String, String)> = {
        let layout = match state.layout.lock() {
            Ok(l) => l.clone(),
            Err(e) => {
                tracing::error!("Failed to lock layout for zone migration: {}", e);
                return 0;
            }
        };
        let mut map = Vec::new();
        for zone in &layout.zones {
            for item in &zone.items {
                if let Some(ref hp) = item.hidden_path {
                    map.push((hp.clone(), zone.id.clone()));
                }
            }
        }
        map
    };

    let mut migrated = 0u32;

    for file_path in &flat_files {
        let file_path_str = file_path.to_string_lossy().to_string();

        // Find the zone that owns this file
        let zone_id = hidden_to_zone
            .iter()
            .find(|(hp, _)| paths_equal_str(hp, &file_path_str))
            .map(|(_, zid)| zid.clone());

        let zone_id = match zone_id {
            Some(z) => z,
            None => {
                // Orphaned file with no zone reference -- leave it for
                // restore_all_hidden to handle as an orphan
                tracing::warn!(
                    "Zone migration: no zone found for flat file {:?}, leaving in place",
                    file_path
                );
                continue;
            }
        };

        // Create zone subdir and move the file
        let zone_dir = zone_hidden_dir(app_handle, &zone_id);
        let file_name = match file_path.file_name() {
            Some(n) => n,
            None => continue,
        };
        let dest = zone_dir.join(file_name);

        // Avoid collision
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
                let new_hidden_str = dest.to_string_lossy().to_string();

                // Update layout item's hidden_path and path
                {
                    let mut layout = state.layout.lock().expect("layout lock poisoned");
                    for zone in &mut layout.zones {
                        if zone.id == zone_id {
                            for item in &mut zone.items {
                                if let Some(ref hp) = item.hidden_path {
                                    if paths_equal_str(hp, &file_path_str) {
                                        item.hidden_path = Some(new_hidden_str.clone());
                                        item.path = new_hidden_str.clone();
                                    }
                                }
                            }
                        }
                    }
                    layout.last_modified = chrono::Utc::now().to_rfc3339();
                }

                // Update manifest
                let manifest_dir = hidden_dir(app_handle);
                let mut manifest = load_manifest(&manifest_dir);
                for entry in &mut manifest.entries {
                    if paths_equal_str(&entry.hidden_path, &file_path_str) {
                        entry.hidden_path = new_hidden_str.clone();
                        entry.zone_id = zone_id.clone();
                    }
                }
                save_manifest(&manifest_dir, &manifest);

                tracing::info!(
                    "Zone migration: {:?} -> {:?} (zone={})",
                    file_path,
                    dest,
                    zone_id
                );
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

    if migrated > 0 {
        state.persist_layout();
    }

    migrated
}

/// Check if a directory only contains manifest.json / manifest.json.tmp (or is empty).
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

/// Check if a directory is completely empty.
fn is_dir_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => false,
    }
}

/// Scan a single directory for orphaned files and restore them to the Desktop.
/// Returns the count of successfully restored files.
fn scan_and_restore_orphans(dir: &Path, desktop_path: &str) -> u32 {
    let mut count = 0u32;
    if desktop_path.is_empty() {
        return count;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return count,
    };

    for entry in entries.flatten() {
        let file_path = entry.path();

        // Skip manifest files and subdirectories
        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name == "manifest.json" || name == "manifest.json.tmp" {
                continue;
            }
        }
        if file_path.is_dir() {
            continue;
        }

        // This file was not tracked by layout or manifest -- move to Desktop
        if let Some(file_name) = file_path.file_name() {
            let dest = PathBuf::from(desktop_path).join(file_name);
            if !dest.exists() {
                if let Ok(()) = std::fs::rename(&file_path, &dest) {
                    tracing::info!(
                        "  Scan tier: restored orphan {:?} -> {:?}",
                        file_path,
                        dest
                    );
                    count += 1;
                }
            }
        }
    }

    count
}
