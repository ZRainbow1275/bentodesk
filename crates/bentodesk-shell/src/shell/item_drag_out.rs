use super::*;

pub(super) fn item_external_drag_modifier_down() -> bool {
    // SAFETY: GetAsyncKeyState is a read-only user32 query and is valid from
    // the UI thread while handling mouse input. Ctrl follows the Explorer
    // convention for "copy this file payload out to another target" and avoids
    // stealing the default in-zone reorder / cross-zone move path.
    unsafe { (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0 }
}

pub(super) struct ItemDragOutDropTargetGuard<'a> {
    root: &'a AppRoot,
}

impl Drop for ItemDragOutDropTargetGuard<'_> {
    fn drop(&mut self) {
        self.root.item_drag_out_active.set(false);
        log_static("items: drag-out self-drop-target resumed\n");
    }
}

pub(super) fn with_item_drag_out_guard<T>(root: &AppRoot, drag: impl FnOnce() -> T) -> T {
    root.item_drag_out_active.set(true);
    log_static("items: drag-out self-drop-target suspended\n");
    let _guard = ItemDragOutDropTargetGuard { root };
    drag()
}

pub(super) fn start_item_drag_out(root: &AppRoot, source_hwnd: HWND, request: PendingItemDragOut) {
    if request.path.is_empty() {
        return;
    }
    let hwnd_bits = source_hwnd as isize;
    let path = request.path.to_string();
    let leaf = item_operation_leaf(path.as_str()).to_owned();
    log_static(format!("items: drag-out started path={path}\n").as_str());
    set_item_operation_status(
        root,
        localized_current(
            format!("正在拖出：{}", item_operation_leaf(path.as_str())),
            format!("Dragging out: {}", item_operation_leaf(path.as_str())),
        ),
    );
    let result = with_item_drag_out_guard(root, || {
        bentodesk_backend::drag_drop::start_drag_operation_from_hwnd(
            std::slice::from_ref(&path),
            hwnd_bits,
            request.copy_only,
        )
    });
    match result {
        Ok(outcome) => {
            tracing::info!(
                target: "bentodesk::drag_drop",
                %path,
                outcome = %outcome.as_str(),
                "item drag-out completed"
            );
            log_static(
                format!(
                    "items: drag-out completed path={} outcome={}\n",
                    path,
                    outcome.as_str()
                )
                .as_str(),
            );
            finalize_item_drag_out(root, &request, leaf.as_str(), outcome);
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::drag_drop",
                %path,
                error = %error,
                "item drag-out failed"
            );
            log_static(format!("items: drag-out failed path={path} error={error}\n").as_str());
            set_item_operation_status(
                root,
                localized_current(
                    format!("拖出失败：{leaf}：{error}"),
                    item_drag_out_status_for_error_message(leaf.as_str(), &error.to_string()),
                ),
            );
        }
    }
    if hwnd_bits != 0 {
        // SAFETY: hwnd_bits came from a live process-owned HWND. Invalidating
        // after the Shell drag loop returns updates the visible status.
        unsafe { InvalidateRect(hwnd_bits as HWND, ptr::null(), 0) };
    }
}

pub(super) fn finalize_item_drag_out(
    root: &AppRoot,
    request: &PendingItemDragOut,
    leaf: &str,
    outcome: bentodesk_backend::drag_drop::DragOutcome,
) {
    match outcome {
        bentodesk_backend::drag_drop::DragOutcome::Copied => {
            set_item_operation_status(
                root,
                localized_current(
                    format!("已复制到外部：{leaf}"),
                    format!("Copied out: {leaf}"),
                ),
            );
        }
        bentodesk_backend::drag_drop::DragOutcome::Moved => {
            let source_missing = match source_missing_after_shell_move(Path::new(
                request.path.as_str(),
            )) {
                Ok(missing) => missing,
                Err(error) => {
                    set_item_operation_status(
                        root,
                        localized_current(
                            format!("无法确认移出结果，已保留：{leaf}"),
                            format!("Move could not be verified; kept: {leaf}"),
                        ),
                    );
                    log_static(
                        format!(
                            "items: drag-out model-kept source-metadata-error zone={} item={} path={} error={}\n",
                            request.zone_id.0, request.item_id.0, request.path, error
                        )
                        .as_str(),
                    );
                    return;
                }
            };
            if !source_missing {
                // A target can report MOVE while expecting the source to delete
                // the original (an unoptimised move). BentoDesk never deletes
                // user bytes during drag-out, so keep the model whenever the
                // source still exists instead of manufacturing data loss.
                set_item_operation_status(
                    root,
                    localized_current(
                        format!("已复制到外部：{leaf}"),
                        format!("Copied out: {leaf}"),
                    ),
                );
                log_static(
                    format!(
                        "items: drag-out model-kept source-still-exists zone={} item={} path={}\n",
                        request.zone_id.0, request.item_id.0, request.path
                    )
                    .as_str(),
                );
                return;
            }
            // The Shell has already completed the MOVE represented by
            // `DROPEFFECT_MOVE`. For stealth-backed items that means the hidden
            // source path no longer exists: routing through ordinary RemoveItem
            // would try to restore that already-moved file, fail, and leave a
            // ghost card in the Zone. This completion path owns model cleanup
            // only; filesystem ownership has transferred to the drop target.
            remove_item_model_after_shell_move(root, request.zone_id, request.item_id);
            let removed = root
                .app
                .borrow()
                .zones
                .item(request.zone_id, request.item_id)
                .is_none();
            if removed {
                flush_dirty_zones(root);
                set_item_operation_status(
                    root,
                    localized_current(format!("已移出：{leaf}"), format!("Moved out: {leaf}")),
                );
                log_static(
                    format!(
                        "items: drag-out model-removed zone={} item={} path={}\n",
                        request.zone_id.0, request.item_id.0, request.path
                    )
                    .as_str(),
                );
            } else {
                log_static(
                    format!(
                        "items: drag-out model-kept zone={} item={} path={}\n",
                        request.zone_id.0, request.item_id.0, request.path
                    )
                    .as_str(),
                );
            }
        }
        bentodesk_backend::drag_drop::DragOutcome::Dropped => {
            // A completed drop without one exact COPY/MOVE effect is not
            // evidence that the source left disk. Keep the Zone model.
            set_item_operation_status(
                root,
                localized_current(
                    format!("已拖出：{leaf}"),
                    item_drag_out_status_for_outcome(leaf, outcome),
                ),
            );
        }
        bentodesk_backend::drag_drop::DragOutcome::Cancelled => {
            set_item_operation_status(
                root,
                localized_current(
                    format!("已取消拖出：{leaf}"),
                    item_drag_out_status_for_outcome(leaf, outcome),
                ),
            );
        }
    }
}

