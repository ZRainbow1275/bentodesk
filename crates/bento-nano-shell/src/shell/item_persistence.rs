//! Native shell owner: `item_persistence`.

use super::*;

pub(super) fn flush_dirty_zones(root: &AppRoot) {
    let app = root.app.borrow();
    // #5 step 15 (2026-06-02) — DRAG JANK FIX. Suppress the synchronous
    // atomic disk write (temp-write + fsync + .bak copy) while a zone drag /
    // zone resize / item drag is in flight. The MoveZone/ResizeZone reducers
    // still update the in-memory z.x/z.y/z.w/z.h every frame, so the zone keeps
    // tracking the cursor 1:1 (no easing); only the per-frame fsync is deferred.
    // `handle_lbutton_up` re-marks dirty on gesture END (one write on release)
    // and WM_DESTROY covers the never-released edge. BulkMoveZones marks dirty
    // itself, so scripted/bulk moves still persist. The `dirty` flag is LEFT SET
    // here so the deferred write lands on the next non-gesture flush.
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
    {
        return;
    }
    if app.dirty.get() && !app.zones_path.as_os_str().is_empty() {
        let _ = storage::write_zones_atomic(&app.zones_path, &app.zones);
        app.dirty.set(false);
    }
}

pub(super) fn drain_backend_events(root: &AppRoot) -> bool {
    drain_desktop_events(root)
        | drain_live_folder_events(root)
        | drain_ghost_events(root)
        | drain_power_events(root)
        | drain_updater_events(root)
        | drain_rules_scheduler_events(root)
}

pub(super) struct HiddenItemPaths {
    pub(super) effective_path: String,
    pub(super) original_path: Option<String>,
    pub(super) hidden_path: Option<String>,
}

pub(super) fn hide_item_file(
    root: &AppRoot,
    zone_id: ZoneId,
    source_path: &str,
) -> HiddenItemPaths {
    let fallback = HiddenItemPaths {
        effective_path: source_path.to_owned(),
        original_path: None,
        hidden_path: None,
    };
    if !root.app.borrow().stealth_enabled.get() {
        tracing::debug!(
            target: "bentodesk::stealth",
            path = %source_path,
            "AddItem: stealth.enabled is false; keeping original path"
        );
        return fallback;
    }
    let Some(config) = stealth_config_for_source(root, source_path) else {
        tracing::warn!(
            target: "bentodesk::stealth",
            path = %source_path,
            "AddItem: stealth config unavailable; keeping original path"
        );
        return fallback;
    };

    match bento_nano_backend::stealth::hide_file(
        &config,
        source_path,
        &zone_id.0.to_string(),
        stealth_file_type(Path::new(source_path)),
        None,
        None,
        None,
    ) {
        Ok((original, hidden)) => HiddenItemPaths {
            effective_path: hidden.clone(),
            original_path: Some(original),
            hidden_path: Some(hidden),
        },
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::stealth",
                ?zone_id,
                path = %source_path,
                error = %e,
                "AddItem: stealth hide failed; keeping original path"
            );
            fallback
        }
    }
}

pub(super) fn restore_item_file(item: &bento_nano_zone::ZoneItem) -> bool {
    let (Some(original), Some(hidden)) =
        (item.original_path.as_deref(), item.hidden_path.as_deref())
    else {
        return true;
    };
    match bento_nano_backend::stealth::restore_file(original, hidden) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::stealth",
                original,
                hidden,
                error = %e,
                "RemoveItem: stealth restore failed"
            );
            false
        }
    }
}

pub(super) fn remove_item_from_zone(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bento_nano_app::ItemId,
) -> bool {
    let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
    let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
    let Some(item) = item else {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            ?item_id,
            "RemoveItem rejected: zone/item missing"
        );
        set_item_operation_status(
            root,
            localized_current(
                format!("移除项目失败：区域 {}，项目 {}", zone_id.0, item_id.0),
                format!(
                    "Remove item rejected: zone {} item {}",
                    zone_id.0, item_id.0
                ),
            ),
        );
        return true;
    };

    let display_path = item_file_display_path(&item);
    let leaf = item_operation_leaf(display_path.as_str()).to_owned();
    if !restore_item_file(&item) {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            ?item_id,
            "RemoveItem kept item because hidden file restore failed"
        );
        set_item_operation_status(
            root,
            localized_current(
                format!("移除项目失败：{leaf}"),
                format!("Remove item failed: {leaf}"),
            ),
        );
        return true;
    }

    let mut app = root.app.borrow_mut();
    if app.zones.remove_item(zone_id, zone_item_id) {
        app.mark_dirty();
        app.item_operation_status
            .borrow_mut()
            .replace(localized_current(
                format!("已移除项目：{leaf}"),
                format!("Removed item: {leaf}"),
            ));
        return true;
    }

    tracing::warn!(
        target: "bentodesk::items",
        ?zone_id,
        ?item_id,
        "RemoveItem rejected after restore: zone/item missing"
    );
    app.item_operation_status
        .borrow_mut()
        .replace(localized_current(
            format!("移除项目失败：{leaf}"),
            format!("Remove item rejected: {leaf}"),
        ));
    true
}

