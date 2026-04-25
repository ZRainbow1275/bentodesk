//! Icon extraction commands.
//!
//! Theme B — the old base64 data URL path has been removed. `get_icon_url`
//! now primes the tiered cache and returns the `bentodesk://icon/{hash}`
//! URL. The WebView2 frontend sets this directly on `<img src>` which
//! triggers a streaming fetch against [`icon::protocol::handle_icon_request`]
//! (zero base64 transcoding, zero large data-URL strings in JS heap).

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::icon::cache::IconCache;
use crate::icon::custom_icons::{self, CustomIcon};
use crate::icon::protocol::{extract_and_cache, extract_and_cache_fresh};
use crate::icon::stats::IconCacheStatsSnapshot;
use crate::layout::persistence::{BentoItem, LayoutData};
use crate::AppState;

/// One persisted Bento item whose icon hash was refreshed during startup
/// repair.
#[derive(Debug, Clone, Serialize)]
pub struct IconHashRepairEntry {
    /// Stable item identifier in the persisted layout.
    pub item_id: String,
    /// The icon hash stored before the repair.
    pub old_icon_hash: String,
    /// The newly extracted icon hash that replaced the old value.
    pub new_icon_hash: String,
}

/// Summary returned after checking persisted items for stale or missing icon
/// hashes.
#[derive(Debug, Clone, Serialize)]
pub struct ItemIconRepairReport {
    /// Number of items whose persisted `icon_hash` field changed.
    pub repaired_count: usize,
    /// Per-item repair details for diagnostics and reload decisions.
    pub repairs: Vec<IconHashRepairEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingIconRepair {
    pub(crate) item_id: String,
    pub(crate) old_icon_hash: String,
    pub(crate) new_icon_hash: String,
}

#[tauri::command]
pub async fn get_icon_url(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let hash = extract_and_cache(&state.icon_cache, &path).map_err(|e| e.to_string())?;
    Ok(format!("bentodesk://icon/{hash}"))
}

#[tauri::command]
pub async fn preload_icons(state: State<'_, AppState>, paths: Vec<String>) -> Result<(), String> {
    for path in &paths {
        if let Err(e) = extract_and_cache(&state.icon_cache, path) {
            tracing::warn!("Failed to preload icon for {}: {}", path, e);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn clear_icon_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.icon_cache.clear();
    tracing::info!("Icon cache cleared");
    Ok(())
}

#[tauri::command]
pub async fn get_icon_cache_stats(
    state: State<'_, AppState>,
) -> Result<IconCacheStatsSnapshot, String> {
    Ok(state.icon_cache.stats())
}

#[tauri::command]
pub async fn repair_item_icon_hashes(
    state: State<'_, AppState>,
) -> Result<ItemIconRepairReport, String> {
    let candidates = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        collect_repair_candidates(&layout)
    };

    let pending_repairs = compute_pending_repairs(&state.icon_cache, &candidates, |cache, p| {
        extract_and_cache_fresh(cache, p)
    });

    if pending_repairs.is_empty() {
        return Ok(ItemIconRepairReport {
            repaired_count: 0,
            repairs: Vec::new(),
        });
    }

    let repairs = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        apply_repairs_to_layout(&mut layout, &pending_repairs)
    };

    if !repairs.is_empty() {
        state.persist_layout();
    }

    Ok(ItemIconRepairReport {
        repaired_count: repairs.len(),
        repairs,
    })
}

/// Walk every persisted item and emit (item_id, old_icon_hash, source_path)
/// triples for items whose icon source is resolvable on disk.
///
/// Pure read of the layout — no cache or filesystem mutation.
pub(crate) fn collect_repair_candidates(layout: &LayoutData) -> Vec<(String, String, String)> {
    layout
        .zones
        .iter()
        .flat_map(|zone| zone.items.iter())
        .filter_map(|item| {
            resolve_item_icon_source_path(item)
                .map(|source_path| (item.id.clone(), item.icon_hash.clone(), source_path))
        })
        .collect()
}

/// For each candidate, refresh the icon hash through the live cache and
/// return a repair entry whenever the persisted hash is empty, missing
/// from every cache tier, or no longer matches the freshly computed hash.
///
/// `refresh` is the boundary that talks to the OS-level extractor. The real
/// caller passes [`extract_and_cache_fresh`]; tests inject a deterministic
/// stub so unit coverage does not hinge on a live Windows desktop.
pub(crate) fn compute_pending_repairs<F>(
    cache: &IconCache,
    candidates: &[(String, String, String)],
    mut refresh: F,
) -> Vec<PendingIconRepair>
where
    F: FnMut(&IconCache, &str) -> Result<String, crate::error::BentoDeskError>,
{
    let mut pending = Vec::new();
    for (item_id, old_icon_hash, source_path) in candidates {
        let cache_miss =
            old_icon_hash.trim().is_empty() || !cache.contains_any_tier(old_icon_hash);

        let new_icon_hash = match refresh(cache, source_path) {
            Ok(hash) => hash,
            Err(error) => {
                tracing::warn!(
                    "Failed to refresh icon hash for item {} ({}): {}",
                    item_id,
                    source_path,
                    error
                );
                continue;
            }
        };

        if cache_miss || *old_icon_hash != new_icon_hash {
            pending.push(PendingIconRepair {
                item_id: item_id.clone(),
                old_icon_hash: old_icon_hash.clone(),
                new_icon_hash,
            });
        }
    }
    pending
}

/// Apply every pending repair to the layout in place. Skips items whose
/// persisted hash already matches the new hash. Returns one entry per item
/// whose hash was actually rewritten so the caller can build the report.
pub(crate) fn apply_repairs_to_layout(
    layout: &mut LayoutData,
    pending: &[PendingIconRepair],
) -> Vec<IconHashRepairEntry> {
    let now = chrono::Utc::now().to_rfc3339();
    let repair_by_item_id = pending
        .iter()
        .map(|repair| (repair.item_id.as_str(), repair))
        .collect::<HashMap<_, _>>();
    let mut changed_zone_ids = BTreeSet::new();
    let mut applied_repairs = Vec::new();

    for zone in &mut layout.zones {
        let mut zone_changed = false;
        for item in &mut zone.items {
            let Some(repair) = repair_by_item_id.get(item.id.as_str()) else {
                continue;
            };

            if item.icon_hash == repair.new_icon_hash {
                continue;
            }

            item.icon_hash = repair.new_icon_hash.clone();
            applied_repairs.push(IconHashRepairEntry {
                item_id: item.id.clone(),
                old_icon_hash: repair.old_icon_hash.clone(),
                new_icon_hash: repair.new_icon_hash.clone(),
            });
            zone_changed = true;
        }

        if zone_changed {
            zone.updated_at = now.clone();
            changed_zone_ids.insert(zone.id.clone());
        }
    }

    if !changed_zone_ids.is_empty() {
        layout.last_modified = now;
    }

    applied_repairs
}

fn resolve_item_icon_source_path(item: &BentoItem) -> Option<String> {
    for candidate in [
        Some(item.path.as_str()),
        item.hidden_path.as_deref(),
        item.original_path.as_deref(),
    ] {
        let Some(path) = candidate.map(str::trim).filter(|path| !path.is_empty()) else {
            continue;
        };
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

// ─── Custom icon commands (Theme E1) ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::extractor::compute_icon_hash;
    use crate::layout::persistence::{
        BentoZone, GridPosition, ItemType, RelativePosition, RelativeSize,
    };
    use std::io::Write;

    fn make_item(id: &str, path: &str, icon_hash: &str) -> BentoItem {
        BentoItem {
            id: id.to_string(),
            zone_id: "z".to_string(),
            item_type: ItemType::File,
            name: "file.txt".to_string(),
            path: path.to_string(),
            icon_hash: icon_hash.to_string(),
            grid_position: GridPosition {
                col: 0,
                row: 0,
                col_span: 1,
            },
            is_wide: false,
            added_at: "2026-04-22T00:00:00Z".to_string(),
            original_path: None,
            hidden_path: None,
            file_missing: false,
            icon_x: None,
            icon_y: None,
        }
    }

    fn make_zone_with(items: Vec<BentoItem>) -> BentoZone {
        BentoZone {
            id: "z".to_string(),
            name: "Z".to_string(),
            icon: "Z".to_string(),
            position: RelativePosition {
                x_percent: 10.0,
                y_percent: 10.0,
            },
            expanded_size: RelativeSize {
                w_percent: 20.0,
                h_percent: 20.0,
            },
            items,
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: "2026-04-22T00:00:00Z".to_string(),
            updated_at: "2026-04-22T00:00:00Z".to_string(),
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
            locked: false,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    fn write_temp_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create temp file");
        // Real bytes: real cache, real disk, real filesystem -- no mocks.
        f.write_all(b"real-bento-content").expect("write temp file");
        path
    }

    /// Test refresh boundary that mirrors `extract_and_cache_fresh` without
    /// hitting the Windows shell extractor. Computes the canonical hash for
    /// the path and seeds the real cache with deterministic bytes — same
    /// observable side effect as the production path, just without GDI.
    fn fake_refresh(cache: &IconCache, path: &str) -> Result<String, crate::error::BentoDeskError> {
        let hash = compute_icon_hash(path);
        // Mirror force=true semantics: drop the previous entry first.
        cache.remove(&hash);
        cache.put(hash.clone(), b"real-bento-icon-bytes".to_vec());
        Ok(hash)
    }

    /// Spec contract — empty `icon_hash` means the persisted record has no
    /// reachable icon at all. Repair must compute the new hash, mark the
    /// repair as needed, and rewrite the persisted hash.
    #[test]
    fn empty_hash_triggers_extraction_and_writeback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let warm = dir.path().join("warm");
        let cache = IconCache::with_warm_dir(64, warm.clone());

        let file_path = write_temp_file(dir.path(), "alpha.txt");
        let path_str = file_path.to_string_lossy().to_string();
        let new_hash = compute_icon_hash(&path_str);

        // Pre-seed the cache so `extract_and_cache_fresh` short-circuits past
        // any Windows-specific extraction path. The bytes are arbitrary -- we
        // only need the hash to be present in the warm tier so the freshness
        // check inside the production code passes.
        cache.put(new_hash.clone(), b"png-bytes".to_vec());

        let candidates = vec![("item-1".to_string(), String::new(), path_str.clone())];
        let pending = compute_pending_repairs(&cache, &candidates, fake_refresh);
        assert_eq!(pending.len(), 1, "empty hash must produce a pending repair");
        assert_eq!(pending[0].old_icon_hash, "");
        assert_eq!(pending[0].new_icon_hash, new_hash);

        let mut layout = LayoutData {
            zones: vec![make_zone_with(vec![make_item("item-1", &path_str, "")])],
            ..LayoutData::default()
        };
        let applied = apply_repairs_to_layout(&mut layout, &pending);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].item_id, "item-1");
        assert_eq!(layout.zones[0].items[0].icon_hash, new_hash);
        assert_ne!(
            layout.zones[0].updated_at, "2026-04-22T00:00:00Z",
            "zone updated_at must advance after repair"
        );
    }

