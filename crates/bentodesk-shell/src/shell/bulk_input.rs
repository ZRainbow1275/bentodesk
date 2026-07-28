//! Native shell owner: `bulk_input`.

use super::*;

pub(super) fn handle_bulk_manager_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    let text_editing = {
        let app = root.app.borrow();
        app.bulk_manager.borrow().text_edit().is_some()
    };
    if text_editing {
        return handle_bulk_manager_text_edit_keydown(root, vk, hwnd);
    }
    let search_focused = {
        let app = root.app.borrow();
        app.bulk_manager.borrow().search_focused()
    };
    if search_focused {
        return handle_bulk_manager_search_keydown(root, vk, hwnd);
    }
    match vk {
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().select_prev();
            app.bulk_manager_status.borrow_mut().take();
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().select_next();
            app.bulk_manager_status.borrow_mut().take();
            request_redraw(hwnd);
            0
        }
        VK_SPACE_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().toggle_cursor_selection();
            app.bulk_manager_status.borrow_mut().take();
            request_redraw(hwnd);
            0
        }
        VK_A_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().select_all();
            app.bulk_manager_status
                .borrow_mut()
                .replace(SmolStr::new_static(context_menu_text(
                    "已选择所有可见区域",
                    "Selected all visible zones",
                )));
            request_redraw(hwnd);
            0
        }
        VK_I_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().invert_selection();
            app.bulk_manager_status
                .borrow_mut()
                .replace(SmolStr::new_static(context_menu_text(
                    "已反选可见区域",
                    "Inverted visible selection",
                )));
            request_redraw(hwnd);
            0
        }
        VK_D_KEY | VK_DELETE_KEY => {
            let delete_decision = {
                let app = root.app.borrow();
                let mut manager = app.bulk_manager.borrow_mut();
                if manager.selected().is_empty() {
                    app.bulk_manager_status
                        .borrow_mut()
                        .replace(SmolStr::new_static(context_menu_text(
                            "尚未选择要删除的区域",
                            "No zones selected to delete",
                        )));
                    None
                } else {
                    let selected_count = manager.selected().len();
                    match manager.confirm_delete_or_arm() {
                        Some(ids) => Some(ids),
                        None => {
                            app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
                                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                                    format!("再次选择“删除”以确认移除 {selected_count} 个区域")
                                } else {
                                    format!(
                                        "Select Delete again to remove {selected_count} selected zone(s)"
                                    )
                                },
                            ));
                            None
                        }
                    }
                }
            };
            if let Some(ids) = delete_decision {
                root.dispatcher.push(Command::BulkDeleteZones(ids));
            }
            request_redraw(hwnd);
            0
        }
        VK_H_KEY => {
            let ids = {
                let app = root.app.borrow();
                app.bulk_manager.borrow().selected().to_vec()
            };
            if ids.is_empty() {
                let app = root.app.borrow();
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择要隐藏的区域",
                        "No zones selected to hide",
                    )));
            } else {
                root.dispatcher.push(Command::BulkSetZonesVisible {
                    ids,
                    visible: false,
                });
            }
            request_redraw(hwnd);
            0
        }
        VK_S_KEY => {
            let ids = {
                let app = root.app.borrow();
                app.bulk_manager.borrow().selected().to_vec()
            };
            if ids.is_empty() {
                let app = root.app.borrow();
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择要显示的区域",
                        "No zones selected to show",
                    )));
            } else {
                root.dispatcher
                    .push(Command::BulkSetZonesVisible { ids, visible: true });
            }
            request_redraw(hwnd);
            0
        }
        VK_G_KEY => queue_bulk_layout(root, hwnd, BulkLayoutAlgorithm::Grid),
        VK_R_KEY => queue_bulk_layout(root, hwnd, BulkLayoutAlgorithm::Row),
        VK_C_KEY => queue_bulk_layout(root, hwnd, BulkLayoutAlgorithm::Column),
        VK_P_KEY => queue_bulk_layout(root, hwnd, BulkLayoutAlgorithm::Spiral),
        VK_O_KEY => queue_bulk_layout(root, hwnd, BulkLayoutAlgorithm::Organic),
        VK_U_KEY => queue_bulk_metadata_update(root, hwnd),
        VK_T_KEY => begin_bulk_text_edit(root, hwnd, BulkTextEditField::Alias),
        VK_F_KEY => focus_bulk_manager_search(root, hwnd),
        VK_F3_KEY => {
            log_static("bulk: keydown F3 -> icon picker\n");
            queue_bulk_icon_picker(root, hwnd)
        }
        VK_F4_KEY => queue_bulk_accent_picker(root, hwnd),
        VK_M_KEY => {
            let ids = {
                let app = root.app.borrow();
                app.bulk_manager.borrow().selected().to_vec()
            };
            if ids.is_empty() {
                let app = root.app.borrow();
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择要移动的区域",
                        "No zones selected to move",
                    )));
            } else {
                root.dispatcher.push(Command::BulkMoveZones {
                    ids,
                    delta: DispatchPoint::new(20, 20),
                });
            }
            request_redraw(hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().click_close();
            drop(app);
            drain_bulk_manager_action(root, hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_bulk_manager_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let (hit, has_rows, has_selection) = {
        let app = root.app.borrow();
        let manager = app.bulk_manager.borrow();
        let visible_count = manager.visible_count();
        let visible_window_start = bulk_manager_panel::bulk_manager_visible_window_start(
            manager.cursor_index(),
            visible_count,
        );
        (
            bulk_manager_panel::bulk_manager_hit_test(
                app.viewport,
                visible_count,
                visible_window_start,
                x,
                y,
            ),
            visible_count > 0,
            !manager.selected().is_empty(),
        )
    };
    log_static(format!("bulk: pointer up x={x:.1} y={y:.1} hit={hit:?}\n").as_str());
    let Some(hit) = hit else {
        return false;
    };
    if !bulk_manager_panel::bulk_manager_action_enabled(hit, has_rows, has_selection) {
        return true;
    }
    let mapped_key = match hit {
        BulkManagerPointerHit::SearchInput => {
            let _ = focus_bulk_manager_search(root, hwnd);
            None
        }
        BulkManagerPointerHit::Sort(key) => {
            let app = root.app.borrow();
            let status = {
                let mut manager = app.bulk_manager.borrow_mut();
                manager.blur_search();
                manager.set_sort_key(key);
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    let direction = match manager.sort_direction() {
                        bulk_manager_panel::SortDirection::Ascending => "升序",
                        bulk_manager_panel::SortDirection::Descending => "降序",
                    };
                    SmolStr::new(format!("已按{}{}排列", bulk_sort_key_text(key), direction))
                } else {
                    let direction = match manager.sort_direction() {
                        bulk_manager_panel::SortDirection::Ascending => "ascending",
                        bulk_manager_panel::SortDirection::Descending => "descending",
                    };
                    SmolStr::new(format!("Sorted Bulk rows by {} ({direction})", key.label()))
                }
            };
            app.bulk_manager_status.borrow_mut().replace(status);
            request_redraw(hwnd);
            None
        }
        BulkManagerPointerHit::SelectAll => Some(VK_A_KEY),
        BulkManagerPointerHit::Invert => Some(VK_I_KEY),
        BulkManagerPointerHit::Hide => Some(VK_H_KEY),
        BulkManagerPointerHit::Show => Some(VK_S_KEY),
        BulkManagerPointerHit::LayoutGrid => Some(VK_G_KEY),
        BulkManagerPointerHit::LayoutRow => Some(VK_R_KEY),
        BulkManagerPointerHit::LayoutColumn => Some(VK_C_KEY),
        BulkManagerPointerHit::LayoutSpiral => Some(VK_P_KEY),
        BulkManagerPointerHit::LayoutOrganic => Some(VK_O_KEY),
        BulkManagerPointerHit::Update => Some(VK_U_KEY),
        BulkManagerPointerHit::TextEdit => {
            let _ = begin_bulk_text_edit(root, hwnd, BulkTextEditField::Alias);
            None
        }
        BulkManagerPointerHit::IconPicker => {
            let _ = queue_bulk_icon_picker(root, hwnd);
            None
        }
        BulkManagerPointerHit::AccentPicker => {
            let _ = queue_bulk_accent_picker(root, hwnd);
            None
        }
        BulkManagerPointerHit::Delete => Some(VK_D_KEY),
        BulkManagerPointerHit::Move => Some(VK_M_KEY),
        BulkManagerPointerHit::Close => Some(VK_ESCAPE_KEY),
        BulkManagerPointerHit::Row(row_index) => {
            let app = root.app.borrow();
            app.bulk_manager
                .borrow_mut()
                .toggle_visible_row_selection(row_index);
            app.bulk_manager_status.borrow_mut().take();
            log_static(format!("bulk: row toggled row_index={row_index}\n").as_str());
            request_redraw(hwnd);
            None
        }
    };
    if let Some(vk) = mapped_key {
        let _ = handle_bulk_manager_keydown(root, vk, hwnd);
    }
    true
}

