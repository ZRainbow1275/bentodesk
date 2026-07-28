//! Native shell owner: `tooltips_main`.

use super::*;

pub(super) fn tooltip_text_for_hover(app: &AppState, x: f32, y: f32) -> Option<SmolStr> {
    let (zone_id, item_id, _) = ui::hit_test_zone_item(app, x, y)?;
    let zone = app.zones.get(zone_id)?;
    let item = zone.item(item_id)?;
    let display_path = item_file_display_path(item);
    if item.file_missing {
        return Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("文件已丢失：{display_path}")
            } else {
                format!("Missing: {display_path}")
            },
        ));
    }
    if display_path == item.name.as_ref() {
        Some(SmolStr::new(item.name.as_ref()))
    } else {
        Some(SmolStr::new(format!("{} —{display_path}", item.name)))
    }
}

pub(super) fn stack_bloom_hover_suppresses_main_tooltip(app: &AppState, x: f32, y: f32) -> bool {
    if app.stack_tray.borrow().is_some() || app.selected_zone.get().is_some() {
        return false;
    }
    ui::hit_test_zone(app, x, y)
        .and_then(|zone_id| app.zones.stack_anchor_for(zone_id))
        .is_some()
}

pub(super) fn toolbar_tooltip_text_for_event(app: &AppState, event_id: u32) -> Option<SmolStr> {
    match event_id {
        ui::events::PIN => {
            if app.is_pinned.get() {
                Some(SmolStr::new_static(context_menu_text(
                    "取消固定窗口",
                    "Unpin window",
                )))
            } else {
                Some(SmolStr::new_static(context_menu_text(
                    "固定窗口",
                    "Pin window",
                )))
            }
        }
        ui::events::SETTINGS => Some(SmolStr::new_static(context_menu_text(
            "打开设置",
            "Open settings",
        ))),
        ui::events::HIDE => Some(SmolStr::new_static(context_menu_text(
            "隐藏 BentoDesk",
            "Hide BentoDesk",
        ))),
        ui::events::ADD_ZONE => Some(SmolStr::new_static(context_menu_text(
            "创建区域",
            "Create zone",
        ))),
        ui::events::EXIT => Some(SmolStr::new_static(context_menu_text(
            "退出 BentoDesk",
            "Quit BentoDesk",
        ))),
        _ => None,
    }
}

pub(super) fn toolbar_tooltip_text_for_hover(
    app: &AppState,
    win: &WindowState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let id = ui::hit_test(win, x, y)?;
    let event_id = match app.tree.get(id) {
        Ok(WidgetNode::IconButton(button)) => button.on_click_event,
        _ => return None,
    };
    toolbar_tooltip_text_for_event(app, event_id)
}

pub(super) fn tooltip_text_for_main_hover(
    app: &AppState,
    win: &WindowState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    toolbar_tooltip_text_for_hover(app, win, x, y).or_else(|| {
        if stack_bloom_hover_suppresses_main_tooltip(app, x, y) {
            None
        } else {
            tooltip_text_for_hover(app, x, y)
        }
    })
}

pub(super) fn settings_tooltip_text_for_hit(
    _app: &AppState,
    _hit: ui::SettingsHit,
) -> Option<SmolStr> {
    // The Tauri Settings surface uses inline labels/descriptions and does not
    // spawn hover tooltips. Native tooltip HWNDs also escape the clipped aux
    // panel and read as stray "Enable high" windows, so Settings clears them.
    None
}

pub(super) fn search_tooltip_text_for_result(hit: &search_bar::SearchHit) -> SmolStr {
    if hit.breadcrumb.is_empty() {
        hit.name.clone()
    } else {
        SmolStr::new(format!("{} - {}", hit.name, hit.breadcrumb))
    }
}

pub(super) fn search_tooltip_text_for_hit(
    app: &AppState,
    hit: SearchBarPointerHit,
) -> Option<SmolStr> {
    match hit {
        SearchBarPointerHit::Close => {
            let label = bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SEARCH_CLOSE);
            Some(SmolStr::new_static(if label.is_empty() {
                "Close search"
            } else {
                label
            }))
        }
        SearchBarPointerHit::Row(row_index) => {
            let search = app.search_bar.borrow();
            search
                .results
                .get(row_index)
                .map(search_tooltip_text_for_result)
        }
    }
}

