//! Native shell owner: `pointer_up_drag`.

use super::*;

pub(super) fn handle_lbutton_double_click(
    root: &AppRoot,
    slot: &WindowSlot,
    _hwnd: HWND,
    x: f32,
    y: f32,
) {
    if slot.kind != WindowKind::Main
        || should_ignore_main_pointer_while_settings_aux_open(root, slot.kind)
    {
        return;
    }
    let command = {
        let app = root.app.borrow();
        item_open_command_for_double_click(&app, x, y)
    };
    if let Some(command) = command {
        root.dispatcher.push(command);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DragSelectionRelease {
    KeepCurrent,
    Restore(Option<ZoneId>),
}

pub(super) fn drag_selection_release(app: &AppState, moved: bool) -> DragSelectionRelease {
    let Some((dragged, _body_visible_at_start)) = app.zone_drag_body_visible_at_start.get() else {
        return DragSelectionRelease::KeepCurrent;
    };
    if !moved {
        return DragSelectionRelease::KeepCurrent;
    }
    // Tauri's drag model is collapse-to-zen with no automatic re-expand at
    // release. Restore an unrelated prior selection, but clear the dragged
    // zone itself if it was the panel selected before mouse-down.
    DragSelectionRelease::Restore(
        app.zone_drag_selected_before_start
            .get()
            .filter(|selected| *selected != dragged),
    )
}

pub(super) fn handle_lbutton_up(root: &AppRoot, slot: &WindowSlot, hwnd: HWND, x: f32, y: f32) {
    if root.app.borrow().active_context_menu.borrow().is_some() {
        handle_context_menu_lbutton_up(root, hwnd, x, y);
        return;
    }
    if slot.kind == WindowKind::MiniBar && handle_minibar_lbutton_up(root, slot, x, y) {
        return;
    }
    if slot.kind == WindowKind::ZoneEditor && handle_zone_editor_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::IconPicker && handle_icon_picker_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::PalettePicker && handle_palette_picker_lbutton_up(root, hwnd, x, y)
    {
        return;
    }
    if slot.kind == WindowKind::CapsulePicker && handle_capsule_picker_lbutton_up(root, hwnd, x, y)
    {
        return;
    }
    if slot.kind == WindowKind::Timeline && handle_timeline_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::SnapshotPicker
        && handle_snapshot_picker_lbutton_up(root, hwnd, x, y)
    {
        return;
    }
    if slot.kind == WindowKind::RulesWizard && handle_rules_wizard_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::BulkManager && handle_bulk_manager_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::Suggestor && handle_suggestor_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::Search && handle_search_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if should_ignore_main_pointer_while_settings_aux_open(root, slot.kind) {
        return;
    }
    if slot.kind == WindowKind::Main && handle_stack_bloom_preview_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::Main && handle_stack_tray_lbutton_up(root, hwnd, x, y) {
        return;
    }
    if slot.kind == WindowKind::Main && handle_stack_bloom_lbutton_up(root, hwnd, x, y) {
        return;
    }
    let app = root.app.borrow();
    let was_drag = app.zone_drag.get().is_some();
    let was_resize = app.zone_resize.get().is_some();
    let dragged_zone = app.zone_drag.get().map(|(id, _, _)| id);
    let zone_drag_moved = was_drag
        && app
            .zone_drag_origin
            .get()
            .map(|(_, _, moved)| moved)
            .unwrap_or(false);
    // M4 F2 — a drop that overlaps another zone forms a stack. Only when an
    // actual drag latched (moved past the 4-DIP threshold), never a
    // sub-threshold click-release. Compute the target BEFORE clearing
    // zone_drag; the dragged zone's live rect is already written into
    // app.zones by the drag mouse-move hot path. Anchor = the overlapped
    // zone, member = the dragged zone 鈬?Command::StackZone(anchor, dragged)
    // (matches the (parent, child) reducer contract and the context-menu push
    // at the StackWith arm).
    let stack_command = if was_drag {
        if zone_drag_moved {
            app.zone_drag.get().and_then(|(dragged, _, _)| {
                bento_nano_app::zone_gesture_geometry::stack_target_for_drop(&app.zones, dragged)
                    .map(|anchor| Command::StackZone(anchor, dragged))
            })
        } else {
            None
        }
    } else {
        None
    };
    let stack_drop_anchor = stack_command.as_ref().and_then(|command| match command {
        Command::StackZone(anchor, _) => Some(*anchor),
        _ => None,
    });
    let stack_capsule_click_anchor = if was_drag && !zone_drag_moved {
        dragged_zone.filter(|id| {
            app.zones
                .get(*id)
                .is_some_and(|zone| zone.is_stack_anchor())
        })
    } else {
        None
    };
    let stack_drag_settle_anchor = if zone_drag_moved {
        stack_drop_anchor.or_else(|| {
            dragged_zone.filter(|id| {
                app.zones
                    .get(*id)
                    .is_some_and(|zone| zone.is_stack_anchor())
            })
        })
    } else {
        None
    };
    let selection_release = if was_drag {
        drag_selection_release(&app, zone_drag_moved)
    } else {
        DragSelectionRelease::KeepCurrent
    };
    let click_expand_zone = if was_drag && !zone_drag_moved {
        app.zone_drag_body_visible_at_start
            .get()
            .and_then(|(zone_id, body_was_visible)| {
                zone_accepts_click_expand(&app, zone_id, body_was_visible).then_some(zone_id)
            })
    } else {
        None
    };
    if was_drag {
        if let DragSelectionRelease::Restore(selection) = selection_release {
            app.selected_zone.set(selection);
        }
        app.zone_drag.set(None);
        app.zone_drag_origin.set(None);
        app.zone_drag_body_visible_at_start.set(None);
        app.zone_drag_selected_before_start.set(None);
        app.mark_dirty();
    }
    if was_resize {
        app.zone_resize.set(None);
        app.mark_dirty();
    }
    let click_expand_changed = if let Some(zone_id) = click_expand_zone {
        // A sub-threshold pill click selects the Zone. Animate that selection
        // from the capsule instead of letting `selected_zone` expose a full
        // panel for one frame on mouse-up.
        let now_ms = unsafe { GetTickCount() };
        begin_zone_pill_segment(&app, zone_id, 0.0, true, now_ms);
        true
    } else {
        false
    };
    let stack_bloom_click_changed = if let Some(anchor) = stack_capsule_click_anchor {
        // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
        let now_ms = unsafe { GetTickCount() };
        toggle_stack_bloom_from_capsule_click(&app, anchor, now_ms)
    } else {
        false
    };
    let mut stack_drop_surface_changed = false;
    if zone_drag_moved {
        if let Some(anchor) = stack_drop_anchor {
            // The queued StackZone command owns the model mutation. Preserve
            // the unchanged release hover now, then reveal only after the
            // dispatcher has created a resolvable stack relation.
            hold_free_zone_drag_result_collapsed_until_reentry(&app, anchor, true);
            root.pending_stack_drop_bloom.set(Some(anchor));
        } else if let Some(anchor) = stack_drag_settle_anchor {
            // Moving an existing stack leaves the pointer over an already-valid
            // StackWrapper, so Tauri blooms it on this same release turn.
            // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
            let now_ms = unsafe { GetTickCount() };
            reveal_stack_at_drop_pointer(&app, anchor, now_ms);
            stack_drop_surface_changed = true;
        } else if let Some(dragged) = dragged_zone {
            hold_free_zone_drag_result_collapsed_until_reentry(&app, dragged, false);
        }
    }
    // V-8 — release any in-flight pill press regardless of release location.
    // M3-A2 — and any in-flight item-card press, on the same up event.
    // SAFETY: GetTickCount is total + thread-safe.
    let press_released = {
        let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let pill = release_pill_press_animator(&app, now_ms);
        let item = release_item_press_animator(&app, now_ms);
        pill || item
    };
    if press_released {
        app.mark_dirty();
    }
    let item_drag = app.item_drag.borrow_mut().take();
    let was_item_drag = item_drag.is_some();
    if was_drag || was_resize || was_item_drag {
        if drag_proof_log_enabled() {
            log_static("drag: release_cleared active_drag=none\n");
        }
        // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
        let now_ms = unsafe { GetTickCount() };
        log_animation_proof_state(&app, "lbutton_up_after_clear", now_ms, Some(x), Some(y));
    }
    let item_command = item_drag.as_ref().and_then(|candidate| {
        if !candidate.is_internal_dragging {
            return None;
        }
        let target_zone_id = item_drag_target_zone_for_point(&app, x, y)?;
        if target_zone_id != candidate.zone_id {
            return Some(Command::MoveItemToZone(
                candidate.zone_id,
                target_zone_id,
                bento_nano_app::ItemId(candidate.item_id.0),
            ));
        }
        let (grid_x, grid_y) = item_grid_position_for_drag_point(&app, target_zone_id, x, y)?;
        Some(Command::MoveItem(
            candidate.zone_id,
            bento_nano_app::ItemId(candidate.item_id.0),
            DispatchPoint::new(grid_x, grid_y),
        ))
    });
    drop(app);
    // M4 F2 — push the drop-overlap stack (computed above before the borrow
    // was dropped). Fires only when a real drag latched and a valid target
    // was found.
    if let Some(command) = stack_command {
        root.dispatcher.push(command);
    }
    if let Some(command) = item_command {
        root.dispatcher.push(command);
    }
    if was_drag || was_resize || was_item_drag {
        // SAFETY: ReleaseCapture canonical.
        unsafe { ReleaseCapture() };
    }
    if stack_bloom_click_changed || stack_drop_surface_changed || click_expand_changed {
        arm_hover_frame_timer(hwnd);
        request_redraw(hwnd);
    }
}

pub(super) fn should_start_item_drag_out(
    app: &AppState,
    x: f32,
    y: f32,
    external_drag_requested: bool,
) -> bool {
    external_drag_requested
        || x < 0.0
        || y < 0.0
        || x >= app.viewport.width
        || y >= app.viewport.height
        || item_drag_target_zone_for_point(app, x, y).is_none()
}

pub(super) fn handle_active_pointer_drag(
    root: &AppRoot,
    slot: &WindowSlot,
    x: f32,
    y: f32,
) -> bool {
    {
        let app = root.app.borrow();
        let item_candidate = app.item_drag.borrow().clone();
        if let Some(mut candidate) = item_candidate {
            let dx = (x as i32 - candidate.start_x).abs();
            let dy = (y as i32 - candidate.start_y).abs();
            if drag_proof_log_enabled() {
                log_static(
                    format!(
                        "items: drag-proof mouse_move x={x:.1} y={y:.1} dx={dx} dy={dy} threshold={} path={}\n",
                        ITEM_DRAG_THRESHOLD_DIP,
                        candidate.path
                    )
                    .as_str(),
                );
            }
            if dx >= ITEM_DRAG_THRESHOLD_DIP || dy >= ITEM_DRAG_THRESHOLD_DIP {
                candidate.last_x = x as i32;
                candidate.last_y = y as i32;
                let copy_only = item_external_drag_modifier_down();
                if should_start_item_drag_out(&app, x, y, copy_only) {
                    if drag_proof_log_enabled() {
                        log_static(
                            format!(
                                "items: drag-proof starting_external x={x:.1} y={y:.1} path={}\n",
                                candidate.path
                            )
                            .as_str(),
                        );
                    }
                    app.item_drag.borrow_mut().take();
                    drop(app);
                    root.pending_item_drag_out
                        .borrow_mut()
                        .replace(PendingItemDragOut {
                            zone_id: candidate.zone_id,
                            item_id: candidate.item_id,
                            path: candidate.path,
                            copy_only,
                        });
                    // SAFETY: `slot.hwnd` is the live source HWND and the
                    // message only carries process-local state stored above.
                    unsafe {
                        PostMessageW(slot.hwnd, WM_ITEM_DRAG_OUT, 0, 0);
                    }
                    return true;
                }
                candidate.is_internal_dragging = true;
                // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
                let now_ms = unsafe { GetTickCount() };
                reset_pointer_drag_hover_channels(&app, None, now_ms);
                app.item_drag.borrow_mut().replace(candidate);
                log_animation_proof_state(&app, "item_drag_live", now_ms, Some(x), Some(y));
            }
            return true;
        }
    }

    {
        let mut app = root.app.borrow_mut();
        if let Some((id, dx, dy)) = app.zone_drag.get() {
            let (sx, sy, already_moved) = app
                .zone_drag_origin
                .get()
                .unwrap_or((x as i32, y as i32, true));
            if !already_moved
                && !bento_nano_app::zone_gesture_geometry::exceeds_drag_threshold(
                    x as i32 - sx,
                    y as i32 - sy,
                )
            {
                return true;
            }
            // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
            let now_ms = unsafe { GetTickCount() };
            if !already_moved {
                app.zone_drag_origin.set(Some((sx, sy, true)));
                reset_pointer_drag_hover_channels(&app, Some(id), now_ms);
                log_animation_proof_state(&app, "zone_drag_latched", now_ms, Some(x), Some(y));
            }
            let nx = x as i32 - dx;
            let ny = y as i32 - dy;
            let (cx, cy) = if let Some(z) = app.zones.get(id) {
                let (_, _, drag_w, drag_h) =
                    bento_nano_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, z);
                bento_nano_platform::clamp_rect_into_union_bounds(
                    nx,
                    ny,
                    drag_w,
                    drag_h,
                    &slot.state.monitors,
                )
            } else {
                (nx, ny)
            };
            let moved = move_zone_live(&mut app, id, DispatchPoint::new(cx, cy));
            if moved && drag_proof_log_enabled() {
                log_static(
                    format!(
                        "drag: live_move zone_id={} x={cx} y={cy} now_ms={now_ms}\n",
                        id.0
                    )
                    .as_str(),
                );
            }
            log_animation_proof_state(&app, "zone_drag_live", now_ms, Some(x), Some(y));
            return true;
        }
        if let Some((id, w0, h0)) = app.zone_resize.get() {
            let _ = (w0, h0);
            if let Some(z) = app.zones.get(id) {
                let new_w = ((x as i32) - z.x).max(80);
                let new_h = ((y as i32) - z.y).max(60);
                // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
                let now_ms = unsafe { GetTickCount() };
                reset_pointer_drag_hover_channels(&app, Some(id), now_ms);
                let _ = resize_zone_live(&mut app, id, DispatchSize::new(new_w, new_h));
                log_animation_proof_state(&app, "zone_resize_live", now_ms, Some(x), Some(y));
            }
            return true;
        }
    }

    false
}

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
        bento_nano_backend::drag_drop::start_drag_operation_from_hwnd(
            std::slice::from_ref(&path),
            hwnd_bits,
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
    outcome: bento_nano_backend::drag_drop::DragOutcome,
) {
    match outcome {
        bento_nano_backend::drag_drop::DragOutcome::Dropped if request.copy_only => {
            set_item_operation_status(
                root,
                localized_current(
                    format!("已复制到外部：{leaf}"),
                    format!("Copied out: {leaf}"),
                ),
            );
        }
        bento_nano_backend::drag_drop::DragOutcome::Dropped => {
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
        bento_nano_backend::drag_drop::DragOutcome::Cancelled => {
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

pub(super) fn remove_item_model_after_shell_move(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bento_nano_zone::ZoneItemId,
) {
    let mut app = root.app.borrow_mut();
    if app.zones.remove_item(zone_id, item_id) {
        app.mark_dirty();
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
        bento_nano_backend::drag_drop::DragOutcome,
        bento_nano_backend::drag_drop::DragDropError,
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
    outcome: bento_nano_backend::drag_drop::DragOutcome,
) -> String {
    match outcome {
        bento_nano_backend::drag_drop::DragOutcome::Dropped => {
            format!("Dragged out: {leaf}")
        }
        bento_nano_backend::drag_drop::DragOutcome::Cancelled => {
            format!("Drag out cancelled: {leaf}")
        }
    }
}

#[cfg(test)]
pub(super) fn item_drag_out_status_for_error(
    leaf: &str,
    error: &bento_nano_backend::drag_drop::DragDropError,
) -> String {
    format!("Drag out failed: {leaf}: {error}")
}

pub(super) fn item_drag_out_status_for_error_message(leaf: &str, error: &str) -> String {
    format!("Drag out failed: {leaf}: {error}")
}