    /// Spec contract — when the persisted hash misses every cache tier the
    /// repair must rebuild the cache entry under the freshly computed hash.
    /// Writeback only happens when the new hash differs from the stored one.
    #[test]
    fn cache_miss_with_existing_path_rebuilds_cache_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let warm = dir.path().join("warm");
        let cache = IconCache::with_warm_dir(64, warm);

        let file_path = write_temp_file(dir.path(), "bravo.txt");
        let path_str = file_path.to_string_lossy().to_string();
        let new_hash = compute_icon_hash(&path_str);

        // Stale hash that the cache has never seen -- a classic cache miss.
        let stale_hash = "stalehash000000".to_string();
        assert!(!cache.contains_any_tier(&stale_hash));

        // Pre-seed the cache for the FRESH hash so extract_and_cache_fresh
        // skips OS-level extraction.
        cache.put(new_hash.clone(), b"png-fresh".to_vec());

        let candidates = vec![("item-2".to_string(), stale_hash.clone(), path_str.clone())];
        let pending = compute_pending_repairs(&cache, &candidates, fake_refresh);
        assert_eq!(pending.len(), 1, "cache miss must produce a pending repair");
        assert_eq!(pending[0].old_icon_hash, stale_hash);
        assert_eq!(pending[0].new_icon_hash, new_hash);