pub(super) fn source_missing_after_shell_move(path: &Path) -> std::io::Result<bool> {
    source_missing_from_metadata(std::fs::symlink_metadata(path))
}

pub(super) fn source_missing_from_metadata(
    metadata: std::io::Result<std::fs::Metadata>,
) -> std::io::Result<bool> {
    match metadata {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

pub(super) fn remove_item_model_after_shell_move(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bentodesk_zone::ZoneItemId,
) {
    let removed_item = {
        let mut app = root.app.borrow_mut();
        let item = app.zones.item(zone_id, item_id).cloned();
        if app.zones.remove_item(zone_id, item_id) {
            app.mark_dirty();
            item
        } else {
            None
        }
    };
    let Some(item) = removed_item else {
        return;
    };
    let (Some(original), Some(hidden)) =
        (item.original_path.as_deref(), item.hidden_path.as_deref())
    else {
        return;
    };
    if Path::new(hidden).exists() {
        return;
    }
    let Some(config) = stealth_config_for_source(root, original) else {
        return;
    };
    let result = bentodesk_backend::stealth::hidden_dir_for(&config)
        .and_then(|dir| bentodesk_backend::stealth::manifest_remove(&dir, original));
    if let Err(error) = result {
        tracing::warn!(
            target: "bentodesk::stealth",
            original,
            hidden,
            %error,
            "drag-out completed but stale recovery manifest entry could not be removed"
        );
    }
}

#[cfg(test)]
pub(super) fn start_item_drag_out_with<StartDrag>(
    root: &AppRoot,
    path: String,
    start_drag: StartDrag,
) -> bool
where
    StartDrag: FnOnce(
        &[String],
    ) -> Result<
        bentodesk_backend::drag_drop::DragOutcome,
        bentodesk_backend::drag_drop::DragDropError,
    >,
{
    if path.is_empty() {
        return false;
    }
    let leaf = item_operation_leaf(path.as_str()).to_owned();
    log_static(format!("items: drag-out started path={path}\n").as_str());
    match with_item_drag_out_guard(root, || start_drag(std::slice::from_ref(&path))) {
        Ok(outcome) => {
            tracing::info!(
                target: "bentodesk::drag_drop",
                %path,
                outcome = %outcome.as_str(),
                "item drag-out completed"
            );
            log_static(
                format!(
                    "items: drag-out completed path={} outcome={}\n",
                    path,
                    outcome.as_str()
                )
                .as_str(),
            );
            set_item_operation_status(
                root,
                SmolStr::new(item_drag_out_status_for_outcome(leaf.as_str(), outcome)),
            );
            true
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::drag_drop",
                %path,
                error = %e,
                "item drag-out failed"
            );
            log_static(format!("items: drag-out failed path={path} error={e}\n").as_str());
            set_item_operation_status(
                root,
                SmolStr::new(item_drag_out_status_for_error(leaf.as_str(), &e)),
            );
            true
        }
    }
}

pub(super) fn item_drag_out_status_for_outcome(
    leaf: &str,
    outcome: bentodesk_backend::drag_drop::DragOutcome,
) -> String {
    match outcome {
        bentodesk_backend::drag_drop::DragOutcome::Copied => {
            format!("Copied out: {leaf}")
        }
        bentodesk_backend::drag_drop::DragOutcome::Moved => {
            format!("Moved out: {leaf}")
        }
        bentodesk_backend::drag_drop::DragOutcome::Dropped => {
            format!("Dragged out: {leaf}")
        }
        bentodesk_backend::drag_drop::DragOutcome::Cancelled => {
            format!("Drag out cancelled: {leaf}")
        }
    }
}

#[cfg(test)]
pub(super) fn item_drag_out_status_for_error(
    leaf: &str,
    error: &bentodesk_backend::drag_drop::DragDropError,
) -> String {
    format!("Drag out failed: {leaf}: {error}")
}

pub(super) fn item_drag_out_status_for_error_message(leaf: &str, error: &str) -> String {
    format!("Drag out failed: {leaf}: {error}")
}
