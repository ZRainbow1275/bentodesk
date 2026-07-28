//! Command handlers for the `stacks` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::SetZoneGridColumns(id, columns) => {
            let mut app = root.app.borrow_mut();
            if let Some(z) = app.zones.get_mut(id) {
                z.set_grid_columns(columns);
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::SetZoneCapsule(id, size, shape) => {
            let mut app = root.app.borrow_mut();
            if let Some(z) = app.zones.get_mut(id) {
                z.set_capsule(
                    std::borrow::Cow::Owned(size.to_string()),
                    std::borrow::Cow::Owned(shape.to_string()),
                );
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::OpenLiveFolderPicker(id) => {
            effects.needs_redraw |= open_live_folder_picker(root, id);
        }
        Command::BindZoneToFolder(id, folder) => {
            match bind_zone_to_folder(root, id, folder.as_str()) {
                Ok(changed) => {
                    effects.needs_redraw |= changed;
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::live_folder",
                        zone_id = id.0,
                        folder = %folder,
                        error = %error,
                        "BindZoneToFolder failed"
                    );
                    set_live_folder_error(
                        root,
                        localized_current(
                            format!("区域 {} 绑定文件夹失败：{error}", id.0),
                            format!("Bind live folder failed for Zone {}: {error}", id.0),
                        ),
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        Command::UnbindZoneFolder(id) => match unbind_zone_folder(root, id) {
            Ok(changed) => {
                effects.needs_redraw |= changed;
            }
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::live_folder",
                    zone_id = id.0,
                    error = %error,
                    "UnbindZoneFolder failed"
                );
                set_live_folder_error(
                    root,
                    localized_current(
                        format!("区域 {} 解除文件夹绑定失败：{error}", id.0),
                        format!("Unbind live folder failed for Zone {}: {error}", id.0),
                    ),
                );
                effects.needs_redraw = true;
            }
        },
        Command::RefreshLiveFolder(id) => match refresh_live_folder_zone(root, id) {
            Ok(changed) => {
                effects.needs_redraw |= changed;
            }
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::live_folder",
                    zone_id = id.0,
                    error = %error,
                    "RefreshLiveFolder failed"
                );
                set_live_folder_error(
                    root,
                    localized_current(
                        format!("区域 {} 刷新文件夹失败：{error}", id.0),
                        format!("Refresh live folder failed for Zone {}: {error}", id.0),
                    ),
                );
                effects.needs_redraw = true;
            }
        },
        Command::ReorderZone(id, idx) => {
            let mut app = root.app.borrow_mut();
            if app.zones.move_to_index(id, idx as usize) {
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::AutoArrangeZone(id) => {
            let mut app = root.app.borrow_mut();
            if app.zones.auto_arrange_items(id) {
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::DuplicateZone => {
            if duplicate_selected_zone(root) {
                effects.needs_redraw = true;
            }
        }
        Command::ToggleSelectedZoneLock => {
            queue_toggle_selected_zone_lock(root);
            effects.needs_redraw = true;
        }
        Command::ToggleAllZonesVisible => {
            queue_toggle_all_zones_visible(root);
            effects.needs_redraw = true;
        }
        Command::ReflowVisibleZones => {
            queue_reflow_visible_zones(root);
            effects.needs_redraw = true;
        }
        Command::FocusNextZone => {
            if focus_visible_zone(root, true) {
                effects.needs_redraw = true;
            }
        }
        Command::FocusPreviousZone => {
            if focus_visible_zone(root, false) {
                effects.needs_redraw = true;
            }
        }
        Command::StackZone(parent, child) => {
            let reveal_at_drop = root.pending_stack_drop_bloom.get() == Some(parent);
            let mut app = root.app.borrow_mut();
            if app.zones.stack(parent, child) {
                log_static(
                    format!("stack: StackZone anchor={} child={}\n", parent.0, child.0).as_str(),
                );
                // Tauri mounts the replacement StackCapsule with
                // `.spring-emerge` (240 ms). Reuse the bounded pill
                // animator instead of adding another timer/state path.
                let now_ms = unsafe { GetTickCount() };
                app.pill_animator.borrow_mut().start(
                    parent,
                    bentodesk_app::animator::AnimChannel::StackEmerge,
                    now_ms,
                    bentodesk_app::animator::STACK_EMERGE_DURATION_MS,
                    1.0,
                    0.0,
                    bentodesk_app::animator::Easing::Linear,
                );
                // A pointer drop leaves the new StackWrapper under the
                // cursor, so Tauri immediately blooms its petals. This
                // does not open the management tray or focused preview;
                // the latter still needs the existing petal-hover intent.
                if reveal_at_drop {
                    root.pending_stack_drop_bloom.set(None);
                    reveal_stack_at_drop_pointer(&app, parent, now_ms);
                    log_static(
                        format!("stack: DropBloom anchor={} child={}\n", parent.0, child.0)
                            .as_str(),
                    );
                }
                app.mark_dirty();
                effects.needs_redraw = true;
            } else {
                if reveal_at_drop {
                    root.pending_stack_drop_bloom.set(None);
                }
                tracing::warn!(
                    target: "bentodesk::dispatcher",
                    ?parent, ?child,
                    "StackZone rejected missing/self stack target"
                );
            }
        }
        Command::UnstackZone(id) => {
            let mut app = root.app.borrow_mut();
            let viewport_w = app.viewport.width.max(1.0).round() as i32;
            let viewport_h = app.viewport.height.max(1.0).round() as i32;
            if app.zones.unstack_with_scatter(id, viewport_w, viewport_h) {
                log_static(format!("stack: UnstackZone id={}\n", id.0).as_str());
                app.stack_tray.borrow_mut().take();
                app.mark_dirty();
                effects.needs_redraw = true;
            } else {
                tracing::debug!(
                    target: "bentodesk::dispatcher",
                    ?id,
                    "UnstackZone no-op: zone was not stacked"
                );
            }
        }
        Command::OpenStackTray(id) => {
            let app = root.app.borrow();
            if let Some(anchor) = app.zones.stack_anchor_for(id) {
                if let Some(members) = app.zones.stack_member_ids(anchor) {
                    let selected = if members.contains(&id) { id } else { anchor };
                    log_static(
                        format!(
                            "stack: OpenStackTray anchor={} selected={} members={}\n",
                            anchor.0,
                            selected.0,
                            members.len()
                        )
                        .as_str(),
                    );
                    clear_stack_tray_open_hover_state(&app);
                    app.stack_tray.borrow_mut().replace(
                        StackTrayState::new(anchor, selected).with_status(localized_current(
                            "已打开叠放管理",
                            "Stack manager opened",
                        )),
                    );
                    arm_stack_tray_memory_trim(hwnd);
                    effects.needs_redraw = true;
                }
            } else {
                tracing::warn!(
                    target: "bentodesk::stack",
                    ?id,
                    "OpenStackTray rejected: zone is not in a stack"
                );
            }
        }
        Command::CloseStackTray => {
            let app = root.app.borrow();
            app.stack_tray_drag.set(None);
            if app.stack_tray.borrow_mut().take().is_some() {
                arm_resident_memory_trim(hwnd);
                effects.needs_redraw = true;
            }
        }
        Command::PreviewStackMember(anchor, member) => {
            let app = root.app.borrow();
            let valid = app
                .zones
                .stack_member_ids(anchor)
                .map(|members| members.contains(&member))
                .unwrap_or(false);
            if valid {
                let title = app
                    .zones
                    .get(member)
                    .map(|zone| {
                        localized_current(
                            format!("正在预览 {}", zone.title),
                            format!("Previewing {}", zone.title),
                        )
                    })
                    .unwrap_or_else(|| {
                        localized_current("正在预览叠放成员", "Previewing stack member")
                    });
                log_static(
                    format!(
                        "stack: PreviewStackMember anchor={} member={}\n",
                        anchor.0, member.0
                    )
                    .as_str(),
                );
                let mut state = app.stack_tray.borrow_mut();
                let next = match state.as_ref() {
                    Some(current) if current.is_management() => {
                        Some(StackTrayState::new(anchor, member).with_status(title))
                    }
                    Some(current)
                        if current.is_bloom_preview()
                            && current.anchor_zone_id == anchor
                            && current.selected_member_id == member =>
                    {
                        None
                    }
                    _ => Some(StackTrayState::bloom_preview(anchor, member)),
                };
                *state = next;
                effects.needs_redraw = true;
            } else {
                app.stack_tray.borrow_mut().replace(
                    StackTrayState::new(anchor, anchor).with_status(localized_current(
                        "叠放成员已不存在",
                        "Stack member no longer exists",
                    )),
                );
                effects.needs_redraw = true;
            }
        }
        Command::ToggleStackBloomPreview(anchor, member) => {
            let app = root.app.borrow();
            let valid = app
                .zones
                .stack_member_ids(anchor)
                .is_some_and(|members| members.contains(&member));
            if valid {
                let mut interaction = app.stack_bloom_interaction.get();
                let close_same_sticky = interaction.preview_sticky
                    && app.stack_tray.borrow().as_ref().is_some_and(|state| {
                        state.is_bloom_preview()
                            && state.anchor_zone_id == anchor
                            && state.selected_member_id == member
                    });
                if close_same_sticky {
                    app.stack_tray.borrow_mut().take();
                    interaction.active_member = None;
                    interaction.active_member_leave_started_ms = None;
                    interaction.preview_sticky = false;
                    interaction.hover_preview_opened = true;
                    log_static(
                        format!(
                            "stack: CloseStickyBloomPreview anchor={} member={}\n",
                            anchor.0, member.0
                        )
                        .as_str(),
                    );
                } else {
                    // A hover-open preview is committed in place on the
                    // first click; only a second click on that same
                    // sticky petal closes it.
                    app.stack_tray
                        .borrow_mut()
                        .replace(StackTrayState::bloom_preview(anchor, member));
                    interaction.active_member = Some(member);
                    interaction.active_member_started_ms = 0;
                    interaction.active_member_leave_started_ms = None;
                    interaction.preview_sticky = true;
                    interaction.hover_preview_opened = true;
                    log_static(
                        format!(
                            "stack: CommitStickyBloomPreview anchor={} member={}\n",
                            anchor.0, member.0
                        )
                        .as_str(),
                    );
                }
                app.stack_bloom_interaction.set(interaction);
                effects.needs_redraw = true;
            } else {
                tracing::warn!(
                    target: "bentodesk::stack",
                    ?anchor,
                    ?member,
                    "ToggleStackBloomPreview rejected: stale member"
                );
            }
        }
        Command::DetachStackMember(anchor, member) => {
            let mut app = root.app.borrow_mut();
            let member_belongs_to_anchor = app
                .zones
                .stack_member_ids(anchor)
                .map(|members| members.contains(&member))
                .unwrap_or(false);
            if !member_belongs_to_anchor {
                app.stack_tray.borrow_mut().replace(
                    StackTrayState::new(anchor, anchor).with_status(localized_current(
                        "无法移出已失效的叠放成员",
                        "Cannot detach a stale stack member",
                    )),
                );
                effects.needs_redraw = true;
                return;
            }
            if let Some(outcome) = app.zones.detach_from_stack(member) {
                log_static(
                    format!(
                        "stack: DetachStackMember anchor={} member={} new_anchor={}\n",
                        anchor.0,
                        member.0,
                        outcome.new_anchor.map(|id| id.0).unwrap_or(0)
                    )
                    .as_str(),
                );
                if let Some(new_anchor) = outcome.new_anchor {
                    app.stack_tray.borrow_mut().replace(
                        StackTrayState::new(new_anchor, new_anchor).with_status(localized_current(
                            "已移出叠放成员",
                            "Stack member detached",
                        )),
                    );
                } else {
                    app.stack_tray.borrow_mut().take();
                }
                app.stack_tray_drag.set(None);
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::DissolveStack(id) => {
            let mut app = root.app.borrow_mut();
            let anchor = app.zones.stack_anchor_for(id).unwrap_or(id);
            let viewport_w = app.viewport.width.max(1.0).round() as i32;
            let viewport_h = app.viewport.height.max(1.0).round() as i32;
            if app
                .zones
                .dissolve_stack_scattered(anchor, viewport_w, viewport_h)
            {
                log_static(format!("stack: DissolveStack anchor={}\n", anchor.0).as_str());
                app.stack_tray.borrow_mut().take();
                app.stack_tray_drag.set(None);
                app.mark_dirty();
                effects.needs_redraw = true;
            } else {
                app.stack_tray.borrow_mut().replace(
                    StackTrayState::new(anchor, anchor).with_status(localized_current(
                        "该叠放已解散",
                        "Stack is already dissolved",
                    )),
                );
                effects.needs_redraw = true;
            }
        }
        Command::ReorderStackMember(anchor, member, target_index) => {
            let mut app = root.app.borrow_mut();
            if app.zones.reorder_stack_member(anchor, member, target_index) {
                log_static(
                    format!(
                        "stack: ReorderStackMember anchor={} member={} target_index={}\n",
                        anchor.0, member.0, target_index
                    )
                    .as_str(),
                );
                let status = app
                    .zones
                    .get(member)
                    .map(|zone| {
                        localized_current(
                            format!("已移动 {}", zone.title),
                            format!("Moved {}", zone.title),
                        )
                    })
                    .unwrap_or_else(|| localized_current("已更新叠放顺序", "Stack order updated"));
                app.stack_tray
                    .borrow_mut()
                    .replace(StackTrayState::new(anchor, member).with_status(status));
                app.stack_tray_drag.set(None);
                app.mark_dirty();
                effects.needs_redraw = true;
            } else {
                app.stack_tray.borrow_mut().replace(
                    StackTrayState::new(anchor, anchor)
                        .with_status(localized_current("未改变叠放顺序", "Stack order unchanged")),
                );
                app.stack_tray_drag.set(None);
                effects.needs_redraw = true;
            }
        }
        _ => unreachable!("command routed to the wrong stacks dispatcher"),
    }
}
