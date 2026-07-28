//! T-094b — Restore-side operations of the stealth subsystem.
//!
//! Owns the *taking-files-back-out-of-`.bentodesk/`* half of the original
//! 1.x `hidden_items.rs`:
//!
//! - [`restore_file`] / [`restore_file_tracked`] — single-file restore
//!   (rename then copy+delete fallback for cross-drive desktops).
//! - [`restore_zone_items_with_dirs`] — bulk restore for one zone, walking
//!   the spec G identity ladder so ambiguous display names are dropped
//!   into a `skipped` report rather than silently moving the wrong file.
//! - [`reconcile_zone_items_with_dirs`] — inverse motion (desktop file →
//!   `.bentodesk/{zone_id}/`) used when a layout entry references a file
//!   that is still visible on the desktop.
//! - [`restore_all_hidden`] — 3-tier exit-safety drain (layout, manifest,
//!   directory scan) for graceful shutdown.
//! - [`verify_references`] — health check that returns the original paths
//!   whose hidden mirror is missing on disk.

use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use super::hide::{ensure_stealth, hidden_dir_for, unique_hidden_path};
use super::sync::{SafetyManifest, load_manifest, manifest_add, manifest_remove, save_manifest};
use super::{StealthConfig, StealthError, StealthEvent, paths_equal_str, paths_match, status};

mod model;

pub use model::{
    ReconcileReport, RestoreSkippedItem, RestoreSkippedReason, RestoreZoneItemsReport, StealthItem,
};

// ─── Identity ladder (spec G) ───────────────────────────────────────────

/// Result of resolving a single item's "where is this file right now"
/// question. Shadows 1.x `crate::commands::item::RestoreIdentity` so the
/// ladder behaves identically without a dependency on the app layer.
enum RestoreIdentity {
    /// Tier 1 — `original_path` resolves on disk.
    Original(PathBuf),
    /// Tier 2 — `hidden_path` resolves on disk.
    Hidden(PathBuf),
    /// Tier 3 — exactly one display-name match across `desktop_dir` AND
    /// the shallow scan of `hidden_root`.
    DisplayName(PathBuf),
    /// Tier 4 — multiple display-name matches; refusing to guess.
    AmbiguousDisplayName,
    /// Tier 5 — no match anywhere.
    Unrecognised,
}

/// Walk the spec G identity ladder for `item`.
///
/// The shallow scan of `hidden_root` (top-level only, no recursion into
/// zone subdirs) is the production behaviour of 1.x — the test suite for
/// `reconcile_zone_items_with_dirs` (and the architect's R8 split rationale)
/// pins it. Recursing would re-introduce the homonyms-in-different-zones
/// false-match that the per-zone subdirectory isolation was designed to
/// fix.
fn resolve_restore_identity(
    item: &StealthItem,
    desktop_dir: &Path,
    hidden_root: &Path,
) -> RestoreIdentity {
    // Tier 1 — original_path on disk?
    if let Some(orig) = item.original_path.as_deref() {
        let p = Path::new(orig);
        if p.exists() {
            return RestoreIdentity::Original(p.to_path_buf());
        }
    }

    // Tier 2 — hidden_path on disk?
    if let Some(hidden) = item.hidden_path.as_deref() {
        let p = Path::new(hidden);
        if p.exists() {
            return RestoreIdentity::Hidden(p.to_path_buf());
        }
    }

    // Tier 3/4 — shallow display-name scan on (desktop_dir, hidden_root).
    let mut matches: Vec<PathBuf> = Vec::new();

    let scan = |root: &Path, out: &mut Vec<PathBuf>| {
        if !root.is_dir() {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.eq_ignore_ascii_case(&item.name) {
                        out.push(path);
                    }
                }
            }
        }
    };

    scan(desktop_dir, &mut matches);
    scan(hidden_root, &mut matches);

    match matches.len() {
        0 => RestoreIdentity::Unrecognised,
        1 => RestoreIdentity::DisplayName(matches.into_iter().next().unwrap_or_default()),
        _ => RestoreIdentity::AmbiguousDisplayName,
    }
}

// ─── Single-file restore ────────────────────────────────────────────────