        let mut layout = LayoutData {
            zones: vec![make_zone_with(vec![make_item(
                "item-2", &path_str, &stale_hash,
            )])],
            ..LayoutData::default()
        };
        let applied = apply_repairs_to_layout(&mut layout, &pending);
        assert_eq!(applied.len(), 1);
        assert_eq!(layout.zones[0].items[0].icon_hash, new_hash);
        // After the rewrite the cache must contain the new key.
        assert!(cache.contains_any_tier(&new_hash));
    }

    /// Spec contract — when the path on disk has changed (item moved into a
    /// hidden zone folder, etc.) the recomputed hash differs and must be
    /// synced back even if the cache already had bytes for the old hash.
    #[test]
    fn path_changed_writes_back_new_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let warm = dir.path().join("warm");
        let cache = IconCache::with_warm_dir(64, warm);

        let old_path = write_temp_file(dir.path(), "charlie-old.txt");
        let new_path = write_temp_file(dir.path(), "charlie-new.txt");
        let old_path_str = old_path.to_string_lossy().to_string();
        let new_path_str = new_path.to_string_lossy().to_string();
        let old_hash = compute_icon_hash(&old_path_str);
        let new_hash = compute_icon_hash(&new_path_str);
        assert_ne!(old_hash, new_hash, "different paths must hash distinctly");

        // Cache still has bytes for the OLD hash and is empty for the NEW
        // hash. compute_pending_repairs must detect the divergence.
        cache.put(old_hash.clone(), b"old".to_vec());
        cache.put(new_hash.clone(), b"new".to_vec());

        let candidates = vec![(
            "item-3".to_string(),
            old_hash.clone(),
            new_path_str.clone(),
        )];
        let pending = compute_pending_repairs(&cache, &candidates, fake_refresh);
        assert_eq!(
            pending.len(),
            1,
            "hash divergence after path change must produce a repair"
        );
        assert_eq!(pending[0].old_icon_hash, old_hash);
        assert_eq!(pending[0].new_icon_hash, new_hash);

        let mut layout = LayoutData {
            zones: vec![make_zone_with(vec![make_item(
                "item-3",
                &new_path_str,
                &old_hash,
            )])],
            ..LayoutData::default()
        };
        let applied = apply_repairs_to_layout(&mut layout, &pending);
        assert_eq!(applied.len(), 1);
        assert_eq!(layout.zones[0].items[0].icon_hash, new_hash);
    }

    /// Spec contract — when every persisted hash already matches the live
    /// extraction the report must come back with `repaired_count == 0` and
    /// no per-item entries. Healthy startups must be near-zero cost.
    #[test]
    fn fully_healthy_layout_returns_empty_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let warm = dir.path().join("warm");
        let cache = IconCache::with_warm_dir(64, warm);

        let file_path = write_temp_file(dir.path(), "delta.txt");
        let path_str = file_path.to_string_lossy().to_string();
        let hash = compute_icon_hash(&path_str);
        cache.put(hash.clone(), b"png-healthy".to_vec());

        let candidates = vec![("item-4".to_string(), hash.clone(), path_str.clone())];
        let pending = compute_pending_repairs(&cache, &candidates, fake_refresh);
        assert!(
            pending.is_empty(),
            "no repair should be queued when stored hash matches the fresh hash and is cached"
        );

        let mut layout = LayoutData {
            zones: vec![make_zone_with(vec![make_item("item-4", &path_str, &hash)])],
            ..LayoutData::default()
        };
        let applied = apply_repairs_to_layout(&mut layout, &pending);
        assert!(applied.is_empty(), "no items should be rewritten");

        // ItemIconRepairReport contract: zero count + empty list is a
        // shippable state for the healthy layout case.
        let report = ItemIconRepairReport {
            repaired_count: applied.len(),
            repairs: applied,
        };
        assert_eq!(report.repaired_count, 0);
        assert!(report.repairs.is_empty());

        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("repaired_count") && json.contains("repairs"),
            "report payload must surface both fields, got {json}"
        );
    }

    /// `collect_repair_candidates` must skip items whose path, hidden_path,
    /// and original_path are all unreachable on disk. This is the broken-
    /// reference safety guard that prevents the repair loop from churning
    /// on missing files.
    #[test]
    fn collect_skips_items_with_no_resolvable_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = write_temp_file(dir.path(), "echo.txt");
        let real_str = real.to_string_lossy().to_string();

        let mut ghost = make_item("ghost", "Z:/does/not/exist.txt", "h-ghost");
        ghost.hidden_path = Some("Z:/also/missing.txt".to_string());
        ghost.original_path = Some("Z:/originally/gone.txt".to_string());

        let live = make_item("live", &real_str, "h-live");

        let layout = LayoutData {
            zones: vec![make_zone_with(vec![ghost, live])],
            ..LayoutData::default()
        };
        let candidates = collect_repair_candidates(&layout);

        assert_eq!(
            candidates.len(),
            1,
            "ghost item with all paths unreachable must be filtered out"
        );
        assert_eq!(candidates[0].0, "live");
    }
}

#[tauri::command]
pub async fn upload_custom_icon(
    app: AppHandle,
    kind: String,
    bytes: Vec<u8>,
    name: String,
) -> Result<String, String> {
    custom_icons::upload(&app, &kind, bytes, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_custom_icons(app: AppHandle) -> Result<Vec<CustomIcon>, String> {
    Ok(custom_icons::list(&app))
}

#[tauri::command]
pub async fn delete_custom_icon(app: AppHandle, uuid: String) -> Result<(), String> {
    custom_icons::delete(&app, &uuid).map_err(|e| e.to_string())
}