pub(super) fn search_tooltip_text_for_hover(app: &AppState, x: f32, y: f32) -> Option<SmolStr> {
    let visible_count = app.search_bar.borrow().visible_count();
    let hit = search_bar::search_hit_test(app.viewport, visible_count, x, y)?;
    search_tooltip_text_for_hit(app, hit)
}

pub(super) fn icon_picker_tooltip_text_for_hit(
    app: &AppState,
    hit: IconPickerHit,
) -> Option<SmolStr> {
    let icon = icon_picker_slug_for_hit(hit)?;
    let selected = app
        .icon_picker
        .borrow()
        .as_ref()
        .is_some_and(|session| session.selected_icon == icon);
    if selected {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("当前图标：{icon}")
            } else {
                format!("Selected icon {icon}")
            },
        ))
    } else {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("选择图标：{icon}")
            } else {
                format!("Choose icon {icon}")
            },
        ))
    }
}

pub(super) fn icon_picker_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let hit = picker_geometry::icon_picker_hit_test(app.viewport, x, y, ALL_ICON_KINDS.len())?;
    icon_picker_tooltip_text_for_hit(app, hit)
}

pub(super) fn palette_picker_tooltip_text_for_hit(
    app: &AppState,
    hit: PalettePickerHit,
) -> Option<SmolStr> {
    match hit {
        PalettePickerHit::Swatch(index) => {
            let swatch = palette_picker::swatch_table().get(index)?;
            let selected = app
                .palette_picker
                .borrow()
                .as_ref()
                .and_then(|session| session.selected_accent.as_ref())
                .is_some_and(|accent| accent == &swatch.hex);
            if selected {
                Some(SmolStr::new(
                    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                        format!("当前颜色：{}", swatch.hex)
                    } else {
                        format!("Selected color {} {}", swatch.slug, swatch.hex)
                    },
                ))
            } else {
                Some(SmolStr::new(
                    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                        format!("选择颜色：{}", swatch.hex)
                    } else {
                        format!("Choose color {} {}", swatch.slug, swatch.hex)
                    },
                ))
            }
        }
        PalettePickerHit::Clear => {
            let has_accent = app
                .palette_picker
                .borrow()
                .as_ref()
                .and_then(|session| session.selected_accent.as_ref())
                .is_some();
            if has_accent {
                Some(SmolStr::new_static(context_menu_text(
                    "清除强调色",
                    "Clear accent color",
                )))
            } else {
                Some(SmolStr::new_static(context_menu_text(
                    "当前未设置强调色",
                    "Accent color already clear",
                )))
            }
        }
    }
}

pub(super) fn palette_picker_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let hit = picker_geometry::palette_picker_hit_test(
        app.viewport,
        x,
        y,
        palette_picker::swatch_table().len(),
    )?;
    palette_picker_tooltip_text_for_hit(app, hit)
}

pub(super) fn suggestor_file_count_label(count: usize) -> String {
    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        format!("{count} 个文件")
    } else if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

pub(super) fn suggestor_action_row_tooltip_text(
    app: &AppState,
    row_index: usize,
    action_zh: &str,
    action_en: &str,
) -> Option<SmolStr> {
    let suggestor = app.suggestor.borrow();
    let entry = suggestor.entries().get(row_index)?;
    Some(SmolStr::new(
        if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
            format!(
                "{action_zh}：{}（{}）",
                entry.suggestion.name,
                suggestor_file_count_label(entry.selected_path_count())
            )
        } else {
            format!(
                "{action_en} suggestion {} ({})",
                entry.suggestion.name,
                suggestor_file_count_label(entry.selected_path_count())
            )
        },
    ))
}

