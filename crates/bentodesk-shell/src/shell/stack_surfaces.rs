//! Native shell owner: `stack_surfaces`.

use super::*;

pub(super) fn stack_tray_hit(
    root: &AppRoot,
    x: f32,
    y: f32,
) -> Option<(StackTrayPointerHit, ZoneId, Vec<ZoneId>)> {
    let app = root.app.borrow();
    if app.settings_open.get() || app.about_open.get() {
        return None;
    }
    // #5 drag stability (2026-06-08) — the StackTray must not steal mouse-up
    // from an active zone/resize/item drag. StackTray's own row drag uses
    // `stack_tray_drag` and is intentionally not gated here.
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
    {
        return None;
    }
    let state = app.stack_tray.borrow().clone()?;
    if !state.is_management() {
        return None;
    }
    let anchor = app.zones.get(state.anchor_zone_id)?;
    let members = app.zones.stack_member_ids(anchor.id)?.into_vec();
    let hit = stack_tray::stack_tray_hit_test(app.viewport, anchor, members.len(), x, y)?;
    Some((hit, anchor.id, members))
}

pub(super) fn handle_stack_tray_lbutton_down(root: &AppRoot, x: f32, y: f32) -> bool {
    let Some((hit, anchor, members)) = stack_tray_hit(root, x, y) else {
        return false;
    };
    if let StackTrayPointerHit::Row(row) = hit
        && let Some(member) = members.get(row).copied()
    {
        let app = root.app.borrow();
        app.stack_tray_drag
            .set(Some(StackTrayDragState::new(anchor, member, row)));
    }
    true
}

pub(super) fn handle_stack_bloom_preview_lbutton_down(
    app: &AppState,
    hwnd: HWND,
    x: f32,
    y: f32,
) -> bool {
    if stack_bloom_preview_hit_for_point(app, x, y).is_none() {
        return false;
    }
    let Some((_, member, item_id)) = stack_bloom_preview_item_hit_for_point(app, x, y) else {
        return true;
    };
    let Some(item) = app
        .zones
        .get(member)
        .and_then(|zone| zone.items.iter().find(|item| item.id == item_id))
    else {
        return true;
    };
    if item.file_missing {
        return true;
    }
    app.item_drag.borrow_mut().replace(ItemDragCandidate {
        zone_id: member,
        item_id,
        path: SmolStr::new(item.path.as_ref()),
        start_x: x as i32,
        start_y: y as i32,
        last_x: x as i32,
        last_y: y as i32,
        is_internal_dragging: false,
    });
    // SAFETY: GetTickCount is total + thread-safe; `hwnd` is the live Main
    // window dispatching this pointer message.
    let now_ms = unsafe { GetTickCount() };
    start_item_press_animator(app, member, item_id, now_ms);
    unsafe { SetCapture(hwnd) };
    true
}

pub(super) fn handle_stack_bloom_preview_lbutton_up(
    root: &AppRoot,
    hwnd: HWND,
    x: f32,
    y: f32,
) -> bool {
    let app = root.app.borrow();
    let Some((anchor, member, preview)) = stack_bloom_preview_hit_for_point(&app, x, y) else {
        return false;
    };
    // A preview card uses the ordinary item-drag release path. Returning false
    // here lets that one shared path release capture, animate the card and
    // commit a reorder/cross-zone move; the preview must not swallow mouse-up.
    if app.item_drag.borrow().is_some() {
        return false;
    }
    let contains = |rect: bentodesk_style::Rect| {
        x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
    };
    let close = contains(stack_tray::focused_bloom_preview_close_rect(preview));
    let search = contains(stack_tray::focused_bloom_preview_search_rect(preview));
    let inline_search_active =
        app.zone_search_target.get() == Some(member) && !app.zone_search_closing.get();
    let inline_input = inline_search_active && contains(search_bar::zone_inline_rect(preview));
    let inline_clear = inline_input
        && !app.search_bar.borrow().query.is_empty()
        && contains(search_bar::zone_inline_clear_rect(preview));
    drop(app);

    if inline_input {
        if inline_clear {
            root.app.borrow().search_bar.borrow_mut().clear();
        } else {
            set_main_inline_search_keyboard_focus(hwnd, true);
        }
        {
            let app = root.app.borrow();
            // SAFETY: GetTickCount is total and thread-safe.
            touch_inline_zone_search(&app, unsafe { GetTickCount() });
        }
        request_redraw(hwnd);
        return true;
    }

    if search {
        let already_open = {
            let app = root.app.borrow();
            app.zone_search_target.get() == Some(member) && !app.zone_search_closing.get()
        };
        if already_open {
            set_main_inline_search_keyboard_focus(hwnd, true);
        } else {
            open_inline_zone_search(root, member, hwnd);
        }
        log_static(
            format!(
                "stack: SearchBloomPreview anchor={} member={} inline=true\n",
                anchor.0, member.0
            )
            .as_str(),
        );
        request_redraw(hwnd);
        return true;
    }

    if close {
        if root.app.borrow().zone_search_target.get() == Some(member) {
            close_inline_zone_search(root, hwnd);
        }
        let app = root.app.borrow();
        app.stack_tray.borrow_mut().take();
        let mut interaction = app.stack_bloom_interaction.get();
        interaction.active_member = None;
        interaction.active_member_leave_started_ms = None;
        interaction.hover_preview_opened = true;
        interaction.preview_sticky = false;
        app.stack_bloom_interaction.set(interaction);
        drop(app);
        log_static(
            format!(
                "stack: CloseBloomPreview anchor={} member={}\n",
                anchor.0, member.0
            )
            .as_str(),
        );
        request_redraw(hwnd);
        return true;
    }

    true
}

