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

// ─── Public report types ────────────────────────────────────────────────

/// Reasons a single restore was skipped (spec G identity ladder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreSkippedReason {
    /// Multiple on-disk files share the same display name and the layout
    /// entry has no `original_path` / `hidden_path` to disambiguate. The
    /// resolver refuses to guess; the caller surfaces a manual-action UI.
    AmbiguousDisplayName,
    /// No on-disk file matches the layout entry by any tier of the ladder.
    Unrecognised,
}

/// One skipped item bubbled up from [`restore_zone_items_with_dirs`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSkippedItem {
    pub item_id: String,
    pub item_name: String,
    pub reason: RestoreSkippedReason,
}

/// Outcome of [`restore_zone_items_with_dirs`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestoreZoneItemsReport {
    /// Number of items successfully moved back to their authoritative
    /// on-disk location (Tier 1 / 2 / 3).
    pub restored: u32,
    /// Items the spec G identity ladder refused to restore (Tier 4 / 5).
    pub skipped: Vec<RestoreSkippedItem>,
}

/// Outcome of [`reconcile_zone_items_with_dirs`].
///
/// Same shape as 1.x `ReconcileReport`. The frontend treats it as a status
/// payload — re-fetches `list_zones` when `reconciled_count > 0`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Items whose physical file was moved from `desktop_dir` into
    /// `.bentodesk/{zone_id}/` during this pass.
    pub reconciled_count: u32,
    /// Items already in `.bentodesk/`. No action taken.
    pub already_managed_count: u32,
    /// Items where neither `hidden_path` nor `original_path` resolves on
    /// disk. Marked `file_missing = true`.
    pub missing_count: u32,
    /// Items with no `original_path` AND no resolvable `hidden_path`.
    /// Counted alongside `missing_count` to avoid silent skips.
    pub unknown_count: u32,
}

/// Mutable item shape consumed by reconcile / restore.
///
/// 1.x reached into `crate::layout::persistence::BentoItem` directly via
/// `AppState`. The nano backend does NOT depend on app-layer types per the
/// layer rule (spec §15), so this is a minimal struct with only the fields
/// the stealth subsystem actually reads or writes. The caller (typically
/// `bento-nano-app::backend_bridge`) builds these from its own layout
/// store and writes the mutated `hidden_path` / `file_missing` back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthItem {
    /// Layout-unique item ID.
    pub id: String,
    /// Display name (used by the spec G identity ladder).
    pub name: String,
    /// Where the file originally lived on the user's desktop.
    pub original_path: Option<String>,
    /// Where the file currently lives in `.bentodesk/{zone_id}/`.
    pub hidden_path: Option<String>,
    /// `true` when the resolver could not find the file at any known path.
    pub file_missing: bool,
}

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
mod tests {
    use super::*;

    fn item(id: &str, name: &str, original: Option<&str>, hidden: Option<&str>) -> StealthItem {
        StealthItem {
            id: id.to_string(),
            name: name.to_string(),
            original_path: original.map(String::from),
            hidden_path: hidden.map(String::from),
            file_missing: false,
        }
    }

