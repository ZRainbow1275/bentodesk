//! Native shell owner: `tooltips_aux`.

use super::*;

pub(super) fn timeline_row_tooltip_text(app: &AppState, row_index: usize) -> Option<SmolStr> {
    let panel = app.timeline_panel.borrow();
    let entry = panel.entries().get(row_index)?;
    let label = if entry.delta_summary.trim().is_empty() {
        entry.id.as_str()
    } else {
        entry.delta_summary.as_str()
    };
    if panel.cursor_index() == row_index {
        Some(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("当前时间线记录：{label}")
            } else {
                format!("Selected checkpoint {label}")
            },
        ))
    } else {
        Some(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("选择时间线记录：{label}")
            } else {
                format!("Select checkpoint {label}")
            },
        ))
    }
}

pub(super) fn timeline_tooltip_text_for_hit(
    app: &AppState,
    hit: TimelinePointerHit,
) -> Option<SmolStr> {
    match hit {
        TimelinePointerHit::Save => Some(SmolStr::new_static(context_menu_text(
            "保存当前布局记录",
            "Save manual checkpoint",
        ))),
        TimelinePointerHit::Pin => Some(SmolStr::new_static(context_menu_text(
            "固定已选记录",
            "Pin selected checkpoint",
        ))),
        TimelinePointerHit::Restore => Some(SmolStr::new_static(context_menu_text(
            "恢复已选记录",
            "Restore selected checkpoint",
        ))),
        TimelinePointerHit::Delete => Some(SmolStr::new_static(context_menu_text(
            "删除已选记录",
            "Delete selected checkpoint",
        ))),
        TimelinePointerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭时间线",
            "Close timeline",
        ))),
        TimelinePointerHit::Row(row_index) => timeline_row_tooltip_text(app, row_index),
    }
}

pub(super) fn timeline_tooltip_text_for_hover(app: &AppState, x: f32, y: f32) -> Option<SmolStr> {
    let visible_count = app.timeline_panel.borrow().entries().len();
    let hit = timeline_panel::timeline_hit_test(app.viewport, visible_count, x, y)?;
    timeline_tooltip_text_for_hit(app, hit)
}

pub(super) fn snapshot_picker_row_tooltip_text(
    app: &AppState,
    row_index: usize,
) -> Option<SmolStr> {
    let picker = app.snapshot_picker.borrow();
    let entry = picker.entries().get(row_index)?;
    if picker.cursor_index() == row_index {
        Some(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("当前布局快照：{}", entry.name)
            } else {
                format!("Selected snapshot {}", entry.name)
            },
        ))
    } else {
        Some(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("选择布局快照：{}", entry.name)
            } else {
                format!("Select snapshot {}", entry.name)
            },
        ))
    }
}

pub(super) fn snapshot_picker_tooltip_text_for_hit(
    app: &AppState,
    hit: SnapshotPickerPointerHit,
) -> Option<SmolStr> {
    match hit {
        SnapshotPickerPointerHit::Save => Some(SmolStr::new_static(context_menu_text(
            "保存布局快照",
            "Save layout snapshot",
        ))),
        SnapshotPickerPointerHit::Load => Some(SmolStr::new_static(context_menu_text(
            "载入已选快照",
            "Load selected snapshot",
        ))),
        SnapshotPickerPointerHit::Delete => Some(SmolStr::new_static(context_menu_text(
            "删除已选快照",
            "Delete selected snapshot",
        ))),
        SnapshotPickerPointerHit::Timeline => Some(SmolStr::new_static(context_menu_text(
            "打开桌面时间线",
            "Open timeline",
        ))),
        SnapshotPickerPointerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭布局快照",
            "Close snapshot picker",
        ))),
        SnapshotPickerPointerHit::Row(row_index) => {
            snapshot_picker_row_tooltip_text(app, row_index)
        }
    }
}

pub(super) fn snapshot_picker_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let visible_count = app.snapshot_picker.borrow().entries().len();
    let hit = snapshot_picker::snapshot_picker_hit_test(app.viewport, visible_count, x, y)?;
    snapshot_picker_tooltip_text_for_hit(app, hit)
}

pub(super) fn capsule_picker_row_tooltip_text(app: &AppState, row_index: usize) -> Option<SmolStr> {
    let picker = app.capsule_picker.borrow();
    let entry = picker.entries().get(row_index)?;
    let selected = picker.selected_index() == row_index;
    let verb = if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        if selected {
            "当前胶囊"
        } else {
            "选择胶囊"
        }
    } else if selected {
        "Selected"
    } else {
        "Select"
    };
    Some(SmolStr::new(
        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
            format!("{verb}：{}（保存于 {}）", entry.name, entry.captured_at)
        } else {
            format!(
                "{verb} capsule {} captured {}",
                entry.name, entry.captured_at
            )
        },
    ))
}

