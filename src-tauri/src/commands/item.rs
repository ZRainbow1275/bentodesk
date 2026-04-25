//! Item management commands.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::error::BentoDeskError;
use crate::guardrails;
use crate::hidden_items;
use crate::icon::protocol::extract_and_cache_fresh;
use crate::icon_positions;
use crate::layout::persistence::{BentoItem, GridPosition, ItemType};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

#[tauri::command]
pub async fn add_item(
    state: State<'_, AppState>,
    zone_id: String,
    path: String,
) -> Result<BentoItem, String> {
    // Security: validate that the file resides on any legitimate Desktop source
    // (user / public / OneDrive / override). Prevents a compromised frontend
    // from moving arbitrary system files into .bentodesk/.
    {
        let desktop_path = {
            let settings = state.settings.lock().map_err(|e| e.to_string())?;
            settings.desktop_path.clone()
        };
        if !hidden_items::is_desktop_path_with_custom(&path, Some(&desktop_path)) {
            let allowed_sources: Vec<String> =
                crate::desktop_sources::all_desktop_dirs(Some(&desktop_path))
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
            // The variant centralises the JSON envelope so any future command
            // that needs the same OUTSIDE_DESKTOP UX (e.g. a future drag-into
            // command) can return the same shape without re-deriving it.
            return Err(BentoDeskError::OutsideDesktop {
                path: path.clone(),
                allowed_sources,
            }
            .to_ipc_string());
        }
    }

    // Guardrail: verify item count stays within the safety envelope.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_add_items(&layout, &settings, &zone_id, 1)?;
    }

    let file_path = std::path::Path::new(&path);

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let item_type = if file_path.is_dir() {
        ItemType::Folder
    } else {
        match ext {
            "lnk" => ItemType::Shortcut,
            "exe" | "msi" => ItemType::Application,
            _ => ItemType::File,
        }
    };

    let name = if ext == "lnk" || ext == "url" {
        // Strip shortcut extension from display name
        file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    } else {
        file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }
    .unwrap_or_else(|| path.clone());

    // Extract and cache the icon (before hiding, so the file is still accessible).
    // Use fresh extraction to invalidate any stale generic icon from a prior cache.
    let icon_hash = extract_and_cache_fresh(&state.icon_cache, &path).map_err(|e| e.to_string())?;

    // Look up the icon's current desktop position before hiding.
    // The display name shown on the desktop matches the file_name (without
    // extension for .lnk/.url, with extension for everything else).
    let (icon_x, icon_y) = {
        let backup = state.icon_backup.lock().ok();
        let desktop_path = std::path::Path::new(&path);
        match backup.as_ref().and_then(|b| b.as_ref()) {
            Some(layout) => icon_positions::lookup_icon_position_for_path(layout, desktop_path)
                .map(|(x, y)| (Some(x), Some(y)))
                .unwrap_or((None, None)),
            None => (None, None),
        }
    };

    // Hide the file from the Desktop by moving it into .bentodesk/{zone_id}/ subfolder.
    let (original_path, hidden_path) =
        match hidden_items::hide_file(&state.app_handle, &path, &zone_id) {
            Some((orig, hidden)) => (Some(orig), Some(hidden)),
            None => (None, None),
        };

    let item = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let row = zone.items.len() as u32 / zone.grid_columns;
        let col = zone.items.len() as u32 % zone.grid_columns;

        // path stores the file's CURRENT location (hidden_path if hidden succeeded,
        // original path otherwise). This ensures open_file and icon extraction work.
        let effective_path = hidden_path.clone().unwrap_or_else(|| path.clone());
        let item = BentoItem {
            id: uuid::Uuid::new_v4().to_string(),
            zone_id: zone_id.clone(),
            item_type,
            name,
            path: effective_path,
            icon_hash,
            grid_position: GridPosition {
                col,
                row,
                col_span: 1,
            },
            is_wide: false,
            added_at: chrono::Utc::now().to_rfc3339(),
            original_path,
            hidden_path,
            icon_x,
            icon_y,
            file_missing: false,
        };

        zone.items.push(item.clone());
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        item
    };
    state.persist_layout();

    // Sync zone metadata to manifest for complete backup
    hidden_items::sync_zone_metadata(&state.app_handle);

    timeline_hook::record_change(&state.app_handle, "item_add");

    Ok(item)
}

