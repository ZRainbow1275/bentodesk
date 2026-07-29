//! Native shell owner: `item_file_ops`.

use super::*;

pub(super) fn rename_item_file(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bentodesk_app::ItemId,
    new_leaf: &str,
) -> bool {
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    let zone_item_id = bentodesk_zone::ZoneItemId(item_id.0);
    let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
    let Some(item) = item else {
        set_item_operation_status(
            root,
            SmolStr::new(if zh {
                "无法重命名：该项目已不在区域中".to_owned()
            } else {
                format!(
                    "Rename rejected: item {} is no longer in zone {}",
                    item_id.0, zone_id.0
                )
            }),
        );
        return true;
    };
    let new_leaf = match normalized_rename_leaf(new_leaf) {
        Ok(name) => name,
        Err(error) => {
            set_item_operation_status(
                root,
                SmolStr::new(if zh {
                    format!(
                        "无法重命名：{}",
                        localized_rename_validation_error(error, true)
                    )
                } else {
                    format!("Rename rejected: {error}")
                }),
            );
            return true;
        }
    };
    let source_path = item_filesystem_path(&item).to_owned();
    let source = Path::new(&source_path);
    if !source.exists() {
        let mut app = root.app.borrow_mut();
        if app.zones.mark_item_missing(item.path.as_ref(), true) {
            app.mark_dirty();
        }
        app.item_operation_status
            .borrow_mut()
            .replace(SmolStr::new(if zh {
                format!("无法重命名：找不到 {}", item.name)
            } else {
                format!("Rename failed: missing {}", item.name)
            }));
        return true;
    }
    let target = match renamed_peer_path(&source_path, &new_leaf) {
        Ok(path) => path,
        Err(error) => {
            set_item_operation_status(
                root,
                SmolStr::new(if zh {
                    "无法重命名：无法读取所在文件夹".to_owned()
                } else {
                    format!("Rename failed: {error}")
                }),
            );
            return true;
        }
    };
    if target == source {
        set_item_operation_status(
            root,
            SmolStr::new_static(if zh {
                "名称没有变化"
            } else {
                "Rename skipped: name unchanged"
            }),
        );
        return true;
    }
    if target.exists() {
        set_item_operation_status(
            root,
            SmolStr::new(if zh {
                format!("无法重命名：{new_leaf} 已存在")
            } else {
                format!("Rename failed: target exists: {new_leaf}")
            }),
        );
        return true;
    }
    if let Err(error) = std::fs::rename(source, &target) {
        tracing::warn!(
            target: "bentodesk::items",
            ?zone_id,
            ?item_id,
            source = %source.display(),
            target = %target.display(),
            error = %error,
            "RenameItemFile failed"
        );
        set_item_operation_status(
            root,
            SmolStr::new(if zh {
                format!("重命名失败：{error}")
            } else {
                format!("Rename failed: {error}")
            }),
        );
        return true;
    }

    let effective_path = target.to_string_lossy().to_string();
    let original_path = match item.original_path.as_deref() {
        Some(original) => match renamed_peer_path(original, &new_leaf) {
            Ok(path) => Some(std::borrow::Cow::Owned(path.to_string_lossy().to_string())),
            Err(error) => {
                set_item_operation_status(
                    root,
                    SmolStr::new(if zh {
                        "重命名失败：无法同步原始路径".to_owned()
                    } else {
                        format!("Rename failed: {error}")
                    }),
                );
                return true;
            }
        },
        None => None,
    };
    let hidden_path = item
        .hidden_path
        .as_ref()
        .map(|_| std::borrow::Cow::Owned(effective_path.clone()));
    let display_path = original_path
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| effective_path.clone());
    let icon_hash = load_icon_hash_for_path(&bentodesk_app::ItemPath::new(effective_path.as_str()))
        .unwrap_or_else(|| item.icon_hash.to_string());
    let mut app = root.app.borrow_mut();
    if app.zones.update_item_file_metadata(
        zone_id,
        zone_item_id,
        std::borrow::Cow::Owned(effective_path.clone()),
        Some(display_path.as_str()),
        original_path,
        hidden_path,
    ) {
        let _ = app.zones.set_item_icon_hash(
            zone_id,
            effective_path.as_str(),
            std::borrow::Cow::Owned(icon_hash),
        );
        app.mark_dirty();
        app.item_operation_status
            .borrow_mut()
            .replace(SmolStr::new(if zh {
                format!("已重命名为：{new_leaf}")
            } else {
                format!("Renamed file: {new_leaf}")
            }));
        log_static(
            format!(
                "item-file: RenameItemFile renamed zone={} item={} from={} to={}\n",
                zone_id.0, item_id.0, source_path, effective_path
            )
            .as_str(),
        );
        true
    } else {
        app.item_operation_status
            .borrow_mut()
            .replace(SmolStr::new_static(if zh {
                "重命名失败：项目已不在区域中"
            } else {
                "Rename failed: item disappeared"
            }));
        true
    }
}