pub(super) fn suggestor_preview_file_tooltip_text(
    app: &AppState,
    preview_offset: usize,
) -> Option<SmolStr> {
    let suggestor = app.suggestor.borrow();
    let entry = suggestor.selected_entry()?;
    let path_index = entry.preview_path_index(preview_offset)?;
    let path = entry.suggestion.matching_files.get(path_index)?;
    let selected = entry.is_path_selected(path_index);
    let verb = if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        if selected { "取消选择" } else { "选择" }
    } else if selected {
        "Deselect"
    } else {
        "Select"
    };
    Some(SmolStr::new(
        if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
            format!("{verb}文件：{}", smart_group_suggestor::path_basename(path))
        } else {
            format!(
                "{verb} preview file {}",
                smart_group_suggestor::path_basename(path)
            )
        },
    ))
}

pub(super) fn suggestor_tooltip_text_for_hit(
    app: &AppState,
    hit: SuggestorPointerHit,
) -> Option<SmolStr> {
    match hit {
        SuggestorPointerHit::Apply(row_index) => {
            suggestor_action_row_tooltip_text(app, row_index, "应用建议", "Apply")
        }
        SuggestorPointerHit::Dismiss(row_index) => {
            suggestor_action_row_tooltip_text(app, row_index, "忽略建议", "Dismiss")
        }
        // The row itself is already a large, fully labelled card. A delayed
        // tooltip here produced the stray “选择建议：” mini-window under the
        // native suggestor; keep tooltips only on compact action affordances.
        SuggestorPointerHit::Row(_) => None,
        SuggestorPointerHit::SelectAllFiles => Some(SmolStr::new_static(context_menu_text(
            "选择所有匹配文件",
            "Select all preview files",
        ))),
        SuggestorPointerHit::SelectNoFiles => Some(SmolStr::new_static(context_menu_text(
            "清空文件选择",
            "Clear preview file selection",
        ))),
        SuggestorPointerHit::TogglePreviewFile(preview_offset) => {
            suggestor_preview_file_tooltip_text(app, preview_offset)
        }
        SuggestorPointerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭智能分组建议",
            "Close smart suggestions",
        ))),
    }
}

pub(super) fn suggestor_tooltip_text_for_hover(app: &AppState, x: f32, y: f32) -> Option<SmolStr> {
    let (visible_count, preview_file_count) = {
        let suggestor = app.suggestor.borrow();
        (
            suggestor.visible_count(),
            suggestor.selected_preview_file_count(),
        )
    };
    let hit = smart_group_suggestor::suggestor_hit_test(
        app.viewport,
        visible_count,
        preview_file_count,
        x,
        y,
    )?;
    suggestor_tooltip_text_for_hit(app, hit)
}

pub(super) fn bulk_manager_row_tooltip_text(app: &AppState, row_index: usize) -> Option<SmolStr> {
    let manager = app.bulk_manager.borrow();
    let row = manager.visible_rows().get(row_index).cloned()?;
    let selected = manager.is_selected(row.id);
    let selection_action = if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        if selected { "取消选择" } else { "选择" }
    } else if selected {
        "Deselect"
    } else {
        "Select"
    };
    let item_label = if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        format!("{} 个项目", row.item_count)
    } else if row.item_count == 1 {
        "1 item".to_owned()
    } else {
        format!("{} items", row.item_count)
    };
    Some(SmolStr::new(
        if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
            format!(
                "{selection_action}区域：{}（{item_label}）",
                row.display_name
            )
        } else {
            format!("{selection_action} {} ({item_label})", row.display_name)
        },
    ))
}