pub(super) fn handle_bulk_manager_char(root: &AppRoot, wparam: u32) -> bool {
    let Some(ch) = char::from_u32(wparam) else {
        return false;
    };
    let app = root.app.borrow();
    let mut manager = app.bulk_manager.borrow_mut();
    if manager.text_edit().is_some() {
        return manager.push_text_edit_char(ch);
    }
    if !manager.search_focused() {
        return false;
    }
    if !manager.push_search_char(ch) {
        return false;
    }
    let status = bulk_manager_search_status(manager.search(), manager.visible_count());
    drop(manager);
    app.bulk_manager_status.borrow_mut().replace(status);
    true
}

pub(super) fn bulk_manager_search_status(term: &str, visible_count: usize) -> SmolStr {
    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) && term.is_empty() {
        SmolStr::new(format!("已清空搜索，共 {visible_count} 个区域"))
    } else if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        SmolStr::new(format!("“{term}”匹配 {visible_count} 个区域"))
    } else if term.is_empty() {
        SmolStr::new(format!("Bulk search cleared —{visible_count} zones listed"))
    } else {
        SmolStr::new(format!(
            "Bulk search '{term}' —{visible_count} matching zones"
        ))
    }
}

pub(super) fn focus_bulk_manager_search(root: &AppRoot, hwnd: HWND) -> LRESULT {
    focus_window_for_keyboard(hwnd);
    {
        let app = root.app.borrow();
        let status = {
            let mut manager = app.bulk_manager.borrow_mut();
            manager.focus_search();
            bulk_manager_search_status(manager.search(), manager.visible_count())
        };
        app.bulk_manager_status.borrow_mut().replace(status);
    }
    request_redraw(hwnd);
    0
}