pub(super) fn delete_item_file_to_recycle_bin(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bentodesk_app::ItemId,
) -> bool {
    delete_item_file_to_recycle_bin_using(root, zone_id, item_id, delete_path_to_recycle_bin)
}

pub(super) fn delete_item_file_to_recycle_bin_using<F>(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bentodesk_app::ItemId,
    recycle: F,
) -> bool
where
    F: FnOnce(&Path) -> Result<RecycleDeleteOutcome, FileOperationError>,
{
    let zone_item_id = bentodesk_zone::ZoneItemId(item_id.0);
    let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
    let Some(item) = item else {
        set_item_operation_status(
            root,
            localized_current(
                format!("删除失败：项目 {} 已不在区域 {} 中", item_id.0, zone_id.0),
                format!(
                    "Delete rejected: item {} is no longer in zone {}",
                    item_id.0, zone_id.0
                ),
            ),
        );
        return true;
    };
    let source_path = item_filesystem_path(&item).to_owned();
    let source = Path::new(&source_path);
    if !source.exists() {
        let mut app = root.app.borrow_mut();
        if app.zones.mark_item_missing(item.path.as_ref(), true) {
            app.mark_dirty();
        }
        app.item_operation_status
            .borrow_mut()
            .replace(localized_current(
                format!("删除失败：找不到 {}", item.name),
                format!("Delete failed: missing {}", item.name),
            ));
        return true;
    }
    match recycle(source) {
        Ok(RecycleDeleteOutcome::Recycled) => {
            let mut app = root.app.borrow_mut();
            if app.zones.remove_item(zone_id, zone_item_id) {
                app.mark_dirty();
            }
            app.item_operation_status
                .borrow_mut()
                .replace(localized_current(
                    format!("已删除文件：{}", item.name),
                    format!("Deleted file: {}", item.name),
                ));
            log_static(
                format!(
                    "item-file: DeleteItemFileToRecycleBin recycled zone={} item={} path={}\n",
                    zone_id.0, item_id.0, source_path
                )
                .as_str(),
            );
            true
        }
        Ok(RecycleDeleteOutcome::Aborted) => {
            log_static(
                format!(
                    "item-file: DeleteItemFileToRecycleBin aborted zone={} item={} path={}\n",
                    zone_id.0, item_id.0, source_path
                )
                .as_str(),
            );
            set_item_operation_status(root, localized_current("已取消删除", "Delete cancelled"));
            true
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::items",
                ?zone_id,
                ?item_id,
                path = %source.display(),
                error = %error,
                "DeleteItemFileToRecycleBin failed"
            );
            set_item_operation_status(
                root,
                localized_current(
                    format!("删除失败：{error}"),
                    format!("Delete failed: {error}"),
                ),
            );
            true
        }
    }
}

pub(super) struct HiddenMovePaths {
    pub(super) effective_path: String,
    pub(super) hidden_path: String,
}