#[tauri::command]
pub async fn remove_item(
    state: State<'_, AppState>,
    zone_id: String,
    item_id: String,
) -> Result<(), String> {
    // Extract hidden file info and icon position before removing from layout
    let hidden_info: Option<(String, String, Option<i32>, Option<i32>, bool)> = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.items
            .iter()
            .find(|i| i.id == item_id)
            .and_then(|item| match (&item.original_path, &item.hidden_path) {
                (Some(orig), Some(hidden)) => Some((
                    orig.clone(),
                    hidden.clone(),
                    item.icon_x,
                    item.icon_y,
                    item.file_missing,
                )),
                _ => None,
            })
    };

    // Restore hidden file back to Desktop (move from .bentodesk/ to original path).
    // If the file is missing (deleted externally), skip restore and just remove the item.
    if let Some((ref original, ref hidden, icon_x, icon_y, file_missing)) = hidden_info {
        if !file_missing {
            if !hidden_items::restore_file_tracked(&state.app_handle, original, hidden) {
                return Err(format!(
                    "Cannot remove item: failed to restore file. \
                     The file is safe in .bentodesk/ and will be \
                     restored on exit."
                ));
            }
            // Restore succeeded -- optionally set icon position
            if let (Some(x), Some(y)) = (icon_x, icon_y) {
                if let Err(e) = icon_positions::set_single_icon_position_for_path(
                    std::path::Path::new(original),
                    x,
                    y,
                ) {
                    tracing::warn!("Failed to restore icon position for '{}': {e}", original);
                }
            }
        }
    }

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let len_before = zone.items.len();
        zone.items.retain(|item| item.id != item_id);
        if zone.items.len() == len_before {
            return Err(format!("Item not found: {item_id}"));
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_remove");
    Ok(())
}

#[tauri::command]
pub async fn move_item(
    state: State<'_, AppState>,
    from_zone_id: String,
    to_zone_id: String,
    item_id: String,
) -> Result<(), String> {
    // Guardrail: verify target zone can accept one more item.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_move_item_into_zone(&layout, &settings, &to_zone_id)?;
    }

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;

        // Find and remove from source zone
        let from_zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == from_zone_id)
            .ok_or_else(|| format!("Source zone not found: {from_zone_id}"))?;

        let item_idx = from_zone
            .items
            .iter()
            .position(|i| i.id == item_id)
            .ok_or_else(|| format!("Item not found: {item_id}"))?;

        let mut item = from_zone.items.remove(item_idx);
        from_zone.updated_at = chrono::Utc::now().to_rfc3339();

        // Add to target zone
        let to_zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == to_zone_id)
            .ok_or_else(|| format!("Target zone not found: {to_zone_id}"))?;

        // Physically move the hidden file to the new zone's subdirectory.
        // Clone the hidden_path string first to avoid borrow conflicts.
        if let (Some(hidden_str), true) = (item.hidden_path.clone(), item.original_path.is_some()) {
            let new_dir = hidden_items::zone_hidden_dir(&state.app_handle, &to_zone_id);
            if let Some(filename) = std::path::Path::new(&hidden_str).file_name() {
                let new_hidden = new_dir.join(filename);
                let old_path = std::path::Path::new(&hidden_str);
                // Avoid moving to the same location
                if old_path != new_hidden && old_path.exists() {
                    match std::fs::rename(old_path, &new_hidden) {
                        Ok(()) => {
                            let new_hidden_str = new_hidden.to_string_lossy().to_string();
                            item.hidden_path = Some(new_hidden_str.clone());
                            item.path = new_hidden_str;
                            tracing::info!(
                                "Moved hidden file between zones: {} -> {:?}",
                                hidden_str,
                                new_hidden
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to move hidden file between zones ({} -> {:?}): {}",
                                hidden_str,
                                new_hidden,
                                e
                            );
                            // Non-fatal: item stays in old zone dir but is logically in new zone
                        }
                    }
                }
            }
        }

        item.zone_id = to_zone_id;
        let row = to_zone.items.len() as u32 / to_zone.grid_columns;
        let col = to_zone.items.len() as u32 % to_zone.grid_columns;
        item.grid_position = GridPosition {
            col,
            row,
            col_span: if item.is_wide { 2 } else { 1 },
        };

        to_zone.items.push(item);
        to_zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_move");

    Ok(())
}

