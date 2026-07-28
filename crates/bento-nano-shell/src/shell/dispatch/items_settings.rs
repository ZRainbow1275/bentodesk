//! Command handlers for the `items_settings` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    _hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::AddItem(zone_id, path) => {
            effects.needs_redraw |= add_item_to_zone(root, zone_id, path.0.as_str());
        }
        Command::RemoveItem(zone_id, item_id) => {
            effects.needs_redraw |= remove_item_from_zone(root, zone_id, item_id);
        }
        Command::OpenItemFile(zone_id, item_id) => {
            effects.needs_redraw |= open_item_file_from_zone(root, zone_id, item_id);
        }
        Command::OpenItemFileRename(zone_id, item_id) => {
            open_item_file_rename(root, zone_id, item_id);
            effects.needs_redraw = true;
        }
        Command::RenameItemFile(zone_id, item_id, new_leaf) => {
            effects.needs_redraw |= rename_item_file(root, zone_id, item_id, new_leaf.as_str());
        }
        Command::DeleteItemFileToRecycleBin(zone_id, item_id) => {
            effects.needs_redraw |= delete_item_file_to_recycle_bin(root, zone_id, item_id);
        }
        Command::CopyItemPath(path) => {
            let _copied = copy_item_path_with(root, path.0.as_str(), copy_text_to_clipboard);
        }
        Command::MoveItem(zone_id, item_id, point) => {
            let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
            let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
            let Some(item) = item else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?zone_id,
                    ?item_id,
                    ?point,
                    "MoveItem rejected: zone/item missing"
                );
                set_item_operation_status(
                    root,
                    localized_current(
                        format!("移动项目失败：区域 {}，项目 {}", zone_id.0, item_id.0),
                        format!("Move item rejected: zone {} item {}", zone_id.0, item_id.0),
                    ),
                );
                effects.needs_redraw = true;
                return;
            };
            let display_path = item_file_display_path(&item);
            let leaf = item_operation_leaf(display_path.as_str()).to_owned();
            let mut app = root.app.borrow_mut();
            if app.zones.move_item(zone_id, zone_item_id, point.x, point.y) {
                app.mark_dirty();
                app.item_operation_status
                    .borrow_mut()
                    .replace(localized_current(
                        format!("已移动项目：{leaf}（{}，{}）", point.x, point.y),
                        format!("Moved item: {leaf} ({}, {})", point.x, point.y),
                    ));
                effects.needs_redraw = true;
            } else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?zone_id,
                    ?item_id,
                    ?point,
                    "MoveItem rejected: zone/item missing"
                );
                app.item_operation_status
                    .borrow_mut()
                    .replace(localized_current(
                        format!("移动项目失败：{leaf}"),
                        format!("Move item rejected: {leaf}"),
                    ));
                effects.needs_redraw = true;
            }
        }
        Command::ToggleItemWide(zone_id, item_id) => {
            let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
            let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
            let Some(item) = item else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?zone_id,
                    ?item_id,
                    "ToggleItemWide rejected: zone/item missing"
                );
                set_item_operation_status(
                    root,
                    localized_current(
                        format!("切换宽卡片失败：区域 {}，项目 {}", zone_id.0, item_id.0),
                        format!(
                            "Toggle wide rejected: zone {} item {}",
                            zone_id.0, item_id.0
                        ),
                    ),
                );
                effects.needs_redraw = true;
                return;
            };
            let display_path = item_file_display_path(&item);
            let leaf = item_operation_leaf(display_path.as_str()).to_owned();
            let mut app = root.app.borrow_mut();
            if app.zones.toggle_item_wide(zone_id, zone_item_id) {
                let is_wide = app
                    .zones
                    .item(zone_id, zone_item_id)
                    .is_some_and(|item| item.is_wide);
                app.mark_dirty();
                app.item_operation_status
                    .borrow_mut()
                    .replace(localized_current(
                        format!("已{}宽卡片：{leaf}", if is_wide { "启用" } else { "关闭" }),
                        format!(
                            "Item wide {}: {leaf}",
                            if is_wide { "enabled" } else { "disabled" }
                        ),
                    ));
                effects.needs_redraw = true;
            } else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?zone_id,
                    ?item_id,
                    "ToggleItemWide rejected: zone/item missing"
                );
                app.item_operation_status
                    .borrow_mut()
                    .replace(localized_current(
                        format!("切换宽卡片失败：{leaf}"),
                        format!("Toggle wide rejected: {leaf}"),
                    ));
                effects.needs_redraw = true;
            }
        }
        Command::MoveItemToZone(from_zone_id, to_zone_id, item_id) => {
            let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
            let item = root
                .app
                .borrow()
                .zones
                .item(from_zone_id, zone_item_id)
                .cloned();
            let Some(item) = item else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?from_zone_id,
                    ?to_zone_id,
                    ?item_id,
                    "MoveItemToZone rejected: source item missing"
                );
                set_item_operation_status(
                    root,
                    localized_current(
                        format!("移动项目失败：区域 {}，项目 {}", from_zone_id.0, item_id.0),
                        format!(
                            "Move item rejected: zone {} item {}",
                            from_zone_id.0, item_id.0
                        ),
                    ),
                );
                effects.needs_redraw = true;
                return;
            };
            let display_path = item_file_display_path(&item);
            let leaf = item_operation_leaf(display_path.as_str()).to_owned();
            let had_hidden_file = item.hidden_path.is_some();
            let moved_paths = move_hidden_item_file_between_zones(root, &item, to_zone_id);
            let moved_hidden_file = moved_paths.is_some();
            let mut app = root.app.borrow_mut();
            if app.zones.move_item_to_zone(
                from_zone_id,
                to_zone_id,
                zone_item_id,
                moved_paths
                    .as_ref()
                    .map(|paths| std::borrow::Cow::Owned(paths.effective_path.clone())),
                moved_paths
                    .as_ref()
                    .map(|paths| std::borrow::Cow::Owned(paths.hidden_path.clone())),
            ) {
                app.mark_dirty();
                let status = if had_hidden_file && moved_hidden_file {
                    localized_current(
                        format!("已移动隐藏项目到区域：{leaf}"),
                        format!("Moved hidden item to zone: {leaf}"),
                    )
                } else if had_hidden_file {
                    localized_current(
                        format!("项目已移动，但隐藏文件未移动：{leaf}"),
                        format!("Moved item to zone without hidden move: {leaf}"),
                    )
                } else {
                    localized_current(
                        format!("已移动项目到区域：{leaf}"),
                        format!("Moved item to zone: {leaf}"),
                    )
                };
                app.item_operation_status.borrow_mut().replace(status);
                effects.needs_redraw = true;
            } else {
                tracing::warn!(
                    target: "bentodesk::items",
                    ?from_zone_id,
                    ?to_zone_id,
                    ?item_id,
                    "MoveItemToZone rejected: zone/item missing"
                );
                app.item_operation_status
                    .borrow_mut()
                    .replace(localized_current(
                        format!("移动项目失败：{leaf}"),
                        format!("Move item rejected: {leaf}"),
                    ));
                effects.needs_redraw = true;
            }
        }
        Command::SetSetting { key, value } => {
            // F2-03 — round-trip `Command::SetSetting` through the
            // process-global vault. The two SettingValue enums (one in
            // app::dispatcher, one in backend::config_vault) are
            // byte-equivalent by serde shape (see `f1-delivery.md` §6
            // gap 8), so the variant-by-variant translation here is
            // load-bearing for the cross-crate layering rule (backend
            // can't depend on app).
            if let Some(action) = key.as_str().strip_prefix(KEYBINDING_PREFIX) {
                let app = root.app.borrow();
                match &value {
                    bento_nano_app::SettingValue::Str(chord)
                        if keybindings_section::is_reserved_chord(chord.as_str()) =>
                    {
                        set_keybinding_feedback(
                            &app,
                            action,
                            localized_current("该组合键由 Windows 保留", "Reserved by Windows"),
                            true,
                        );
                        effects.needs_redraw = true;
                        return;
                    }
                    bento_nano_app::SettingValue::Str(chord) => {
                        match validate_keybinding_candidate(root, action, chord.as_str()) {
                            Ok(()) => {}
                            Err(hotkey::BindingValidationError::UnsupportedActionOrChord) => {
                                set_keybinding_feedback(
                                    &app,
                                    action,
                                    localized_current("不支持此快捷键", "Unsupported shortcut"),
                                    true,
                                );
                                effects.needs_redraw = true;
                                return;
                            }
                            Err(hotkey::BindingValidationError::ChordAlreadyAssigned) => {
                                set_keybinding_feedback(
                                    &app,
                                    action,
                                    localized_current("该快捷键已被使用", "Already in use"),
                                    true,
                                );
                                effects.needs_redraw = true;
                                return;
                            }
                        }
                    }
                    _ => {
                        set_keybinding_feedback(
                            &app,
                            action,
                            localized_current("快捷键必须是文本", "Shortcut must be text"),
                            true,
                        );
                        effects.needs_redraw = true;
                        return;
                    }
                }
            }
            let mut stored_in_vault = false;
            match bento_nano_backend::config_vault::Vault::global() {
                Some(mtx) => match mtx.lock() {
                    Ok(mut vault) => {
                        if key.as_str() == SETTING_ENCRYPTION_MODE
                            || key.as_str() == SETTING_ZONE_DISPLAY_MODE
                        {
                            match persist_setting_to_vault(&mut vault, key.as_str(), &value) {
                                Ok(true) => {
                                    let app = root.app.borrow();
                                    if apply_setting_value_to_app(&app, key.as_str(), &value) {
                                        effects.needs_redraw = true;
                                    }
                                    // P9 (#7 fix wave 2026-06-01) — mirror Tauri
                                    // `applyMode`: after a SUCCESSFUL None/DPAPI
                                    // mode change set the §10 success banner
                                    // `${ENCRYPTION_MODE_APPLIED} ${modeLabel}`
                                    // (green #34d399). `persist_setting_to_vault`
                                    // returns `Ok(true)` ONLY on a valid, flushed
                                    // None/DPAPI change (Passphrase + rejects are
                                    // `Ok(false)`), so this never fires a false
                                    // success.
                                    if key.as_str() == SETTING_ENCRYPTION_MODE {
                                        set_encryption_mode_applied_banner(&app);
                                        effects.needs_redraw = true;
                                    }
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    tracing::warn!(
                                        target: "bentodesk::vault",
                                        %key, error = %e,
                                        "SetSetting validated setting flush failed; runtime state not updated"
                                    );
                                }
                            }
                            return;
                        }
                        let backend_value = backend_setting_value_from_app(&value);
                        vault.set_setting(&key, backend_value);
                        stored_in_vault = true;
                        if let Err(e) = vault.flush() {
                            tracing::warn!(
                                target: "bentodesk::vault",
                                %key, error = %e,
                                "SetSetting flush failed — value retained in memory"
                            );
                            if let Some(action) = key.as_str().strip_prefix(KEYBINDING_PREFIX) {
                                stored_in_vault = false;
                                let app = root.app.borrow();
                                set_keybinding_feedback(
                                    &app,
                                    action,
                                    localized_current(
                                        format!("保存失败：{e}"),
                                        format!("Save failed: {e}"),
                                    ),
                                    true,
                                );
                                effects.needs_redraw = true;
                            }
                        }
                    }
                    Err(_poisoned) => {
                        // Poisoned mutex — another thread panicked while
                        // holding the lock. Drop this set rather than
                        // recover the inner Vault (an inconsistent vault
                        // is worse than a missed write).
                        tracing::warn!(
                            target: "bentodesk::vault",
                            %key,
                            "SetSetting: vault mutex poisoned"
                        );
                        if let Some(action) = key.as_str().strip_prefix(KEYBINDING_PREFIX) {
                            let app = root.app.borrow();
                            set_keybinding_feedback(
                                &app,
                                action,
                                localized_current(
                                    "设置存储锁定失败",
                                    "Settings storage lock failed",
                                ),
                                true,
                            );
                            effects.needs_redraw = true;
                        }
                    }
                },
                None => {
                    // init_global never ran (resolver failed at startup);
                    // log + continue so the dispatcher pump stays live.
                    tracing::warn!(
                        target: "bentodesk::vault",
                        %key,
                        "SetSetting: vault not initialised"
                    );
                    if let Some(action) = key.as_str().strip_prefix(KEYBINDING_PREFIX) {
                        let app = root.app.borrow();
                        set_keybinding_feedback(
                            &app,
                            action,
                            localized_current("设置存储不可用", "Settings storage unavailable"),
                            true,
                        );
                        effects.needs_redraw = true;
                    }
                }
            }
            if stored_in_vault {
                let hotkey_changed = apply_hotkey_setting_to_runtime(root, key.as_str(), &value);
                let app = root.app.borrow();
                if let Some(action) = key.as_str().strip_prefix(KEYBINDING_PREFIX) {
                    if let bento_nano_app::SettingValue::Str(chord) = &value {
                        set_keybinding_feedback(
                            &app,
                            action,
                            localized_current(format!("已保存 {chord}"), format!("Saved {chord}")),
                            false,
                        );
                        effects.needs_redraw = true;
                    }
                }
                if apply_setting_value_to_app(&app, key.as_str(), &value) || hotkey_changed {
                    effects.needs_redraw = true;
                }
            }
        }
        Command::ResetKeybinding { action } => {
            let Some(default_chord) = hotkey::default_chord_for_action(action.as_str()) else {
                let app = root.app.borrow();
                set_keybinding_feedback(
                    &app,
                    action.as_str(),
                    localized_current("不支持此操作", "Unsupported action"),
                    true,
                );
                effects.needs_redraw = true;
                return;
            };
            if let Err(error) = validate_keybinding_candidate(root, action.as_str(), default_chord)
            {
                let app = root.app.borrow();
                let message = match error {
                    hotkey::BindingValidationError::UnsupportedActionOrChord => {
                        localized_current("不支持此默认快捷键", "Unsupported default shortcut")
                    }
                    hotkey::BindingValidationError::ChordAlreadyAssigned => {
                        localized_current("默认快捷键已被使用", "Default shortcut already in use")
                    }
                };
                set_keybinding_feedback(&app, action.as_str(), message, true);
                effects.needs_redraw = true;
                return;
            }
            match bento_nano_backend::config_vault::Vault::global() {
                Some(mtx) => match mtx.lock() {
                    Ok(mut vault) => {
                        match persist_keybinding_reset_to_vault(&mut vault, action.as_str()) {
                            Ok(true) => {
                                let _changed =
                                    apply_hotkey_binding(root, action.as_str(), default_chord);
                                let app = root.app.borrow();
                                set_keybinding_feedback(
                                    &app,
                                    action.as_str(),
                                    localized_current(
                                        format!("已重置为 {default_chord}"),
                                        format!("Reset to {default_chord}"),
                                    ),
                                    false,
                                );
                                effects.needs_redraw = true;
                            }
                            Ok(false) => {
                                let app = root.app.borrow();
                                set_keybinding_feedback(
                                    &app,
                                    action.as_str(),
                                    localized_current("不支持此操作", "Unsupported action"),
                                    true,
                                );
                                effects.needs_redraw = true;
                            }
                            Err(error) => {
                                let app = root.app.borrow();
                                set_keybinding_feedback(
                                    &app,
                                    action.as_str(),
                                    localized_current(
                                        format!("重置失败：{error}"),
                                        format!("Reset failed: {error}"),
                                    ),
                                    true,
                                );
                                effects.needs_redraw = true;
                            }
                        }
                    }
                    Err(_poisoned) => {
                        let app = root.app.borrow();
                        set_keybinding_feedback(
                            &app,
                            action.as_str(),
                            localized_current("设置存储锁定失败", "Settings storage lock failed"),
                            true,
                        );
                        effects.needs_redraw = true;
                    }
                },
                None => {
                    let app = root.app.borrow();
                    set_keybinding_feedback(
                        &app,
                        action.as_str(),
                        localized_current("设置存储不可用", "Settings storage unavailable"),
                        true,
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        _ => unreachable!("command routed to the wrong items_settings dispatcher"),
    }
}