pub(super) fn capsule_picker_tooltip_text_for_hit(
    app: &AppState,
    hit: CapsulePickerHit,
) -> Option<SmolStr> {
    match hit {
        CapsulePickerHit::Capture => Some(SmolStr::new_static(context_menu_text(
            "保存当前桌面布局",
            "Save the current Desktop layout",
        ))),
        CapsulePickerHit::Restore => Some(SmolStr::new_static(context_menu_text(
            "恢复选中的场景胶囊",
            "Restore the selected context capsule",
        ))),
        CapsulePickerHit::Delete => Some(SmolStr::new_static(context_menu_text(
            "删除选中的场景胶囊",
            "Delete the selected context capsule",
        ))),
        CapsulePickerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭场景胶囊",
            "Close context capsules",
        ))),
        CapsulePickerHit::Hint => Some(SmolStr::new_static(context_menu_text(
            "C 保存当前布局，Enter 恢复，Delete 删除",
            "Capsule shortcuts: C capture, Enter/R restore, D/Delete delete",
        ))),
        CapsulePickerHit::Error => app.capsule_picker.borrow().last_error().map(|error| {
            SmolStr::new(
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!("场景胶囊错误：{error}")
                } else {
                    format!("Capsule error: {error}")
                },
            )
        }),
        CapsulePickerHit::Empty => Some(SmolStr::new_static(context_menu_text(
            "暂无场景胶囊；按 C 保存当前布局",
            "No capsules yet; press C to capture the current layout",
        ))),
        CapsulePickerHit::Row(row_index) => capsule_picker_row_tooltip_text(app, row_index),
    }
}

pub(super) fn capsule_picker_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let (visible_count, has_error) = {
        let picker = app.capsule_picker.borrow();
        (picker.entries().len(), picker.last_error().is_some())
    };
    let hit =
        capsule_picker::capsule_picker_hit_test(app.viewport, visible_count, has_error, x, y)?;
    capsule_picker_tooltip_text_for_hit(app, hit)
}

pub(super) fn item_file_rename_tooltip_text_for_hit(
    app: &AppState,
    hit: ItemFileRenameHit,
) -> Option<SmolStr> {
    let session = app.item_file_rename.borrow();
    let Some(session) = session.as_ref() else {
        return match hit {
            ItemFileRenameHit::CurrentPath => Some(SmolStr::new_static(context_menu_text(
                "未选择任何项目",
                "No item selected",
            ))),
            ItemFileRenameHit::Input => Some(SmolStr::new_static(context_menu_text(
                "输入新的文件名",
                "Type the new filename",
            ))),
            ItemFileRenameHit::Status => Some(SmolStr::new_static(context_menu_text(
                "Enter 确认重命名，Esc 取消",
                "Enter to rename; Esc cancels",
            ))),
        };
    };
    match hit {
        ItemFileRenameHit::CurrentPath => Some(SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                format!("当前文件：{}", session.current_path)
            } else {
                format!("Current file {}", session.current_path)
            },
        )),
        ItemFileRenameHit::Input => {
            if session.draft_name.trim().is_empty() {
                Some(SmolStr::new_static(context_menu_text(
                    "输入新的文件名",
                    "Type the new filename",
                )))
            } else {
                Some(SmolStr::new(
                    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                        format!("重命名为：{}", session.draft_name)
                    } else {
                        format!("Rename to {}", session.draft_name)
                    },
                ))
            }
        }
        ItemFileRenameHit::Status => session
            .status
            .as_ref()
            .map(|status| {
                SmolStr::new(
                    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                        format!("重命名校验：{status}")
                    } else {
                        format!("Rename validation: {status}")
                    },
                )
            })
            .or_else(|| {
                Some(SmolStr::new_static(context_menu_text(
                    "Enter 确认重命名，Esc 取消",
                    "Enter to rename; Esc cancels",
                )))
            }),
    }
}

pub(super) fn item_file_rename_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let hit = item_file_rename_geometry::item_file_rename_hit_test(app.viewport, x, y)?;
    item_file_rename_tooltip_text_for_hit(app, hit)
}

pub(super) fn tooltip_command_for_text(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    desired: Option<SmolStr>,
) -> Option<Command> {
    let current = app
        .active_tooltip
        .borrow()
        .as_ref()
        .map(|session| session.text.clone());
    match (desired, current) {
        (Some(text), Some(current_text)) if text == current_text => None,
        (Some(text), _) => Some(Command::ShowTooltip { anchor, text }),
        (None, Some(_)) => Some(Command::HideTooltip),
        (None, None) => None,
    }
}