pub(super) fn open_item_file_from_zone(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bento_nano_app::ItemId,
) -> bool {
    open_item_file_from_zone_with(root, zone_id, item_id, shell_execute_path)
}

pub(super) fn open_item_file_from_zone_with<OpenPath>(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bento_nano_app::ItemId,
    mut open_path: OpenPath,
) -> bool
where
    OpenPath: FnMut(&str, &str, Option<&str>) -> Result<(), i32>,
{
    let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
    let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
    let Some(item) = item else {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            ?item_id,
            "OpenItemFile rejected: zone/item missing"
        );
        set_item_operation_status(
            root,
            localized_current(
                format!("打开项目失败：区域 {}，项目 {}", zone_id.0, item_id.0),
                format!("Open item rejected: zone {} item {}", zone_id.0, item_id.0),
            ),
        );
        return true;
    };

    let display_path = item_file_display_path(&item);
    if item.file_missing {
        let leaf = item_operation_leaf(display_path.as_str());
        set_item_operation_status(
            root,
            localized_current(
                format!("打开项目失败：找不到 {leaf}"),
                format!("Open item rejected: {leaf} missing"),
            ),
        );
        return true;
    }

    let filesystem_path = item_filesystem_path(&item).to_owned();
    let result = open_path("open", filesystem_path.as_str(), None);
    log_open_item_file_launch(
        zone_id,
        item_id,
        display_path.as_str(),
        filesystem_path.as_str(),
        result,
    );
    set_shell_launch_status(root, "Open", display_path.as_str(), result);
    true
}

pub(super) fn set_item_operation_status(root: &AppRoot, status: SmolStr) {
    let app = root.app.borrow();
    app.item_operation_status.borrow_mut().replace(status);
}

pub(super) fn item_operation_leaf(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

pub(super) fn add_item_to_zone(root: &AppRoot, zone_id: ZoneId, path: &str) -> bool {
    add_item_to_zone_with(root, zone_id, path, None, load_icon_hash_for_path)
}

pub(super) fn add_item_to_zone_with<IconHashForPath>(
    root: &AppRoot,
    zone_id: ZoneId,
    path: &str,
    desktop_source: Option<&str>,
    icon_hash_for_path: IconHashForPath,
) -> bool
where
    IconHashForPath: FnOnce(&bento_nano_app::ItemPath) -> Option<String>,
{
    let source = Path::new(path);
    let leaf = item_operation_leaf(path);
    if !bento_nano_backend::desktop_sources::is_under_any_desktop(source, desktop_source) {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            path,
            "AddItem rejected: path is outside recognised Desktop sources"
        );
        set_item_operation_status(
            root,
            localized_current(
                format!("添加项目失败：{leaf} 不在桌面目录中"),
                format!("Add item rejected outside Desktop: {leaf}"),
            ),
        );
        return true;
    }
    if !source.exists() {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            path,
            "AddItem rejected: source file does not exist"
        );
        set_item_operation_status(
            root,
            localized_current(
                format!("添加项目失败：找不到 {leaf}"),
                format!("Add item failed: missing {leaf}"),
            ),
        );
        return true;
    }

    let item_path = bento_nano_app::ItemPath::new(path);
    let icon_hash = icon_hash_for_path(&item_path).unwrap_or_default();
    let hidden = hide_item_file(root, zone_id, path);
    let mut app = root.app.borrow_mut();
    match app.zones.add_item_with_metadata(
        zone_id,
        std::borrow::Cow::Owned(hidden.effective_path.clone()),
        Some(path),
        std::borrow::Cow::Owned(icon_hash),
        hidden.original_path.map(std::borrow::Cow::Owned),
        hidden.hidden_path.map(std::borrow::Cow::Owned),
    ) {
        Some(item_id) => {
            app.mark_dirty();
            app.item_operation_status
                .borrow_mut()
                .replace(localized_current(
                    format!("已添加项目：{leaf}"),
                    format!("Added item: {leaf}"),
                ));
            tracing::info!(
                target: "bentodesk::items",
                ?zone_id,
                ?item_id,
                original = path,
                effective = %hidden.effective_path,
                "AddItem: item persisted into zone"
            );
            log_static(
                format!(
                    "AddItem: item persisted into zone zone_id={} item_id={} original={}\n",
                    zone_id.0, item_id.0, path
                )
                .as_str(),
            );
            true
        }
        None => {
            tracing::warn!(
                target: "bentodesk::items",
                ?zone_id,
                path,
                "AddItem rejected: zone missing or item id overflow"
            );
            app.item_operation_status
                .borrow_mut()
                .replace(localized_current(
                    format!("添加项目失败：区域 {}", zone_id.0),
                    format!("Add item rejected: zone {}", zone_id.0),
                ));
            true
        }
    }
}