pub(super) fn bulk_manager_tooltip_text_for_hit(
    app: &AppState,
    hit: BulkManagerPointerHit,
) -> Option<SmolStr> {
    match hit {
        BulkManagerPointerHit::SearchInput => Some(SmolStr::new_static(context_menu_text(
            "筛选区域",
            "Filter bulk zone rows",
        ))),
        BulkManagerPointerHit::Sort(key) => Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("按{}排序", bulk_sort_key_text(key))
            } else {
                format!("Sort bulk rows by {}", key.label())
            },
        )),
        BulkManagerPointerHit::SelectAll => Some(SmolStr::new_static(context_menu_text(
            "选择所有可见区域",
            "Select all visible zones",
        ))),
        BulkManagerPointerHit::Invert => Some(SmolStr::new_static(context_menu_text(
            "反选可见区域",
            "Invert visible selection",
        ))),
        BulkManagerPointerHit::Hide => Some(SmolStr::new_static(context_menu_text(
            "隐藏已选区域",
            "Hide selected zones",
        ))),
        BulkManagerPointerHit::Show => Some(SmolStr::new_static(context_menu_text(
            "显示已选区域",
            "Show selected zones",
        ))),
        BulkManagerPointerHit::LayoutGrid => Some(SmolStr::new_static(context_menu_text(
            "应用网格布局",
            "Apply grid layout",
        ))),
        BulkManagerPointerHit::LayoutRow => Some(SmolStr::new_static(context_menu_text(
            "应用横排布局",
            "Apply row layout",
        ))),
        BulkManagerPointerHit::LayoutColumn => Some(SmolStr::new_static(context_menu_text(
            "应用纵列布局",
            "Apply column layout",
        ))),
        BulkManagerPointerHit::LayoutSpiral => Some(SmolStr::new_static(context_menu_text(
            "应用环绕布局",
            "Apply spiral layout",
        ))),
        BulkManagerPointerHit::LayoutOrganic => Some(SmolStr::new_static(context_menu_text(
            "应用自然布局",
            "Apply organic layout",
        ))),
        BulkManagerPointerHit::Update => Some(SmolStr::new_static(context_menu_text(
            "应用属性修改",
            "Apply metadata updates",
        ))),
        BulkManagerPointerHit::TextEdit => Some(SmolStr::new_static(context_menu_text(
            "编辑已选区域文字",
            "Edit selected zone text",
        ))),
        BulkManagerPointerHit::IconPicker => Some(SmolStr::new_static(context_menu_text(
            "修改已选区域图标",
            "Choose selected zone icons",
        ))),
        BulkManagerPointerHit::AccentPicker => Some(SmolStr::new_static(context_menu_text(
            "修改已选区域颜色",
            "Choose selected zone colors",
        ))),
        BulkManagerPointerHit::Delete => Some(SmolStr::new_static(context_menu_text(
            "删除已选区域",
            "Delete selected zones",
        ))),
        BulkManagerPointerHit::Move => Some(SmolStr::new_static(context_menu_text(
            "移动已选区域",
            "Move selected zones",
        ))),
        BulkManagerPointerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭批量管理器",
            "Close bulk manager",
        ))),
        BulkManagerPointerHit::Row(row_index) => bulk_manager_row_tooltip_text(app, row_index),
    }
}

pub(super) fn bulk_manager_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let manager = app.bulk_manager.borrow();
    let visible_count = manager.visible_count();
    let visible_window_start = bulk_manager_panel::bulk_manager_visible_window_start(
        manager.cursor_index(),
        visible_count,
    );
    let hit = bulk_manager_panel::bulk_manager_hit_test(
        app.viewport,
        visible_count,
        visible_window_start,
        x,
        y,
    )?;
    bulk_manager_tooltip_text_for_hit(app, hit)
}

pub(super) fn rules_wizard_next_save_tooltip_text(app: &AppState) -> SmolStr {
    match app.rules_wizard.borrow().step() {
        WizardStep::Conditions => SmolStr::new_static(context_menu_text(
            "继续设置执行操作",
            "Continue to rule action",
        )),
        WizardStep::Action => {
            SmolStr::new_static(context_menu_text("预览匹配文件", "Preview matching files"))
        }
        WizardStep::Preview => SmolStr::new_static(context_menu_text(
            "继续填写规则详情",
            "Continue to rule details",
        )),
        WizardStep::Name => SmolStr::new_static(context_menu_text(
            "保存前检查规则",
            "Review rule before saving",
        )),
        WizardStep::Review => SmolStr::new_static(context_menu_text("保存规则", "Save rule")),
    }
}

pub(super) fn rules_wizard_row_tooltip_text(app: &AppState, row_index: usize) -> Option<SmolStr> {
    let rules = app.rules_wizard_rules.borrow();
    let rule = rules.get(row_index)?;
    if app.rules_wizard_rule_cursor.get() == row_index {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("当前规则：{}", rule.name)
            } else {
                format!("Selected rule {}", rule.name)
            },
        ))
    } else {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("选择规则：{}", rule.name)
            } else {
                format!("Select rule {}", rule.name)
            },
        ))
    }
}

