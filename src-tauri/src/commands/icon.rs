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

use crate::icon::custom_icons::{self, CustomIcon};
use crate::icon::protocol::{extract_and_cache, extract_and_cache_fresh};
use crate::icon::stats::IconCacheStatsSnapshot;
use crate::layout::persistence::BentoItem;
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
struct PendingIconRepair {
    item_id: String,
    old_icon_hash: String,
    new_icon_hash: String,
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
        layout
            .zones
            .iter()
            .flat_map(|zone| zone.items.iter())
            .filter_map(|item| {
                resolve_item_icon_source_path(item)
                    .map(|source_path| (item.id.clone(), item.icon_hash.clone(), source_path))
            })
            .collect::<Vec<_>>()
    };

    let mut pending_repairs = Vec::new();
    for (item_id, old_icon_hash, source_path) in candidates {
        let cache_miss =
            old_icon_hash.trim().is_empty() || !state.icon_cache.contains_any_tier(&old_icon_hash);

        let new_icon_hash = match extract_and_cache_fresh(&state.icon_cache, &source_path) {
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

        if cache_miss || old_icon_hash != new_icon_hash {
            pending_repairs.push(PendingIconRepair {
                item_id,
                old_icon_hash,
                new_icon_hash,
            });
        }
    }

    if pending_repairs.is_empty() {
        return Ok(ItemIconRepairReport {
            repaired_count: 0,
            repairs: Vec::new(),
        });
    }

    let now = chrono::Utc::now().to_rfc3339();
    let repairs = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let repair_by_item_id = pending_repairs
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
            layout.last_modified = now.clone();
        }

        applied_repairs
    };

    if !repairs.is_empty() {
        state.persist_layout();
    }

    Ok(ItemIconRepairReport {
        repaired_count: repairs.len(),
        repairs,
    })
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