pub(super) fn move_hidden_item_file_between_zones(
    root: &AppRoot,
    item: &bentodesk_zone::ZoneItem,
    to_zone_id: ZoneId,
) -> Option<HiddenMovePaths> {
    let original = item.original_path.as_deref()?;
    let hidden = item.hidden_path.as_deref()?;
    let source = Path::new(hidden);
    if !source.exists() {
        tracing::warn!(
            target: "bentodesk::stealth",
            hidden,
            ?to_zone_id,
            "MoveItemToZone: hidden file missing; moving layout only"
        );
        return None;
    }
    let config = stealth_config_for_source(root, original)?;
    let zone_dir =
        match bentodesk_backend::stealth::zone_hidden_dir_for(&config, &to_zone_id.0.to_string()) {
            Ok(dir) => dir,
            Err(e) => {
                tracing::warn!(
                    target: "bentodesk::stealth",
                    hidden,
                    ?to_zone_id,
                    error = %e,
                    "MoveItemToZone: target hidden dir unavailable; moving layout only"
                );
                return None;
            }
        };
    let file_name = source.file_name()?;
    let target = zone_dir.join(file_name);
    if source == target {
        let path = target.to_string_lossy().to_string();
        return Some(HiddenMovePaths {
            effective_path: path.clone(),
            hidden_path: path,
        });
    }
    if target.exists() {
        tracing::warn!(
            target: "bentodesk::stealth",
            hidden,
            target = %target.display(),
            "MoveItemToZone: target hidden file exists; moving layout only"
        );
        return None;
    }
    match std::fs::rename(source, &target) {
        Ok(()) => {
            let path = target.to_string_lossy().to_string();
            Some(HiddenMovePaths {
                effective_path: path.clone(),
                hidden_path: path,
            })
        }
        Err(rename_err) if source.is_file() => {
            match bentodesk_backend::stealth::copy_file_without_overwrite(source, &target) {
                Ok(_) => match std::fs::remove_file(source) {
                    Ok(()) => {
                        let path = target.to_string_lossy().to_string();
                        Some(HiddenMovePaths {
                            effective_path: path.clone(),
                            hidden_path: path,
                        })
                    }
                    Err(remove_err) => {
                        let _ = std::fs::remove_file(&target);
                        tracing::warn!(
                            target: "bentodesk::stealth",
                            hidden,
                            target = %target.display(),
                            rename_error = %rename_err,
                            remove_error = %remove_err,
                            "MoveItemToZone: copy succeeded but source removal failed; moving layout only"
                        );
                        None
                    }
                },
                Err(copy_err) => {
                    tracing::warn!(
                        target: "bentodesk::stealth",
                        hidden,
                        target = %target.display(),
                        rename_error = %rename_err,
                        copy_error = %copy_err,
                        "MoveItemToZone: hidden file move failed; moving layout only"
                    );
                    None
                }
            }
        }
        Err(rename_err) => {
            tracing::warn!(
                target: "bentodesk::stealth",
                hidden,
                target = %target.display(),
                error = %rename_err,
                "MoveItemToZone: hidden folder move failed; moving layout only"
            );
            None
        }
    }
}

pub(super) fn stealth_config_for_source(
    root: &AppRoot,
    source_path: &str,
) -> Option<bentodesk_backend::stealth::StealthConfig> {
    let desktop_path = Path::new(source_path)
        .parent()?
        .to_string_lossy()
        .to_string();
    let app_data_dir = {
        let app = root.app.borrow();
        app.zones_path.parent()?.to_string_lossy().to_string()
    };
    Some(bentodesk_backend::stealth::StealthConfig {
        desktop_path: smol_str::SmolStr::new(desktop_path),
        app_data_dir: smol_str::SmolStr::new(app_data_dir),
    })
}