pub(super) fn rules_wizard_condition_row_tooltip_text(
    app: &AppState,
    condition_index: usize,
) -> Option<SmolStr> {
    let wizard = app.rules_wizard.borrow();
    let row = wizard.conditions().get(condition_index)?;
    let predicate = if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        predicate_kind_display_text(row.kind)
    } else {
        predicate_kind_name(row.kind)
    };
    if wizard.condition_cursor() == condition_index {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("正在编辑条件 {}（{predicate}）", condition_index + 1)
            } else {
                format!("Editing condition {} ({predicate})", condition_index + 1)
            },
        ))
    } else {
        Some(SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                format!("编辑条件 {}（{predicate}）", condition_index + 1)
            } else {
                format!("Edit condition {} ({predicate})", condition_index + 1)
            },
        ))
    }
}

pub(super) fn rules_wizard_tooltip_text_for_hit(
    app: &AppState,
    hit: RulesWizardPointerHit,
) -> Option<SmolStr> {
    match hit {
        RulesWizardPointerHit::NextSave => Some(rules_wizard_next_save_tooltip_text(app)),
        RulesWizardPointerHit::Predicate => Some(SmolStr::new_static(context_menu_text(
            "切换条件类型",
            "Cycle condition predicate",
        ))),
        RulesWizardPointerHit::Action => Some(SmolStr::new_static(context_menu_text(
            "切换执行操作",
            "Cycle rule action",
        ))),
        RulesWizardPointerHit::RunMode => Some(SmolStr::new_static(context_menu_text(
            "切换运行方式",
            "Cycle run mode",
        ))),
        RulesWizardPointerHit::Combine => Some(SmolStr::new_static(context_menu_text(
            "切换全部/任一条件",
            "Toggle all/any conditions",
        ))),
        RulesWizardPointerHit::AddCondition => Some(SmolStr::new_static(context_menu_text(
            "添加条件",
            "Add condition row",
        ))),
        RulesWizardPointerHit::RemoveCondition => Some(SmolStr::new_static(context_menu_text(
            "移除当前条件",
            "Remove current condition row",
        ))),
        RulesWizardPointerHit::NextCondition => Some(SmolStr::new_static(context_menu_text(
            "编辑下一个条件",
            "Edit next condition row",
        ))),
        RulesWizardPointerHit::Edit => Some(SmolStr::new_static(context_menu_text(
            "编辑已选规则",
            "Edit selected rule",
        ))),
        RulesWizardPointerHit::Run => Some(SmolStr::new_static(context_menu_text(
            "立即运行已选规则",
            "Run selected rule now",
        ))),
        RulesWizardPointerHit::Delete => Some(SmolStr::new_static(context_menu_text(
            "删除已选规则",
            "Delete selected rule",
        ))),
        RulesWizardPointerHit::Close => Some(SmolStr::new_static(context_menu_text(
            "关闭自动整理规则",
            "Close rules wizard",
        ))),
        RulesWizardPointerHit::ConditionRow(condition_index) => {
            rules_wizard_condition_row_tooltip_text(app, condition_index)
        }
        RulesWizardPointerHit::Row(row_index) => rules_wizard_row_tooltip_text(app, row_index),
    }
}

pub(super) fn rules_wizard_tooltip_text_for_hover(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let visible_count = app.rules_wizard_rules.borrow().len();
    let visible_window_start = rules_wizard::rules_wizard_visible_rule_window_start(
        app.rules_wizard_rule_cursor.get(),
        visible_count,
    );
    let wizard = app.rules_wizard.borrow();
    let condition_count = if wizard.step() == WizardStep::Conditions {
        wizard.conditions().len()
    } else {
        0
    };
    let condition_window_start = rules_wizard::rules_wizard_visible_condition_window_start(
        wizard.condition_cursor(),
        condition_count,
    );
    let hit = rules_wizard::rules_wizard_hit_test(
        app.viewport,
        visible_count,
        visible_window_start,
        condition_count,
        condition_window_start,
        x,
        y,
    )?;
    rules_wizard_tooltip_text_for_hit(app, hit)
}