pub(super) fn handle_bulk_manager_search_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            let status = {
                let mut manager = app.bulk_manager.borrow_mut();
                let _ = manager.backspace_search();
                bulk_manager_search_status(manager.search(), manager.visible_count())
            };
            app.bulk_manager_status.borrow_mut().replace(status);
            request_redraw(hwnd);
            0
        }
        VK_ESCAPE_KEY | VK_ENTER => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().blur_search();
            app.bulk_manager_status
                .borrow_mut()
                .replace(SmolStr::new_static(context_menu_text(
                    "已结束搜索输入",
                    "Bulk search focus cleared",
                )));
            request_redraw(hwnd);
            0
        }
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().select_prev();
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().select_next();
            request_redraw(hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_bulk_manager_text_edit_keydown(
    root: &AppRoot,
    vk: u32,
    hwnd: HWND,
) -> LRESULT {
    match vk {
        VK_ESCAPE_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().cancel_text_edit();
            app.bulk_manager_status
                .borrow_mut()
                .replace(SmolStr::new_static(context_menu_text(
                    "已取消批量文字编辑",
                    "Bulk text edit cancelled",
                )));
            request_redraw(hwnd);
            0
        }
        VK_BACKSPACE => {
            let app = root.app.borrow();
            let _ = app.bulk_manager.borrow_mut().backspace_text_edit();
            app.bulk_manager_status.borrow_mut().take();
            request_redraw(hwnd);
            0
        }
        VK_F2_KEY => {
            let app = root.app.borrow();
            app.bulk_manager.borrow_mut().cycle_text_edit_field();
            let field = app
                .bulk_manager
                .borrow()
                .text_edit()
                .map(|edit| edit.field)
                .unwrap_or(BulkTextEditField::Alias);
            app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!(
                        "正在编辑{}；Enter 应用，Esc 取消",
                        bulk_text_field_text(field)
                    )
                } else {
                    format!(
                        "Editing {} — type value, Enter apply, Esc cancel",
                        field.label()
                    )
                },
            ));
            request_redraw(hwnd);
            0
        }
        VK_ENTER => commit_bulk_text_edit(root, hwnd),
        _ => 0,
    }
}

