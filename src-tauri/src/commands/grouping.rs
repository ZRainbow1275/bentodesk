//! Smart auto-grouping commands.

use std::path::Path;
use tauri::State;

use crate::grouping::scanner::{self, FileInfo};
use crate::grouping::suggestions::{self, SuggestedGroup};
use crate::layout::persistence::{AutoGroupRule, BentoItem, GridPosition, ItemType};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

#[tauri::command]
pub async fn scan_desktop(state: State<'_, AppState>) -> Result<Vec<FileInfo>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let desktop_path_str = settings.desktop_path.clone();
    drop(settings);
    let desktop_path = Path::new(&desktop_path_str);
    scanner::scan_desktop_files(desktop_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn suggest_groups(
    state: State<'_, AppState>,
    files: Vec<String>,
) -> Result<Vec<SuggestedGroup>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let desktop_path_str = settings.desktop_path.clone();
    drop(settings);
    let desktop_path = Path::new(&desktop_path_str);

    // Scan and filter to requested file paths
    let all_files = scanner::scan_desktop_files(desktop_path).map_err(|e| e.to_string())?;
    let filtered: Vec<FileInfo> = if files.is_empty() {
        all_files
    } else {
        all_files
            .into_iter()
            .filter(|f| files.contains(&f.path))
            .collect()
    };

    Ok(suggestions::suggest_groups(&filtered))
}

/// AI-powered recommendations (Theme E2-b). Runs the hierarchical clustering
/// signal on scanned desktop files, filtering by confidence threshold.
#[tauri::command]
pub async fn get_ai_recommendations(
    state: State<'_, AppState>,
    min_confidence: f64,
) -> Result<Vec<SuggestedGroup>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let desktop_path_str = settings.desktop_path.clone();
    drop(settings);
    let desktop_path = Path::new(&desktop_path_str);

    let files = scanner::scan_desktop_files(desktop_path).map_err(|e| e.to_string())?;
    let ai = crate::grouping::ai_recommender::clusters_to_suggestions(&files);
    let threshold = min_confidence.clamp(0.0, 1.0);
    Ok(ai
        .into_iter()
        .filter(|s| s.confidence >= threshold)
        .collect())
}

#[tauri::command]
pub async fn apply_auto_group(
    state: State<'_, AppState>,
    zone_id: String,
    rule: AutoGroupRule,
    selected_paths: Option<Vec<String>>,
) -> Result<Vec<BentoItem>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let desktop_path_str = settings.desktop_path.clone();
    drop(settings);

    let desktop_path = Path::new(&desktop_path_str);
    let files = scanner::scan_desktop_files(desktop_path).map_err(|e| e.to_string())?;

    // Filter files matching the rule. When `selected_paths` is provided the
    // user has manually narrowed the set via checkbox UI — respect that
    // exact list (after confirming each path still matches the rule).
    let matching: Vec<&FileInfo> = files
        .iter()
        .filter(|f| matches_rule(f, &rule))
        .filter(|f| match &selected_paths {
            Some(allow) => allow.iter().any(|p| p == &f.path),
            None => true,
        })
        .collect();

    // Collect paths that are already in the zone so we can skip them before
    // doing expensive icon extraction + hide operations.
    let existing_paths: Vec<String> = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.items.iter().map(|i| i.path.clone()).collect()
    };

    // Also check original_paths to avoid re-adding already-hidden items
    let existing_original_paths: Vec<String> = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.items
            .iter()
            .filter_map(|i| i.original_path.clone())
            .collect()
    };

    // For each matching file: extract icon, lookup desktop position, hide file.
    // This mirrors the logic in `add_item` so that auto-grouped files are
    // treated identically to manually-added files.
    struct PreparedItem {
        name: String,
        path: String,
        icon_hash: String,
        item_type: ItemType,
        original_path: Option<String>,
        hidden_path: Option<String>,
        icon_x: Option<i32>,
        icon_y: Option<i32>,
    }

    let mut prepared: Vec<PreparedItem> = Vec::new();
    for file in &matching {
        // Skip if already in this zone (by current path or original path)
        if existing_paths.contains(&file.path) || existing_original_paths.contains(&file.path) {
            continue;
        }

        let file_path = std::path::Path::new(&file.path);
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let item_type = if file.is_directory {
            ItemType::Folder
        } else {
            match ext {
                "lnk" => ItemType::Shortcut,
                "exe" | "msi" => ItemType::Application,
                _ => ItemType::File,
            }
        };

        let name = if ext == "lnk" || ext == "url" {
            file_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        } else {
            file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        }
        .unwrap_or_else(|| file.name.clone());

        // Extract icon (fresh, before hiding)
        let icon_hash =
            crate::icon::protocol::extract_and_cache_fresh(&state.icon_cache, &file.path)
                .map_err(|e| e.to_string())?;

        // Look up icon desktop position before hiding
        let (icon_x, icon_y) = {
            let backup = state.icon_backup.lock().ok();
            let display_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            match backup.as_ref().and_then(|b| b.as_ref()) {
                Some(layout) => crate::icon_positions::lookup_icon_position(layout, &display_name)
                    .map(|(x, y)| (Some(x), Some(y)))
                    .unwrap_or((None, None)),
                None => (None, None),
            }
        };

        // Hide the file from the desktop by moving it into .bentodesk/{zone_id}/
        let (original_path, hidden_path) =
            match crate::hidden_items::hide_file(&state.app_handle, &file.path, &zone_id) {
                Some((orig, hidden)) => (Some(orig), Some(hidden)),
                None => (None, None),
            };

        // path = file's current location (hidden if succeeded, original otherwise)
        let effective_path = hidden_path.clone().unwrap_or_else(|| file.path.clone());
        prepared.push(PreparedItem {
            name,
            path: effective_path,
            icon_hash,
            item_type,
            original_path,
            hidden_path,
            icon_x,
            icon_y,
        });
    }

    let added_items = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;

        let mut added = Vec::new();
        for prep in prepared {
            let idx = zone.items.len() as u32;
            let item = BentoItem {
                id: uuid::Uuid::new_v4().to_string(),
                zone_id: zone_id.clone(),
                item_type: prep.item_type,
                name: prep.name,
                path: prep.path,
                icon_hash: prep.icon_hash,
                grid_position: GridPosition {
                    col: idx % zone.grid_columns,
                    row: idx / zone.grid_columns,
                    col_span: 1,
                },
                is_wide: false,
                added_at: chrono::Utc::now().to_rfc3339(),
                original_path: prep.original_path,
                hidden_path: prep.hidden_path,
                icon_x: prep.icon_x,
                icon_y: prep.icon_y,
                file_missing: false,
            };

            zone.items.push(item.clone());
            added.push(item);
        }

        zone.auto_group = Some(rule);
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
        added
    };
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "grouping_apply");

    Ok(added_items)
}