#[cfg(test)]
pub(super) fn tooltip_command_for_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_main_hover(
    app: &AppState,
    win: &WindowState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = tooltip_text_for_main_hover(app, win, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_search_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = search_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_icon_picker_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = icon_picker_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_palette_picker_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = palette_picker_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_suggestor_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = suggestor_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_bulk_manager_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = bulk_manager_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_rules_wizard_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = rules_wizard_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_timeline_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = timeline_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_snapshot_picker_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = snapshot_picker_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_capsule_picker_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = capsule_picker_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_zone_editor_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    _x: f32,
    _y: f32,
) -> Option<Command> {
    // The compact editor labels every control in-place. Hover tooltips were
    // rendered as a detached black strip below the dialog and obscured the
    // desktop, so this surface only clears a stale tooltip from another HWND.
    tooltip_command_for_text(app, anchor, None)
}

pub(super) fn tooltip_command_for_item_file_rename_hover(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = item_file_rename_tooltip_text_for_hover(app, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn tooltip_command_for_minibar_hover(
    app: &AppState,
    viewport: bentodesk_style::Size,
    anchor: bentodesk_app::WindowHandle,
    x: f32,
    y: f32,
) -> Option<Command> {
    let desired = minibar_tooltip_text_for_hover(app, viewport, x, y);
    tooltip_command_for_text(app, anchor, desired)
}

pub(super) fn panel_header_button_hover_for_hit(
    hit: Option<(ZoneId, ui::HeaderButton)>,
) -> Option<PanelHeaderButtonHover> {
    let (zone_id, button) = hit?;
    let button = match button {
        ui::HeaderButton::Search => PanelHeaderButtonKind::Search,
        ui::HeaderButton::Close => PanelHeaderButtonKind::Close,
    };
    Some(PanelHeaderButtonHover::new(zone_id, button))
}

pub(super) fn update_panel_header_button_hover(app: &AppState, x: f32, y: f32) -> bool {
    let hover = panel_header_button_hover_for_hit(ui::hit_test_zone_header_button(app, x, y));
    app.set_panel_header_button_hover(hover)
}

pub(super) fn settings_encryption_mode_hover_for_hit(
    hit: ui::SettingsHit,
) -> Option<SettingsEncryptionMode> {
    match hit {
        ui::SettingsHit::SelectEncryptionModeNone => Some(SettingsEncryptionMode::None),
        ui::SettingsHit::SelectEncryptionModeDpapi => Some(SettingsEncryptionMode::Dpapi),
        ui::SettingsHit::SelectEncryptionModePassphrase => Some(SettingsEncryptionMode::Passphrase),
        _ => None,
    }
}

pub(super) fn update_settings_encryption_mode_hover_for_hit(
    app: &AppState,
    hit: ui::SettingsHit,
) -> bool {
    app.set_settings_encryption_mode_hover(settings_encryption_mode_hover_for_hit(hit))
}

pub(super) fn settings_appearance_hover_for_hit(
    hit: ui::SettingsHit,
) -> Option<bentodesk_app::theme_picker::AppearanceHit> {
    match hit {
        ui::SettingsHit::SelectTheme(id) => {
            Some(bentodesk_app::theme_picker::AppearanceHit::Card(id))
        }
        ui::SettingsHit::SelectAccent(idx) => {
            Some(bentodesk_app::theme_picker::AppearanceHit::Accent(idx))
        }
        ui::SettingsHit::EditAccentColor => {
            Some(bentodesk_app::theme_picker::AppearanceHit::AccentEditor)
        }
        ui::SettingsHit::OpenAccentColorPicker => {
            Some(bentodesk_app::theme_picker::AppearanceHit::AccentPicker)
        }
        ui::SettingsHit::ClearAccentColor => {
            Some(bentodesk_app::theme_picker::AppearanceHit::AccentClear)
        }
        _ => None,
    }
}

pub(super) fn update_settings_appearance_hover_for_hit(
    app: &AppState,
    hit: ui::SettingsHit,
) -> bool {
    app.set_settings_appearance_hover(settings_appearance_hover_for_hit(hit))
}

pub(super) fn settings_close_hover_for_hit(hit: ui::SettingsHit) -> bool {
    matches!(hit, ui::SettingsHit::Close)
}

pub(super) fn update_settings_close_hover_for_hit(app: &AppState, hit: ui::SettingsHit) -> bool {
    app.set_settings_close_hover(settings_close_hover_for_hit(hit))
}

pub(super) fn tooltip_command_for_settings_hit(
    app: &AppState,
    anchor: bentodesk_app::WindowHandle,
    hit: ui::SettingsHit,
) -> Option<Command> {
    let desired = settings_tooltip_text_for_hit(app, hit);
    tooltip_command_for_text(app, anchor, desired)
}