pub(super) fn begin_bulk_text_edit(
    root: &AppRoot,
    hwnd: HWND,
    field: BulkTextEditField,
) -> LRESULT {
    let selected_count = {
        let app = root.app.borrow();
        app.bulk_manager.borrow().selected().len()
    };
    let app = root.app.borrow();
    if selected_count == 0 {
        app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("请先选择区域，再编辑{}", bulk_text_field_text(field))
            } else {
                format!("Select zones before editing {}", field.label())
            },
        ));
    } else {
        app.bulk_manager.borrow_mut().start_text_edit(field);
        app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!(
                    "正在为 {selected_count} 个区域编辑{}；Enter 应用",
                    bulk_text_field_text(field)
                )
            } else {
                format!(
                    "Editing {} for {selected_count} zones — type value, F2 field, Enter apply",
                    field.label()
                )
            },
        ));
    }
    request_redraw(hwnd);
    0
}

pub(super) fn commit_bulk_text_edit(root: &AppRoot, hwnd: HWND) -> LRESULT {
    let (ids, edit) = {
        let app = root.app.borrow();
        (
            app.bulk_manager.borrow().selected().to_vec(),
            app.bulk_manager.borrow().text_edit().cloned(),
        )
    };
    if ids.is_empty() {
        let app = root.app.borrow();
        app.bulk_manager_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "尚未选择要编辑的区域",
                "No zones selected for text edit",
            )));
        request_redraw(hwnd);
        return 0;
    }
    let Some(edit) = edit else {
        return 0;
    };
    let updates = match bulk_text_updates_for_selected(&ids, edit.field, &edit.draft) {
        Ok(updates) => updates,
        Err(error) => {
            let app = root.app.borrow();
            app.bulk_manager_status.borrow_mut().replace(error);
            request_redraw(hwnd);
            return 0;
        }
    };
    {
        let app = root.app.borrow();
        app.bulk_manager.borrow_mut().cancel_text_edit();
        app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!(
                    "正在为 {} 个区域应用{}修改",
                    ids.len(),
                    bulk_text_field_text(edit.field)
                )
            } else {
                format!(
                    "Applying {} text edit to {} zones",
                    edit.field.label(),
                    ids.len()
                )
            },
        ));
    }
    root.dispatcher.push(Command::BulkUpdateZones(updates));
    request_redraw(hwnd);
    0
}

pub(super) fn bulk_text_updates_for_selected(
    ids: &[ZoneId],
    field: BulkTextEditField,
    draft: &str,
) -> Result<Vec<BulkZoneUpdate>, SmolStr> {
    let trimmed = draft.trim();
    let mut updates = Vec::with_capacity(ids.len());
    for id in ids {
        updates.push(bulk_text_update_for_id(*id, field, trimmed)?);
    }
    Ok(updates)
}