/// Restore one hidden file by moving it back to `original_path`.
///
/// Returns `Ok(())` on success or when the destination already holds a
/// file (treated as already-restored — the hidden copy is removed to avoid
/// duplication). Returns `Err` only on hard I/O failure.
pub fn restore_file(original_path: &str, hidden_path: &str) -> Result<(), StealthError> {
    let source = Path::new(hidden_path);
    let dest = Path::new(original_path);

    if !source.exists() {
        tracing::warn!(
            "Cannot restore — hidden file does not exist: {}",
            hidden_path
        );
        return Err(StealthError::RestoreSourceMissing {
            path: source.to_path_buf(),
        });
    }

    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| StealthError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
    }

    if dest.exists() {
        tracing::warn!(
            "Restore destination already exists, skipping rename: {}",
            original_path
        );
        let _ = std::fs::remove_file(source);
        return Ok(());
    }

    // Same-drive rename (instant). Fall back to copy + delete on EXDEV.
    if let Err(e) = std::fs::rename(source, dest) {
        tracing::warn!(
            "fs::rename failed for restore ({} -> {}): {}. Trying copy+delete fallback.",
            hidden_path,
            original_path,
            e
        );
        match std::fs::copy(source, dest) {
            Ok(_) => {
                if let Err(rm_err) = std::fs::remove_file(source) {
                    tracing::error!(
                        "Restore: copy ok but delete of hidden source failed for {}: {} — duplicate at {}",
                        hidden_path,
                        rm_err,
                        hidden_path
                    );
                }
            }
            Err(e2) => {
                tracing::error!(
                    "Restore failed ({} -> {}): {}",
                    hidden_path,
                    original_path,
                    e2
                );
                return Err(StealthError::Io {
                    path: source.to_path_buf(),
                    message: e2.to_string(),
                });
            }
        }
    }

    tracing::info!(
        "Restored desktop item: {} -> {}",
        hidden_path,
        original_path
    );
    Ok(())
}

/// Restore + remove the matching manifest entry. The 1.x bridge between
/// `restore_file` and `manifest_remove`.
pub fn restore_file_tracked(
    config: &StealthConfig,
    original_path: &str,
    hidden_path: &str,
    events: Option<&Sender<StealthEvent>>,
) -> Result<(), StealthError> {
    restore_file(original_path, hidden_path)?;
    let hdir = hidden_dir_for(config)?;
    if let Err(e) = manifest_remove(&hdir, original_path) {
        tracing::warn!("restore_file_tracked: manifest_remove failed for {original_path}: {e}");
    }
    if let Some(tx) = events {
        let _ = tx.send(StealthEvent::Restored {
            original: original_path.to_string(),
            hidden: hidden_path.to_string(),
        });
        let _ = tx.send(StealthEvent::StatusChanged(status()));
    }
    Ok(())
}

// ─── Bulk zone restore (used when a zone is deleted) ────────────────────

/// Pure helper: restore every item under one zone, walking the spec G
/// identity ladder. Mirrors 1.x `restore_zone_items_with_dirs`.
///
/// Test-friendly because it takes the desktop and hidden roots explicitly
/// and never touches the manifest. The production wrapper [`restore_zone_items`]
/// composes manifest cleanup on top.
pub fn restore_zone_items_with_dirs(
    items: &[StealthItem],
    desktop_dir: &Path,
    hidden_root: &Path,
) -> RestoreZoneItemsReport {
    let mut report = RestoreZoneItemsReport::default();

    for item in items {
        let identity = resolve_restore_identity(item, desktop_dir, hidden_root);

        match identity {
            RestoreIdentity::Original(path) => {
                // Tier 1 — destination is the authoritative path. The
                // hidden mirror (if any) is the file we move from.
                let dest = path.to_string_lossy().into_owned();
                if let Some(hidden) = item.hidden_path.as_deref() {
                    tracing::info!("  Zone delete restore (Tier 1): {} -> {}", hidden, dest);
                    if restore_file(&dest, hidden).is_ok() {
                        report.restored += 1;
                    } else {
                        tracing::error!("  Zone delete restore FAILED for: {}", dest);
                    }
                } else {
                    tracing::info!(
                        "  Zone delete restore (Tier 1, no-op): {} already on desktop",
                        dest
                    );
                    report.restored += 1;
                }
            }
            RestoreIdentity::Hidden(path) => {
                // Tier 2 — file lives in `.bentodesk/<zone>/`. Destination
                // is `original_path` if known, otherwise we synthesise one
                // inside `desktop_dir` from the file name so the user sees
                // it again instead of losing track of it.
                let source = path.to_string_lossy().into_owned();
                let dest = item.original_path.clone().unwrap_or_else(|| {
                    desktop_dir
                        .join(path.file_name().unwrap_or_default())
                        .to_string_lossy()
                        .into_owned()
                });
                tracing::info!("  Zone delete restore (Tier 2): {} -> {}", source, dest);
                if restore_file(&dest, &source).is_ok() {
                    report.restored += 1;
                } else {
                    tracing::error!("  Zone delete restore FAILED for: {}", dest);
                }
            }
            RestoreIdentity::DisplayName(path) => {
                // Tier 3 — single display-name match.
                let resolved = path.to_string_lossy().into_owned();
                if path.starts_with(desktop_dir) {
                    tracing::info!(
                        "  Zone delete restore (Tier 3, on-desktop): {} already visible",
                        resolved
                    );
                    report.restored += 1;
                } else {
                    let dest = desktop_dir
                        .join(path.file_name().unwrap_or_default())
                        .to_string_lossy()
                        .into_owned();
                    tracing::info!("  Zone delete restore (Tier 3): {} -> {}", resolved, dest);
                    if restore_file(&dest, &resolved).is_ok() {
                        report.restored += 1;
                    } else {
                        tracing::error!("  Zone delete restore FAILED for: {}", dest);
                    }
                }
            }
            RestoreIdentity::AmbiguousDisplayName => {
                tracing::warn!(
                    "  Zone delete restore SKIPPED (ambiguous): item {} ({})",
                    item.id,
                    item.name
                );
                report.skipped.push(RestoreSkippedItem {
                    item_id: item.id.clone(),
                    item_name: item.name.clone(),
                    reason: RestoreSkippedReason::AmbiguousDisplayName,
                });
            }
            RestoreIdentity::Unrecognised => {
                tracing::warn!(
                    "  Zone delete restore SKIPPED (unrecognised): item {} ({})",
                    item.id,
                    item.name
                );
                report.skipped.push(RestoreSkippedItem {
                    item_id: item.id.clone(),
                    item_name: item.name.clone(),
                    reason: RestoreSkippedReason::Unrecognised,
                });
            }
        }
    }

    report
}