pub(super) fn handle_stack_tray_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let Some((hit, anchor, members)) = stack_tray_hit(root, x, y) else {
        let app = root.app.borrow();
        app.stack_tray_drag.set(None);
        return false;
    };
    match hit {
        StackTrayPointerHit::Row(row) => {
            let drag = {
                let app = root.app.borrow();
                let drag = app.stack_tray_drag.get();
                app.stack_tray_drag.set(None);
                drag
            };
            if let Some(drag) = drag {
                if drag.anchor_zone_id == anchor && drag.from_index != row {
                    root.dispatcher
                        .push(Command::ReorderStackMember(anchor, drag.member_id, row));
                } else if let Some(member) = members.get(row).copied() {
                    root.dispatcher
                        .push(Command::PreviewStackMember(anchor, member));
                }
            } else if let Some(member) = members.get(row).copied() {
                root.dispatcher
                    .push(Command::PreviewStackMember(anchor, member));
            }
        }
        StackTrayPointerHit::Detach(row) => {
            let app = root.app.borrow();
            app.stack_tray_drag.set(None);
            if let Some(member) = members.get(row).copied() {
                root.dispatcher
                    .push(Command::DetachStackMember(anchor, member));
            }
        }
        StackTrayPointerHit::Dissolve => {
            let app = root.app.borrow();
            app.stack_tray_drag.set(None);
            root.dispatcher.push(Command::DissolveStack(anchor));
        }
        StackTrayPointerHit::Close => {
            let app = root.app.borrow();
            app.stack_tray_drag.set(None);
            root.dispatcher.push(Command::CloseStackTray);
        }
    }
    request_redraw(hwnd);
    true
}

pub(super) fn handle_stack_bloom_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let bloom_hit = {
        let app = root.app.borrow();
        stack_bloom_hit_for_point(&app, x, y)
    };
    let Some((anchor, member)) = bloom_hit else {
        return false;
    };
    root.dispatcher
        .push(Command::ToggleStackBloomPreview(anchor, member));
    request_redraw(hwnd);
    true
}

