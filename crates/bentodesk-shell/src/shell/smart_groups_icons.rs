//! Native shell owner: `smart_groups_icons`.

use super::*;

pub(super) const SMART_GROUP_MIN_WIDTH_DIP: i32 = 320;
pub(super) const SMART_GROUP_MIN_HEIGHT_DIP: i32 = 220;

pub(super) fn smart_group_zone_dimensions(viewport_width: f32, viewport_height: f32) -> (i32, i32) {
    let viewport_width = viewport_width.round().max(1.0) as i32;
    let viewport_height = viewport_height.round().max(1.0) as i32;
    let width = ((viewport_width as f32 * 0.25).round() as i32)
        .max(SMART_GROUP_MIN_WIDTH_DIP)
        .min(viewport_width);
    let height = ((viewport_height as f32 * 0.45).round() as i32)
        .max(SMART_GROUP_MIN_HEIGHT_DIP)
        .min(viewport_height);
    (width, height)
}

/// Resolve the concrete Zone that owns a suggestor session. Tauri always
/// applies a reviewed suggestion to the Zone that opened the dialog; the tray
/// path falls back to the first Zone and creates one only on an empty layout.
pub(super) fn ensure_suggestor_target_zone(app: &mut AppState) -> ZoneId {
    if let Some(id) = selected_or_first_zone_id(app) {
        app.selected_zone.set(Some(id));
        return id;
    }

    let id = app.alloc_zone_id();
    let (width, height) = smart_group_zone_dimensions(app.viewport.width, app.viewport.height);
    let max_x = ((app.viewport.width.round() as i32) - width).max(0);
    let max_y = ((app.viewport.height.round() as i32) - height).max(0);
    let x = ((app.viewport.width * 0.30).round() as i32).clamp(0, max_x);
    let y = ((app.viewport.height * 0.20).round() as i32).clamp(0, max_y);
    let title = if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        "自动整理"
    } else {
        "Auto Organize"
    };
    let mut zone = Zone::new(id, Cow::Borrowed(title), x, y, width, height);
    zone.icon = Cow::Borrowed("lightning");
    app.zones.add(zone);
    app.selected_zone.set(Some(id));
    app.mark_dirty();
    id
}

/// Give background-created groups readable expanded geometry and distinct
/// deterministic origins. Explicit Auto Organize never reaches this path; it
/// opens the reviewable suggestor instead.
pub(super) fn layout_new_auto_group_zones(app: &mut AppState, ids: &[ZoneId]) -> usize {
    let positions = compute_bulk_layout_positions(BulkLayoutAlgorithm::Grid, ids.len());
    let viewport_width = app.viewport.width.max(1.0);
    let viewport_height = app.viewport.height.max(1.0);
    let (width, height) = smart_group_zone_dimensions(viewport_width, viewport_height);
    let max_x = ((viewport_width.round() as i32) - width).max(0);
    let max_y = ((viewport_height.round() as i32) - height).max(0);
    let mut changed = 0usize;
    for (index, id) in ids.iter().enumerate() {
        let Some((x_percent, y_percent)) = positions.get(index).copied() else {
            continue;
        };
        let Some(zone) = app.zones.get_mut(*id) else {
            continue;
        };
        let x = percent_to_logical(x_percent, viewport_width).clamp(0, max_x);
        let y = percent_to_logical(y_percent, viewport_height).clamp(0, max_y);
        if (zone.x, zone.y, zone.w, zone.h) != (x, y, width, height) {
            zone.x = x;
            zone.y = y;
            zone.w = width;
            zone.h = height;
            changed = changed.saturating_add(1);
        }
    }
    changed
}