/// M1e — build a `StealthConfig` for the current desktop, for the Settings
/// Stealth §7 Reapply action. Reuses `stealth_config_for_source` (which derives
/// `desktop_path` from `source_path.parent()`) by handing it a sentinel child
/// of the configured/primary Desktop directory, so the parent it strips back
/// to is exactly that Desktop dir. Returns `None` when no Desktop dir can be
/// resolved or `zones_path` has no parent — callers must handle it without
/// panicking.
pub(super) fn stealth_config_now(
    root: &AppRoot,
) -> Option<bentodesk_backend::stealth::StealthConfig> {
    // Prefer the user-configured desktop path; fall back to the first
    // discovered real Desktop directory.
    let configured = {
        let app = root.app.borrow();
        let draft = app.desktop_path_draft.borrow();
        let trimmed = draft.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    };
    let desktop_dir = configured.or_else(|| {
        bentodesk_backend::desktop_sources::all_desktop_dirs(None)
            .into_iter()
            .next()
    })?;
    // Hand the helper a child path so its `.parent()` yields `desktop_dir`.
    let sentinel = desktop_dir.join(".bentodesk-reapply");
    stealth_config_for_source(root, &sentinel.to_string_lossy())
}

/// M1e — re-read `bentodesk_backend::stealth::status()` (synchronous probe)
/// into the cached `app.stealth_status` snapshot. Called when Settings opens
/// and after Refresh/Reapply so the immediate-mode paint + hit-test read a
/// consistent snapshot.
pub(super) fn refresh_stealth_status(root: &AppRoot) {
    let status = bentodesk_backend::stealth::status();
    let app = root.app.borrow();
    *app.stealth_status.borrow_mut() = Some(status);
}

/// M1i 2026-05-29 — re-resolve the real Desktop sources and repopulate the
/// cached read-only §2 list on `AppState`. Called on Settings-open and on the
/// Refresh (`↻`) button (`RefreshDesktopSources`). Each resolved path is
/// classified via `desktop_sources::classify_desktop_source` and tagged as
/// watched, matching Tauri `collect_desktop_sources` where every
/// `all_desktop_dirs` source has the watcher attached. Runs ONCE per refresh —
/// never per frame.
pub(super) fn refresh_desktop_sources(root: &AppRoot) {
    let app = root.app.borrow();
    // Resolve the live sources, threading the user's custom override so a
    // non-standard `desktop_path` shows up as a Custom card.
    let custom = app.desktop_path_draft.borrow().clone();
    let custom_opt = if custom.trim().is_empty() {
        None
    } else {
        Some(custom.as_str())
    };
    let dirs = bentodesk_backend::desktop_sources::all_desktop_dirs(custom_opt);
    let rows = desktop_source_rows_for_settings(&dirs);
    let watched_count = rows.iter().filter(|(_, _, watched)| *watched).count();
    log_static(
        format!(
            "settings: desktop_sources count={} watched={}\n",
            rows.len(),
            watched_count
        )
        .as_str(),
    );
    app.desktop_sources.replace(rows);
}

pub(super) fn configured_desktop_sources_for_app(app: &AppState) -> Vec<PathBuf> {
    let snapshot = app.snapshot_settings();
    validate_settings_sources(&snapshot)
        .unwrap_or_else(|_| bentodesk_backend::desktop_sources::all_desktop_dirs(None))
}

pub(super) fn desktop_source_rows_for_settings(
    dirs: &[PathBuf],
) -> Vec<(
    bentodesk_backend::desktop_sources::DesktopSourceKind,
    SmolStr,
    bool,
)> {
    let mut rows: Vec<(
        bentodesk_backend::desktop_sources::DesktopSourceKind,
        SmolStr,
        bool,
    )> = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let kind = bentodesk_backend::desktop_sources::classify_desktop_source(dir);
        let display = dir.to_string_lossy();
        rows.push((kind, SmolStr::new(display.as_ref()), true));
    }
    rows
}

pub(super) fn stealth_file_type(path: &Path) -> &'static str {
    if path.is_dir() {
        return "Folder";
    }
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "lnk" | "LNK" => "Shortcut",
        "exe" | "EXE" | "msi" | "MSI" => "Application",
        _ => "File",
    }
}