pub(super) fn handle_timeline_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_UP_KEY => {
            select_prev_timeline_checkpoint(root);
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            select_next_timeline_checkpoint(root);
            request_redraw(hwnd);
            0
        }
        VK_S_KEY => {
            root.dispatcher.push(Command::SaveCheckpoint {
                id: None,
                label: Some(localized_current("手动保存", "manual save")),
            });
            request_redraw(hwnd);
            0
        }
        VK_P_KEY => {
            if let Some(checkpoint_id) = selected_timeline_checkpoint_id(root) {
                root.dispatcher.push(Command::SaveCheckpoint {
                    id: Some(checkpoint_id),
                    label: None,
                });
            } else {
                set_timeline_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要固定的时间线记录",
                        "No checkpoint selected to pin",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_ENTER | VK_R_KEY => {
            if let Some(checkpoint_id) = selected_timeline_checkpoint_id(root) {
                let should_restore = {
                    let app = root.app.borrow();
                    let mut panel = app.timeline_panel.borrow_mut();
                    if panel.confirm_restore_or_arm(checkpoint_id.clone()) {
                        true
                    } else {
                        panel.set_status(SmolStr::new(
                            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                                "再次选择“恢复”以替换当前布局".to_owned()
                            } else {
                                format!(
                                    "Press Restore again to replace the current layout with checkpoint {checkpoint_id}"
                                )
                            },
                        ));
                        false
                    }
                };
                if should_restore {
                    root.dispatcher
                        .push(Command::RestoreCheckpoint(checkpoint_id));
                }
            } else {
                set_timeline_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要恢复的时间线记录",
                        "No checkpoint selected to restore",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_DELETE_KEY | VK_D_KEY => {
            if let Some(checkpoint_id) = selected_timeline_checkpoint_id(root) {
                let should_delete = {
                    let app = root.app.borrow();
                    let mut panel = app.timeline_panel.borrow_mut();
                    if panel.confirm_delete_or_arm(checkpoint_id.clone()) {
                        true
                    } else {
                        panel.set_status(SmolStr::new(
                            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                                "再次选择“删除”以确认移除该记录".to_owned()
                            } else {
                                format!(
                                    "Press Delete again to permanently remove checkpoint {checkpoint_id}"
                                )
                            },
                        ));
                        false
                    }
                };
                if should_delete {
                    root.dispatcher
                        .push(Command::DeleteCheckpoint(checkpoint_id));
                }
            } else {
                set_timeline_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要删除的时间线记录",
                        "No checkpoint selected to delete",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            // SAFETY: hwnd is the focused Timeline HWND.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_timeline_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
        let visible_count = app.timeline_panel.borrow().entries().len();
        timeline_panel::timeline_hit_test(app.viewport, visible_count, x, y)
    };
    let Some(hit) = hit else {
        return false;
    };
    let mapped_key = match hit {
        TimelinePointerHit::Save => Some(VK_S_KEY),
        TimelinePointerHit::Pin => Some(VK_P_KEY),
        TimelinePointerHit::Restore => Some(VK_R_KEY),
        TimelinePointerHit::Delete => Some(VK_D_KEY),
        TimelinePointerHit::Close => Some(VK_ESCAPE_KEY),
        TimelinePointerHit::Row(row_index) => {
            select_timeline_checkpoint(root, row_index);
            request_redraw(hwnd);
            None
        }
    };
    if let Some(vk) = mapped_key {
        let _ = handle_timeline_keydown(root, vk, hwnd);
    }
    true
}

pub(super) fn handle_snapshot_picker_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.snapshot_picker.borrow_mut().select_prev();
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.snapshot_picker.borrow_mut().select_next();
            request_redraw(hwnd);
            0
        }
        VK_S_KEY => {
            root.dispatcher.push(Command::SaveSnapshot {
                name: Some(snapshot_capture_name(root)),
            });
            request_redraw(hwnd);
            0
        }
        VK_ENTER | VK_L_KEY => {
            if let Some(snapshot_id) = selected_snapshot_id(root) {
                root.dispatcher.push(Command::LoadSnapshot(snapshot_id));
            } else {
                set_snapshot_picker_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要载入的布局快照",
                        "No snapshot selected to load",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_DELETE_KEY | VK_D_KEY => {
            if let Some(snapshot_id) = selected_snapshot_id(root) {
                let should_delete = {
                    let app = root.app.borrow();
                    let mut picker = app.snapshot_picker.borrow_mut();
                    if picker.row_action().is_awaiting_for(snapshot_id.as_str()) {
                        picker.clear_delete_confirm();
                        true
                    } else {
                        picker.begin_delete_confirm(snapshot_id);
                        false
                    }
                };
                if should_delete && let Some(snapshot_id) = selected_snapshot_id(root) {
                    root.dispatcher.push(Command::DeleteSnapshot(snapshot_id));
                }
            } else {
                set_snapshot_picker_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要删除的布局快照",
                        "No snapshot selected to delete",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_T_KEY => {
            // SAFETY: hwnd is the focused SnapshotPicker HWND.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            root.dispatcher.push(Command::OpenTimeline);
            0
        }
        VK_ESCAPE_KEY => {
            let was_confirming = {
                let app = root.app.borrow();
                let mut picker = app.snapshot_picker.borrow_mut();
                let was_confirming = !matches!(
                    picker.row_action(),
                    bentodesk_app::business::timeline::snapshot_picker::RowAction::Default
                );
                picker.clear_delete_confirm();
                was_confirming
            };
            if !was_confirming {
                // SAFETY: hwnd is the focused SnapshotPicker HWND.
                unsafe { ShowWindow(hwnd, SW_HIDE) };
            }
            request_redraw(hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_snapshot_picker_lbutton_up(
    root: &AppRoot,
    hwnd: HWND,
    x: f32,
    y: f32,
) -> bool {
    let hit = {
        let app = root.app.borrow();
        let visible_count = app.snapshot_picker.borrow().entries().len();
        snapshot_picker::snapshot_picker_hit_test(app.viewport, visible_count, x, y)
    };
    let Some(hit) = hit else {
        return false;
    };
    let mapped_key = match hit {
        SnapshotPickerPointerHit::Save => Some(VK_S_KEY),
        SnapshotPickerPointerHit::Load => Some(VK_L_KEY),
        SnapshotPickerPointerHit::Delete => Some(VK_D_KEY),
        SnapshotPickerPointerHit::Timeline => Some(VK_T_KEY),
        SnapshotPickerPointerHit::Close => Some(VK_ESCAPE_KEY),
        SnapshotPickerPointerHit::Row(row_index) => {
            let app = root.app.borrow();
            app.snapshot_picker.borrow_mut().select_index(row_index);
            request_redraw(hwnd);
            None
        }
    };
    if let Some(vk) = mapped_key {
        let _ = handle_snapshot_picker_keydown(root, vk, hwnd);
    }
    true
}

pub(super) fn handle_capsule_picker_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.capsule_picker.borrow_mut().select_prev();
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.capsule_picker.borrow_mut().select_next();
            request_redraw(hwnd);
            0
        }
        VK_C_KEY => {
            let name = context_capsule_capture_name(root);
            {
                let app = root.app.borrow();
                let mut picker = app.capsule_picker.borrow_mut();
                picker.set_busy(true);
                picker.set_error(None);
            }
            root.dispatcher.push(Command::CaptureCapsule(name));
            request_redraw(hwnd);
            0
        }
        VK_ENTER | VK_R_KEY => {
            if let Some(capsule_id) = selected_context_capsule_id(root) {
                let app = root.app.borrow();
                let mut picker = app.capsule_picker.borrow_mut();
                picker.set_busy(true);
                picker.set_error(None);
                drop(picker);
                drop(app);
                root.dispatcher.push(Command::RestoreCapsule(capsule_id));
            } else {
                set_context_capsule_picker_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要恢复的场景胶囊",
                        "No capsule selected to restore",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_DELETE_KEY | VK_D_KEY => {
            if let Some(capsule_id) = selected_context_capsule_id(root) {
                let app = root.app.borrow();
                let mut picker = app.capsule_picker.borrow_mut();
                picker.set_busy(true);
                picker.set_error(None);
                drop(picker);
                drop(app);
                root.dispatcher.push(Command::DeleteCapsule(capsule_id));
            } else {
                set_context_capsule_picker_error(
                    root,
                    SmolStr::new_static(context_menu_text(
                        "尚未选择要删除的场景胶囊",
                        "No capsule selected to delete",
                    )),
                );
            }
            request_redraw(hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            // SAFETY: hwnd is the focused CapsulePicker HWND.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_capsule_picker_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let (visible_count, has_error) = {
        let app = root.app.borrow();
        let picker = app.capsule_picker.borrow();
        (picker.entries().len(), picker.last_error().is_some())
    };
    let viewport = root.app.borrow().viewport;
    let Some(hit) =
        capsule_picker::capsule_picker_hit_test(viewport, visible_count, has_error, x, y)
    else {
        return false;
    };
    let key = match hit {
        CapsulePickerHit::Capture => Some(VK_C_KEY),
        CapsulePickerHit::Restore if visible_count > 0 => Some(VK_ENTER),
        CapsulePickerHit::Delete if visible_count > 0 => Some(VK_DELETE_KEY),
        CapsulePickerHit::Close => Some(VK_ESCAPE_KEY),
        CapsulePickerHit::Row(index) => {
            let app = root.app.borrow();
            let _ = app.capsule_picker.borrow_mut().select_index(index);
            request_redraw(hwnd);
            None
        }
        CapsulePickerHit::Restore
        | CapsulePickerHit::Delete
        | CapsulePickerHit::Hint
        | CapsulePickerHit::Error
        | CapsulePickerHit::Empty => None,
    };
    if let Some(key) = key {
        let _ = handle_capsule_picker_keydown(root, key, hwnd);
    }
    true
}