pub(super) fn set_shell_launch_status(
    root: &AppRoot,
    action: &'static str,
    path: &str,
    result: Result<(), i32>,
) {
    let leaf = item_operation_leaf(path);
    log_shell_launch_status(action, path, result);
    match result {
        Ok(()) => set_item_operation_status(
            root,
            localized_current(
                format!(
                    "已请求{}：{leaf}",
                    if action == "Reveal" {
                        "定位"
                    } else {
                        "打开"
                    }
                ),
                format!("{action} requested: {leaf}"),
            ),
        ),
        Err(code) => set_item_operation_status(
            root,
            localized_current(
                format!(
                    "{}失败：{leaf}（ShellExecuteW：{code}）",
                    if action == "Reveal" {
                        "定位"
                    } else {
                        "打开"
                    }
                ),
                format!("{action} failed for {leaf}: ShellExecuteW failed: {code}"),
            ),
        ),
    }
}

pub(super) fn log_shell_launch_status(action: &'static str, path: &str, result: Result<(), i32>) {
    let leaf = item_operation_leaf(path);
    match result {
        Ok(()) => {
            log_static(format!("item-launch: {action} requested: {leaf}; path={path}\n").as_str())
        }
        Err(code) => log_static(
            format!("item-launch: {action} failed: {leaf}; code={code}; path={path}\n").as_str(),
        ),
    }
}

pub(super) fn log_open_item_file_launch(
    zone_id: ZoneId,
    item_id: bento_nano_app::ItemId,
    display_path: &str,
    filesystem_path: &str,
    result: Result<(), i32>,
) {
    match result {
        Ok(()) => log_static(
            format!(
                "item-launch: OpenItemFile requested: zone={} item={} display={} filesystem={}\n",
                zone_id.0, item_id.0, display_path, filesystem_path
            )
            .as_str(),
        ),
        Err(code) => log_static(
            format!(
                "item-launch: OpenItemFile failed: zone={} item={} code={} display={} filesystem={}\n",
                zone_id.0, item_id.0, code, display_path, filesystem_path
            )
            .as_str(),
        ),
    }
}

pub(super) fn item_filesystem_path(item: &bento_nano_zone::ZoneItem) -> &str {
    item.hidden_path
        .as_deref()
        .unwrap_or_else(|| item.path.as_ref())
}

pub(super) fn item_file_display_path(item: &bento_nano_zone::ZoneItem) -> String {
    item.original_path
        .as_deref()
        .unwrap_or_else(|| item.path.as_ref())
        .to_owned()
}

pub(super) fn normalized_rename_leaf(candidate: &str) -> Result<String, &'static str> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Err("empty name");
    }
    if trimmed == "." || trimmed == ".." {
        return Err("reserved name");
    }
    if trimmed.ends_with('.') || trimmed.ends_with(' ') {
        return Err("trailing dot/space");
    }
    if trimmed.chars().count() > 255 {
        return Err("name too long");
    }
    if trimmed.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return Err("invalid filename character");
    }
    Ok(trimmed.to_owned())
}

pub(super) fn localized_rename_validation_error(error: &str, zh: bool) -> &str {
    if !zh {
        return error;
    }
    match error {
        "empty name" => "名称不能为空",
        "reserved name" => "不能使用保留名称",
        "trailing dot/space" => "名称不能以句点或空格结尾",
        "name too long" => "名称过长",
        "invalid filename character" => "名称包含 Windows 不允许的字符",
        _ => "名称无效",
    }
}

pub(super) fn renamed_peer_path(current_path: &str, new_leaf: &str) -> Result<PathBuf, SmolStr> {
    let current = Path::new(current_path);
    let Some(parent) = current.parent() else {
        return Err(SmolStr::new_static("parent folder unavailable"));
    };
    Ok(parent.join(new_leaf))
}