#[tauri::command]
pub async fn reorder_items(
    state: State<'_, AppState>,
    zone_id: String,
    item_ids: Vec<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let mut reordered = Vec::with_capacity(item_ids.len());
        for id in &item_ids {
            if let Some(item) = zone.items.iter().find(|i| &i.id == id) {
                reordered.push(item.clone());
            }
        }
        // Add any items not in the reorder list (shouldn't happen, but defensive)
        for item in &zone.items {
            if !item_ids.contains(&item.id) {
                reordered.push(item.clone());
            }
        }

        // Recalculate grid positions
        for (i, item) in reordered.iter_mut().enumerate() {
            let idx = i as u32;
            item.grid_position = GridPosition {
                col: idx % zone.grid_columns,
                row: idx / zone.grid_columns,
                col_span: if item.is_wide { 2 } else { 1 },
            };
        }

        zone.items = reordered;
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "item_reorder");

    Ok(())
}

#[tauri::command]
pub async fn toggle_item_wide(
    state: State<'_, AppState>,
    zone_id: String,
    item_id: String,
) -> Result<BentoItem, String> {
    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone_idx = layout
            .zones
            .iter()
            .position(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let item_idx = layout.zones[zone_idx]
            .items
            .iter()
            .position(|i| i.id == item_id)
            .ok_or_else(|| format!("Item not found: {item_id}"))?;

        let item = &mut layout.zones[zone_idx].items[item_idx];
        item.is_wide = !item.is_wide;
        item.grid_position.col_span = if item.is_wide { 2 } else { 1 };
        let result = item.clone();

        layout.zones[zone_idx].updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        result
    };
    state.persist_layout();

    Ok(result)
}

// ─── Layout-restore identity fallback (spec G) ───────────────────────────

/// Outcome of resolving a `BentoItem` against a real desktop directory at
/// layout-restore time. The variants encode every branch of the fallback
/// ladder so callers (and tests) can distinguish "we restored this" from
/// "we deliberately skipped this and logged a warning".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreIdentity {
    /// Tier 1 — `original_path` exists on the real desktop.
    Original(PathBuf),
    /// Tier 2 — `hidden_path` (the `.bentodesk/` mirror) exists on disk.
    Hidden(PathBuf),
    /// Tier 3 — only `name` matches and exactly one file on the desktop
    /// shares that display name, so the match is unambiguous.
    DisplayName(PathBuf),
    /// Tier 4 — `name` matches, multiple desktop files share that display
    /// name. We refuse to guess and emit a warning instead.
    AmbiguousDisplayName,
    /// Tier 5 — none of `original_path`, `hidden_path`, or `name` resolve
    /// against either the desktop or the hidden mirror. The item is skipped.
    Unrecognised,
}