/// Production wrapper for [`restore_zone_items_with_dirs`] that also
/// removes manifest entries for every attempted item.
pub fn restore_zone_items(
    config: &StealthConfig,
    items: &[StealthItem],
) -> Result<RestoreZoneItemsReport, StealthError> {
    let hdir = hidden_dir_for(config)?;
    let desktop_dir = PathBuf::from(config.desktop_path.as_str());

    let report = restore_zone_items_with_dirs(items, &desktop_dir, &hdir);

    // `manifest_remove` is a no-op for entries it cannot find, so calling
    // it for every item (including skipped ones) is safe and keeps the
    // manifest converging on the layout state.
    for item in items {
        if let Some(orig) = item.original_path.as_deref() {
            if let Err(e) = manifest_remove(&hdir, orig) {
                tracing::warn!("restore_zone_items: manifest_remove failed for {orig}: {e}");
            }
        }
    }

    Ok(report)
}

// ─── Reconcile (inverse of restore) ─────────────────────────────────────

/// Pure helper: reconcile a zone's items against on-disk reality.
///
/// Mirrors 1.x `reconcile_zone_items_with_dirs`. Per-item logic:
/// 1. If `hidden_path` exists on disk → already managed.
/// 2. Else if `original_path` exists on disk and is inside `desktop_dir`
///    → move it into `hidden_dir/zone_id/{filename}` (with collision
///    suffix), update `item.hidden_path`, increment `reconciled_count`.
/// 3. Else → flag `item.file_missing = true`.
///
/// `hidden_dir` corresponds to `.bentodesk/` (NOT a per-zone subdir). The
/// helper creates `hidden_dir/zone_id/` if it does not yet exist; stealth
/// attribute application is the caller's responsibility (the production
/// wrapper [`reconcile_zone_items`] handles it).
pub fn reconcile_zone_items_with_dirs(
    items: &mut [StealthItem],
    zone_id: &str,
    desktop_dir: &Path,
    hidden_dir: &Path,
) -> ReconcileReport {
    let mut report = ReconcileReport::default();

    for item in items.iter_mut() {
        // Tier 1 — already hidden on disk: nothing to do.
        if let Some(hidden) = item.hidden_path.as_deref() {
            if Path::new(hidden).exists() {
                report.already_managed_count += 1;
                continue;
            }
            tracing::warn!(
                "reconcile: stale hidden_path for item {} ({}) — physical file does not exist at {}, clearing field so Tier 2 retries",
                item.id,
                item.name,
                hidden
            );
            item.hidden_path = None;
        }

        // Tier 2 — original still on the user's desktop: physically move
        // it into the zone's hidden subfolder.
        //
        // Safety guard: ONLY reconcile items whose `original_path` is
        // located inside `desktop_dir`. A layout entry pointing at a
        // system path (e.g. C:\Program Files\foo.exe) would otherwise
        // get silently swept into `.bentodesk/{zone}/`.
        let original = match item.original_path.as_deref() {
            Some(o)
                if Path::new(o).exists()
                    && paths_match(Path::new(o).parent().unwrap_or(Path::new("")), desktop_dir) =>
            {
                Some(o.to_string())
            }
            _ => None,
        };

        if let Some(orig) = original {
            let zone_dir = hidden_dir.join(zone_id);
            if let Err(e) = std::fs::create_dir_all(&zone_dir) {
                tracing::error!(
                    "reconcile: failed to create zone dir {:?}: {} — skipping item {}",
                    zone_dir,
                    e,
                    item.id
                );
                report.missing_count += 1;
                item.file_missing = true;
                continue;
            }

            let source = Path::new(&orig);
            let dest = unique_hidden_path(&zone_dir, source);

            // Same-drive rename should be instant. Fall back to copy +
            // delete on cross-drive desktops.
            let moved = match std::fs::rename(source, &dest) {
                Ok(()) => true,
                Err(rename_err) => {
                    tracing::warn!(
                        "reconcile: fs::rename failed for {} -> {:?}: {} — trying copy+delete",
                        orig,
                        dest,
                        rename_err
                    );
                    match std::fs::copy(source, &dest) {
                        Ok(_) => match std::fs::remove_file(source) {
                            Ok(()) => true,
                            Err(rm_err) => {
                                tracing::error!(
                                    "reconcile: copy succeeded but delete of original failed for {}: {}. Removing copy to stay safe.",
                                    orig,
                                    rm_err
                                );
                                let _ = std::fs::remove_file(&dest);
                                false
                            }
                        },
                        Err(copy_err) => {
                            tracing::error!(
                                "reconcile: copy fallback failed for {} -> {:?}: {}",
                                orig,
                                dest,
                                copy_err
                            );
                            false
                        }
                    }
                }
            };

            if moved {
                let new_hidden = dest.to_string_lossy().into_owned();
                tracing::info!(
                    "reconcile: zone '{}' moved {} -> {}",
                    zone_id,
                    orig,
                    new_hidden
                );
                item.hidden_path = Some(new_hidden);
                item.file_missing = false;
                report.reconciled_count += 1;
            } else {
                report.missing_count += 1;
                item.file_missing = true;
            }
            continue;
        }

        // Tier 3 — neither hidden_path nor original_path resolves.
        if item.original_path.is_none() && item.hidden_path.is_none() {
            tracing::warn!(
                "reconcile: zone '{}' item {} ({}) has no path information",
                zone_id,
                item.id,
                item.name
            );
            report.unknown_count += 1;
        } else {
            tracing::warn!(
                "reconcile: zone '{}' item {} ({}) — both original and hidden paths missing on disk",
                zone_id,
                item.id,
                item.name
            );
            report.missing_count += 1;
        }
        item.file_missing = true;
    }

    report
}