pub(super) fn bulk_text_update_for_id(
    id: ZoneId,
    field: BulkTextEditField,
    value: &str,
) -> Result<BulkZoneUpdate, SmolStr> {
    let trimmed = value.trim();
    let mut update = BulkZoneUpdate {
        id,
        ..BulkZoneUpdate::default()
    };
    match field {
        BulkTextEditField::Alias => {
            update.alias = Some(SmolStr::new(trimmed));
        }
        BulkTextEditField::Icon => {
            if trimmed.is_empty() {
                return Err(SmolStr::new_static(context_menu_text(
                    "图标名称不能为空",
                    "Icon edit requires a non-empty icon slug",
                )));
            }
            update.icon = Some(SmolStr::new(trimmed));
        }
        BulkTextEditField::Accent => {
            if !is_bulk_hex_color(trimmed) {
                return Err(SmolStr::new_static(context_menu_text(
                    "颜色必须使用 #rrggbb 格式",
                    "Accent edit requires #rrggbb",
                )));
            }
            update.accent_color = Some(SmolStr::new(trimmed));
        }
        BulkTextEditField::CapsuleSize => {
            let lower = trimmed.to_ascii_lowercase();
            let value = match lower.as_str() {
                "small" | "medium" | "large" => lower,
                _ => {
                    return Err(SmolStr::new_static(context_menu_text(
                        "胶囊尺寸必须为 small、medium 或 large",
                        "Capsule edit requires small/medium/large",
                    )));
                }
            };
            update.capsule_size = Some(SmolStr::new(value));
        }
        BulkTextEditField::DisplayMode => {
            let lower = trimmed.to_ascii_lowercase();
            match lower.as_str() {
                "hover" | "always" | "click" => {
                    update.display_mode = Some(Some(SmolStr::new(lower)));
                }
                "clear" | "inherit" | "none" => {
                    update.display_mode = Some(None);
                }
                _ => {
                    return Err(SmolStr::new_static(context_menu_text(
                        "显示模式必须为 hover、always、click 或 clear",
                        "Mode edit requires hover/always/click/clear",
                    )));
                }
            }
        }
    }
    Ok(update)
}

pub(super) fn is_bulk_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn queue_bulk_layout(
    root: &AppRoot,
    hwnd: HWND,
    algorithm: BulkLayoutAlgorithm,
) -> LRESULT {
    let ids = {
        let app = root.app.borrow();
        bulk_layout_target_ids(&app)
    };
    if ids.is_empty() {
        let app = root.app.borrow();
        app.bulk_manager_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "暂无可调整布局的区域",
                "No zones listed to layout",
            )));
    } else {
        root.dispatcher
            .push(Command::BulkApplyLayout { ids, algorithm });
    }
    request_redraw(hwnd);
    0
}

pub(super) fn queue_bulk_metadata_update(root: &AppRoot, hwnd: HWND) -> LRESULT {
    let updates = {
        let app = root.app.borrow();
        bulk_metadata_updates_for_target_ids(&app)
    };
    if updates.is_empty() {
        let app = root.app.borrow();
        app.bulk_manager_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "暂无可更新的区域",
                "No zones listed to update",
            )));
    } else {
        root.dispatcher.push(Command::BulkUpdateZones(updates));
    }
    request_redraw(hwnd);
    0
}

pub(super) fn queue_bulk_icon_picker(root: &AppRoot, hwnd: HWND) -> LRESULT {
    let selected_count = {
        let app = root.app.borrow();
        app.bulk_manager.borrow().selected().len()
    };
    if selected_count == 0 {
        let app = root.app.borrow();
        app.bulk_manager_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "请先选择要修改图标的区域",
                "Select zones before picking an icon",
            )));
        log_static("bulk: icon picker rejected selected_count=0\n");
    } else {
        root.dispatcher
            .push(Command::OpenIconPicker { zone_id: None });
        let app = root.app.borrow();
        app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("为 {selected_count} 个已选区域选择图标")
            } else {
                format!("Pick an icon for {selected_count} selected zones")
            },
        ));
        log_static(
            format!("bulk: icon picker requested selected_count={selected_count}\n").as_str(),
        );
    }
    request_redraw(hwnd);
    0
}

pub(super) fn queue_bulk_accent_picker(root: &AppRoot, hwnd: HWND) -> LRESULT {
    let selected_count = {
        let app = root.app.borrow();
        app.bulk_manager.borrow().selected().len()
    };
    if selected_count == 0 {
        let app = root.app.borrow();
        app.bulk_manager_status
            .borrow_mut()
            .replace(SmolStr::new_static(context_menu_text(
                "请先选择要修改颜色的区域",
                "Select zones before picking a color",
            )));
    } else {
        root.dispatcher.push(Command::OpenPalettePicker {
            target: PaletteTarget::BulkManagerSelectedAccent,
        });
        let app = root.app.borrow();
        app.bulk_manager_status.borrow_mut().replace(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("为 {selected_count} 个已选区域选择颜色")
            } else {
                format!("Pick a color for {selected_count} selected zones")
            },
        ));
    }
    request_redraw(hwnd);
    0
}