/// Resolve a `BentoItem` to the real on-disk file the next layout restore
/// should target.
///
/// Priority chain (spec G — "图标恢复不能只按 display_name"):
/// 1. `original_path` — exists on the desktop, restore there.
/// 2. `hidden_path` — exists inside `.bentodesk/<zone>/`, restore from there.
/// 3. `name` — unique match against the desktop directory listing.
/// 4. `name` — multiple matches → refuse, log warning, skip.
/// 5. None of the above → unrecognised, skip with warning.
///
/// `desktop_dir` is the real desktop directory (or override) and
/// `hidden_dir` is the `.bentodesk/` root. Both must already exist on disk;
/// callers must NOT pass synthesized paths.
pub fn resolve_restore_identity(
    item: &BentoItem,
    desktop_dir: &Path,
    hidden_dir: &Path,
) -> RestoreIdentity {
    // Tier 1: original_path beats every other identifier when the file is
    // still sitting where the user originally placed it.
    if let Some(orig) = item
        .original_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(orig);
        if p.exists() {
            return RestoreIdentity::Original(p);
        }
    }

    // Tier 2: hidden_path under .bentodesk/<zone>/.
    if let Some(hidden) = item
        .hidden_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(hidden);
        if p.exists() {
            return RestoreIdentity::Hidden(p);
        }
    }

    // Tier 3 / 4: scan the real desktop directory for files whose visible
    // caption (file_name) matches `item.name`. Hidden mirror is consulted
    // too because items removed from the desktop may still live there.
    let display_name = item.name.trim();
    if display_name.is_empty() {
        tracing::warn!(
            "restore_identity: item {} has empty display name and no resolvable path",
            item.id
        );
        return RestoreIdentity::Unrecognised;
    }

    let mut matches = collect_display_name_matches(desktop_dir, display_name);
    matches.extend(collect_display_name_matches(hidden_dir, display_name));
    // Deduplicate canonicalised entries — a file could be referenced under
    // different relative roots but still resolve to the same inode.
    matches.sort();
    matches.dedup();

    match matches.len() {
        0 => {
            tracing::warn!(
                "restore_identity: item {} ({}) cannot be restored — no path or name match",
                item.id,
                item.name
            );
            RestoreIdentity::Unrecognised
        }
        1 => RestoreIdentity::DisplayName(matches.into_iter().next().expect("len==1")),
        _ => {
            tracing::warn!(
                "restore_identity: item {} ({}) skipped — {} candidates share the display name; refusing to guess",
                item.id,
                item.name,
                matches.len()
            );
            RestoreIdentity::AmbiguousDisplayName
        }
    }
}