/// Automatically add a newly created file to any zone whose auto_group rule
/// matches. Called by the frontend when the file watcher detects a new file.
///
/// Returns the list of (zone_id, item) pairs that were added.
#[tauri::command]
pub async fn auto_group_new_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<Vec<(String, BentoItem)>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    if !settings.auto_group_enabled {
        return Ok(Vec::new());
    }
    drop(settings);

    // Build a FileInfo for the new file
    let p = std::path::Path::new(&file_path);
    if !p.exists() {
        return Ok(Vec::new());
    }

    let metadata = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = p.extension().map(|e| e.to_string_lossy().to_string());

    let file_info = FileInfo {
        name: file_name.clone(),
        path: file_path.clone(),
        size: metadata.len(),
        file_type: if metadata.is_dir() {
            "directory".to_string()
        } else {
            extension.clone().unwrap_or_else(|| "unknown".to_string())
        },
        modified_at: String::new(),
        created_at: String::new(),
        is_directory: metadata.is_dir(),
        extension,
    };

    // Collect zones with auto_group rules that match this file
    let matching_zones: Vec<(String, crate::layout::persistence::AutoGroupRule)> = {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        layout
            .zones
            .iter()
            .filter_map(|z| {
                z.auto_group.as_ref().and_then(|rule| {
                    if matches_rule(&file_info, rule) {
                        // Check if the file is already in the zone (by path or original_path)
                        let already_exists = z.items.iter().any(|i| {
                            i.path == file_path || i.original_path.as_deref() == Some(&file_path)
                        });
                        if already_exists {
                            None
                        } else {
                            Some((z.id.clone(), rule.clone()))
                        }
                    } else {
                        None
                    }
                })
            })
            .collect()
    };

    if matching_zones.is_empty() {
        return Ok(Vec::new());
    }

    // Determine item type
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    let item_type = if metadata.is_dir() {
        ItemType::Folder
    } else {
        match ext {
            "lnk" => ItemType::Shortcut,
            "exe" | "msi" => ItemType::Application,
            _ => ItemType::File,
        }
    };

    let display_name = if ext == "lnk" || ext == "url" {
        p.file_stem().map(|s| s.to_string_lossy().to_string())
    } else {
        p.file_name().map(|n| n.to_string_lossy().to_string())
    }
    .unwrap_or_else(|| file_name.clone());

    // Extract icon
    let icon_hash = crate::icon::protocol::extract_and_cache_fresh(&state.icon_cache, &file_path)
        .map_err(|e| e.to_string())?;

    // Look up icon desktop position before hiding
    let (icon_x, icon_y) = {
        let backup = state.icon_backup.lock().ok();
        match backup.as_ref().and_then(|b| b.as_ref()) {
            Some(layout) => crate::icon_positions::lookup_icon_position(layout, &file_name)
                .map(|(x, y)| (Some(x), Some(y)))
                .unwrap_or((None, None)),
            None => (None, None),
        }
    };

    // Hide the file from the desktop by moving it into .bentodesk/{zone_id}/
    // Use the first matching zone's ID for physical storage.
    let primary_zone_id = &matching_zones[0].0;
    let (original_path, hidden_path) =
        match crate::hidden_items::hide_file(&state.app_handle, &file_path, primary_zone_id) {
            Some((orig, hidden)) => (Some(orig), Some(hidden)),
            None => (None, None),
        };

    // path = file's current location (hidden if succeeded, original otherwise)
    let effective_path = hidden_path.clone().unwrap_or_else(|| file_path.clone());
    // Add to each matching zone
    let mut added: Vec<(String, BentoItem)> = Vec::new();
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        for (zone_id, _rule) in &matching_zones {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == zone_id) {
                let idx = zone.items.len() as u32;
                let item = BentoItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    zone_id: zone_id.clone(),
                    item_type: item_type.clone(),
                    name: display_name.clone(),
                    path: effective_path.clone(),
                    icon_hash: icon_hash.clone(),
                    grid_position: GridPosition {
                        col: idx % zone.grid_columns,
                        row: idx / zone.grid_columns,
                        col_span: 1,
                    },
                    is_wide: false,
                    added_at: chrono::Utc::now().to_rfc3339(),
                    original_path: original_path.clone(),
                    hidden_path: hidden_path.clone(),
                    icon_x,
                    icon_y,
                    file_missing: false,
                };
                zone.items.push(item.clone());
                zone.updated_at = chrono::Utc::now().to_rfc3339();
                added.push((zone_id.clone(), item));
            }
        }
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();

    if !added.is_empty() {
        tracing::info!(
            "Auto-grouped '{}' into {} zone(s)",
            display_name,
            added.len()
        );
        timeline_hook::record_change(&state.app_handle, "grouping_auto_new_file");
    }

    Ok(added)
}