/// Production wrapper: reconcile + manifest update + stealth re-stamp.
///
/// Mutates `items` in place. Caller is responsible for persisting the
/// surrounding layout once the pass completes.
pub fn reconcile_zone_items(
    config: &StealthConfig,
    items: &mut [StealthItem],
    zone_id: &str,
) -> Result<ReconcileReport, StealthError> {
    let hdir = hidden_dir_for(config)?;
    let desktop_dir = PathBuf::from(config.desktop_path.as_str());

    // Snapshot pre-pass `hidden_path` state for diffing.
    let prev_hidden: Vec<Option<String>> = items.iter().map(|i| i.hidden_path.clone()).collect();

    let report = reconcile_zone_items_with_dirs(items, zone_id, &desktop_dir, &hdir);

    // Stamp stealth on the zone subdir if any item landed there.
    if report.reconciled_count > 0 {
        let zone_dir = hdir.join(zone_id);
        if zone_dir.exists() {
            ensure_stealth(&zone_dir);
        }
    }

    // Add manifest entries for newly-managed items.
    for (idx, item) in items.iter().enumerate() {
        let was_hidden = prev_hidden
            .get(idx)
            .and_then(|p| p.as_deref())
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        let is_hidden_now = item
            .hidden_path
            .as_deref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);

        if !was_hidden && is_hidden_now {
            if let (Some(original), Some(hidden)) =
                (item.original_path.as_deref(), item.hidden_path.as_deref())
            {
                let file_size = std::fs::metadata(hidden).map(|m| m.len()).unwrap_or(0);
                let display_name = Path::new(hidden)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Err(e) = manifest_add(
                    &hdir,
                    super::sync::ManifestAddParams {
                        original_path: original,
                        hidden_path: hidden,
                        zone_id,
                        file_size_bytes: file_size,
                        display_name: &display_name,
                        icon_x: None,
                        icon_y: None,
                        file_type: "",
                    },
                ) {
                    tracing::warn!("reconcile: manifest_add failed for {original}: {e}");
                }
            }
        }
    }

    Ok(report)
}