/// List every file inside `dir` whose file name (caption) equals
/// `display_name`. Returns `Vec<PathBuf>` sorted by `sort()` so callers
/// can dedupe deterministically. Missing directory → empty vec.
fn collect_display_name_matches(dir: &Path, display_name: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().to_string();
            if name == display_name {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{GridPosition, ItemType};
    use std::fs;
    use std::io::Write;

    fn make_item(
        id: &str,
        name: &str,
        path: &str,
        original_path: Option<&str>,
        hidden_path: Option<&str>,
    ) -> BentoItem {
        BentoItem {
            id: id.to_string(),
            zone_id: "z".to_string(),
            item_type: ItemType::File,
            name: name.to_string(),
            path: path.to_string(),
            icon_hash: "h".to_string(),
            grid_position: GridPosition {
                col: 0,
                row: 0,
                col_span: 1,
            },
            is_wide: false,
            added_at: "2026-04-22T00:00:00Z".to_string(),
            original_path: original_path.map(String::from),
            hidden_path: hidden_path.map(String::from),
            file_missing: false,
            icon_x: None,
            icon_y: None,
        }
    }

    fn touch(path: &Path) {
        let mut f = fs::File::create(path).expect("create temp file");
        f.write_all(b"real-content").expect("write temp file");
    }

    fn setup_dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let desktop = tmp.path().join("desktop");
        let hidden = tmp.path().join(".bentodesk").join("zone-a");
        fs::create_dir_all(&desktop).expect("desktop dir");
        fs::create_dir_all(&hidden).expect("hidden dir");
        (tmp, desktop, hidden)
    }

    /// Tier 1 — `original_path` exists on the real desktop. Restore must
    /// target the original location, never fall through to lower tiers.
    #[test]
    fn tier_1_original_path_wins_when_present_on_desktop() {
        let (_tmp, desktop, hidden) = setup_dirs();
        let original = desktop.join("report.pdf");
        touch(&original);
        // A homonym in the hidden dir must NOT trick the resolver into
        // returning Tier 2 / 3 — Tier 1 is authoritative.
        touch(&hidden.join("report.pdf"));

        let item = make_item(
            "i-1",
            "report.pdf",
            &original.to_string_lossy(),
            Some(&original.to_string_lossy()),
            Some(&hidden.join("report.pdf").to_string_lossy()),
        );

        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::Original(original));
    }

    /// Tier 2 — `original_path` is missing on disk but `hidden_path`
    /// resolves. The item still lives in `.bentodesk/` so restore must
    /// target the hidden mirror.
    #[test]
    fn tier_2_hidden_path_used_when_original_missing() {
        let (_tmp, desktop, hidden) = setup_dirs();
        let hidden_file = hidden.join("notes.txt");
        touch(&hidden_file);
        // original_path was never recorded.
        let item = make_item(
            "i-2",
            "notes.txt",
            &hidden_file.to_string_lossy(),
            None,
            Some(&hidden_file.to_string_lossy()),
        );

        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::Hidden(hidden_file));
    }

    /// Tier 3 — neither path is recorded but the desktop has exactly one
    /// file matching the persisted display name. Caller can safely use it.
    #[test]
    fn tier_3_display_name_used_when_paths_missing_and_match_unique() {
        let (_tmp, desktop, hidden) = setup_dirs();
        let candidate = desktop.join("invoice.pdf");
        touch(&candidate);
        // Distractor with a different name -- must NOT match.
        touch(&desktop.join("other.pdf"));

        let item = make_item("i-3", "invoice.pdf", "", None, None);

        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::DisplayName(candidate));
    }

    /// Tier 4 — duplicate display names across desktop and hidden dirs.
    /// Resolver refuses to guess and surfaces `AmbiguousDisplayName` so
    /// the caller can log + skip rather than restoring the wrong file.
    #[test]
    fn tier_4_ambiguous_display_name_refuses_to_guess() {
        let (_tmp, desktop, hidden) = setup_dirs();
        // Two real, distinct files with the same caption -- different bytes,
        // different parent dirs. No sane heuristic can pick between them.
        touch(&desktop.join("draft.docx"));
        touch(&hidden.join("draft.docx"));

        let item = make_item("i-4", "draft.docx", "", None, None);

        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::AmbiguousDisplayName);
    }

    /// Tier 5 — every signal misses: paths point at vanished files and
    /// no desktop entry shares the display name. Resolver returns
    /// `Unrecognised` so the restore loop can skip cleanly.
    #[test]
    fn tier_5_completely_unknown_item_is_skipped() {
        let (_tmp, desktop, hidden) = setup_dirs();
        // Empty desktop + empty hidden dir.

        let item = make_item(
            "i-5",
            "ghost.bin",
            "",
            Some("Z:/never/existed.bin"),
            Some("Z:/also/missing.bin"),
        );

        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::Unrecognised);
    }

    /// Edge — empty display name and no paths must collapse to
    /// `Unrecognised` rather than scanning every file in the desktop dir.
    #[test]
    fn empty_name_with_no_paths_is_unrecognised() {
        let (_tmp, desktop, hidden) = setup_dirs();
        touch(&desktop.join("anything.txt"));

        let item = make_item("i-6", "   ", "", None, None);
        let identity = resolve_restore_identity(&item, &desktop, &hidden);
        assert_eq!(identity, RestoreIdentity::Unrecognised);
    }
}