/// Background smart-layout path used by the desktop watcher. It resolves real
/// Desktop folders, scans live files, and merges or materialises suggestions.
pub(super) fn auto_organize_desktop(root: &AppRoot) -> bool {
    let desktop_dirs = configured_desktop_sources_for_app(&root.app.borrow());
    if desktop_dirs.is_empty() {
        tracing::warn!(
            target: "bentodesk::auto_organize",
            "AutoOrganize: no Desktop sources resolved"
        );
        log_static("auto_organize: no desktop sources resolved\n");
        return false;
    }

    let mut files = Vec::new();
    for dir in &desktop_dirs {
        match bentodesk_backend::grouping::scan_desktop_files(dir) {
            Ok(mut scanned) => files.append(&mut scanned),
            Err(e) => tracing::warn!(
                target: "bentodesk::auto_organize",
                path = %dir.display(),
                error = %e,
                "AutoOrganize: Desktop scan failed for source"
            ),
        }
    }

    if files.is_empty() {
        tracing::info!(
            target: "bentodesk::auto_organize",
            sources = desktop_dirs.len(),
            "AutoOrganize: Desktop sources contained no groupable files"
        );
        log_static(
            format!(
                "auto_organize: no groupable files sources={}\n",
                desktop_dirs.len()
            )
            .as_str(),
        );
        return false;
    }

    let suggestions = bentodesk_backend::grouping::suggest_groups(&files);
    if suggestions.is_empty() {
        tracing::info!(
            target: "bentodesk::auto_organize",
            files = files.len(),
            "AutoOrganize: grouping backend produced no suggestions"
        );
        log_static(format!("auto_organize: no suggestions files={}\n", files.len()).as_str());
        return false;
    }

    let mut applied = 0usize;
    let mut merged_items = 0usize;
    let mut created_ids = Vec::new();
    let mut app = root.app.borrow_mut();
    for suggestion in &suggestions {
        if let Some((_zone_id, added)) =
            bentodesk_backend::grouping::merge_auto_group(suggestion, &mut app.zones)
        {
            merged_items = merged_items.saturating_add(added);
            continue;
        }
        match bentodesk_backend::grouping::apply_auto_group(suggestion, &mut app.zones) {
            Ok(new_id) => {
                let bump = new_id.0.saturating_add(1).max(1);
                if app.next_zone_id.get() <= new_id.0 {
                    app.next_zone_id.set(bump);
                }
                created_ids.push(new_id);
                applied += 1;
            }
            Err(e) => tracing::warn!(
                target: "bentodesk::auto_organize",
                name = %suggestion.name,
                error = %e,
                "AutoOrganize: apply_auto_group rejected suggestion"
            ),
        }
    }

    let laid_out = layout_new_auto_group_zones(&mut app, &created_ids);

    if applied > 0 || merged_items > 0 {
        app.mark_dirty();
        tracing::info!(
            target: "bentodesk::auto_organize",
            files = files.len(),
            suggestions = suggestions.len(),
            applied,
            merged_items,
            "AutoOrganize: backend suggestions applied or merged into zones"
        );
        log_static(
            format!(
                "auto_organize: applied={} merged_items={} suggestions={} files={}\n",
                applied,
                merged_items,
                suggestions.len(),
                files.len()
            )
            .as_str(),
        );
        if laid_out > 0 {
            log_static(
                format!("auto_organize: laid_out={laid_out} readable_geometry=true\n").as_str(),
            );
        }
        true
    } else {
        log_static(
            format!(
                "auto_organize: applied=0 suggestions={} files={}\n",
                suggestions.len(),
                files.len()
            )
            .as_str(),
        );
        false
    }
}

/// Direct selected-stack replacement for 1.x `load_icon` IPC. It uses the
/// process-global icon cache initialised at startup and calls the backend
/// Win32 extractor/cache pipeline with the concrete filesystem path.
pub(super) fn load_icon_hash_for_path(path: &bentodesk_app::ItemPath) -> Option<String> {
    let Some(cache) = bentodesk_backend::icon::cache_handle() else {
        tracing::warn!(
            target: "bentodesk::icon",
            path = %path.0,
            "LoadIcon: icon subsystem not initialised"
        );
        return None;
    };

    match bentodesk_backend::icon::protocol::extract_and_cache(&cache, path.0.as_str()) {
        Ok(hash) => {
            tracing::debug!(
                target: "bentodesk::icon",
                path = %path.0,
                %hash,
                "LoadIcon: extracted and cached icon"
            );
            Some(hash)
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::icon",
                path = %path.0,
                error = %e,
                "LoadIcon: backend extractor/cache failed"
            );
            None
        }
    }
}

pub(super) fn apply_loaded_item_icon(
    root: &AppRoot,
    path: &bentodesk_app::ItemPath,
    hash: &str,
) -> bool {
    let mut app = root.app.borrow_mut();
    let mut changed = false;
    let zone_ids: Vec<_> = app.zones.iter().map(|zone| zone.id).collect();
    for zone_id in zone_ids {
        if app.zones.set_item_icon_hash(
            zone_id,
            path.0.as_str(),
            std::borrow::Cow::Owned(hash.to_owned()),
        ) {
            changed = true;
        }
    }
    if changed {
        app.mark_dirty();
    }
    changed
}