/// Check whether a file matches an auto-group rule.
fn matches_rule(file: &FileInfo, rule: &AutoGroupRule) -> bool {
    use crate::layout::persistence::GroupRuleType;

    match &rule.rule_type {
        GroupRuleType::Extension => {
            if let (Some(exts), Some(file_ext)) = (&rule.extensions, &file.extension) {
                exts.iter().any(|e| e == file_ext)
            } else {
                false
            }
        }
        GroupRuleType::NamePattern => {
            if let Some(pattern) = &rule.pattern {
                // Patterns from suggestions use a `^` prefix to indicate
                // "starts with" semantics (e.g. `^projecta`).  Strip the
                // anchor and perform a case-insensitive prefix match.
                let pat = pattern.strip_prefix('^').unwrap_or(pattern);
                file.name.to_lowercase().starts_with(&pat.to_lowercase())
            } else {
                false
            }
        }
        GroupRuleType::ModifiedDate => {
            // Date-based grouping matches all files (they are then sub-grouped by date)
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{AutoGroupRule, GroupRuleType};

    fn test_file(name: &str) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            path: format!("C:\\Desktop\\{name}"),
            size: 0,
            file_type: "unknown".to_string(),
            modified_at: String::new(),
            created_at: String::new(),
            is_directory: false,
            extension: None,
        }
    }

    #[test]
    fn matches_rule_name_pattern_with_caret_prefix() {
        let rule = AutoGroupRule {
            rule_type: GroupRuleType::NamePattern,
            pattern: Some("^projecta".to_string()),
            extensions: None,
        };
        // Should match files whose name starts with the prefix (case-insensitive)
        assert!(matches_rule(&test_file("ProjectA_report.pdf"), &rule));
        assert!(matches_rule(&test_file("projecta_data.csv"), &rule));
        // Should NOT match files that merely contain the prefix mid-name
        assert!(!matches_rule(&test_file("my_projecta.txt"), &rule));
        // Should NOT match unrelated files
        assert!(!matches_rule(&test_file("unrelated.doc"), &rule));
    }

    #[test]
    fn matches_rule_name_pattern_without_caret() {
        let rule = AutoGroupRule {
            rule_type: GroupRuleType::NamePattern,
            pattern: Some("report".to_string()),
            extensions: None,
        };
        // Without ^, still uses starts_with (graceful fallback)
        assert!(matches_rule(&test_file("report_final.pdf"), &rule));
        assert!(!matches_rule(&test_file("my_report.pdf"), &rule));
    }
}