    fn touch_file(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, b"content").expect("touch file");
    }

    // Hand-rolled tempdir (no `tempfile` workspace dep).
    struct TmpDir(PathBuf);
    impl TmpDir {
        fn as_path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> TmpDir {
        let suffix = super::super::unique_suffix();
        let path =
            std::env::temp_dir().join(format!("bento-restore-{}-{}", std::process::id(), suffix));
        std::fs::create_dir_all(&path).expect("tempdir");
        TmpDir(path)
    }

    // ── restore_zone_items_with_dirs — spec G ──────────────────────

    #[test]
    fn restore_zone_items_skips_ambiguous_distractor_without_moving_files() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("desktop");
        let hidden_root = tmp.as_path().join(".bentodesk");
        let zone_hidden = hidden_root.join("z-test");
        std::fs::create_dir_all(&desktop).expect("desktop");
        std::fs::create_dir_all(&zone_hidden).expect("zone hidden");

        // Two homonyms scanned by the resolver's shallow walk.
        let desktop_homonym = desktop.join("report.pdf");
        let hidden_homonym = hidden_root.join("report.pdf");
        touch_file(&desktop_homonym);
        touch_file(&hidden_homonym);

        let ambiguous = item("ambig-1", "report.pdf", None, None);

        let recoverable_path = zone_hidden.join("notes.txt");
        touch_file(&recoverable_path);
        let recoverable_dest = desktop.join("notes.txt");
        let recoverable = item(
            "ok-1",
            "notes.txt",
            Some(&recoverable_dest.to_string_lossy()),
            Some(&recoverable_path.to_string_lossy()),
        );

        let report =
            restore_zone_items_with_dirs(&[ambiguous, recoverable], &desktop, &hidden_root);

        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].item_id, "ambig-1");
        assert_eq!(
            report.skipped[0].reason,
            RestoreSkippedReason::AmbiguousDisplayName
        );

        assert!(
            desktop_homonym.exists(),
            "desktop homonym must NOT be deleted"
        );
        assert!(hidden_homonym.exists(), "hidden homonym must NOT be moved");

        assert_eq!(report.restored, 1);
        assert!(recoverable_dest.exists());
        assert!(!recoverable_path.exists());
    }

    // ── reconcile_zone_items_with_dirs ─────────────────────────────

    #[test]
    fn reconcile_moves_real_desktop_files_into_zone_subfolder() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("Desktop");
        let hidden = desktop.join(".bentodesk");
        std::fs::create_dir_all(&desktop).expect("create desktop");

        let zone_id = "zone-1";
        let names = [
            "Steam.lnk",
            "Discord.lnk",
            "VSCode.lnk",
            "Brave.lnk",
            "OBS.lnk",
        ];

        let mut items: Vec<StealthItem> = names
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                let original = desktop.join(name);
                touch_file(&original);
                let stale_hidden = hidden
                    .join(zone_id)
                    .join(name)
                    .to_string_lossy()
                    .into_owned();
                item(
                    &format!("item-{idx}"),
                    name,
                    Some(&original.to_string_lossy()),
                    Some(&stale_hidden),
                )
            })
            .collect();

        let report = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);

        assert_eq!(report.reconciled_count, 5);
        assert_eq!(report.already_managed_count, 0);
        assert_eq!(report.missing_count, 0);
        assert_eq!(report.unknown_count, 0);

        let zone_dir = hidden.join(zone_id);
        assert!(zone_dir.is_dir());

        for (idx, name) in names.iter().enumerate() {
            let original = desktop.join(name);
            assert!(!original.exists(), "{name} should have moved");
            let it = &items[idx];
            assert!(!it.file_missing);
            let new_hidden = it.hidden_path.as_deref().expect("hidden_path");
            assert!(Path::new(new_hidden).exists());
            assert!(Path::new(new_hidden).starts_with(&zone_dir));
        }
    }

    #[test]
    fn reconcile_is_idempotent_after_first_pass() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("Desktop");
        let hidden = desktop.join(".bentodesk");
        std::fs::create_dir_all(&desktop).expect("create desktop");

        let zone_id = "zone-idem";
        let original = desktop.join("Notes.lnk");
        touch_file(&original);

        let mut items = vec![item(
            "i-1",
            "Notes.lnk",
            Some(&original.to_string_lossy()),
            None,
        )];

        let pass1 = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);
        assert_eq!(pass1.reconciled_count, 1);

        let pass2 = reconcile_zone_items_with_dirs(&mut items, zone_id, &desktop, &hidden);
        assert_eq!(pass2.reconciled_count, 0);
        assert_eq!(pass2.already_managed_count, 1);
        assert_eq!(pass2.missing_count, 0);
    }

    #[test]
    fn reconcile_flags_items_with_no_resolvable_path_as_missing() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("Desktop");
        let hidden = desktop.join(".bentodesk");
        std::fs::create_dir_all(&desktop).expect("create desktop");

        let mut items = vec![item(
            "ghost",
            "ghost.lnk",
            Some(&desktop.join("ghost.lnk").to_string_lossy()),
            Some(&hidden.join("zone-x").join("ghost.lnk").to_string_lossy()),
        )];

        let report = reconcile_zone_items_with_dirs(&mut items, "zone-x", &desktop, &hidden);

        assert_eq!(report.reconciled_count, 0);
        assert_eq!(report.missing_count, 1);
        assert_eq!(report.unknown_count, 0);
        assert!(items[0].file_missing);
    }

    #[test]
    fn reconcile_isolates_filenames_across_zones() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("Desktop");
        let hidden = desktop.join(".bentodesk");
        std::fs::create_dir_all(&desktop).expect("create desktop");

        let original = desktop.join("Settings.lnk");
        touch_file(&original);

        let mut zone_a_items = vec![item(
            "a-1",
            "Settings.lnk",
            Some(&original.to_string_lossy()),
            None,
        )];
        let report_a =
            reconcile_zone_items_with_dirs(&mut zone_a_items, "zone-a", &desktop, &hidden);
        assert_eq!(report_a.reconciled_count, 1);

        let mut zone_b_items = vec![item(
            "b-1",
            "Settings.lnk",
            Some(&original.to_string_lossy()),
            None,
        )];
        let report_b =
            reconcile_zone_items_with_dirs(&mut zone_b_items, "zone-b", &desktop, &hidden);
        assert_eq!(report_b.reconciled_count, 0);
        assert_eq!(report_b.missing_count, 1);
        assert!(zone_b_items[0].file_missing);

        assert!(hidden.join("zone-a").join("Settings.lnk").exists());
        assert!(!hidden.join("zone-b").join("Settings.lnk").exists());
    }

    // ── restore_file ────────────────────────────────────────────────

    #[test]
    fn restore_file_moves_back_to_original() {
        let tmp = tempdir();
        let hidden = tmp.as_path().join(".bentodesk").join("doc.txt");
        let original = tmp.as_path().join("desktop").join("doc.txt");
        touch_file(&hidden);

        restore_file(&original.to_string_lossy(), &hidden.to_string_lossy()).expect("restore");

        assert!(original.exists());
        assert!(!hidden.exists());
    }

    #[test]
    fn restore_file_skips_when_destination_already_exists() {
        let tmp = tempdir();
        let hidden = tmp.as_path().join(".bentodesk").join("doc.txt");
        let original = tmp.as_path().join("desktop").join("doc.txt");
        touch_file(&hidden);
        touch_file(&original);

        restore_file(&original.to_string_lossy(), &hidden.to_string_lossy()).expect("restore");

        // Destination preserved; hidden source removed to avoid duplication.
        assert!(original.exists());
        assert!(!hidden.exists());
    }

    #[test]
    fn restore_file_errors_when_source_missing() {
        let tmp = tempdir();
        let hidden = tmp.as_path().join("nonexistent.txt");
        let original = tmp.as_path().join("desktop").join("nonexistent.txt");
        let result = restore_file(&original.to_string_lossy(), &hidden.to_string_lossy());
        assert!(matches!(
            result,
            Err(StealthError::RestoreSourceMissing { .. })
        ));
    }

    // ── verify_references ──────────────────────────────────────────

    #[test]
    fn verify_references_reports_missing_hidden_files() {
        let tmp = tempdir();
        let present = tmp.as_path().join("present.txt");
        touch_file(&present);

        let items = vec![
            item(
                "ok",
                "present.txt",
                Some("/some/orig"),
                Some(&present.to_string_lossy()),
            ),
            item(
                "bad",
                "missing.txt",
                Some("/missing/orig"),
                Some("/non/existent.txt"),
            ),
        ];

        let missing = verify_references(&items);
        assert_eq!(missing, vec!["/missing/orig"]);
    }
}