pub(super) fn item_icon_startup_rehydrate_force(
    path: &str,
    icon_hash: &str,
    cache_has_icon: bool,
) -> Option<bool> {
    if icon_hash.starts_with("builtin:") {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    let shortcut_identity_changed = (lower.ends_with(".lnk") || lower.ends_with(".url"))
        && !icon_hash.is_empty()
        && icon_hash != bentodesk_backend::icon::protocol::icon_cache_key(path);
    if shortcut_identity_changed {
        Some(true)
    } else if icon_hash.is_empty() || !cache_has_icon {
        Some(false)
    } else {
        None
    }
}

pub(super) fn start_startup_icon_rehydrate(root: &AppRoot, hwnd: HWND) {
    let Some(cache) = bentodesk_backend::icon::cache_handle() else {
        return;
    };
    let mut paths = {
        let app = root.app.borrow();
        let mut paths = Vec::new();
        for zone in app.zones.iter() {
            for item in &zone.items {
                let icon_hash = item.icon_hash.as_ref();
                let cache_has_icon = !icon_hash.is_empty() && cache.contains_any_tier(icon_hash);
                if !item.path.is_empty()
                    && let Some(force) = item_icon_startup_rehydrate_force(
                        item.path.as_ref(),
                        icon_hash,
                        cache_has_icon,
                    )
                {
                    paths.push((item.path.to_string(), force));
                }
            }
        }
        paths
    };
    paths.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let mut unique_paths: Vec<(String, bool)> = Vec::with_capacity(paths.len());
    for (path, force) in paths {
        if let Some((previous_path, previous_force)) = unique_paths.last_mut()
            && *previous_path == path
        {
            *previous_force |= force;
            continue;
        }
        unique_paths.push((path, force));
    }
    let paths = unique_paths;
    if paths.is_empty() {
        log_static("icon: startup rehydrate cache-complete queued=0\n");
        return;
    }

    let queued = paths.len();
    let sender = root.dispatcher.sender();
    let raw_hwnd = hwnd as isize;
    log_static(format!("icon: startup rehydrate queued={queued}\n").as_str());
    let spawn = std::thread::Builder::new()
        .name("bento-icon-rehydrate".to_owned())
        .stack_size(512 * 1024)
        .spawn(move || {
            // Shell shortcut resolution and WIC PNG encoding both require a
            // COM apartment on the calling thread. The UI thread already owns
            // one; this short-lived worker must initialise its own STA.
            if let Err(error) = unsafe { OleInitialize(None) } {
                log_static(
                    format!("icon: startup rehydrate OLE init failed error={error}\n").as_str(),
                );
                return;
            }
            let mut results = Vec::with_capacity(queued);
            let mut extracted = 0usize;
            let mut failed = 0usize;
            let mut first_error = None;
            for (path, force) in paths {
                let result = if force {
                    bentodesk_backend::icon::protocol::extract_and_cache_fresh(&cache, &path)
                } else {
                    bentodesk_backend::icon::protocol::extract_and_cache(&cache, &path)
                };
                match result {
                    Ok(hash) => {
                        extracted = extracted.saturating_add(1);
                        results.push((path, hash));
                    }
                    Err(error) => {
                        failed = failed.saturating_add(1);
                        if first_error.is_none() {
                            first_error = Some(format!("{path}: {error}"));
                        }
                        tracing::warn!(
                            target: "bentodesk::icon",
                            %path,
                            %error,
                            "startup icon extraction failed; keeping retryable fallback"
                        );
                    }
                }
            }
            // SAFETY: balances the successful OleInitialize call above on the
            // same worker thread, after all Shell/WIC objects have been dropped.
            unsafe { OleUninitialize() };
            for (path, hash) in results {
                if sender
                    .send(Command::ApplyLoadedIcon {
                        path: bentodesk_app::ItemPath::new(path),
                        hash: SmolStr::new(hash),
                    })
                    .is_err()
                {
                    return;
                }
            }
            // SAFETY: the raw value came from the live Main HWND. PostMessageW
            // only queues a value; a destroyed HWND simply makes the call fail.
            let _ = unsafe {
                PostMessageW(
                    raw_hwnd as HWND,
                    WM_ICON_CACHE_READY,
                    WPARAM::default(),
                    LPARAM::default(),
                )
            };
            log_static(
                format!(
                    "icon: startup rehydrate completed={extracted} failed={failed} total={queued} first_error={}\n",
                    first_error.as_deref().unwrap_or("-")
                )
                .as_str(),
            );
        });
    if let Err(error) = spawn {
        tracing::warn!(
            target: "bentodesk::icon",
            %error,
            "failed to start startup icon rehydrate worker"
        );
    }
}