// ─── 3-tier exit-safety drain ───────────────────────────────────────────

/// Restore ALL hidden items. 3-tier strategy:
///
/// 1. **Layout** — iterate provided `items` and move files back.
/// 2. **Manifest** — cross-check against safety manifest for items missed
///    by layout.
/// 3. **Directory scan** — sweep `.bentodesk/` for files not tracked by
///    either layout or manifest.
///
/// Called on application exit to leave the desktop in its original state.
/// 1.x took `&AppHandle` to read `state.layout`; the nano port takes the
/// flattened `items` slice that the caller assembles from its layout store.
pub fn restore_all_hidden(
    config: &StealthConfig,
    items: &[StealthItem],
    events: Option<&Sender<StealthEvent>>,
) -> Result<u32, StealthError> {
    let hdir = hidden_dir_for(config)?;
    let desktop_path = config.desktop_path.to_string();

    tracing::info!("=== restore_all_hidden: starting ===");

    // -- Tier 1: Restore from layout -----------------------------------
    let mut restored = 0u32;
    let mut failed = 0u32;
    let mut attempted_originals: Vec<String> = Vec::new();

    for item in items {
        if let (Some(orig), Some(hidden)) = (&item.original_path, &item.hidden_path) {
            attempted_originals.push(orig.clone());
            if restore_file(orig, hidden).is_ok() {
                restored += 1;
            } else {
                failed += 1;
            }
        }
    }

    tracing::info!("  Layout tier: restored={}, failed={}", restored, failed);

    // -- Tier 2: Restore from manifest --------------------------------
    let manifest = load_manifest(&hdir)?;
    let mut manifest_restored = 0u32;
    let mut manifest_failed = 0u32;

    for entry in &manifest.entries {
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
        if restore_file(&entry.original_path, &entry.hidden_path).is_ok() {
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

    // -- Tier 3: Directory scan --------------------------------------
    let mut scan_restored = 0u32;
    if hdir.exists() {
        scan_restored += scan_and_restore_orphans(&hdir, &desktop_path);

        if let Ok(entries) = std::fs::read_dir(&hdir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') {
                            continue;
                        }
                    }
                    scan_restored += scan_and_restore_orphans(&entry_path, &desktop_path);

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

    // Clear the manifest.
    if let Err(e) = save_manifest(&hdir, &SafetyManifest::default()) {
        tracing::warn!("restore_all_hidden: failed to clear manifest: {e}");
    }

    let total = restored + manifest_restored + scan_restored;
    tracing::info!(
        "=== restore_all_hidden: complete — {} total files restored ===",
        total
    );

    if let Some(tx) = events {
        let _ = tx.send(StealthEvent::RestoreAllComplete { total });
        let _ = tx.send(StealthEvent::StatusChanged(status()));
    }

    Ok(total)
}

/// Scan a single directory for orphaned files and move them to `desktop_path`.
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

        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name == "manifest.json" || name == "manifest.json.tmp" {
                continue;
            }
        }
        if file_path.is_dir() {
            continue;
        }

        if let Some(file_name) = file_path.file_name() {
            let dest = PathBuf::from(desktop_path).join(file_name);
            if !dest.exists() && std::fs::rename(&file_path, &dest).is_ok() {
                tracing::info!("  Scan tier: restored orphan {:?} -> {:?}", file_path, dest);
                count += 1;
            }
        }
    }

    count
}

/// Check whether a directory contains zero entries.
fn is_dir_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => false,
    }
}

// ─── Reference verification ─────────────────────────────────────────────

/// Return `original_path`s whose `hidden_path` is missing on disk.
pub fn verify_references(items: &[StealthItem]) -> Vec<String> {
    let mut missing = Vec::new();

    for item in items {
        if let Some(hidden) = &item.hidden_path {
            if !Path::new(hidden).exists() {
                tracing::warn!(
                    "Reference verification: hidden file missing — item='{}', hidden_path='{}'",
                    item.name,
                    hidden
                );
                if let Some(orig) = &item.original_path {
                    missing.push(orig.clone());
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

#[cfg(test)]
mod tests;
